#!/usr/bin/env python3
"""OnlyY normal RPC/PG/Kafka/dual-Router plans, direct original SBF replay."""
from pathlib import Path
import base64,copy,hashlib,importlib.util,json,secrets,subprocess,tempfile,threading,time
from meteora_mm_smoke import Chain,OUT,ROOT,FAULTS,digest,helper_hashes as shared_helper_hashes
from infrastructure import Infrastructure,port
from configuration import config_for,save_config,service_command,sample_for
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import http,quote,wait_scope
from weighted_evidence import source_hash,write_result
spec=importlib.util.spec_from_file_location('only_y_original',ROOT/'integration/tools/ci/meteora-only-y-reference.py');generator=importlib.util.module_from_spec(spec);spec.loader.exec_module(generator)
ORIGINAL=generator.TARGET;REFERENCE=ROOT/'integration/test/contracts/meteora-only-y.json';PROFILE=generator.PROFILE
TARGET=OUT/'core-meteora-only-y-normal-process.json';CLOCK='SysvarC1ock11111111111111111111111111111111'
SELECTED=[14,34,54,74,94,114,134,154,174,194,202,218,228,230,232,234]
golden=json.loads(REFERENCE.read_bytes())
def require(ok,why):
    if not ok:raise ValueError(why)
def canonical_hash(value):
    raw=json.dumps(value,sort_keys=True,separators=(',',':'),ensure_ascii=False).replace('<','\\u003c').replace('>','\\u003e').replace('&','\\u0026');return hashlib.sha256(raw.encode()).hexdigest()
def binaries():return {p:digest(ROOT/p)for p in ['chain-parser-go/bin/fixture-parser-go','chain-parser-rust/target/debug/chain-parser-rust','dex-router/target/debug/dex-router']}
def helper_hashes():return shared_helper_hashes() | {str(p.relative_to(ROOT)):digest(p)for p in [Path(__file__),Path(generator.__file__),ROOT/'integration/tools/acceptance/meteora_mm_smoke.py']}
def runtime_input(evidence):
    mm=json.loads((ROOT/'integration/test/contracts/meteora-mm.json').read_bytes());cfg=copy.deepcopy(mm['decoder']['config']);cfg['protocol']=PROFILE;ids=set(cfg['accounts']);cases=[];banks=[]
    for index,e in enumerate(evidence['records']):
        n=e['native'];clock=copy.deepcopy(mm['decoder']['cases'][0]['accounts'][CLOCK]);banks.append(dict(accounts={CLOCK:clock,**{k:dict(owner=a['owner'],executable=a['executable'],lamports=a['lamports'],data=[a['data_base64'],'base64'])for k,a in n['before'].items()if k in ids}}))
        step=copy.deepcopy(golden['cases'][index]['steps'][0]);step['input']=step['specified_amount'];step['output']=step['actual_output'];cases.append(dict(steps=[step]))
    return dict(decoder=dict(config=cfg,shared_accounts=mm['decoder']['shared_accounts'],cases=banks),cases=cases)

def run():
    original=json.loads(ORIGINAL.read_bytes());f=runtime_input(original);chain=Chain(f);servers=[chain.provider(),chain.provider()];processes={};logs={};records=[];source=source_hash();tag=secrets.token_hex(6)
    names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'}
    report=dict(schema_version=1,passed=False,source_tree_sha256=source,authority='CONTROLLED_RETAINED_SBF_MM_BANKS_RELABELED_SLOTS_SYNTHETIC_RPC_ACTUAL_PG_KAFKA',all_requested_features_complete=False,production_ready=False,checks=[],reference_sha256=digest(REFERENCE),original_reference_sha256=digest(ORIGINAL),tool_sha256=digest(__file__),binaries=binaries(),helper_sha256=helper_hashes())
    write_result(TARGET,report)
    try:
        with Infrastructure()as infra,tempfile.TemporaryDirectory(prefix='xone-meteora-only-y-')as folder:
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
                logs[a]=(OUT/f'meteora-only-y-{tag}-{a}.log').open('ab');processes[a]=subprocess.Popen(service_command(names[a],configs[a],root/a/'fixtures'if a=='go'else None),cwd=ROOT/names[a],stdout=logs[a],stderr=subprocess.STDOUT)
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
                c=f['cases'][chain.index]['steps'][0];p=c['pool'];zero=c['zero_for_one'];raw=original['records'][chain.index]['native']
                request=dict(chainKey='solana:localnet',kind='EXACT_IN',sellToken=p['token0'if zero else'token1'],buyToken=p['token1'if zero else'token0'],sellAmount=c['input'],caller=raw['payer'],recipient=raw['recipient'],slippageBps=50,verification='FAST')
                code,q=http(base(a)+'/v1/quote',token,request);require(code==200,'QUOTE_FAILED:'+str((code,q)));require(q['grossBuyAmount']==c['output']and q['consumedIn']==c['input']and q['refund']=='0','QUOTE_AMOUNTS');require(q['verification']['level']=='INDICATIVE'and q['netBuyAmountEstimate']is None,'AUTHORITY_BOUNDARY')
                if capture:
                    code,e=http(base(a)+'/internal/quotes/'+q['quoteId']+'/evidence',token);require(code==200,'PLAN_EVIDENCE');snapshot=http(base('rust')+'/internal/chains/solana:localnet/baseline',token)[1];require(snapshot['reference']==q['snapshotRef'],'PLAN_SOURCE_BANK');records.append(dict(case_index=chain.index,snapshot=snapshot,request=request,quote=q,execution_plan=e['execution_plan']))
                return q
            try:
                for a in ['go','rust']:start(a);wait(base(a)+'/health/ready')
                for a in ['router1','router2']:start(a);wait(base(a)+'/health/live');wait_scope(base(a),token,'solana:localnet')
                for index in SELECTED:
                    c=f['cases'][index]
                    require(c['steps'][0]['passed']and not c['steps'][0]['exact_out'],'ONLY_Y_CASE_SELECTION')
                    chain.advance(index)
                    for a in ['router1','router2']:await_condition(lambda a=a:current(a),'Meteora scope not published');check(a)
                report['checks'].append('16-original-only-y-banks-in-both-router-processes')
                chain.advance(14)
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
    input_path=OUT/f'meteora-only-y-plans-{tag}.json';output_path=OUT/f'meteora-only-y-projected-{tag}.json';input_path.write_text(json.dumps(dict(schema_version=1,records=records))+'\n')
    projected=dict(schema_version=1,authority='captured public API plans and original-bank direct replay; complete model state separately checked by same-source Rust differential tests',records=records)
    output_path.write_text(json.dumps(projected)+'\n')
    replay=[];native_records=[]
    for record in records:
        index=record['case_index'];case=original['records'][index];plan=record['execution_plan'];req=record['request'];q=record['quote'];snapshot=record['snapshot'];routes=plan['allocation']['routes']
        require(plan['snapshot']==snapshot['reference']and len(routes)==1,'ONLY_Y_PLAN_SCOPE')
        require(plan['context']['caller']==req['caller']and plan['context']['recipient']==req['recipient']and plan['context']['sell_token']==req['sellToken']and plan['context']['buy_token']==req['buyToken'],'ONLY_Y_PLAN_CONTEXT')
        require(plan['plan_hash']==q['planHash']and canonical_hash(dict(context=plan['context'],allocation=plan['allocation'],snapshot=plan['snapshot'],min_buy_amount=plan['min_buy_amount'],economic_policy_hash=plan['economic_policy_hash']))==plan['plan_hash'],'ONLY_Y_PLAN_HASH')
        route=routes[0];require(len(route['path']['edges'])==1 and route['path']['edges'][0]['protocol']==PROFILE and route['amount_in']==req['sellAmount']and route['amount_out']==q['grossBuyAmount'],'ONLY_Y_PUBLIC_AMOUNTS')
        native=generator.execute(case['mode'],case['native']['zero_for_one'],int(route['amount_in']),False)['native']
        require(native['passed']and native['payer_input_delta']==route['amount_in']and native['recipient_output_delta']==route['amount_out'],'ONLY_Y_ACTUAL_FUNDS')
        before=generator.pool(native['before'],native['clock'],native['program_sha256'])['meteora'];actual=snapshot['pools'][0]['meteora'];after=generator.pool(native['after'],native['clock'],native['program_sha256'])['meteora'];expected=golden['cases'][index]['steps'][0]['after']['meteora']
        for key in ['pair_data','oracle_data','bin_arrays','raw_vault0','raw_vault1']:
            require(before[key]==actual[key],'ONLY_Y_SOURCE_BANK:'+key);require(after[key]==expected[key],'ONLY_Y_ORIGINAL_COMPLETE_AFTER:'+key)
        native_records.append(dict(case_index=index,plan_hash=plan['plan_hash'],native=native));replay.append(dict(case_index=index,plan_hash=plan['plan_hash'],amount_in=native['payer_input_delta'],amount_out=native['recipient_output_delta']))
    native_path=OUT/f'meteora-only-y-original-plan-capture-{tag}.json';native_path.write_text(json.dumps(dict(schema_version=1,records=native_records),indent=2)+'\n')
    require(source_hash()==source,'SOURCE_CHANGED_DURING_PROCESS_TEST')
    report.update(passed=True,normal_exact_out_exercised=False,api_records=len(records),original_sbf_swaps=len(replay),original_replay=replay,original_capture=dict(path=str(native_path.relative_to(ROOT)),sha256=digest(native_path)),negative_controls=len(FAULTS),atomic_account_batches=chain.reads,block_reads=chain.blocks,model_projection=dict(path=str(output_path.relative_to(ROOT)),sha256=digest(output_path)),captured_plans=dict(path=str(input_path.relative_to(ROOT)),sha256=digest(input_path)),source_tree_sha256_after=source_hash(),measured_at_unix_ms=int(time.time()*1000));write_result(TARGET,report);return report

if __name__=='__main__':
    try:
        r=run();print(json.dumps({k:r[k]for k in ['passed','api_records','original_sbf_swaps','negative_controls','source_tree_sha256']}))
    except Exception as e:
        r=json.loads(TARGET.read_bytes())if TARGET.exists()else{};r.update(passed=False,error=str(e),error_type=type(e).__name__);write_result(TARGET,r);raise
