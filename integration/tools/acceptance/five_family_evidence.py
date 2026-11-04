#!/usr/bin/env python3
"""Evidence for implemented subsets; never claims all five families complete."""
from pathlib import Path
import argparse,json,secrets,sys,time
ROOT=Path(__file__).resolve().parents[3];OUT=ROOT/'integration/artifacts/local';PREFIX='five-family'
if __name__=='__main__':
 OUT.mkdir(parents=True,exist_ok=True);suffix='-runtime-binding.json'if'--bind-runtime'in sys.argv else'-scope-execution.json'if'--seal-scopes'in sys.argv else'-acceptance.json';target=OUT/(PREFIX+suffix);tmp=target.with_name(target.name+'.'+secrets.token_hex(6)+'.tmp')
 try:tmp.write_text(json.dumps({'passed':False,'state':'RUNNING'})+'\n');tmp.replace(target)
 finally:tmp.unlink(missing_ok=True)
import weighted_evidence as common
require=common.require;sha=common.sha;write_result=common.write_result;runtime_fingerprint=common.runtime_fingerprint;snapshot_inputs=common.snapshot_inputs
PRODUCERS=[('smoke.py','three-service-smoke.json'),('weighted_static_smoke.py','weighted-static-three-service-smoke.json'),('multi_model_smoke.py','multi-model-three-service-smoke.json'),('solana_rewards_smoke.py','rewards-orca-three-service-smoke.json',['--family','orca']),('solana_rewards_smoke.py','rewards-raydium-clmm-three-service-smoke.json',['--family','raydium-clmm']),('singleton_smoke.py','singleton-three-service-smoke.json')]
SCOPES=[p[1]for p in PRODUCERS]
COUNTS={'balancer-multi-quotes.json':{'cases':648},'balancer-multi-managed.json':{'cases':588,'weight_cases':8,'guards':4},'balancer-composable-math.json':{'regular':756,'bpt':504},'balancer-composable-admitted.json':{'regular':48},'curve-multi-quotes.json':{'cases':720},'curve-ng-quotes.json':{'cases':522,'math_cases':6},'curve-crypto-quotes.json':{'cases':288},'singleton-advanced-quotes.json':{'cases':240},'singleton-advanced-bin-quotes.json':{'cases':180},'solana-advanced/orca-rewards.json':{'cases':14,'kernels':5,'sequences':4},'solana-advanced/raydium-clmm-rewards.json':{'cases':12,'kernels':13,'sequences':4}}
def load(n):return json.loads((OUT/n).read_text())
def bind_runtime():return common.bind_bundle(PREFIX)
def seal_scopes():return common.seal_bundle(PREFIX,SCOPES)
def original_sources():
 reference=ROOT/'integration/tools/reference';counts={}
 for rev in ['e281ff40583c39bf1d46c322334b2b13936e0e33','e91a2b643a49856f51a648d175667c1b48cf3377']:
  manifest=json.loads((reference/'balancer-multi'/(rev+'.json')).read_text());require(manifest['revision']==rev,'Balancer revision changed')
  for item in manifest['files']:require(sha(reference/'balancer-multi/sources'/rev/item['path'])==item['sha256'],'Balancer source changed '+item['path'])
  counts[rev]=len(manifest['files'])
 manifest=json.loads((reference/'curve-models/sources.json').read_text())
 for item in manifest:require(sha(reference/'curve-models'/item['local'])==item['sha256'],'Curve source changed '+item['local'])
 counts['curve']=len(manifest)
 manifest=json.loads((reference/'singleton-advanced/source-metadata.json').read_text())
 for item in manifest['files']:require(sha(ROOT/'integration/.runtime/singleton-reference/sources'/item['local'])==item['sha256'],'singleton source changed '+item['local'])
 for item in manifest['harnesses']:require(sha(reference/'singleton-advanced'/item['name'])==item['sha256'],'singleton harness changed')
 counts['singleton']=len(manifest['files'])
 for family in ['orca-whirlpools','raydium-clmm']:
  base=reference/family;manifest=json.loads((base/'source-metadata.json').read_text());require(manifest['source_adaptations']==[],'native source adaptations')
  archives=manifest.get('archives',[{'tree':'upstream','files':manifest.get('files',[])}]);count=0
  for archive in archives:
   for item in archive['files']:require(sha(base/archive['tree']/item['path'])==item['sha256'],'native source changed '+item['path']);count+=1
  counts[family]=count
 return counts
def validate_corpora():
 results={}
 for name,expected in COUNTS.items():
  p=ROOT/'integration/test/contracts'/name;data=json.loads(p.read_text())
  for key,count in expected.items():require(len(data[key])==count,'corpus scope changed '+name+'/'+key)
  results[name]={'sha256':sha(p),'counts':expected}
 metadata=json.loads((ROOT/'integration/tools/reference/singleton-advanced/source-metadata.json').read_text());require(results['singleton-advanced-quotes.json']['sha256']==metadata['fixture_sha256'] and results['singleton-advanced-bin-quotes.json']['sha256']==metadata['bin_fixture_sha256'],'singleton fixture changed')
 generation=load('singleton-advanced-generation.json');require(generation['passed']is True and generation['cases']==240 and generation['bin_cases']==180,'singleton generator failed')
 require(generation['source_metadata_sha256']==sha(ROOT/'integration/tools/reference/singleton-advanced/source-metadata.json'),'singleton pins changed')
 for name,digest in generation['oracle_inputs'].items():require(sha(ROOT/name)==digest,'singleton execution input changed')
 admitted=load('balancer-composable-admitted-generation.json');require(admitted['passed']is True and admitted['cases']==48 and admitted['corpus_sha256']==results['balancer-composable-admitted.json']['sha256'],'admitted Composable reference failed')
 for name,digest in admitted['oracle_inputs'].items():require(sha(ROOT/name)==digest,'admitted Composable execution input changed')
 return results

def validate():
 from multi_model_smoke import PROCESS_CHECKS
 from solana_rewards_smoke import CHECKS
 runtime,inputs=common.validate_bundle(PREFIX,SCOPES);sources=original_sources();corpora=validate_corpora();multi=load('multi-model-three-service-smoke.json');require(multi['checks']==PROCESS_CHECKS and multi['pool_count']==6 and multi['production_ready']is False,'multi process scope changed')
 for family in ['orca','raydium-clmm']:
  p=load('rewards-'+family+'-three-service-smoke.json');require(p['family']==family and p['checks']==CHECKS and p['steps']==3 and len(p['step_native_sha256'])==3,'reward scope changed');require(p['production_ready']is False and p['sbf_executed']is False and p['token_program_executed']is False,'reward execution authority overstated')
 review=load(PREFIX+'-review.json');require(review['source_tree_sha256']==runtime['source_tree_sha256'] and review['input_hashes']==inputs,'review stale')
 for stage in ['spec_review','quality_review']:require(review[stage]['status']=='CLOSED' and review[stage]['open_important_findings']==0,'open review '+stage)
 return{'passed':True,'scope':'implemented bounded protocol subsets; remaining family features explicitly unimplemented','all_requested_features_complete':False,'production_ready':False,'source_tree_sha256':runtime['source_tree_sha256'],'input_hashes':inputs,'source_counts':sources,'reference_corpora':corpora,'verification_sha256':sha(OUT/'verification.json'),'scope_execution_sha256':sha(OUT/(PREFIX+'-scope-execution.json')),'review_sha256':sha(OUT/(PREFIX+'-review.json')),'fresh_scope_reports':SCOPES,'multi_asset_process_checks':len(PROCESS_CHECKS),'reward_process_checks_per_family':len(CHECKS),'measured_at_unix_ms':int(time.time()*1000)}
def run():
 target=OUT/(PREFIX+'-acceptance.json');write_result(target,{'passed':False,'state':'RUNNING'})
 try:r=validate();write_result(target,r);return r
 except BaseException as e:write_result(target,{'passed':False,'error':str(e),'error_type':type(e).__name__});raise
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--bind-runtime',action='store_true');p.add_argument('--seal-scopes',action='store_true');a=p.parse_args();r=bind_runtime()if a.bind_runtime else seal_scopes()if a.seal_scopes else run();print(json.dumps({'passed':r['passed'],'all_requested_features_complete':False}))
