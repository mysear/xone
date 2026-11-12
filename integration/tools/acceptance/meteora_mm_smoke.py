#!/usr/bin/env python3
"""Normal Rust owner -> actual dedicated PG/Kafka -> two Router processes.

Retained original banks have explicitly relabeled local slots and loader headers.
Every captured real API plan is replayed against the retained original SBF in a
new controlled LiteSVM. This does not qualify a deployed mainnet program/bank.
"""
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
import base64,copy,hashlib,importlib.util,json,secrets,subprocess,tempfile,threading,time
from infrastructure import ROOT,Infrastructure,port
from configuration import config_for,save_config,service_command,sample_for
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import http,quote,wait_scope
from weighted_evidence import source_hash,write_result
OUT=ROOT/'integration/artifacts/local';TARGET=OUT/'core-meteora-mm-normal-process.json'
REFERENCE=ROOT/'integration/test/contracts/meteora-mm.json';ORIGINAL=ROOT/'integration/artifacts/local/core-meteora-dlmm-mm-original-20261110.json'
CLOCK='SysvarC1ock11111111111111111111111111111111'
FAULTS=['program-code','token-code','clock-slot','missing-array','array-binding','limit-order','reward','vault-authority','native-backing']
def require(ok,message):
    if not ok:raise ValueError(message)
def digest(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def helper_hashes():
    paths=['integration/tools/acceptance/infrastructure.py','integration/tools/acceptance/configuration.py','integration/tools/acceptance/kafka_smoke.py','integration/tools/acceptance/reorg_smoke.py','integration/tools/acceptance/smoke.py','integration/tools/acceptance/weighted_evidence.py','integration/tools/acceptance/r01_public_catalog_solana_bank.py','integration/tools/acceptance/r01_public_catalog_solana_rebuild.py','integration/tools/reference/meteora-dlmm/harness/native_preflight.py','integration/tools/reference/meteora-dlmm/harness/idl_layout.py','integration/tools/reference/meteora-dlmm/source-metadata.json','integration/tools/ci/meteora-mm-state.py','integration/test/source/deployment-catalog/solana-rebuild/runtime-lock.json']
    return {name:digest(ROOT/name)for name in paths}
def binaries():
    return {name:digest(ROOT/path)for name,path in [('go_fixture','chain-parser-go/bin/fixture-parser-go'),('rust_parser','chain-parser-rust/target/debug/chain-parser-rust'),('router','dex-router/target/debug/dex-router'),('projector','dex-router/target/debug/examples/meteora-plan-projector')]}
def reusable():
    try:
        r=json.loads(TARGET.read_bytes());current=source_hash()
        if not(r.get('passed')is True and r.get('source_tree_sha256')==r.get('source_tree_sha256_after')==current and r.get('tool_sha256')==digest(__file__)and r.get('helper_sha256')==helper_hashes()and r.get('binaries')==binaries()and r.get('reference_sha256')==digest(REFERENCE)and r.get('original_reference_sha256')==digest(ORIGINAL)):return False
        for key in ['model_projection','captured_plans','original_capture']:
            item=r[key];path=(ROOT/item['path']).resolve()
            if not path.is_relative_to(ROOT)or digest(path)!=item['sha256']:return False
        if not(r.get('api_records')==r.get('original_sbf_swaps')==len(r.get('original_replay',[]))==71 and r.get('negative_controls')==len(FAULTS)):return False
        for key in ['model_projection','captured_plans','original_capture']:
            if len(json.loads((ROOT/r[key]['path']).read_bytes()).get('records',[]))!=71:return False
        return True
    except (OSError,ValueError,KeyError,TypeError):return False
def b58(raw):
    alphabet='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';n=int.from_bytes(raw,'big');s=''
    while n:s=alphabet[n%58]+s;n//=58
    return '1'*(len(raw)-len(raw.lstrip(b'\0')))+s
class Chain:
    def __init__(self,f):self.f=f;self.head=300000000;self.index=3;self.bad=None;self.reads=0;self.blocks=0;self.lock=threading.RLock()
    def advance(self,index=None,bad=None):
        with self.lock:self.index=self.index if index is None else index;self.bad=bad;self.head+=1
    def accounts(self):
        d=self.f['decoder'];out=copy.deepcopy(d['shared_accounts']);out.update(copy.deepcopy(d['cases'][self.index]['accounts']));p=d['config'];cfg=p['meteora']
        def patch(key,fn):
            raw=bytearray(base64.b64decode(out[key]['data'][0]));fn(raw);out[key]['data'][0]=base64.b64encode(raw).decode()
        def flip(raw,offset):raw[offset]^=1
        patch(CLOCK,lambda r:r.__setitem__(slice(0,8),(self.head+(self.bad=='clock-slot')).to_bytes(8,'little')))
        if self.bad=='program-code':patch(cfg['expected_programdata'],lambda r:flip(r,45))
        if self.bad=='token-code':patch(cfg['token_program_upgrade']['programdata'],lambda r:flip(r,45))
        if self.bad=='missing-array':out[p['accounts'][6]]=None
        if self.bad=='array-binding':patch(p['accounts'][6],lambda r:flip(r,24))
        if self.bad=='limit-order':patch(p['accounts'][6],lambda r:flip(r,56+48))
        if self.bad=='reward':patch(p['id'],lambda r:flip(r,264))
        if self.bad=='vault-authority':patch(p['accounts'][1],lambda r:flip(r,32))
        if self.bad=='native-backing':out[p['accounts'][2]]['lamports']=0
        return out
    def block(self,n):return dict(blockhash=b58(hashlib.sha256(f'meteora-mm/{n}'.encode()).digest()),previousBlockhash=b58(hashlib.sha256(f'meteora-mm/{n-1}'.encode()).digest()),parentSlot=n-1,blockTime=1734392448,blockHeight=n)
    def provider(self):
        chain=self
        class Handler(BaseHTTPRequestHandler):
            def log_message(self,*args):pass
            def do_POST(self):
                request=json.loads(self.rfile.read(int(self.headers['Content-Length'])));m=request['method'];params=request['params']
                with chain.lock:
                    if m=='getGenesisHash':result='localnet'
                    elif m=='getSlot':result=chain.head
                    elif m=='getBlocks':
                        require(params[1]-params[0]<4096,'HISTORY_REQUEST_BUDGET');result=list(range(params[0],min(params[1],chain.head)+1))
                    elif m=='getMultipleAccounts':
                        accounts=chain.accounts();require(set(params[0])==set(accounts),'DEPENDENCY_CLOSURE');require(params[1]==dict(encoding='base64',commitment='finalized'),'ACCOUNT_BANK_POLICY');result=dict(context=dict(slot=chain.head),value=[accounts[id]for id in params[0]]);chain.reads+=1
                    elif m=='getBlock':result=chain.block(params[0]);chain.blocks+=1
                    else:raise ValueError('UNEXPECTED_RPC:'+m)
                raw=json.dumps(dict(jsonrpc='2.0',id=request['id'],result=result)).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
        server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start();return server

def run():
    f=json.loads(REFERENCE.read_bytes());original=json.loads(ORIGINAL.read_bytes());chain=Chain(f);servers=[chain.provider(),chain.provider()];processes={};logs={};records=[];source=source_hash();tag=secrets.token_hex(6)
    names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'}
    report=dict(schema_version=1,passed=False,source_tree_sha256=source,authority='CONTROLLED_RETAINED_SBF_MM_BANKS_RELABELED_SLOTS_SYNTHETIC_RPC_ACTUAL_PG_KAFKA',all_requested_features_complete=False,production_ready=False,checks=[],reference_sha256=digest(REFERENCE),original_reference_sha256=digest(ORIGINAL),tool_sha256=digest(__file__),binaries=binaries(),helper_sha256=helper_hashes())
    write_result(TARGET,report)
    try:
        with Infrastructure()as infra,tempfile.TemporaryDirectory(prefix='xone-meteora-mm-')as folder:
            root=Path(folder);token=secrets.token_urlsafe(32);ports={a:port()for a in names};configs={};sources=[f'http://127.0.0.1:{ports[a]}'for a in ['go','rust']]
            for alias,name in names.items():
                c=config_for(name,root/alias,token,ports[alias]);c['history']['enabled']=False;c['journal'].update(transport='kafka',source_urls=sources,poll_interval_ms=150,request_timeout_ms=15000);c['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=alias,transaction_timeout_ms=4000)
                c['control'].update(backend='disabled'if name=='dex-router'else'postgres',dsn_file=''if name=='dex-router'else str(infra.dsnfile),instance_id=alias,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000);c['startup']['max_baseline_age_ms']=315360000000
                if alias=='rust':c['chains'][0].update(genesis_hash='localnet',confirmation_tag='finalized',rpc_urls=[f'http://127.0.0.1:{s.server_port}'for s in servers],pools=[copy.deepcopy(f['decoder']['config'])])
                elif alias=='go':sample_for('evm:31337',root/alias/'fixtures',int(time.time()*1000))
                else:c['routing'].update(max_state_age_ms=315360000000,quote_ttl_ms=60000,max_splits=1,split_parts=4);c['api']['quote_timeout_ms']=30000
                configs[alias]=save_config(root/alias/'config',c)
            def base(a):return f'http://127.0.0.1:{ports[a]}'
            def start(a):
                logs[a]=(OUT/f'meteora-mm-{tag}-{a}.log').open('ab');processes[a]=subprocess.Popen(service_command(names[a],configs[a],root/a/'fixtures'if a=='go'else None),cwd=ROOT/names[a],stdout=logs[a],stderr=subprocess.STDOUT)
            def stop(a):
                p=processes.pop(a,None)
                if p:
                    p.terminate()
                    try:p.wait(timeout=10)
                    except subprocess.TimeoutExpired:p.kill();p.wait()
                if a in logs:logs.pop(a).close()
            def status(a):return http(base(a)+'/internal/chains/solana:localnet/status',token)
            def current(a):return status(a)[1].get('snapshot_ref',{}).get('block',{}).get('number_or_slot')==chain.head
            def check(a,capture=True):
                c=f['cases'][chain.index]['steps'][0];p=c['pool'];zero=c['zero_for_one'];raw=original['cases'][chain.index]
                request=dict(chainKey='solana:localnet',kind='EXACT_IN',sellToken=p['token0'if zero else'token1'],buyToken=p['token1'if zero else'token0'],sellAmount=c['input'],caller=raw['payer'],recipient=raw['recipient'],slippageBps=50,verification='FAST')
                code,q=http(base(a)+'/v1/quote',token,request);require(code==200,'QUOTE_FAILED:'+str((code,q)));require(q['grossBuyAmount']==c['output']and q['consumedIn']==c['input']and q['refund']=='0','QUOTE_AMOUNTS');require(q['verification']['level']=='INDICATIVE'and q['netBuyAmountEstimate']is None,'AUTHORITY_BOUNDARY')
                if capture:
                    code,e=http(base(a)+'/internal/quotes/'+q['quoteId']+'/evidence',token);require(code==200,'PLAN_EVIDENCE');snapshot=http(base('rust')+'/internal/chains/solana:localnet/baseline',token)[1];require(snapshot['reference']==q['snapshotRef'],'PLAN_SOURCE_BANK');records.append(dict(case_index=chain.index,snapshot=snapshot,request=request,quote=q,execution_plan=e['execution_plan']))
                return q
            try:
                for a in ['go','rust']:start(a);wait(base(a)+'/health/ready')
                for a in ['router1','router2']:start(a);wait(base(a)+'/health/live');wait_scope(base(a),token,'solana:localnet')
                for index,c in enumerate(f['cases']):
                    if not c['steps'][0]['passed']:continue
                    chain.advance(index)
                    for a in ['router1','router2']:await_condition(lambda a=a:current(a),'Meteora scope not published');check(a)
                report['checks'].append('35-original-bank-amounts-in-both-router-processes')
                chain.advance(3)
                for a in ['router1','router2']:await_condition(lambda a=a:current(a),'Meteora scope not restored')
                for fault in FAULTS:
                    old={a:check(a,False)for a in ['router1','router2']};chain.advance(bad=fault)
                    for a in old:await_condition(lambda a=a:status(a)[0]==503,'Meteora fault not revoked:'+fault);quote(base(a),token,'evm:31337','B')
                    chain.advance()
                    for a in old:await_condition(lambda a=a:current(a),'Meteora fault not recovered:'+fault);check(a,False)
                    report['checks'].append('fault-withdraw-and-recovery:'+fault)
                stop('router1');start('router1');wait(base('router1')+'/health/live');wait_scope(base('router1'),token,'solana:localnet');check('router1');report['checks'].append('router-checkpoint-tail-restart')
            finally:
                for a in list(processes):stop(a)
    finally:
        for s in servers:s.shutdown();s.server_close()
    input_path=OUT/f'meteora-mm-plans-{tag}.json';output_path=OUT/f'meteora-mm-projected-{tag}.json';input_path.write_text(json.dumps(dict(schema_version=1,records=records))+'\n')
    subprocess.run([str(ROOT/'dex-router/target/debug/examples/meteora-plan-projector'),str(input_path),str(output_path)],check=True)
    projected=json.loads(output_path.read_bytes());require(projected['passed'],'PROJECTOR_FAILED')
    import sys
    sys.path.insert(0,str(ROOT/'integration/tools/reference/meteora-dlmm/harness'));from native_preflight import execute
    spec=importlib.util.spec_from_file_location('meteora_projection',ROOT/'integration/tools/ci/meteora-mm-state.py');projection=importlib.util.module_from_spec(spec);spec.loader.exec_module(projection)
    replay=[];native_records=[]
    for record in projected['records']:
        require(len(record['steps'])==1,'SINGLE_ORIGINAL_TEMPLATE');step=record['steps'][0];c=original['cases'][record['case_index']]
        native=execute(zero=step['zero_for_one'],amount=int(step['amount_in']),minimum=1,mm_only=True,protocol_share=c['protocol_share'],elapsed=c['elapsed'],dynamic_control=c['variable_fee_control'])
        require(native['passed']and native['payer_input_delta']==step['amount_in']and native['recipient_output_delta']==step['amount_out'],'ORIGINAL_PLAN_TRANSFERS')
        expected=projection.pool(native['after'],native['clock'],native['program_sha256'])['meteora'];actual=step['after']['meteora']
        for key in ['pair_data','oracle_data','bin_arrays','raw_vault0','raw_vault1']:require(actual[key]==expected[key],'ORIGINAL_FULL_LEDGER:'+key)
        native_records.append(dict(case_index=record['case_index'],plan_hash=record['plan_hash'],native=native))
        replay.append(dict(case_index=record['case_index'],plan_hash=record['plan_hash'],amount_in=native['payer_input_delta'],amount_out=native['recipient_output_delta'],native_sha256=hashlib.sha256(json.dumps(native,sort_keys=True).encode()).hexdigest()))
    native_path=OUT/f'meteora-mm-original-plan-capture-{tag}.json';native_path.write_text(json.dumps(dict(schema_version=1,records=native_records),indent=2)+'\n')
    require(source_hash()==source,'SOURCE_CHANGED_DURING_PROCESS_TEST')
    report.update(passed=True,api_records=len(records),original_sbf_swaps=len(replay),original_replay=replay,original_capture=dict(path=str(native_path.relative_to(ROOT)),sha256=digest(native_path)),negative_controls=len(FAULTS),atomic_account_batches=chain.reads,block_reads=chain.blocks,model_projection=dict(path=str(output_path.relative_to(ROOT)),sha256=digest(output_path)),captured_plans=dict(path=str(input_path.relative_to(ROOT)),sha256=digest(input_path)),source_tree_sha256_after=source_hash(),measured_at_unix_ms=int(time.time()*1000));write_result(TARGET,report);return report
if __name__=='__main__':
    try:
        import sys
        if '--reuse-if-current'in sys.argv and reusable():print(json.dumps(dict(passed=True,reused_current_source_binary_input_tool_receipt=True,path=str(TARGET.relative_to(ROOT)),sha256=digest(TARGET))))
        else:
            r=run();print(json.dumps({k:r[k]for k in ['passed','api_records','original_sbf_swaps','negative_controls','source_tree_sha256']}))
    except Exception as e:
        current=json.loads(TARGET.read_bytes())if TARGET.exists()else{};current.update(passed=False,error=str(e),error_type=type(e).__name__);write_result(TARGET,current);raise
