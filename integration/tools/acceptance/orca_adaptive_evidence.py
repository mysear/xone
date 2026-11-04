#!/usr/bin/env python3
"""Current-source bounded adaptive service acceptance; fail closed on any gap."""
from pathlib import Path
import hashlib,importlib.util,json,secrets,subprocess,sys,time
ROOT=Path(__file__).resolve().parents[3];OUT=ROOT/'integration/artifacts/local';TARGET=OUT/'orca-adaptive-acceptance.json'
def require(ok,message):
 if not ok:raise ValueError(message)
def initial():return {'passed':False,'state':'RUNNING','scope':'cc683ec9-classic-spl-three-fixed-arrays-uninitialized-rewards-adaptive-service','verified':False,'production_ready':False,'all_requested_features_complete':False}
def write(value):
 OUT.mkdir(parents=True,exist_ok=True);tmp=TARGET.with_name(TARGET.name+'.'+secrets.token_hex(6)+'.tmp')
 try:tmp.write_text(json.dumps(value,indent=2)+'\n');tmp.replace(TARGET)
 finally:tmp.unlink(missing_ok=True)
if __name__=='__main__':write(initial())
from weighted_evidence import checked_runtime,runtime_fingerprint,snapshot_inputs,sha
from orca_adaptive_smoke import CHECKS,FAULTS
from orca_adaptive_fixture import corpus,pool,clean_config

def validate_process(p,runtime):
 require(p.get('passed')is True and p.get('state')=='PASSED','adaptive process did not pass')
 require(p.get('source_binding')==runtime,'adaptive process source or binary differs')
 require(p.get('checks')==CHECKS,'adaptive process read/recovery coverage differs')
 require((p.get('quote_checks'),p.get('successful_quotes'),p.get('native_rejections'),p.get('pool_count'),p.get('phase_count'))==(20,16,4,2,5),'adaptive bank/quote inventory differs')
 require(p.get('test_quote_budget')=={'timeout_ms':10000,'max_hops':1,'max_candidates':8,'max_expansions':64,'split_parts':4,'max_splits':2},'adaptive replay budget changed')
 require(p.get('account_batches',0)>0 and p.get('block_reads',0)>0 and p.get('native_plan_steps',0)>=16,'adaptive actual source reads missing')
 require(p.get('verified')is False and p.get('production_ready')is False and p.get('all_requested_features_complete')is False,'adaptive scope overstated')
 require(p.get('trust')=='SYNTHETIC_RPC_ORIGINAL_PACK_NOT_SBF_TRANSACTION','adaptive authority overstated')
def validate_reviews(v,runtime):
 require(v.get('source_tree_sha256')==runtime['source_tree_sha256'],'adaptive reviews stale')
 require(set(v.get('reviews',{}))=={'spec','quality'},'two independent adaptive reviews required')
 for key,r in v['reviews'].items():require(r.get('passed')is True and r.get('open_p1_p2')==0 and r.get('agent_id'),'adaptive review unresolved: '+key)
 require(v.get('scope')=='cc683ec9-adaptive-online-three-fixed-classic-spl','adaptive review scope changed')
def check_projection():
 original=corpus();require(len(original['scenarios'])==4,'adaptive original scenario set differs');projection=json.loads((ROOT/'integration/test/contracts/orca-adaptive-pools.json').read_text());require(projection['source_corpus_sha256']==sha(ROOT/'integration/test/contracts/orca-adaptive-service.json'),'adaptive projection provenance differs');cases=projection['cases'];require(len(cases)==20,'adaptive projected case set differs');by_name={c['name']:c for c in cases};require(len(by_name)==20,'duplicate projection name')
 success=rejected=0
 for scenario in original['scenarios']:
  require(len(scenario['banks'])==5,'adaptive original bank inventory differs')
  for index,b in enumerate(scenario['banks']):
   c=by_name[scenario['name']+'/'+str(index)];e=b['next_trade']['expected'];require(c['config']==clean_config(scenario['config'])and c['pool']==pool(scenario['config'],b['native_state'])and c['params']==b['next_trade']['params'],'adaptive complete original pool projection differs');require(c['expected']['status']==e['status'],'adaptive status differs')
   if e['status']=='fulfilled':
    require(c['expected']['post_pool']==pool(scenario['config'],e['next_state'])and c['expected']['amount_in']==e['amount_in']and c['expected']['amount_out']==e['amount_out']and c['expected']['amount_remaining']=='0','adaptive poststate/amount projection differs');success+=1
   else:require(e['status']=='native_rejected'and c['expected']['error']==e['error'],'adaptive native failure changed');rejected+=1
 require((success,rejected)==(16,4),'adaptive native observation count differs')
 require(original['identity'].get('sbf')is False,'adaptive native source mislabeled SBF')
def run():
 verification,runtime=checked_runtime();inputs=snapshot_inputs();check_projection();report=initial();report.update(source_tree_sha256=runtime['source_tree_sha256'],source_binding=runtime,commands=[])
 for name,script in [('native','orca-adaptive-service-check.py'),('guards','orca-adaptive-service-guards.py')]:
  log=OUT/('orca-adaptive-'+name+'-acceptance.log')
  with log.open('w')as stream:done=subprocess.run([sys.executable,str(ROOT/'integration/tools/ci'/script)],cwd=ROOT,stdout=stream,stderr=subprocess.STDOUT)
  require(done.returncode==0,'adaptive original reference check failed: '+name);report['commands'].append({'name':name,'exit_code':done.returncode,'log':str(log.relative_to(ROOT)),'log_sha256':sha(log)})
 legacy=OUT/'orca-three-service-smoke.json';legacy.unlink(missing_ok=True);legacy_log=OUT/'orca-adaptive-legacy-process.log'
 with legacy_log.open('w')as stream:legacy_run=subprocess.run([sys.executable,'-E',str(ROOT/'integration/tools/acceptance/orca_smoke.py')],cwd=ROOT,stdout=stream,stderr=subprocess.STDOUT)
 require(legacy_run.returncode==0 and json.loads(legacy.read_text()).get('passed')is True,'legacy Orca static/dynamic service regression failed')
 report['legacy_static_dynamic_process']={'passed':True,'exit_code':legacy_run.returncode,'producer_sha256':sha(ROOT/'integration/tools/acceptance/orca_smoke.py'),'report_sha256':sha(legacy),'log_sha256':sha(legacy_log)}
 process=json.loads((OUT/'orca-adaptive-three-service-smoke.json').read_text());validate_process(process,runtime);review=json.loads((OUT/'orca-adaptive-review.json').read_text());validate_reviews(review,runtime)
 # Reference checker refreshes a derived receipt. Its deterministic contents must
 # match the pre-execution commitment; all original sources remain immutable.
 require(runtime_fingerprint()==runtime,'adaptive source/binaries changed during final validation');require(snapshot_inputs()==inputs,'adaptive reference/verifier inputs changed during final validation')
 report.update(passed=True,state='PASSED',source_unchanged=True,input_sha256=hashlib.sha256(json.dumps(inputs,sort_keys=True,separators=(',',':')).encode()).hexdigest(),input_files=len(inputs),verification_sha256=sha(OUT/'verification.json'),process_sha256=sha(OUT/'orca-adaptive-three-service-smoke.json'),review_sha256=sha(OUT/'orca-adaptive-review.json'),measured_at_unix_ms=int(time.time()*1000),native_banks=20,native_success=16,native_rejections=4,service_requests=20,faults=list(FAULTS),remaining=['initialized rewards and dynamic arrays in adaptive combinations','Token2022 and arbitrary extensions','independent SBF/SPL CPI transactions and deployment/history validation','Infinity manager/tree online sourcing and V4/Infinity hooks/settlement'])
 return report

def main():
 write(initial())
 try:r=run();write(r);print(json.dumps({'passed':True,'source_tree_sha256':r['source_tree_sha256'],'all_requested_features_complete':False}))
 except BaseException as e:r=initial();r.update(state='FAILED',error=str(e),error_type=type(e).__name__);write(r);raise
if __name__=='__main__':main()
