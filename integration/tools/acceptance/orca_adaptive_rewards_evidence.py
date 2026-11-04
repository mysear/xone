#!/usr/bin/env python3
"""Current-source acceptance of the classic SPL/fixed-array adaptive rewards scope."""
from pathlib import Path
import hashlib,importlib.util,json,secrets,subprocess,sys,time
ROOT=Path(__file__).resolve().parents[3]
OUT=ROOT/'integration/artifacts/local'
TARGET=OUT/'orca-adaptive-rewards-acceptance.json'
SCOPE='cc683ec9-classic-spl-three-fixed-adaptive-initialized-rewards'
TRUST='SYNTHETIC_RPC_ORIGINAL_PACK_NOT_SBF_TRANSACTION'
BUDGET={'timeout_ms':10000,'max_hops':1,'max_candidates':8,'max_expansions':64,'split_parts':4,'max_splits':2}
def require(ok,message):
    if not ok:raise ValueError(message)
def initial():return dict(passed=False,state='RUNNING',scope=SCOPE,verified=False,production_ready=False,all_requested_features_complete=False)
def write(value):
    OUT.mkdir(parents=True,exist_ok=True)
    temp=TARGET.with_name(TARGET.name+'.'+secrets.token_hex(6)+'.tmp')
    try:temp.write_text(json.dumps(value,indent=2)+'\n');temp.replace(TARGET)
    finally:temp.unlink(missing_ok=True)
if __name__=='__main__':write(initial())
from weighted_evidence import checked_runtime,runtime_fingerprint,snapshot_inputs,sha
from orca_adaptive_rewards_smoke import CHECKS,FAULTS
from orca_adaptive_rewards_fixture import corpus,pool,clean_config

def input_digest(inputs):return hashlib.sha256(json.dumps(inputs,sort_keys=True,separators=(',',':')).encode()).hexdigest()
def validate_process(p,runtime,inputs,now_ms):
    require(p.get('passed')is True and p.get('state')=='PASSED','adaptive rewards process failed')
    require(p.get('source_binding')==runtime,'adaptive rewards source/binaries changed')
    require(bool(inputs)and p.get('input_hashes')==inputs and p.get('inputs_unchanged')is True,'adaptive rewards execution inputs changed')
    start,end=p.get('started_at_unix_ms'),p.get('completed_at_unix_ms')
    require(type(start)is int and type(end)is int and 0<start<=end<=now_ms and now_ms-end<=3_600_000,'adaptive rewards evidence time invalid')
    require(p.get('checks')==CHECKS,'adaptive rewards source/fault inventory incomplete')
    require(tuple(p.get(k)for k in ['quote_checks','successful_quotes','native_rejections','pool_count','phase_count'])==(20,16,4,2,5),'adaptive rewards bank/quote inventory differs')
    require(p.get('test_quote_budget')==BUDGET,'adaptive rewards local replay budget changed')
    require(all(p.get(k,0)>0 for k in ['account_batches','block_reads'])and p.get('native_plan_steps',0)>=16,'adaptive rewards source/native reads missing')
    require(all(p.get(k)is False for k in ['verified','production_ready','all_requested_features_complete'])and p.get('trust')==TRUST,'adaptive rewards proof scope overstated')
def validate_review(v,runtime,inputs):
    require(bool(inputs)and v.get('input_sha256')==input_digest(inputs)and v.get('input_files')==len(inputs),'adaptive rewards review input binding missing/stale')
    require(v.get('source_tree_sha256')==runtime['source_tree_sha256']and v.get('scope')==SCOPE,'adaptive rewards review source/scope stale')
    reviews=v.get('reviews',{})
    require(set(reviews)=={'spec','quality'},'two adaptive rewards reviewers required')
    for key,r in reviews.items():require(r.get('passed')is True and r.get('agent_id')and r.get('open_p1_p2')==0,'unresolved review '+key)
    require(reviews['spec']['agent_id']!=reviews['quality']['agent_id'],'independent adaptive rewards reviewers required')
def check_projection():
    original=corpus();projection=json.loads((ROOT/'integration/test/contracts/orca-adaptive-rewards-pools.json').read_text())
    require(projection['source_corpus_sha256']==sha(ROOT/'integration/test/contracts/orca-adaptive-rewards-service.json'),'adaptive rewards source digest stale')
    names=[f'pool-{i}-{d}-continuous'for i in range(2)for d in ['a-to-b','b-to-a']]
    require([s['name']for s in original['scenarios']]==names,'adaptive rewards original scenarios missing')
    cases=projection['cases'];require([c['name']for c in cases]==[name+'/'+str(i)for name in names for i in range(5)],'adaptive rewards projected exact members differ')
    success=rejected=0
    for scenario in original['scenarios']:
        require(len(scenario['banks'])==5,'adaptive rewards original bank count')
        for index,bank in enumerate(scenario['banks']):
            c=cases[names.index(scenario['name'])*5+index];expected=bank['next_trade']['expected']
            require(c['config']==clean_config(scenario['config'])and c['pool']==pool(scenario['config'],bank['native_state'])and c['params']==bank['next_trade']['params'],'adaptive rewards projected complete source differs')
            require(c['expected']['status']==expected['status'],'adaptive rewards status differs')
            if index<4:
                require(expected['status']=='fulfilled'and c['expected']=={'status':'fulfilled','amount_in':expected['amount_in'],'amount_out':expected['amount_out'],'amount_remaining':'0','post_pool':pool(scenario['config'],expected['next_state'])},'adaptive rewards original complete poststate differs');success+=1
            else:
                require(expected['status']=='native_rejected'and c['expected']=={'status':'native_rejected','error':expected['error']}and expected['post_accounts']==bank['accounts'],'adaptive rewards native failure changed');rejected+=1
    require((success,rejected)==(16,4)and original['identity'].get('sbf')is False,'adaptive rewards native authority/count changed')
def run():
    verification,runtime=checked_runtime();inputs=snapshot_inputs();check_projection();result=initial();result.update(source_tree_sha256=runtime['source_tree_sha256'],source_binding=runtime,commands=[])
    for name,command in [('native',[sys.executable,ROOT/'integration/tools/ci/orca-adaptive-rewards-service-check.py']),('optimized-native',[sys.executable,'-OO',ROOT/'integration/tools/ci/orca-adaptive-rewards-service-check.py']),('native-guards',[sys.executable,ROOT/'integration/tools/ci/orca-adaptive-rewards-service-guards.py'])]:
        log=OUT/('orca-adaptive-rewards-'+name+'-acceptance.log')
        with log.open('w')as stream:done=subprocess.run(list(map(str,command)),cwd=ROOT,stdout=stream,stderr=subprocess.STDOUT)
        require(done.returncode==0,'adaptive rewards native reference/guards failed: '+name)
        result['commands'].append(dict(name=name,exit_code=done.returncode,log=str(log.relative_to(ROOT)),log_sha256=sha(log)))
    process=OUT/'orca-adaptive-rewards-three-service-smoke.json';review=OUT/'orca-adaptive-rewards-review.json'
    validate_process(json.loads(process.read_text()),runtime,inputs,int(time.time()*1000));validate_review(json.loads(review.read_text()),runtime,inputs)
    require(runtime==runtime_fingerprint()and inputs==snapshot_inputs(),'adaptive rewards inputs changed during acceptance')
    result.update(passed=True,state='PASSED',source_unchanged=True,input_sha256=input_digest(inputs),input_files=len(inputs),verification_sha256=sha(OUT/'verification.json'),process_sha256=sha(process),review_sha256=sha(review),measured_at_unix_ms=int(time.time()*1000),native_banks=20,native_success=16,native_rejections=4,service_requests=20,source_faults=len(FAULTS),remaining=['Adaptive dynamic arrays/Token2022/collection','Other Solana advanced protocol combinations','V4/Infinity hooks/full settlement','Real deployment/history, independent SBF/CPI/full transaction and production acceptance'])
    return result

def main():
    write(initial())
    try:result=run();write(result);print(json.dumps({'passed':True,'scope':SCOPE,'report':str(TARGET)}))
    except BaseException as error:
        result=initial();result.update(state='FAILED',error=str(error),error_type=type(error).__name__);write(result);raise
if __name__=='__main__':main()
