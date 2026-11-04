#!/usr/bin/env python3
"""Original local V2 bank through parser/PG/Kafka and isolated REVM workers.

The RPC transport replays a published-contract native bank. Neither the replay
nor PROVIDED_LOCAL_STATE is canonical-chain or production VERIFIED evidence.
"""
from pathlib import Path
import copy, hashlib, importlib.util, json, secrets, subprocess, tempfile, threading, time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
ROOT=Path(__file__).resolve().parents[3]
OUT=ROOT/'integration/artifacts/local'
TARGET=OUT/'evm-execution-three-service-smoke.json'
FRESHNESS=315360000000

def require(ok,message):
    if not ok:raise ValueError(message)
def initial():return dict(passed=False,state='RUNNING',verified=False,production_ready=False,all_requested_features_complete=False,authority='ORIGINAL_LOCAL_BANK_PROVIDED_STATE_REVM_NOT_CANONICAL',checks=[])
def write(value):
    OUT.mkdir(parents=True,exist_ok=True);temporary=TARGET.with_name(TARGET.name+'.'+secrets.token_hex(6)+'.tmp')
    try:temporary.write_text(json.dumps(value,indent=2)+'\n');temporary.replace(TARGET)
    finally:temporary.unlink(missing_ok=True)
if __name__=='__main__':write(initial())
from infrastructure import Infrastructure,port
from configuration import config_for,save_config,service_command,sample_for
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import http,wait_scope
from weighted_evidence import runtime_fingerprint,snapshot_inputs
from curve_ng_smoke import bootstrap_snapshot
spec=importlib.util.spec_from_file_location('wire',ROOT/'integration/tools/ci/curve-crypto-wire-golden.py');wire=importlib.util.module_from_spec(spec);spec.loader.exec_module(wire)

class NativeBank:
    def __init__(self):
        self.document=json.loads((ROOT/'integration/test/contracts/evm-execution.json').read_text())
        self.case=copy.deepcopy(self.document['cases'][0]);self.lock=threading.RLock();self.fault=None;self.reads={}
        self.number=self.case['state_anchor']['block_number'];self.timestamp=self.case['state_anchor']['timestamp']
        self.header=self.case['evidence']['preblock_header'];self.genesis=self.document['reference']['genesis_header']
        self.accounts={a['address']:a for a in self.case['accounts']}
        source=[row['receipt']for row in self.document['reference']['setup_transactions']]+[c['evidence']['receipt']for c in self.document['cases']]
        self.receipts={r['transactionHash']:r for r in source}
        require(all(tx in self.receipts for tx in self.header['transactions']),'native pre-bank receipts missing')
    def select_case(self,index,fault=None):
        # An EVM parser does not reread pools at an unchanged head. Advance to
        # the next actual captured bank before injecting a dependency fault.
        with self.lock:
            self.case=copy.deepcopy(self.document['cases'][index]);self.number=self.case['state_anchor']['block_number'];self.timestamp=self.case['state_anchor']['timestamp']
            self.header=self.case['evidence']['preblock_header'];self.accounts={a['address']:a for a in self.case['accounts']};self.fault=fault
    def config(self):
        p=self.case['pool'];return dict(id=p['pool_id'],protocol=p['protocol'],token0=p['token0'],token1=p['token1'],fee_numerator=p['fee_numerator'],fee_denominator=p['fee_denominator'],program_id='',accounts=[],v2=dict(expected_factory=p['v2']['factory'],expected_code_sha256=p['v2']['code_sha256']))
    def set_fault(self,fault):
        require(fault in [None,'pool-code','factory-getter'],'unknown native-bank source fault')
        with self.lock:self.fault=fault
    def bundle(self):
        c=self.case
        return dict(schema_version=1,fixture={key:copy.deepcopy(c[key])for key in ['schema_version','state_anchor','execution_environment','accounts','block_hashes']},executor=copy.deepcopy(c['executor']),transaction_context=copy.deepcopy(c['transaction_context']))
    def word(self,slot):
        p=self.case['pool'];rows=self.accounts[p['pool_id']]['storage'];matches=[r for r in rows if int(r['slot'],16)==slot]
        require(len(matches)==1,'native V2 getter slot missing');return int(matches[0]['value'],16)
    def rpc(self,method,args):
        with self.lock:
            self.reads[method]=self.reads.get(method,0)+1
            if method=='eth_chainId':return hex(self.case['execution_environment']['chain_id'])
            if method=='eth_getBlockByNumber':
                if args[0]=='0x0':return copy.deepcopy(self.genesis)
                if args[0]in ['latest','safe','finalized']:return copy.deepcopy(self.header)
                target=int(args[0],16)
                headers=[c['evidence']['preblock_header']for c in self.document['cases']]
                matches=[h for h in headers if int(h['number'],16)==target and target<=self.number]
                require(bool(matches),'unrecorded native block')
                return copy.deepcopy(matches[0])
            if method=='eth_getBlockReceipts':
                require(args[0]in [hex(self.number),self.header['hash']],'unrecorded receipt bank')
                return [copy.deepcopy(self.receipts[tx])for tx in self.header['transactions']]
            if method=='eth_getTransactionReceipt':
                require(args[0]in self.receipts,'unrecorded native receipt')
                return copy.deepcopy(self.receipts[args[0]])
            require(method in ['eth_getCode','eth_call'],'unexpected native RPC '+method)
            anchor=args[1];require(isinstance(anchor,dict)and anchor.get('blockHash')==self.header['hash']and anchor.get('requireCanonical')is True,'native dependency not pinned to source H')
            if method=='eth_getCode':
                require(args[0]in self.accounts,'unrecorded native code')
                if self.fault=='pool-code'and args[0]==self.case['pool']['pool_id']:return '0x6000'
                return self.accounts[args[0]]['code']
            require(args[0]['to']==self.case['pool']['pool_id'],'foreign native getter')
            selector=args[0]['data'];require(len(selector)==10,'unexpected V2 getter arguments')
            if selector=='0xc45a0155':values=[0 if self.fault=='factory-getter'else self.word(5)]
            elif selector=='0x0dfe1681':values=[self.word(6)]
            elif selector=='0xd21220a7':values=[self.word(7)]
            elif selector=='0x0902f1ac':
                packed=self.word(8);mask=(1<<112)-1;values=[packed&mask,(packed>>112)&mask,packed>>224]
            else:raise ValueError('unrecorded native selector '+selector)
            return '0x'+''.join(f'{v:064x}'for v in values)
    def provider(self):
        chain=self
        class Handler(BaseHTTPRequestHandler):
            def log_message(self,*args):pass
            def do_POST(self):
                q=json.loads(self.rfile.read(int(self.headers['Content-Length'])));response={'jsonrpc':'2.0','id':q['id']}
                try:response['result']=chain.rpc(q['method'],q['params'])
                except(ValueError,KeyError,TypeError,IndexError)as error:response['error']={'code':-32000,'message':str(error)}
                data=json.dumps(response).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers()
                try:self.wfile.write(data)
                except(BrokenPipeError,ConnectionResetError):pass
        server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start();return server

def run(bank_factory=NativeBank,on_simulation=None):
    start_ms=int(time.time()*1000);inputs=snapshot_inputs();runtime=runtime_fingerprint();report=initial();bank=bank_factory();servers=[bank.provider(),bank.provider()];processes={};logs={};simulations=0
    try:
        with Infrastructure()as infrastructure,tempfile.TemporaryDirectory(prefix='xone-independent-evm-')as folder:
            root=Path(folder);token=secrets.token_urlsafe(32);names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'};ports={key:port()for key in names};configs={};sources=[f'http://127.0.0.1:{ports[key]}'for key in ['go','rust']]
            for actor,name in names.items():
                config=config_for(name,root/actor,token,ports[actor]);config['journal'].update(transport='kafka',source_urls=sources,poll_interval_ms=100,request_timeout_ms=15000);config['journal']['kafka'].update(brokers=[infrastructure.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=actor,transaction_timeout_ms=4000)
                config['control'].update(backend='disabled'if name=='dex-router'else'postgres',dsn_file=''if name=='dex-router'else str(infrastructure.dsnfile),instance_id=actor,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000);config['startup']['max_baseline_age_ms']=FRESHNESS
                if actor=='go':config['chains'][0].update(genesis_hash=bank.genesis['hash'],confirmation_tag='latest',auto_reorg_window=4,rpc_urls=[f'http://127.0.0.1:{s.server_port}'for s in servers],pools=[bank.config()])
                elif actor=='rust':sample_for('solana:localnet',root/actor/'fixtures',bank.timestamp*1000)
                else:
                    config['chains'][0]['genesis_hash']=bank.genesis['hash'];config['routing'].update(max_state_age_ms=FRESHNESS,quote_ttl_ms=600000,max_hops=1,max_candidates=8,max_expansions=64,split_parts=4,max_splits=2);config['api'].update(quote_timeout_ms=10000)
                    image=root/actor/'state.json';image.parent.mkdir(parents=True,exist_ok=True);image.write_text(json.dumps(bank.bundle()))
                    config['simulation'].update(backend='revm-fixture',rpc_url='',max_gas=15000000,max_state_reads=8192,timeout_ms=30000,allowed_targets=[bank.case['executor']['target']],state_file=str(image),max_state_bytes=8388608,max_instructions=2000000,max_memory_bytes=67108864,concurrency=2)
                configs[actor]=save_config(root/actor/'config',config)
            execution_files={str(path.relative_to(root)):path for path in root.rglob('*')if path.is_file()}
            execution_files['infrastructure-postgres-dsn']=infrastructure.dsnfile
            execution_hashes={name:hashlib.sha256(path.read_bytes()).hexdigest()for name,path in sorted(execution_files.items())}
            execution_history=[dict(stage='initial-native-bank',case=bank.case['name'],hashes=copy.deepcopy(execution_hashes))]
            def base(actor):return f'http://127.0.0.1:{ports[actor]}'
            def start(actor):
                logs[actor]=(OUT/f'evm-execution-{actor}.log').open('ab');processes[actor]=subprocess.Popen(service_command(names[actor],configs[actor],root/actor/'fixtures'if actor=='rust'else None),cwd=ROOT/names[actor],stdout=logs[actor],stderr=subprocess.STDOUT)
            def stop(actor):
                process=processes.pop(actor,None)
                if process:
                    process.terminate()
                    try:process.wait(timeout=10)
                    except subprocess.TimeoutExpired:process.kill();process.wait()
                if actor in logs:logs.pop(actor).close()
            def status(actor,key='evm:31337'):return http(base(actor)+'/internal/chains/'+key+'/status',token)
            def baseline():
                code,value=http(base('go')+'/internal/chains/evm:31337/baseline',token);require(code==200,'native source baseline missing');return value
            def synced():
                source=baseline()
                for actor in ['router1','router2']:await_condition(lambda:status(actor)[1].get('snapshot_ref')==source['reference'],'independent executor replica source mismatch')
                require(source['pools']==[bank.case['pool']],'normal parser bank differs from actual original native pool')
                return source
            request=dict(chainKey='evm:31337',kind='EXACT_IN',sellToken=bank.case['token_in'],buyToken=bank.case['token_out'],sellAmount=bank.case['amount_in'],caller=bank.case['transaction']['from'],recipient=bank.case['recipient'],slippageBps=50,verification='FAST')
            def quote(actor):
                code,q=http(base(actor)+'/v1/quote',token,request);require(code==200,('native quote failed',code,q));require(q['grossBuyAmount']==bank.case['quote_context']['quoted_buy_amount']and q['verification']['level']=='INDICATIVE','native quote math or quality differs');return q
            def simulate(actor,q):
                nonlocal simulations
                code,result=http(base(actor)+'/v1/quotes/'+q['quoteId']+'/simulate',token,{},timeout=35)
                require(code==200,('isolated EVM simulation failed',code,result));require(result['quoteId']==q['quoteId']and result['planHash']==q['planHash']and result['snapshotRef']==q['snapshotRef'],'simulation lost quote/view commitment')
                require(result['verification']=={'level':'LOCAL_EVM_EXECUTED','success':True,'verified':False}and result['verified']is False and result['authority']=='PROVIDED_LOCAL_STATE','local execution overstates canonical authority')
                outcome=result['outcome'];require(outcome['actual_buy_amount']==bank.case['expected']['amount_out']and outcome['actual_input_consumed']==request['sellAmount']and outcome['refund']=='0','real token deltas differ from original node')
                require(outcome['execution']['status']=='SUCCESS'and outcome['matches_plan']is True and outcome['verified']is False and outcome['execution']['gas_used']>0,'local executor success evidence missing')
                require(result['workerHash']==runtime['binaries']['dex-router/target/debug/dex-router'],'wrong worker binary executed')
                require(all(len(result[key])==64 for key in ['bindingHash','bundleHash','workerHash'])and len(outcome['receipt_hash'])==64,'execution commitments missing')
                require(result['fees']['chainFeeCompleteness']=='LOCAL_EVM_ONLY'and int(result['fees']['evmFeeWei'])==outcome['execution']['gas_used']*int(bank.case['transaction_context']['gas_price']),'execution gas fee or completeness differs')
                code,evidence=http(base(actor)+'/internal/quotes/'+q['quoteId']+'/evidence',token);require(code==200 and evidence['quote']['quoteId']==q['quoteId']and evidence['simulation_evidence']==result,'stored actual evidence changed quote or result')
                if on_simulation is not None:on_simulation(bank,result)
                simulations+=1;return result
            try:
                for actor in ['go','rust']:start(actor);wait(base(actor)+'/health/ready')
                for actor in ['router1','router2']:start(actor);wait(base(actor)+'/health/live');wait_scope(base(actor),token,'evm:31337');wait_scope(base(actor),token,'solana:localnet')
                synced();quotes={actor:quote(actor)for actor in ['router1','router2']}
                for actor in ['router1','router2']:simulate(actor,quotes[actor])
                report['checks']+=['normal-original-v2-bank-go-rpc-pg-kafka-two-routers','isolated-current-executable-revm-real-token-balances-and-gas','original-recipient-delta-exact-with-quote-and-bound-transaction','cached-execution-evidence-retains-quote-and-all-commitments','local-execution-never-claims-canonical-verified-or-complete-chain-fees']
                code,_=http(base('router1')+'/v1/quotes/'+quotes['router1']['quoteId']+'/simulate','',{});require(code==401,'simulation route missing auth')
                code,_=http(base('router1')+'/v1/quotes/'+quotes['router1']['quoteId']+'/simulate',token,{'recipient':bank.case['transaction']['from']});require(code in [400,422],'simulation accepted context override');report['checks'].append('simulation-auth-and-request-context-overrides-rejected')
                generation=baseline()['reference']['online_generation']
                for index,fault in enumerate(['pool-code','factory-getter'],start=1):
                    bank.select_case(index,fault)
                    for actor in ['router1','router2']:
                        await_condition(lambda:status(actor)[0]==503,'corrupt native source remained ready')
                        code,result=http(base(actor)+'/v1/quotes/'+quotes[actor]['quoteId']+'/simulate',token,{},timeout=35);require(code==409 and result.get('error')=='SNAPSHOT_REORGED',('revoked quote rejection differs',code,result))
                        code,sol=http(base(actor)+'/v1/quote',token,dict(chainKey='solana:localnet',kind='EXACT_IN',sellToken='A',buyToken='B',sellAmount='10003',caller='A',recipient='B',slippageBps=50,verification='FAST'));require(code==200 and sol['verification']['level']=='INDICATIVE','unrelated Solana quote withdrawn')
                    bank.set_fault(None)
                    await_condition(lambda:status('go')[1].get('ready')is True,'native owner did not recover')
                    source=synced();require(source['reference']['online_generation']>generation,'native recovery reused generation');generation=source['reference']['online_generation']
                    request.update(sellToken=bank.case['token_in'],buyToken=bank.case['token_out'],sellAmount=bank.case['amount_in'],caller=bank.case['transaction']['from'],recipient=bank.case['recipient'])
                    for actor in ['router1','router2']:
                        quotes[actor]=quote(actor)
                        code,result=http(base(actor)+'/v1/quotes/'+quotes[actor]['quoteId']+'/simulate',token,{},timeout=35)
                        require(code==422 and result.get('error')=='SIMULATION_ANCHOR_MISMATCH','old loaded state image was used for a newer quote')
                    require(execution_hashes=={name:hashlib.sha256(path.read_bytes()).hexdigest()for name,path in sorted(execution_files.items())},'execution files changed before authorized image update')
                    for actor in ['router1','router2']:
                        image=root/actor/'state.json';image.write_text(json.dumps(bank.bundle()))
                        execution_hashes[str(image.relative_to(root))]=hashlib.sha256(image.read_bytes()).hexdigest()
                        code,result=http(base(actor)+'/v1/quotes/'+quotes[actor]['quoteId']+'/simulate',token,{},timeout=35)
                        require(code==422 and result.get('error')=='SIMULATION_ANCHOR_MISMATCH','state file mutation silently replaced the captured startup image')
                    execution_history.append(dict(stage='authorized-image-reload:'+fault,case=bank.case['name'],hashes=copy.deepcopy(execution_hashes)))
                    for actor in ['router1','router2']:
                        stop(actor);start(actor);wait(base(actor)+'/health/live');wait_scope(base(actor),token,'evm:31337');wait_scope(base(actor),token,'solana:localnet')
                    synced()
                    for actor in ['router1','router2']:quotes[actor]=quote(actor);simulate(actor,quotes[actor])
                    report['checks'].append('source-revocation-new-generation-and-local-worker-recovery:'+fault)
                    report['checks'].append('stale-image-refused-until-explicit-reload:'+fault)
                stop('router2');start('router2');wait(base('router2')+'/health/live');wait_scope(base('router2'),token,'evm:31337');synced();simulate('router2',quote('router2'));report['checks'].append('router-restart-restores-source-and-isolated-execution')
                for actor in ['router1','router2']:
                    q=quote(actor);verified=dict(request,verification='VERIFIED');code,_=http(base(actor)+'/v1/quote',token,verified);require(code==422,'provided local fixture opened VERIFIED');code,_=http(base(actor)+'/v1/quotes/'+q['quoteId']+'/build',token,{});require(code==422,'provided local fixture opened public transaction build')
                report['checks'].append('verified-and-public-build-remain-closed')
                require(runtime==runtime_fingerprint()and inputs==snapshot_inputs(),'source/binary/reference inputs changed during actual execution')
                require(execution_hashes=={name:hashlib.sha256(path.read_bytes()).hexdigest()for name,path in sorted(execution_files.items())},'generated execution config/state inputs changed')
                report.update(passed=True,state='PASSED',source_binding=runtime,input_hashes=inputs,inputs_unchanged=True,started_at_unix_ms=start_ms,completed_at_unix_ms=int(time.time()*1000),simulation_checks=simulations,source_faults=2,execution_file_hashes=execution_hashes,execution_file_history=execution_history,execution_files_unchanged_except_authorized_reloads=True,read_counts=bank.reads,owner_reference=baseline()['reference'],test_limits={'simulation_timeout_ms':30000,'max_gas':15000000,'max_state_reads':8192,'max_instructions':2000000,'max_memory_bytes':67108864,'concurrency':2});return report
            finally:
                for actor in list(processes):stop(actor)
    finally:
        for server in servers:server.shutdown();server.server_close()

def main():
    write(initial())
    try:result=run();write(result);print(json.dumps({'passed':True,'checks':len(result['checks']),'isolated_simulations':result['simulation_checks']}))
    except BaseException as error:
        result=initial();result.update(state='FAILED',error=str(error),error_type=type(error).__name__);write(result);raise
if __name__=='__main__':main()
