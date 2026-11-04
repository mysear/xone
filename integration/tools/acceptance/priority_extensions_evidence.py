#!/usr/bin/env python3
"""Check bounded Crypto service and Orca/Infinity kernel evidence against current code.

No complete-protocol, live-chain, SBF, VERIFIED or production acceptance claim.
"""
from pathlib import Path
import hashlib,json,secrets,subprocess,sys,time
ROOT=Path(__file__).resolve().parents[3];OUT=ROOT/'integration/artifacts/local';REPORT=OUT/'priority-extensions-acceptance.json'
def require(ok,message):
 if not ok:raise ValueError(message)
def write(value):
 OUT.mkdir(parents=True,exist_ok=True);temp=REPORT.with_name(REPORT.name+'.'+secrets.token_hex(6)+'.tmp')
 try:temp.write_text(json.dumps(value,indent=2)+'\n');temp.replace(REPORT)
 finally:temp.unlink(missing_ok=True)
def initial():return {'passed':False,'state':'RUNNING','verified':False,'production_ready':False,'all_requested_features_complete':False,'scope':'crypto-settled-standard-service-and-orca-infinity-kernels'}
if __name__=='__main__':write(initial())
from weighted_evidence import checked_runtime,runtime_fingerprint,snapshot_inputs,sha
EXPECTED_CHECKS=['normal-go-rpc-postgres-kafka-and-two-routers','30-original-exchanges-complete-live-pricing-raw-lp-state','independent-live-protobuf-and-canonical-hash-pinning','native-reverts-never-return-quotes',*['scope-withdraw-and-new-generation:'+f for f in ['lp-code','minter','raw-balance','template','ramp']],'unrelated-solana-quotes-survive-crypto-withdrawal','router-restart-restores-complete-crypto-bank','verified-and-build-remain-disabled']
def validate_process(value,runtime):
 require(value.get('passed')is True and value.get('state')=='PASSED','Crypto normal process verification failed')
 require(value.get('source_binding')==runtime,'Crypto process source/binary binding differs')
 require(value.get('checks')==EXPECTED_CHECKS,'Crypto process coverage differs')
 require(value.get('quote_checks')==60 and value.get('successful_quotes')==40 and value.get('native_revert_rejections')==20,'Crypto original request counts differ')
 require(value.get('phase_count')==15 and value.get('pool_count')==2 and value.get('receipt_reads',0)>0,'Crypto bank/receipt coverage missing')
 require(value.get('verified')is False and value.get('production_ready')is False and value.get('all_requested_features_complete')is False,'Crypto scope overstated')
 require(value.get('trust')=='SYNTHETIC_RPC_NOT_DEPLOYMENT_PROOF','Crypto source authority overstated')
def validate_review(value,runtime):
 require(value.get('source_tree_sha256')==runtime['source_tree_sha256'],'Reviews are stale')
 require(set(value.get('reviews',{}))=={'spec','quality','adaptive_swap_spec','adaptive_swap_quality','crypto_dependency_quality'},'Both independent reviews required for core and adaptive swap')
 for name,review in value['reviews'].items():
  require(review.get('passed')is True and review.get('open_p1_p2')==0 and review.get('agent_id'),'Review did not close: '+name)
 require(value.get('scope')=='crypto-service-orca-adaptive-swap-infinity-multi-bin','Review scope differs')
def validate_corpus_counts(crypto,fee,swap,bins):
 require(len(crypto['cases'])==60,'Crypto reference case count differs')
 require(sum(bool(c['expected'].get('reverted')) for c in crypto['cases'])==16,'Crypto native revert count differs')
 require(len(fee['cases'])==197 and len(fee['sequences'])==2 and sum(len(s['trades']) for s in fee['sequences'])==14,'Orca fee corpus differs')
 require(fee.get('online_profile_enabled')is False,'Orca fee kernel mislabeled online')
 require(len(swap['cases'])==101 and len(swap['failures'])==2 and len(swap['sequences'])==2 and sum(len(s['trades']) for s in swap['sequences'])==12,'Orca swap corpus differs')
 require(swap.get('online_profile_enabled')is False,'Orca swap kernel mislabeled online')
 require(len(bins['cases'])==326,'Infinity reference case count differs')
def check_corpora():
 import importlib.util
 def data(name):return json.loads((ROOT/'integration/test/contracts'/name).read_text())
 crypto=data('curve-crypto-full.json');fee=data('orca-adaptive.json');swap=data('orca-adaptive-swap.json');bins=data('infinity-bin-multi.json');validate_corpus_counts(crypto,fee,swap,bins)
 service=data('curve-crypto-service.json');template=data('curve-crypto-runtime-template.json')
 require(service['source_corpus_sha256']==sha(ROOT/'integration/test/contracts/curve-crypto-full.json'),'Crypto service reference digest differs')
 require(len(service['cases'])==30,'Crypto service case count differs')
 spec=importlib.util.spec_from_file_location('crypto_projection',ROOT/'integration/tools/ci/curve-crypto-service-golden.py');projection=importlib.util.module_from_spec(spec);spec.loader.exec_module(projection)
 original={c['name']:c for c in crypto['cases']}
 for case in service['cases']:
  c=original[case['name']]
  require(case['pool']==projection.make_pool(c,template) and case['expected']==c['expected'],'Crypto service lost original state or observation')
  require(all(case[k]==c[k] for k in ['input','index_in','index_out']),'Crypto service call differs from original')
def run():
 verification,runtime=checked_runtime();inputs=snapshot_inputs();check_corpora();result=initial();result.update(source_tree_sha256=runtime['source_tree_sha256'],source_binding=runtime,commands=[])
 for name,args in [('crypto-original',[sys.executable,ROOT/'integration/tools/ci/curve-crypto-full-verify.py']),('orca-original',[sys.executable,ROOT/'integration/tools/ci/orca-adaptive-golden.py','--check']),('orca-swap-original',[sys.executable,ROOT/'integration/tools/ci/orca-adaptive-swap-golden.py','--check']),('orca-swap-guards',[sys.executable,ROOT/'integration/tools/ci/orca-adaptive-swap-guards.py']),('infinity-original',[sys.executable,ROOT/'integration/tools/ci/infinity-bin-multi-check.py','--with-tests'])]:
  log=OUT/('priority-extensions-'+name+'-evidence.log')
  with log.open('w')as stream:completed=subprocess.run([str(a)for a in args],cwd=ROOT,stdout=stream,stderr=subprocess.STDOUT)
  require(completed.returncode==0,'Reference evidence failed: '+name);result['commands'].append({'name':name,'exit_code':completed.returncode,'log':str(log.relative_to(ROOT)),'log_sha256':sha(log)})
 process=json.loads((OUT/'curve-crypto-three-service-smoke.json').read_text());validate_process(process,runtime);review=json.loads((OUT/'priority-extensions-review.json').read_text());validate_review(review,runtime)
 require(runtime_fingerprint()==runtime,'Source or binaries changed during evidence validation');require(snapshot_inputs()==inputs,'Reference or verifier inputs changed during evidence validation')
 result.update(passed=True,state='PASSED',source_unchanged=True,input_sha256=hashlib.sha256(json.dumps(inputs,sort_keys=True,separators=(',',':')).encode()).hexdigest(),input_files=len(inputs),verification_sha256=sha(OUT/'verification.json'),process_sha256=sha(OUT/'curve-crypto-three-service-smoke.json'),review_sha256=sha(OUT/'priority-extensions-review.json'),measured_at_unix_ms=int(time.time()*1000),crypto_original_cases=60,crypto_original_success=44,crypto_original_reverts=16,crypto_service_requests=60,orca_fee_manager_cases=197,orca_sequence_trades=14,orca_adaptive_swap_cases=101,orca_adaptive_swap_native_failures=2,orca_adaptive_swap_sequence_trades=12,infinity_cases=326,remaining=['Orca initialized Oracle parser/full adaptive online swap','Infinity complete tree source/manager online profile','V4/Infinity arbitrary hooks/override/native manager settlement','Curve active ramp online schedule and other Crypto versions','Independent full transactions, real deployment/history and production'])
 return result

def main():
 write(initial())
 try:value=run();write(value);print(json.dumps({'passed':True,'source_tree_sha256':value['source_tree_sha256'],'all_requested_features_complete':False}))
 except BaseException as error:value=initial();value.update(state='FAILED',error=str(error),error_type=type(error).__name__);write(value);raise
if __name__=='__main__':main()
