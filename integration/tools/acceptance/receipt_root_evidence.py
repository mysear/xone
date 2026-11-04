#!/usr/bin/env python3
"""Verify fresh receipt-root process, source, review and independent-reference evidence."""
import hashlib
import json
from pathlib import Path
import time
import secrets
ROOT = Path(__file__).resolve().parents[3]
SCOPE_REPORTS = ['three-service-smoke', 'journal-integration', 'kafka-three-service-smoke', 'reorg-smoke', 'reorg-http-smoke', 'v3-differential', 'v3-three-service-smoke', 'multidex-differential', 'multidex-three-service-smoke', 'stable-differential', 'stable-three-service-smoke', 'singleton-differential', 'singleton-three-service-smoke', 'raydium-differential', 'raydium-three-service-smoke', 'orca-differential', 'orca-three-service-smoke', 'rayclmm-differential', 'rayclmm-three-service-smoke', 'rayclmm-token-fee-differential', 'rayclmm-token-fee-three-service-smoke', 'rayclmm-bitmap-differential', 'rayclmm-bitmap-three-service-smoke', 'rayclmm-seed-differential', 'rayclmm-seed-three-service-smoke', 'rayclmm-dynamic-differential', 'rayclmm-dynamic-three-service-smoke', 'receipt-witness-three-service-smoke', 'source-wal-three-service-smoke', 'receipt-root-three-service-smoke']

BASE_CHECKS = [
 'six-protocol-version-scopes-at-one-block-consumed-by-two-complete-replicas',
 'pancake-v1-v2-and-sushi-v2-fixed-fees-match-independent-oracles-in-both-directions',
 'pancake-v3-uint32-feeProtocol-and-tick-math-match-pinned-official-sdk',
 'protobuf-preserves-versioned-v2-identity-and-wide-pancake-v3-protocol-fee',
 'complete-receipts-and-state-dependencies-for-all-version-scopes-share-canonical-commit',
 'both-endpoints-reproduce-complete-receipts-under-same-canonical-block',
]
FAULTS = ['common-tail-original-bloom', 'common-tail-rebuilt-bloom', 'primary-status-tamper',
          'witness-status-tamper', 'primary-unsupported-chain-receipt', 'witness-unsupported-chain-receipt']
PROCESS_CHECKS = BASE_CHECKS + ['receipt-'+f+'-disagreement-invalidates-both-old-handles-no-bad-commit-and-current-baseline-restores' for f in FAULTS] + ['memory-only-router-rebuilds-current-scope-after-witness-recovery']
TOOL_FILES = [
 'integration/tools/acceptance/tests/test_receipt_root_startup.py',
 'integration/tools/acceptance/infrastructure.py',
 'integration/tools/acceptance/configuration.py',
 'integration/tools/acceptance/kafka_smoke.py',
 'integration/tools/acceptance/smoke.py',
 'integration/tools/acceptance/rayclmm_dynamic_evidence.py',
 'integration/test/contracts/v3-state.json',
 'integration/test/contracts/dex-versions.json',
 'integration/test/contracts/v3-quotes.json',
 'integration/test/contracts/receipt-root-fixtures.json',
 'integration/tools/acceptance/tests/test_receipt_reference.py',
 'integration/tools/acceptance/receipt_root_evidence.py',
 'integration/tools/acceptance/tests/test_receipt_root_evidence.py',
 'integration/Makefile',
 'integration/tools/acceptance/report.py',
 'integration/tools/ci/progress_bindings.py',
 'integration/tools/reference/receipt-root/package.json',
 'integration/tools/reference/receipt-root/package-lock.json',
 'integration/tools/reference/receipt-root/oracle.cjs',
 'integration/tools/ci/receipt-root-reference.py',
 'integration/tools/acceptance/receipt_reference.py',
 'integration/tools/acceptance/receipt_root_smoke.py',
 'integration/tools/acceptance/receipt_witness_smoke.py',
 'integration/tools/acceptance/reorg_smoke.py',
 'integration/tools/acceptance/v3_smoke.py',
 'integration/tools/acceptance/multidex_smoke.py',
 'integration/tools/acceptance/stable_smoke.py',
 'integration/tools/acceptance/singleton_smoke.py',
]

def sha(path):
 return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def snapshot_inputs(root):
 return {name:sha(Path(root)/name) for name in TOOL_FILES}

def write_result(path, value):
 path=Path(path);temporary=path.with_name(path.name+'.'+secrets.token_hex(6)+'.tmp')
 try:
  temporary.write_text(json.dumps(value,indent=2)+'\n');temporary.replace(path)
 finally:
  temporary.unlink(missing_ok=True)

def current_source_hash():
 from rayclmm_dynamic_evidence import source_hash
 return source_hash()

def require(condition, message):
 if not condition: raise ValueError(message)

def validate(root, verification, report, source_digest):
 root=Path(root);out=root/'integration/artifacts/local'
 require(report.get('passed') is True, 'acceptance report failed')
 require(verification['passed'] is True and verification['source_unchanged_during_verification'] is True, 'verification failed')
 require(report['source_tree_sha256']==verification['source_tree_sha256']==source_digest, 'source changed')
 require(report['verification_sha256']==sha(out/'verification.json'), 'verification changed')
 require(report['fresh_scope_reports']==SCOPE_REPORTS, 'scope set changed')
 require(set(report['scope_report_hashes'])=={n+'.json' for n in SCOPE_REPORTS}, 'scope binding changed')
 for name in SCOPE_REPORTS:
  path=out/(name+'.json');value=json.loads(path.read_text());measured=value.get('measured_at_unix_ms')
  require(value.get('passed') is True, 'failed scope '+name)
  require(isinstance(measured,int) and not isinstance(measured,bool) and measured>=verification['measured_at_unix_ms'], 'stale generation '+name)
  require(path.stat().st_mtime*1000>=verification['measured_at_unix_ms'], 'stale scope file '+name)
  require(sha(path)==report['scope_report_hashes'][path.name], 'scope changed '+name)
 require(set(report['tool_hashes'])==set(TOOL_FILES), 'tool binding changed')
 for name,value in report['tool_hashes'].items():require(sha(root/name)==value, 'tool changed '+name)
 process=json.loads((out/'receipt-root-three-service-smoke.json').read_text())
 require(process['input_hashes']==report['tool_hashes'], 'process inputs changed since execution')
 require(process['checks']==PROCESS_CHECKS and report['process_checks']==len(PROCESS_CHECKS), 'process coverage changed')
 require(process['tool_sha256']==sha(root/'integration/tools/acceptance/receipt_root_smoke.py'), 'process wrapper changed')
 require(process['harness_sha256']==sha(root/'integration/tools/acceptance/receipt_witness_smoke.py'), 'process harness changed')
 require(process['header_trust']=='RPC_ADVERTISED_NOT_INDEPENDENT_CONSENSUS', 'header trust overstated')
 reference=json.loads((root/'chain-parser-go/test/fixtures/receipt-roots.json').read_text())
 require(reference['generator_sha256']==sha(root/'integration/tools/reference/receipt-root/oracle.cjs'), 'reference generator changed')
 require(reference['package_lock_sha256']==sha(root/'integration/tools/reference/receipt-root/package-lock.json'), 'reference lock changed')
 require(len(reference['cases'])==report['reference_vectors']==12, 'reference vector count changed')
 review=json.loads((out/'receipt-root-review.json').read_text())
 require(sha(out/'receipt-root-review.json')==report['review_sha256'], 'review changed')
 require(review['source_tree_sha256']==source_digest, 'review belongs to previous source')
 require(review['input_hashes']==report['tool_hashes'], 'review inputs changed')
 for stage in ('spec_review','quality_review'):
  require(review[stage]['status']=='CLOSED' and all(f['status']=='CLOSED' for f in review[stage].get('findings',[])), 'open review '+stage)
 require(report['production_ready'] is False and report['full_work_packages_accepted']==0 and bool(report['limits']), 'scope overstated')
 return True

def evaluate():
 out=ROOT/'integration/artifacts/local';verification=json.loads((out/'verification.json').read_text())
 input_hashes=snapshot_inputs(ROOT)
 digest=current_source_hash()
 report={'passed':True,'source_tree_sha256':digest,'verification_sha256':sha(out/'verification.json'),
         'fresh_scope_reports':SCOPE_REPORTS,'scope_report_hashes':{n+'.json':sha(out/(n+'.json')) for n in SCOPE_REPORTS},
         'tool_hashes':input_hashes,'process_checks':len(PROCESS_CHECKS),'reference_vectors':12,
         'review_sha256':sha(out/'receipt-root-review.json'),'production_ready':False,'full_work_packages_accepted':0,
         'limits':['RPC-advertised receipt commitment only; independent header/consensus and transactions-root verification remain open.',
                   'Standard Ethereum receipt types 0..4 and legacy post-state only; chain-specific receipt envelopes fail closed.',
                   'Synthetic RPC sources and local PG/Kafka; no real deployment, provider-independence or VERIFIED quote claim.'],
         'measured_at_unix_ms':int(time.time()*1000)}
 validate(ROOT,verification,report,digest)
 require(current_source_hash()==digest, 'source changed during evidence check')
 check={'passed':True,'source_tree_sha256':digest,'scope_reports':len(SCOPE_REPORTS),'process_checks':len(PROCESS_CHECKS),'reference_vectors':12,'production_ready':False,'measured_at_unix_ms':int(time.time()*1000)}
 return report,check

def run():
 out=ROOT/'integration/artifacts/local';out.mkdir(parents=True,exist_ok=True)
 attempt=secrets.token_hex(12)
 pending={'passed':False,'status':'RUNNING','attempt_id':attempt,'production_ready':False,'measured_at_unix_ms':int(time.time()*1000)}
 paths=[out/'receipt-root-acceptance.json',out/'receipt-root-evidence-check.json']
 try:
  for path in paths:write_result(path,pending)
  report,check=evaluate();report['attempt_id']=attempt;check['attempt_id']=attempt
  write_result(paths[0],report);write_result(paths[1],check)
  return check
 except BaseException as exc:
  failed={**pending,'status':'FAILED','error_type':type(exc).__name__,'error':str(exc),'measured_at_unix_ms':int(time.time()*1000)}
  for path in paths:
   try:write_result(path,failed)
   except OSError:pass
  raise

if __name__=='__main__':print(json.dumps(run(),indent=2))
