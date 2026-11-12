#!/usr/bin/env python3
"""Normal Pump RPC/PG/Kafka and dual Router indicative plans replayed in original SBF."""
from pathlib import Path
import base64,copy,hashlib,importlib.util,json,secrets,subprocess,tempfile,threading,time,sys
from meteora_mm_smoke import Chain,OUT,ROOT,digest,helper_hashes as shared_helper_hashes
from infrastructure import Infrastructure,port
from configuration import config_for,save_config,service_command,sample_for
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import http,quote,wait_scope
from weighted_evidence import source_hash,write_result
sys.path.insert(0,str(ROOT/'integration/tools/reference/pump-bonding/harness'))
from native import execute
spec=importlib.util.spec_from_file_location('pump_projection',ROOT/'integration/tools/ci/pump-bonding-reference.py');projection=importlib.util.module_from_spec(spec);spec.loader.exec_module(projection);project_pool=projection.pool
REFERENCE=ROOT/'integration/test/contracts/pump-bonding-decoder.json';ORIGINAL=ROOT/'integration/artifacts/local/core-pump-bonding-original-20261111.json'
reference=json.loads((ROOT/'integration/test/contracts/pump-bonding.json').read_bytes())
TARGET=OUT/'core-pump-bonding-normal-process.json';CLOCK='SysvarC1ock11111111111111111111111111111111'
FAULTS=['program-code','fee-code','clock-slot','cashback','vault-balance','native-backing','creator-identity']
def require(ok,message):
    if not ok:raise ValueError(message)
def binaries():
    return {p:digest(ROOT/p)for p in ['chain-parser-go/bin/fixture-parser-go','chain-parser-rust/target/debug/chain-parser-rust','dex-router/target/debug/dex-router','dex-router/target/debug/examples/pump-plan-projector']}
def helper_hashes():return shared_helper_hashes() | {str(p.relative_to(ROOT)):digest(p)for p in [Path(__file__),ROOT/'integration/tools/acceptance/meteora_mm_smoke.py',ROOT/'integration/tools/ci/pump-bonding-reference.py',ROOT/'integration/tools/reference/pump-bonding/harness/native.py',ROOT/'integration/tools/reference/pump-bonding/harness/layout.py',ROOT/'integration/tools/reference/pump-bonding/source-pins.json',ROOT/'integration/tools/reference/pump-bonding/public-bank-pins.json']}
class PumpChain(Chain):
    def __init__(self,f):super().__init__(f);self.head=467957307;self.index=1
    def block(self,n):
        b=super().block(n);b['blockTime']=1794355000;return b
    def accounts(self):
        values=copy.deepcopy(self.f['shared_accounts']);values.update(copy.deepcopy(self.f['cases'][self.index]['accounts']));cfg=self.f['config'];p=self.f['cases'][self.index]['pool'];s=p['pump']
        def patch(key,offset):
            raw=bytearray(base64.b64decode(values[key]['data'][0]));raw[offset]^=1;values[key]['data'][0]=base64.b64encode(raw).decode()
        raw=bytearray(base64.b64decode(values[CLOCK]['data'][0]));raw[:8]=(self.head+int(self.bad=='clock-slot')).to_bytes(8,'little');values[CLOCK]['data'][0]=base64.b64encode(raw).decode()
        if self.bad=='program-code':patch(s['programdata'],45)
        if self.bad=='fee-code':patch(s['fee_programdata'],45)
        if self.bad=='cashback':patch(p['pool_id'],82)
        if self.bad=='vault-balance':patch(s['vault'],64)
        if self.bad=='native-backing':values[p['pool_id']]['lamports']+=1
        if self.bad=='creator-identity':patch(p['pool_id'],49)
        return values

def run():
    f=json.loads(REFERENCE.read_bytes());original=json.loads(ORIGINAL.read_bytes());chain=PumpChain(f);servers=[chain.provider(),chain.provider()];processes={};logs={};records=[];source=source_hash();tag=secrets.token_hex(6)
    names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'}
    report=dict(schema_version=1,passed=False,source_tree_sha256=source,authority='CONTROLLED_PUMP_CLASSIC_SOL_BANKS_RELABELED_SLOTS_SYNTHETIC_RPC_ACTUAL_PG_KAFKA',all_requested_features_complete=False,production_ready=False,checks=[],reference_sha256=digest(REFERENCE),original_reference_sha256=digest(ORIGINAL),tool_sha256=digest(__file__),binaries=binaries(),helper_sha256=helper_hashes())
    write_result(TARGET,report)
    try:
        with Infrastructure()as infra,tempfile.TemporaryDirectory(prefix='xone-pump-bonding-')as folder:
            root=Path(folder);token=secrets.token_urlsafe(32);ports={a:port()for a in names};configs={};sources=[f'http://127.0.0.1:{ports[a]}'for a in ['go','rust']]
            for alias,name in names.items():
                c=config_for(name,root/alias,token,ports[alias]);c['history']['enabled']=False;c['journal'].update(transport='kafka',source_urls=sources,poll_interval_ms=150,request_timeout_ms=15000);c['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=alias,transaction_timeout_ms=4000)
                c['control'].update(backend='disabled'if name=='dex-router'else'postgres',dsn_file=''if name=='dex-router'else str(infra.dsnfile),instance_id=alias,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000);c['startup']['max_baseline_age_ms']=315360000000;c['journal']['max_payload_bytes']=33554432
                if alias=='rust':c['chains'][0].update(genesis_hash='localnet',confirmation_tag='finalized',rpc_urls=[f'http://127.0.0.1:{s.server_port}'for s in servers],pools=[copy.deepcopy(f['config'])])
                elif alias=='go':sample_for('evm:31337',root/alias/'fixtures',int(time.time()*1000))
                else:c['routing'].update(max_state_age_ms=315360000000,quote_ttl_ms=60000,max_splits=1,split_parts=4);c['api']['quote_timeout_ms']=30000
                configs[alias]=save_config(root/alias/'config',c)
            def base(a):return f'http://127.0.0.1:{ports[a]}'
            def start(a):
                logs[a]=(OUT/f'pump-bonding-{tag}-{a}.log').open('ab');processes[a]=subprocess.Popen(service_command(names[a],configs[a],root/a/'fixtures'if a=='go'else None),cwd=ROOT/names[a],stdout=logs[a],stderr=subprocess.STDOUT)
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
                row=f['cases'][chain.index];c=reference['cases'][row['index']]['steps'][0];p=c['pool'];zero=c['kind']=='sell';raw=original['records'][row['index']]
                request=dict(chainKey='solana:localnet',kind='EXACT_IN',sellToken=p['token0'if zero else'token1'],buyToken=p['token1'if zero else'token0'],sellAmount=c['amount'],caller=p['pump']['user'],recipient=p['pump']['user'],slippageBps=50,verification='FAST')
                code,q=http(base(a)+'/v1/quote',token,request);require(code==200,'QUOTE_FAILED:'+str((code,q)));expected=str(c['user_sol_delta']if zero else c['user_token_delta']);require(q['grossBuyAmount']==expected and q['consumedIn']==c['amount']and q['refund']=='0','QUOTE_AMOUNTS');require(q['verification']['level']=='INDICATIVE'and q['netBuyAmountEstimate']is None,'AUTHORITY_BOUNDARY')
                if capture:
                    code,e=http(base(a)+'/internal/quotes/'+q['quoteId']+'/evidence',token);require(code==200,'PLAN_EVIDENCE');snapshot=http(base('rust')+'/internal/chains/solana:localnet/baseline',token)[1];require(snapshot['reference']==q['snapshotRef'],'PLAN_SOURCE_BANK');records.append(dict(case_index=row['index'],snapshot=snapshot,request=request,quote=q,execution_plan=e['execution_plan']))
                return q
            try:
                for a in ['go','rust']:start(a);wait(base(a)+'/health/ready')
                for a in ['router1','router2']:start(a);wait(base(a)+'/health/live');wait_scope(base(a),token,'solana:localnet')
                for index in [0,1]:
                    chain.advance(index)
                    for a in ['router1','router2']:await_condition(lambda a=a:current(a),'Pump bank not published');check(a)
                report['checks'].append('normal-legacy-sell-and-fixed-sol-buy-both-replicas')
                chain.advance(2)
                for a in ['router1','router2']:
                    await_condition(lambda a=a:status(a)[0]==503,'completed Pump curve remained quoteable')
                    quote(base(a),token,'evm:31337','B')
                code,completed=http(base('rust')+'/internal/chains/solana:localnet/baseline',token)
                require(code==200 and completed['pools'][0]['reserve0']=='0','completed curve missing from canonical source')
                chain.advance(1)
                for a in ['router1','router2']:await_condition(lambda a=a:current(a),'Pump recovery after completed bank failed');check(a,False)
                report['checks'].append('completed-curve-source-retained-graph-withdrawn-and-qualified-scope-restored')

                chain.advance(1)
                for a in ['router1','router2']:await_condition(lambda a=a:current(a),'Pump bank not restored')
                for fault in FAULTS:
                    old={a:check(a,False)for a in ['router1','router2']};chain.advance(bad=fault)
                    for a in old:await_condition(lambda a=a:status(a)[0]==503,'Pump fault not revoked:'+fault);quote(base(a),token,'evm:31337','B')
                    chain.advance()
                    for a in old:await_condition(lambda a=a:current(a),'Pump fault not recovered:'+fault);check(a,False)
                    report['checks'].append('fault-withdraw-and-recovery:'+fault)
                stop('router1');start('router1');wait(base('router1')+'/health/live');wait_scope(base('router1'),token,'solana:localnet');check('router1');report['checks'].append('router-checkpoint-tail-restart')
            finally:
                for a in list(processes):stop(a)
    finally:
        for s in servers:s.shutdown();s.server_close()
    input_path=OUT/f'pump-bonding-plans-{tag}.json';output_path=OUT/f'pump-bonding-projected-{tag}.json';input_path.write_text(json.dumps(dict(schema_version=1,records=records))+'\n')
    subprocess.run([str(ROOT/'dex-router/target/debug/examples/pump-plan-projector'),str(input_path),str(output_path)],check=True)
    projected=json.loads(output_path.read_bytes());require(projected['passed'],'PROJECTOR_FAILED')
    replay=[];native_records=[]
    for record in projected['records']:
        require(len(record['steps'])==1,'SINGLE_ORIGINAL_TEMPLATE');step=record['steps'][0];c=original['records'][record['case_index']]
        native=execute(kind=c['kind'],amount=int(step['amount_in']),limit=1)
        zero=c['kind']=='sell';require(native['passed'],'PUMP_ORIGINAL_REPLAY_FAILED')
        output=native['user_sol_delta']if zero else native['user_token_delta'];used=-native['user_token_delta']if zero else -native['user_sol_delta']
        require(str(used)==step['amount_in']and str(output)==step['amount_out'],'PUMP_ACTUAL_FUNDS')
        for name in ['before','after']:
            expected=project_pool(native[name],native['ids'])['pump']['accounts'];require(expected==step[name]['pump']['accounts'],'PUMP_COMPLETE_ACCOUNT_AFTER_STATE')
        native_records.append(dict(case_index=record['case_index'],plan_hash=record['plan_hash'],native=native));replay.append(dict(case_index=record['case_index'],plan_hash=record['plan_hash'],amount_in=str(used),amount_out=str(output)))
    native_path=OUT/f'pump-bonding-original-plan-capture-{tag}.json';native_path.write_text(json.dumps(dict(schema_version=1,records=native_records),indent=2)+'\n')
    require(source_hash()==source,'SOURCE_CHANGED_DURING_PROCESS_TEST')
    report.update(passed=True,api_records=len(records),original_sbf_swaps=len(replay),original_replay=replay,original_capture=dict(path=str(native_path.relative_to(ROOT)),sha256=digest(native_path)),negative_controls=len(FAULTS),atomic_account_batches=chain.reads,block_reads=chain.blocks,model_projection=dict(path=str(output_path.relative_to(ROOT)),sha256=digest(output_path)),captured_plans=dict(path=str(input_path.relative_to(ROOT)),sha256=digest(input_path)),source_tree_sha256_after=source_hash(),measured_at_unix_ms=int(time.time()*1000));write_result(TARGET,report);return report

if __name__=='__main__':
    try:
        r=run();print(json.dumps({k:r[k]for k in ['passed','api_records','original_sbf_swaps','negative_controls','source_tree_sha256']}))
    except Exception as e:
        r=json.loads(TARGET.read_bytes())if TARGET.exists()else{};r.update(passed=False,error=str(e),error_type=type(e).__name__);write_result(TARGET,r);raise
