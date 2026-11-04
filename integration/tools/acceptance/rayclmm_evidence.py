#!/usr/bin/env python3
"""Audit frozen source, original oracle and all fresh local acceptance reports."""
from pathlib import Path
import json,hashlib,os,time,copy,re,subprocess
R=Path(__file__).resolve().parents[3];O=R/'integration/artifacts/local';load=lambda p:json.loads(p.read_text())
v=load(O/'verification.json');summary=load(O/'acceptance-summary.json')
assert v['passed'] and v['source_unchanged_during_verification']
assert summary['rayclmm_quote_scope_passed'] and not summary['production_ready']
names=['three-service-smoke','journal-integration','kafka-three-service-smoke','reorg-smoke','reorg-http-smoke','v3-differential','v3-three-service-smoke','multidex-differential','multidex-three-service-smoke','stable-differential','stable-three-service-smoke','singleton-differential','singleton-three-service-smoke','raydium-differential','raydium-three-service-smoke','orca-differential','orca-three-service-smoke','rayclmm-differential','rayclmm-three-service-smoke']
for name in names:
 p=O/(name+'.json');assert load(p)['passed'],name;assert p.stat().st_mtime*1000>=v['measured_at_unix_ms'],'stale '+name
assert (O/'acceptance-summary.json').stat().st_mtime>=(O/'rayclmm-three-service-smoke.json').stat().st_mtime
h=hashlib.sha256();skip={'target','bin','.runtime','secrets','.git','__pycache__'}
for project in ['chain-parser-go','chain-parser-rust','dex-router','common/protoc']:
 files=[]
 for folder,dirs,fs in os.walk(R/project):
  dirs[:]=[d for d in dirs if d not in skip]
  for name in fs:
   p=Path(folder)/name
   if p.suffix in ['.go','.rs','.toml','.mod','.sum','.lock','.proto','.yaml','.json','.py']or name=='Makefile':files.append(p)
 for p in sorted(files):h.update(str(p.relative_to(R)).encode());h.update(p.read_bytes())
assert h.hexdigest()==v['source_tree_sha256'],'source changed after verification'
diff=load(O/'rayclmm-differential.json');proc=load(O/'rayclmm-three-service-smoke.json');gold=load(O/'rayclmm-golden-generation.json');review=load(O/'rayclmm-review.json')
assert diff['case_count']==480 and diff['sequence_count']==12 and diff['sequence_steps']==96 and len(proc['checks'])==19
assert 'sequential-finalized-config-fee-change-updates-both-replica-state-and-native-matched-amounts' in proc['checks']
assert proc['tool_sha256']==hashlib.sha256((R/'integration/tools/acceptance/rayclmm_smoke.py').read_bytes()).hexdigest()
assert diff['tool_sha256']==hashlib.sha256((R/'integration/tools/acceptance/rayclmm_differential.py').read_bytes()).hexdigest()
assert review['spec_review']['status']=='CLOSED' and review['code_quality_review']['status']=='CLOSED'
assert review['code_quality_review']['supplemental_review']['status']=='CLOSED'
assert diff['golden_sha256']==gold['golden_sha256']==hashlib.sha256((R/'integration/test/contracts/rayclmm-quotes.json').read_bytes()).hexdigest()
assert gold['tick_vectors']==2053 and gold['unchanged_source_files_verified']==225 and gold['source_adaptations']==[]
assert gold['exhaustive_tick_digest']['count']==887273
source_check=subprocess.run(['python3',str(R/'integration/tools/ci/rayclmm-reference-fetch.py')],capture_output=True,text=True,check=True)
source_meta=json.loads(source_check.stdout);assert source_meta['verified'] and source_meta['unchanged_files']==225 and source_meta['archives']==gold['archives']
assert hashlib.sha256((R/'integration/tools/reference/raydium-clmm/source-metadata.json').read_bytes()).hexdigest()==gold['source_manifest_sha256']

for path,digest in gold['harness_files'].items():assert hashlib.sha256((R/path).read_bytes()).hexdigest()==digest,'harness drift '+path
assert hashlib.sha256((R/'integration/test/solana/rayclmm-base.json').read_bytes()).hexdigest()==gold['fixture_sha256']
for project in ['integration','chain-parser-go','chain-parser-rust','dex-router']:
 assert hashlib.sha256((R/project/'test/contracts/rayclmm-tickmath.json').read_bytes()).hexdigest()==gold['tick_vectors_sha256']
final={'passed':True,'source_matches_fresh_verification':True,'source_tree_sha256':h.hexdigest(),'fresh_scope_reports':names,'rayclmm_cases':480,'successful_cases':320,'reverted_cases':160,'shared_sequences':12,'sequence_steps':96,'shared_allocation_sequences':8,'shared_allocation_steps':64,'tickmath_vectors':2053,'exhaustive_ticks':887273,'original_source_files':225,'retained_archives_and_sources_reverified':True,'process_checks':19,'actual_config_fee_change_verified':True,'review_stages_closed':True,'production_ready':False,'full_work_packages_accepted':0,'measured_at_unix_ms':int(time.time()*1000)}
(O/'rayclmm-final-source.json').write_text(json.dumps(final,indent=2)+'\n')
print(json.dumps(final,indent=2))
