#!/usr/bin/env python3
"""Static Weighted exact-in acceptance, reusing the checked-scope evidence core."""
import json,secrets,sys,time
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3];OUT=ROOT/'integration/artifacts/local'
# Invalidate the requested result before loading the reusable evidence module.
# A broken/missing helper cannot leave an older success eligible for acceptance.
if __name__=='__main__':
 OUT.mkdir(parents=True,exist_ok=True)
 suffix='-runtime-binding.json' if '--bind-runtime' in sys.argv else '-scope-execution.json' if '--seal-scopes' in sys.argv else '-acceptance.json'
 target=OUT/('weighted-static'+suffix);temporary=target.with_name(target.name+'.'+secrets.token_hex(6)+'.tmp')
 try:temporary.write_text(json.dumps({'passed':False,'state':'RUNNING'})+'\n');temporary.replace(target)
 finally:temporary.unlink(missing_ok=True)
import weighted_evidence as common
sha=common.sha;require=common.require;write_result=common.write_result
runtime_fingerprint=common.runtime_fingerprint;snapshot_inputs=common.snapshot_inputs;source_hash=common.source_hash
PREFIX='weighted-static'
SCOPES=['three-service-smoke.json','stable-three-service-smoke.json','weighted-three-service-smoke.json','weighted-static-three-service-smoke.json']
ORACLE_INPUTS=['integration/tools/ci/weighted-static-golden.cjs','integration/tools/reference/weighted-model/Harness.sol','integration/tools/reference/weighted-model/StaticMathRef.sol','integration/tools/reference/weighted-model/source-metadata.json','integration/tools/reference/stable-models/package.json','integration/tools/reference/stable-models/package-lock.json']
LEGACY={'integration/test/contracts/weighted-quotes.json':'8fa01bc569481d7838f8a50dd7c2adb45bbdcd3f64f4dcafed25390c27b2e13c','integration/test/contracts/weighted-frames.json':'8b01bb9f4489b7e76a5f7c0c6b49a1f18702da781c7ab95fe3f1aeb957e6f755','integration/test/contracts/weighted-state.json':'782747d909cded36ed57c12fc6963ae5f023c9a960854ceebd30b1efaa6ea5d6'}
def load(name):return json.loads((OUT/name).read_text())
def bind_runtime():return common.bind_bundle(PREFIX)
def seal_scopes():return common.seal_bundle(PREFIX,SCOPES)
def validate_oracle():
 generation=load(PREFIX+'-golden-generation.json');corpus=ROOT/'integration/test/contracts/weighted-static-quotes.json';powers=ROOT/'integration/test/contracts/weighted-static-powers.json';data=json.loads(corpus.read_text());power_data=json.loads(powers.read_text())
 require(generation['passed'] is True and generation['corpus_sha256']==sha(corpus) and generation['powers_sha256']==sha(powers),'oracle output mismatch')
 require(generation['cases']==len(data['cases'])==5355 and generation['sequences']==len(data['sequences'])==315 and generation['steps']==sum(len(s['steps'])for s in data['sequences'])==1260 and generation['power_cases']==len(power_data['cases'])==240,'oracle coverage changed')
 require(generation['oracle_inputs']=={n:sha(ROOT/n) for n in ORACLE_INPUTS},'oracle inputs changed')
 require(generation['runtime_sha256']==common.oracle_runtime_hash(),'oracle installed runtime changed')
 require(generation['legacy_inputs']==LEGACY and all(sha(ROOT/n)==v for n,v in LEGACY.items()),'legacy corpus changed')
 manifest=json.loads((ROOT/'integration/tools/reference/weighted-model/source-metadata.json').read_text())
 require(manifest['revision']==data['reference']['revision']==power_data['revision']=='e281ff40583c39bf1d46c322334b2b13936e0e33' and len(manifest['files'])==40,'reference source closure changed')
 for item in manifest['files']:require(sha(ROOT/'integration/.runtime/weighted-reference/sources'/item['path'])==item['sha256'],'reference source changed '+item['path'])
 require(generation['reference']==data['reference'] and data['reference']['harness_sha256']==sha(ROOT/'integration/tools/reference/weighted-model/Harness.sol'),'reference flow changed')
 return generation

def validate():
 from weighted_static_smoke import PROCESS_CHECKS,NAMES
 from weighted_smoke import PROCESS_CHECKS as OLD_CHECKS
 runtime,inputs=common.validate_bundle(PREFIX,SCOPES);generation=validate_oracle();p=load(PREFIX+'-three-service-smoke.json');old=load('weighted-three-service-smoke.json')
 require(p['checks']==PROCESS_CHECKS and p['pool_count']==len(NAMES)==6 and p['protocol']=='balancer-v2-weighted2-static','static process scope changed')
 require(old['checks']==OLD_CHECKS and old['pool_count']==3 and old['protocol']=='balancer-v2-weighted2-equal','legacy process scope changed')
 for process in [p,old]:require(process['trust']=='SYNTHETIC_RPC_NOT_DEPLOYMENT_PROOF' and process['production_ready'] is False,'trust overstated')
 review=load(PREFIX+'-review.json');require(review['source_tree_sha256']==runtime['source_tree_sha256'] and review['input_hashes']==inputs,'review stale')
 for stage in ['spec_review','quality_review']:require(review[stage]['status']=='CLOSED' and review[stage]['open_important_findings']==0,'open review '+stage)
 return {'passed':True,'production_ready':False,'source_tree_sha256':runtime['source_tree_sha256'],'input_hashes':inputs,'verification_sha256':sha(OUT/'verification.json'),'scope_execution_sha256':sha(OUT/(PREFIX+'-scope-execution.json')),'review_sha256':sha(OUT/(PREFIX+'-review.json')),'golden_generation_sha256':sha(OUT/(PREFIX+'-golden-generation.json')),'fresh_scope_reports':SCOPES,'cases':generation['cases'],'shared_sequences':generation['sequences'],'sequence_steps':generation['steps'],'power_cases':generation['power_cases'],'process_checks':len(PROCESS_CHECKS),'legacy_process_checks':len(OLD_CHECKS),'measured_at_unix_ms':int(time.time()*1000)}
def run():
 target=OUT/(PREFIX+'-acceptance.json');write_result(target,{'passed':False,'state':'RUNNING'})
 try:r=validate();write_result(target,r);return r
 except BaseException as e:write_result(target,{'passed':False,'error':str(e),'error_type':type(e).__name__});raise
if __name__=='__main__':
 import argparse
 p=argparse.ArgumentParser();p.add_argument('--bind-runtime',action='store_true');p.add_argument('--seal-scopes',action='store_true');a=p.parse_args();r=bind_runtime() if a.bind_runtime else seal_scopes() if a.seal_scopes else run();print(json.dumps({'passed':r['passed']}))
