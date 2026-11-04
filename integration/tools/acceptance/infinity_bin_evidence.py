#!/usr/bin/env python3
"""Bind the bounded original Bin manager/core service proof to current binaries."""
from pathlib import Path
import hashlib,json,secrets,subprocess,sys,time
ROOT=Path(__file__).resolve().parents[3];OUT=ROOT/'integration/artifacts/local';TARGET=OUT/'infinity-bin-acceptance.json'
def require(ok,message):
 if not ok:raise ValueError(message)
def initial():return {'passed':False,'state':'RUNNING','verified':False,'production_ready':False,'all_requested_features_complete':False,'scope':'3c5ef1be-static-zero-hook-standard-erc20-bin-online'}
def write(v):
 OUT.mkdir(parents=True,exist_ok=True);temp=TARGET.with_name(TARGET.name+'.'+secrets.token_hex(6)+'.tmp')
 try:temp.write_text(json.dumps(v,indent=2)+'\n');temp.replace(TARGET)
 finally:temp.unlink(missing_ok=True)
if __name__=='__main__':write(initial())
from weighted_evidence import checked_runtime,runtime_fingerprint,snapshot_inputs,sha
from infinity_bin_smoke import CHECKS,FAULTS
from infinity_bin_fixture import corpus,project

def validate_process(v,runtime,inputs=None,now_ms=None):
 if inputs is None:inputs=snapshot_inputs()
 if now_ms is None:now_ms=int(time.time()*1000)
 require(v.get('input_hashes')==inputs and bool(inputs) and v.get('inputs_unchanged')is True,'Bin process reference/fixture/tool input binding stale')
 start,end=v.get('started_at_unix_ms'),v.get('completed_at_unix_ms')
 require(type(start)is int and type(end)is int and 0<start<=end<=now_ms and now_ms-end<=3_600_000,'Bin process execution window invalid or expired')
 require(v.get('passed')is True and v.get('state')=='PASSED','Bin ordinary process failed')
 require(v.get('source_binding')==runtime,'Bin process source/binary binding stale')
 require(v.get('checks')==CHECKS,'Bin process fault/read coverage differs')
 require((v.get('pool_count'),v.get('banks'),v.get('quote_checks'),v.get('successful_quotes'),v.get('large_input_rejections'))==(2,5,10,8,2),'Bin process native request inventory differs')
 require(v.get('native_plan_steps',0)>=8 and v.get('receipt_reads',0)>0 and all(v.get('read_counts',{}).get(k,0)>0 for k in ['eth_getCode','eth_call']),'Bin actual source/reference reads missing')
 require(v.get('test_quote_budget')=={'max_hops':1,'max_candidates':8,'max_expansions':64,'split_parts':4,'max_splits':2,'timeout_ms':10000},'Bin local replay budget changed')
 require(v.get('verified')is False and v.get('production_ready')is False and v.get('all_requested_features_complete')is False,'Bin scope overstated')
 require(v.get('trust')=='ORIGINAL_MANAGER_GETTERS_SYNTHETIC_BANK_BINPOOL_REFERENCE_NOT_SETTLEMENT','Bin execution authority overstated')
def validate_review(v,runtime):
 require(v.get('source_tree_sha256')==runtime['source_tree_sha256'],'Bin reviews stale')
 require(v.get('scope')=='3c5ef1be-static-zero-hook-bin-manager-tree-and-core','Bin review scope differs')
 require(set(v.get('reviews',{}))=={'spec','quality'},'Bin independent SPEC/quality reviews required')
 for k,r in v['reviews'].items():require(r.get('passed')is True and r.get('agent_id')and r.get('open_p1_p2')==0,'Bin unresolved review: '+k)
def check_projection():
 raw=ROOT/'integration/test/contracts/infinity-bin-service.json';d=corpus();f=json.loads((ROOT/'integration/test/contracts/infinity-bin-pools.json').read_text());require(f['source_corpus_sha256']==sha(raw),'Bin projection source digest stale');require(len(d['RPCbanks'])==len(f['banks'])==5 and len(d['pools'])==2 and len(d['trades'])==4,'Bin source inventory differs');require(d['reference']['scope']['full_manager_swap_settlement_verified']is False and d['reference']['scope']['vault_transient_storage_execution_verified']is False,'Bin reference scope overstated')
 for source,bank in zip(d['RPCbanks'],f['banks']):require(bank=={'id':source['id'],'block':source['block'],'items':[project(d,source,i)for i in range(2)]},'Bin complete source projection differs')
 require(f['trades']==d['trades'],'Bin original swap observations changed')
 detached=json.loads((ROOT/'integration/test/contracts/infinity-bin-detached.json').read_text());require(len(detached['cases'])==16 and sum(len(c['trades'])for c in detached['cases'])==48 and sum(bool(t['expected']['native_reverted'])for c in detached['cases']for t in c['trades'])==9,'Bin original detached burn/swap manifest differs')
def run():
 verification,runtime=checked_runtime();inputs=snapshot_inputs();check_projection();result=initial();result.update(source_tree_sha256=runtime['source_tree_sha256'],source_binding=runtime,commands=[])
 for name,cmd in [('source',['node',ROOT/'integration/tools/ci/infinity-bin-service-check.cjs']),('guards',[sys.executable,'-E',ROOT/'integration/tools/ci/infinity-bin-service-guards.py']),('optimized-guards',[sys.executable,'-OO',ROOT/'integration/tools/ci/infinity-bin-service-guards.py']),('receipt-faults',['node',ROOT/'integration/tools/ci/infinity-bin-service-receipt-test.cjs']),('native-replay',['node',ROOT/'integration/tools/ci/infinity-bin-service-replay-test.cjs']),('detached-native',['node',ROOT/'integration/tools/ci/infinity-bin-detached-check.cjs']),('detached-guards',[sys.executable,ROOT/'integration/tools/ci/infinity-bin-detached-guards.py']),('detached-manager-limit',['node',ROOT/'integration/tools/ci/infinity-bin-detached-manager-proof.cjs']),('detached-receipt-faults',['node',ROOT/'integration/tools/ci/infinity-bin-detached-receipt-test.cjs'])]:
  log=OUT/('infinity-bin-'+name+'-acceptance.log')
  with log.open('w')as stream:p=subprocess.run([str(c)for c in cmd],cwd=ROOT,stdout=stream,stderr=subprocess.STDOUT)
  require(p.returncode==0,'Bin original source check failed: '+name);result['commands'].append({'name':name,'exit_code':p.returncode,'log_sha256':sha(log)})
 legacy=OUT/'singleton-three-service-smoke.json';legacy.unlink(missing_ok=True);legacy_log=OUT/'infinity-bin-legacy-singleton-process.log'
 with legacy_log.open('w')as stream:old=subprocess.run([sys.executable,'-E',str(ROOT/'integration/tools/acceptance/singleton_smoke.py')],cwd=ROOT,stdout=stream,stderr=subprocess.STDOUT)
 require(old.returncode==0 and json.loads(legacy.read_text()).get('passed')is True,'legacy singleton process compatibility failed')
 result['legacy_singleton_process']={'passed':True,'producer_sha256':sha(ROOT/'integration/tools/acceptance/singleton_smoke.py'),'report_sha256':sha(legacy),'log_sha256':sha(legacy_log)}
 report=json.loads((OUT/'infinity-bin-three-service-smoke.json').read_text());validate_process(report,runtime,inputs=inputs);review=json.loads((OUT/'infinity-bin-review.json').read_text());validate_review(review,runtime)
 require(runtime_fingerprint()==runtime,'Bin source or actual binaries changed');require(snapshot_inputs()==inputs,'Bin source/reference inputs changed during validation')
 result.update(passed=True,state='PASSED',source_unchanged=True,verification_sha256=sha(OUT/'verification.json'),process_sha256=sha(OUT/'infinity-bin-three-service-smoke.json'),review_sha256=sha(OUT/'infinity-bin-review.json'),input_sha256=hashlib.sha256(json.dumps(inputs,sort_keys=True,separators=(',',':')).encode()).hexdigest(),input_files=len(inputs),measured_at_unix_ms=int(time.time()*1000),original_manager_runtime_bytes=23852,original_vault_runtime_bytes=9247,original_banks=5,original_trades=4,service_requests=10,source_faults=FAULTS,remaining=['dynamic/swap hooks and fee overrides','native currency and actual Vault/manager unlock settlement','real deployment/history and independent full transactions','other Solana/Curve requested combinations'])
 return result
def main():
 write(initial())
 try:r=run();write(r);print(json.dumps({'passed':True,'source_tree_sha256':r['source_tree_sha256'],'all_requested_features_complete':False}))
 except BaseException as e:r=initial();r.update(state='FAILED',error=str(e),error_type=type(e).__name__);write(r);raise
if __name__=='__main__':main()
