#!/usr/bin/env python3
"""Audit current bitmap-extension source, independent native reference and fresh local runs."""
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
 assert s['rayclmm_bitmap_quote_scope_passed']and not s['production_ready']
 names=['three-service-smoke','journal-integration','kafka-three-service-smoke','reorg-smoke','reorg-http-smoke','v3-differential','v3-three-service-smoke','multidex-differential','multidex-three-service-smoke','stable-differential','stable-three-service-smoke','singleton-differential','singleton-three-service-smoke','raydium-differential','raydium-three-service-smoke','orca-differential','orca-three-service-smoke','rayclmm-differential','rayclmm-three-service-smoke','rayclmm-token-fee-differential','rayclmm-token-fee-three-service-smoke','rayclmm-bitmap-differential','rayclmm-bitmap-three-service-smoke']
 for name in names:
  p=OUT/(name+'.json');assert load(p.name)['passed'],name
  assert p.stat().st_mtime*1000>=v['measured_at_unix_ms'],'stale '+name
 assert (OUT/'acceptance-summary.json').stat().st_mtime>=(OUT/'rayclmm-bitmap-three-service-smoke.json').stat().st_mtime
 digest=source_hash();assert digest==v['source_tree_sha256'],'source changed after verification'
 g=load('rayclmm-bitmap-golden-generation.json');counts=g['counts'];diff=load('rayclmm-bitmap-differential.json');process=load('rayclmm-bitmap-three-service-smoke.json');review=load('rayclmm-bitmap-review.json')
 assert diff['case_count']==counts['cases']and counts['cases']>=160
 assert diff['sequence_count']==counts['sequences']and counts['sequences']>=8 and diff['sequence_steps']==counts['sequence_steps']
 assert counts['source_index_vectors']==86 and counts['invalid_source_index_vectors']==4
 assert len(process['checks'])==28 and 'removed-extension-array-native-success-to-rejection-proves-both-replicas-use-new-bitmap'in process['checks']
 assert diff['golden_sha256']==g['golden_sha256']==sha(ROOT/'integration/test/contracts/rayclmm-bitmap-quotes.json')
 assert g['source_adaptations']==[]and g['unchanged_source_files_verified']==225
 assert g['business_math_imported']is False and g['native_swap_internal_executed']and g['native_extension_account_supplied']and g['source_flip_generated_indices']
 assert g['sbf_transaction_executed']is False and g['token_program_executed']is False
 for path,value in {**g['preserved_hashes'],**g['outputs']}.items():assert sha(ROOT/path)==value,'fixture/sourcevector drift '+path
 native_source=load('rayclmm-bitmap-native-source.json');assert len(native_source['protected_hashes'])==16
 for path,value in native_source['protected_hashes'].items():assert sha(ROOT/path)==value,'protected old data drift '+path
 assert sha(OUT/'rayclmm-bitmap-native-report.md')==native_source['handoff_report_sha256']
 for generation in [g,load('rayclmm-golden-generation.json'),load('rayclmm-token-fee-golden-generation.json')]:
  for path,value in generation['harness_files'].items():assert sha(ROOT/path)==value,'harness changed '+path
 meta=json.loads(subprocess.run(['python3',str(ROOT/'integration/tools/ci/rayclmm-reference-fetch.py')],capture_output=True,text=True,check=True).stdout)
 assert meta['verified']and meta['unchanged_files']==225 and meta['archives']==g['archives']
 assert review['spec_review']['status']=='CLOSED'and review['code_quality_review']['status']=='CLOSED'
 assert review['recovery_acceptance_followup']['spec_review']['status']=='CLOSED'and review['recovery_acceptance_followup']['code_quality_review']['status']=='CLOSED'
 cpmm=load('raydium-three-service-smoke.json');assert cpmm['tool_sha256']==sha(ROOT/'integration/tools/acceptance/raydium_smoke.py')
 assert cpmm['invalid_source_restart_outcome']in ['REJECTED_PROGRAM_VERSION','LIVE_SCOPE_REVOKED']
 assert process['tool_sha256']==sha(ROOT/'integration/tools/acceptance/rayclmm_bitmap_smoke.py')
 assert diff['tool_sha256']==sha(ROOT/'integration/tools/acceptance/rayclmm_bitmap_differential.py')
 corpus=json.loads((ROOT/'integration/test/contracts/rayclmm-bitmap-quotes.json').read_text());same=[v for v in corpus['sequences']if len({step['zero_for_one']for step in v['steps']})==1]
 report={'passed':True,'source_matches_fresh_verification':True,'source_tree_sha256':digest,'fresh_scope_reports':names,'cases':counts['cases'],'successful_cases':counts['successful_cases'],'reverted_cases':counts['reverted_cases'],'shared_sequences':counts['sequences'],'sequence_steps':counts['sequence_steps'],'shared_allocation_sequences':len(same),'shared_allocation_steps':sum(len(v['steps'])for v in same),'source_index_vectors':86,'invalid_source_index_vectors':4,'process_checks':28,'actual_extension_bit_and_array_change_verified':True,'original_480_and_968_corpora_unchanged':True,'retained_archives_and_225_sources_reverified':True,'review_stages_closed':True,'production_ready':False,'full_work_packages_accepted':0,'measured_at_unix_ms':int(time.time()*1000)}
 (OUT/'rayclmm-bitmap-final-source.json').write_text(json.dumps(report,indent=2)+'\n');return report
if __name__=='__main__':print(json.dumps(run(),indent=2))
