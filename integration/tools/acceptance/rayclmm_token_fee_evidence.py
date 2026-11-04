#!/usr/bin/env python3
"""Audit current token-fee source, independent native reference and fresh local runs."""
from pathlib import Path
import hashlib,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[3]
OUT=ROOT/'integration/artifacts/local'
def load(name):return json.loads((OUT/name).read_text())
def sha(path):return hashlib.sha256(path.read_bytes()).hexdigest()
def source_hash():
 h=hashlib.sha256()
 for project in ['chain-parser-go','chain-parser-rust','dex-router','common/protoc']:
  files=[]
  for directory,dirs,names in os.walk(ROOT/project):
   dirs[:]=[d for d in dirs if d not in {'target','bin','.runtime','secrets','.git','__pycache__'}]
   for name in names:
    p=Path(directory)/name
    if p.suffix in ['.go','.rs','.toml','.mod','.sum','.lock','.proto','.yaml','.json','.py']or name=='Makefile':files.append(p)
  for p in sorted(files):h.update(str(p.relative_to(ROOT)).encode());h.update(p.read_bytes())
 return h.hexdigest()
def run():
 v=load('verification.json');s=load('acceptance-summary.json')
 assert v['passed']and v['source_unchanged_during_verification']
 assert s['rayclmm_token_fee_quote_scope_passed']and not s['production_ready']
 names=['three-service-smoke','journal-integration','kafka-three-service-smoke','reorg-smoke','reorg-http-smoke','v3-differential','v3-three-service-smoke','multidex-differential','multidex-three-service-smoke','stable-differential','stable-three-service-smoke','singleton-differential','singleton-three-service-smoke','raydium-differential','raydium-three-service-smoke','orca-differential','orca-three-service-smoke','rayclmm-differential','rayclmm-three-service-smoke','rayclmm-token-fee-differential','rayclmm-token-fee-three-service-smoke']
 for name in names:
  p=OUT/(name+'.json');assert load(p.name)['passed'],name
  assert p.stat().st_mtime*1000>=v['measured_at_unix_ms'],'stale '+name
 assert (OUT/'acceptance-summary.json').stat().st_mtime>=(OUT/'rayclmm-token-fee-three-service-smoke.json').stat().st_mtime
 digest=source_hash();assert digest==v['source_tree_sha256'],'source changed after verification'
 g=load('rayclmm-token-fee-golden-generation.json');old=load('rayclmm-golden-generation.json');diff=load('rayclmm-token-fee-differential.json');process=load('rayclmm-token-fee-three-service-smoke.json');review=load('rayclmm-token-fee-review.json')
 assert diff['case_count']==g['counts']['cases']==968 and g['counts']['successful_cases']==636 and g['counts']['reverted_cases']==332
 assert diff['sequence_count']==g['counts']['sequences']==24 and diff['sequence_steps']==g['counts']['sequence_steps']==192
 assert len(process['checks'])==21 and 'sequential-finalized-pool-fee-mode-change-updates-both-replica-state-and-native-matched-amounts'in process['checks']
 assert diff['golden_sha256']==g['golden_sha256']==sha(ROOT/'integration/test/contracts/rayclmm-token-fee-quotes.json')
 assert g['old_golden_sha256']==old['golden_sha256']==sha(ROOT/'integration/test/contracts/rayclmm-quotes.json')=='ef1a0be7c80d6f7f5e815b75a10223a9a7be225d279bc74e057664b5af4f6f31'
 assert g['old_corpus_byte_identical']and g['base_modes_match_original_raw_rpc_pool']and g['source_adaptations']==[]and g['unchanged_source_files_verified']==225
 assert g['old_generation_report_sha256']==sha(OUT/'rayclmm-golden-generation.json')
 for generation in [g,old]:
  for path,value in generation['harness_files'].items():assert sha(ROOT/path)==value,'harness changed '+path
 assert sha(ROOT/'integration/test/solana/rayclmm-base.json')==g['fixture_sha256']==old['fixture_sha256']
 meta=json.loads(subprocess.run(['python3',str(ROOT/'integration/tools/ci/rayclmm-reference-fetch.py')],capture_output=True,text=True,check=True).stdout)
 assert meta['verified']and meta['unchanged_files']==225 and meta['archives']==g['archives']==old['archives']
 assert review['spec_review']['status']=='CLOSED'and review['code_quality_review']['status']=='CLOSED'
 assert process['tool_sha256']==sha(ROOT/'integration/tools/acceptance/rayclmm_token_fee_smoke.py')
 assert diff['tool_sha256']==sha(ROOT/'integration/tools/acceptance/rayclmm_token_fee_differential.py')
 report={'passed':True,'source_matches_fresh_verification':True,'source_tree_sha256':digest,'fresh_scope_reports':names,'cases':968,'successful_cases':636,'reverted_cases':332,'shared_sequences':24,'sequence_steps':192,'shared_allocation_sequences':16,'shared_allocation_steps':128,'process_checks':21,'fee_modes':[1,2],'actual_pool_mode_change_verified':True,'old_480_case_corpus_unchanged':True,'retained_archives_and_225_sources_reverified':True,'review_stages_closed':True,'production_ready':False,'full_work_packages_accepted':0,'measured_at_unix_ms':int(time.time()*1000)}
 (OUT/'rayclmm-token-fee-final-source.json').write_text(json.dumps(report,indent=2)+'\n');return report
if __name__=='__main__':print(json.dumps(run(),indent=2))
