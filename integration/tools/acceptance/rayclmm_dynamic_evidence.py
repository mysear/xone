#!/usr/bin/env python3
"""Audit Raydium CLMM dynamic fee source, independent native reference and fresh local runs."""
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
def validate_execution_scope(g):
 for key in ['sbf_transaction_executed','token_program_executed','anchor_instruction_executed']:
  if not (g.get(key)is False):raise AssertionError('explicit native-only execution scope required: '+key)
 if not (isinstance(g.get('limits'),list)and g['limits']):raise AssertionError('descriptive execution limits required')
def validate_native_binding(root,g,native):
 root=Path(root);file_sha=lambda name:sha(root/name)
 report='integration/artifacts/local/rayclmm-dynamic-native-source.json'
 actual=json.loads((root/report).read_text())
 if not (actual==g):raise AssertionError('consumer generation differs from current report')
 validate_execution_scope(g)
 if not (native['verified']and native['all_final_checks_passed']and native['source_patched']is False and native['business_math_imported']is False):raise AssertionError()
 if not (native['profile']==g['profile']and native['adapter_version']==g['adapter_version']):raise AssertionError()
 if not (native['native_source_report_sha256']==file_sha(report)):raise AssertionError('stale native verification generation')
 if not (native['generation_reports'].get(report)==file_sha(report)):raise AssertionError('native generation fingerprint mismatch')
 if not (native['counts']==g['counts']):raise AssertionError('native verification counts drift')
 for key in ['cargo_lock_sha256','original_cargo_lock_sha256','source_manifest_sha256']:
  if not (native[key]==g[key]):raise AssertionError('native reference input fingerprint differs '+key)
 for name,value in native['reference_input_hashes'].items():
  if not (file_sha(name)==value):raise AssertionError('native reference input changed '+name)
 if not (native['outputs']==g['outputs']and native['harness_files']==g['harness_files']):raise AssertionError('native output or harness fingerprints drift')
 if not (native['python_audit_mutation_tests']==g['python_audit_mutation_tests']and native['native_rust_tests_passed']>=31):raise AssertionError()
 for mapping in [g['outputs'],g['harness_files'],g['preserved_hashes'],g['prior_report_hashes'],native['generation_reports'],native['protected_snapshot_hashes']]:
  for name,value in mapping.items():
   if not (file_sha(name)==value):raise AssertionError('native evidence drift '+name)
 for key,name in [('cargo_lock_sha256','Cargo.lock'),('original_cargo_lock_sha256','upstream/Cargo.lock'),('source_manifest_sha256','source-metadata.json')]:
  if not (file_sha('integration/tools/reference/raydium-clmm/'+name)==g[key]):raise AssertionError('reference dependency/source fingerprint drift '+name)

def validate_resumed_acceptance(root,verification):
 root=Path(root);out=root/'integration/artifacts/local';path=out/'full-local-acceptance-resume.json'
 if not path.is_file():return False
 audit=json.loads(path.read_text())
 # A later from-scratch verification supersedes this retained historical run.
 if audit['started_at_unix_ms']<verification['measured_at_unix_ms']:return False
 if not (audit['exit_code']==0 and audit['unchanged_source']is True):raise AssertionError('resumed acceptance incomplete or failed')
 if not (audit['source_tree_sha256']==verification['source_tree_sha256']):raise AssertionError('resumed source differs')
 if not (audit['fresh_verification_sha256']==sha(out/'verification.json')):raise AssertionError('resumed verification changed')
 targets=['check','fixture-test','durable-journal-test','reorg-journal-test','v3-quote-test','multidex-quote-test','stable-quote-test','singleton-quote-test','raydium-quote-test']
 names=['three-service-smoke','journal-integration','kafka-three-service-smoke','reorg-smoke','reorg-http-smoke','v3-differential','v3-three-service-smoke','multidex-differential','multidex-three-service-smoke','stable-differential','stable-three-service-smoke','singleton-differential','singleton-three-service-smoke','raydium-differential','raydium-three-service-smoke']
 if not (audit['passed_targets_not_repeated']==targets):raise AssertionError('unverified targets skipped')
 if not (audit['command']==['make',*sum((['-o',target]for target in targets),[]),'full-local-acceptance']):raise AssertionError('actual command skips unverified targets or changes final target')
 if not (set(audit['fresh_passed_scope_reports'])=={name+'.json'for name in names}):raise AssertionError('reused scope evidence missing or extra')
 if not (audit['original_attempt']=='full-local-acceptance-attempt-1.log'and (out/audit['original_attempt']).is_file()):raise AssertionError('original failure not retained')
 if not (audit['original_attempt_exit_code']==2):raise AssertionError('original failure status missing')
 for name,value in audit['fresh_passed_scope_reports'].items():
  file=out/name
  if not (sha(file)==value and json.loads(file.read_text())['passed']):raise AssertionError('reused report changed '+name)
  if not (file.stat().st_mtime*1000>=verification['measured_at_unix_ms']):raise AssertionError('reused report stale '+name)
 process=json.loads((out/'raydium-three-service-smoke.json').read_text())
 if not (process['tool_sha256']==sha(root/'integration/tools/acceptance/raydium_smoke.py')):raise AssertionError('repaired CPMM process tool changed')
 if not (process['lease_recovery_timeout_seconds']>30):raise AssertionError('recovery wait does not cover retained lease')
 return True

def run():
 v=load('verification.json');s=load('acceptance-summary.json')
 if not (v['passed']and v['source_unchanged_during_verification']):raise AssertionError()
 if not (s['rayclmm_dynamic_quote_scope_passed']and not s['production_ready']):raise AssertionError()
 names=['three-service-smoke','journal-integration','kafka-three-service-smoke','reorg-smoke','reorg-http-smoke','v3-differential','v3-three-service-smoke','multidex-differential','multidex-three-service-smoke','stable-differential','stable-three-service-smoke','singleton-differential','singleton-three-service-smoke','raydium-differential','raydium-three-service-smoke','orca-differential','orca-three-service-smoke','rayclmm-differential','rayclmm-three-service-smoke','rayclmm-token-fee-differential','rayclmm-token-fee-three-service-smoke','rayclmm-bitmap-differential','rayclmm-bitmap-three-service-smoke','rayclmm-seed-differential','rayclmm-seed-three-service-smoke','rayclmm-dynamic-differential','rayclmm-dynamic-three-service-smoke']
 for name in names:
  p=OUT/(name+'.json')
  if not (load(p.name)['passed']):raise AssertionError(name)
  if not (p.stat().st_mtime*1000>=v['measured_at_unix_ms']):raise AssertionError('stale '+name)
 if not ((OUT/'acceptance-summary.json').stat().st_mtime>=(OUT/'rayclmm-dynamic-three-service-smoke.json').stat().st_mtime):raise AssertionError()
 digest=source_hash()
 if not (digest==v['source_tree_sha256']):raise AssertionError('source changed after verification')
 resumed=validate_resumed_acceptance(ROOT,v)
 g=load('rayclmm-dynamic-native-source.json');counts=g['counts'];diff=load('rayclmm-dynamic-differential.json');process=load('rayclmm-dynamic-three-service-smoke.json');review=load('rayclmm-dynamic-review.json')
 if not (diff['case_count']==counts['cases']and counts['cases']>=300):raise AssertionError()
 if not (diff['sequence_count']==counts['sequences']and counts['sequences']>=8 and diff['sequence_steps']==counts['sequence_steps']):raise AssertionError()
 if not (len(process['checks'])==41 and 'normal-rust-rpc-dynamic-time-reference-and-spacing-bounded-native-amounts'in process['checks']):raise AssertionError()
 if not ('sequential-finalized-dynamic-control-and-volatility-change-updates-both-replicas-and-native-amounts'in process['checks']):raise AssertionError()
 if not ('explicit-nine-dynamic-fields-and-seed-cross-protobuf-with-native-reset-window-time-proof'in process['checks']):raise AssertionError()
 if not ('fresh-finalized-bank-clocks-and-independent-evm-control-refresh-preserve-180s-admission'in process['checks']):raise AssertionError()
 if not (diff['golden_sha256']==g['golden_sha256']==sha(ROOT/'integration/test/contracts/rayclmm-dynamic-quotes.json')):raise AssertionError()
 if not (g['source_adaptations']==[]and g['unchanged_source_files_verified']==225):raise AssertionError()
 if not (g['business_math_imported']is False and g['native_swap_internal_executed']and g['native_dynamic_fee_info_executed']and g['native_swap_state_dynamic_fee_executed']and g['native_clock_syscall_executed']):raise AssertionError()
 validate_native_binding(ROOT,g,load('rayclmm-dynamic-reference-verification.json'))
 for generation in [g,load('rayclmm-golden-generation.json'),load('rayclmm-token-fee-golden-generation.json'),load('rayclmm-bitmap-golden-generation.json'),load('rayclmm-seed-native-source.json')]:
  for path,value in generation['harness_files'].items():
   if not (sha(ROOT/path)==value):raise AssertionError('harness changed '+path)
 meta=json.loads(subprocess.run(['python3',str(ROOT/'integration/tools/ci/rayclmm-reference-fetch.py')],capture_output=True,text=True,check=True).stdout)
 if not (meta['verified']and meta['unchanged_files']==225 and meta['archives']==g['archives']):raise AssertionError()
 if not (review['spec_review']['status']=='CLOSED'and review['code_quality_review']['status']=='CLOSED'):raise AssertionError()
 if not (process['tool_sha256']==sha(ROOT/'integration/tools/acceptance/rayclmm_dynamic_smoke.py')):raise AssertionError()
 if not (diff['tool_sha256']==sha(ROOT/'integration/tools/acceptance/rayclmm_dynamic_differential.py')):raise AssertionError()
 corpus=json.loads((ROOT/'integration/test/contracts/rayclmm-dynamic-quotes.json').read_text());same=[v for v in corpus['sequences']if len({step['zero_for_one']for step in v['steps']})==1]
 report={'passed':True,'source_matches_fresh_verification':True,'resumed_acceptance_evidence_validated':resumed,'source_tree_sha256':digest,'fresh_scope_reports':names,'counts':counts,'cases':counts['cases'],'successful_cases':counts['successful_cases'],'reverted_cases':counts['reverted_cases'],'shared_sequences':counts['sequences'],'sequence_steps':counts['sequence_steps'],'shared_allocation_sequences':len(same),'shared_allocation_steps':sum(len(v['steps'])for v in same),'process_checks':41,'raw_fixture_variants':counts['raw_fixture_variants'],'time_invariance_probes':counts['time_invariance_probes'],'dynamic_nine_fields_bound_to_original_layout':True,'same_pool_actual_dynamic_parameter_bank_change_verified':True,'original_480_968_364_464_corpora_unchanged':True,'retained_archives_and_225_sources_reverified':True,'review_stages_closed':True,'production_ready':False,'full_work_packages_accepted':0,'measured_at_unix_ms':int(time.time()*1000)}
 (OUT/'rayclmm-dynamic-final-source.json').write_text(json.dumps(report,indent=2)+'\n');return report
if __name__=='__main__':print(json.dumps(run(),indent=2))
