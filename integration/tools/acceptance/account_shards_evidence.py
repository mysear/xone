#!/usr/bin/env python3
"""Fresh, scoped RPC-shard evidence; does not relabel historical full regressions."""
import hashlib
import json
from pathlib import Path
import secrets
import time
ROOT=Path(__file__).resolve().parents[3]
SCOPES=['three-service-smoke','raydium-three-service-smoke','orca-three-service-smoke','rayclmm-three-service-smoke','rayclmm-token-fee-three-service-smoke','rayclmm-bitmap-three-service-smoke','rayclmm-seed-three-service-smoke','rayclmm-dynamic-three-service-smoke','source-wal-three-service-smoke','account-shards-three-service-smoke']
PROCESS_CHECKS=[
 'sixteen-pools-134-accounts-in-two-shards-and-both-quote-directions-in-two-replicas',
 'new-current-bank-publishes-complete-scope-with-provider-context-source-label',
 'slot-mismatch-revokes-only-solana-and-restores-whole-new-generation',
 'clock-change-revokes-only-solana-and-restores-whole-new-generation',
 'short-response-revokes-only-solana-and-restores-whole-new-generation',
 'missing-dependency-revokes-only-solana-and-restores-whole-new-generation',
 'anchor-change-revokes-only-solana-and-restores-whole-new-generation',
 'aggregate-response-budget-revokes-only-solana-and-restores-whole-new-generation',
 'memory-only-router-restores-full-scope-from-owner-checkpoint',
 'parser-restart-verifies-wal-and-new-session-before-fresh-whole-scope',
]
BINARIES=['chain-parser-go/bin/chain-parser-go','chain-parser-go/bin/fixture-parser-go',
          'chain-parser-rust/target/debug/chain-parser-rust','chain-parser-rust/target/debug/fixture-parser-rust',
          'dex-router/target/debug/dex-router']
TOOLS=['integration/tools/acceptance/verify.py','integration/tools/acceptance/tests/test_account_shards_scope.py','integration/Makefile','integration/tools/ci/progress_bindings.py',
       'integration/test/solana/account-shards.json','integration/tools/acceptance/account_shards_smoke.py',
       'integration/tools/acceptance/account_shards_evidence.py',
       'integration/tools/acceptance/tests/test_account_shards_evidence.py',
       'integration/tools/acceptance/tests/test_account_shards_startup.py',
       'integration/tools/acceptance/infrastructure.py','integration/tools/acceptance/configuration.py',
       'integration/tools/acceptance/smoke.py','integration/test/evm/baseline.json',
       'integration/tools/acceptance/kafka_smoke.py','integration/tools/acceptance/reorg_smoke.py',
       'integration/tools/acceptance/receipt_reference.py','integration/test/contracts/receipt-root-fixtures.json',
       'integration/tools/acceptance/rayclmm_dynamic_evidence.py']

SCOPE_PRODUCERS=['smoke.py','raydium_smoke.py','orca_smoke.py','rayclmm_smoke.py',
                 'rayclmm_token_fee_smoke.py','rayclmm_bitmap_smoke.py','rayclmm_seed_smoke.py',
                 'rayclmm_dynamic_smoke.py','source_wal_smoke.py','account_shards_smoke.py']
TOOLS += ['integration/postgres/schema.sql','integration/test/contracts/orca-quotes.json']
TOOLS += ['integration/tools/acceptance/'+name for name in SCOPE_PRODUCERS]

def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def snapshot_inputs(root):
 root=Path(root);names=set(TOOLS)
 # Freeze all scope producers/helpers/tests and every checked-in integration
 # fixture, regardless of its extension. Reports and runtime outputs live elsewhere.
 for folder in ('integration/tools/acceptance','integration/test'):
  for path in (root/folder).rglob('*'):
   if not path.is_file()or '__pycache__'in path.parts or path.name=='.DS_Store':continue
   if folder.endswith('acceptance')and path.suffix!='.py':continue
   names.add(str(path.relative_to(root)))
 return {name:sha(root/name)for name in sorted(names)}
def runtime_fingerprint(root,digest):
 return {'source_tree_sha256':digest,'binaries':{name:sha(Path(root)/name)for name in BINARIES}}
def require(condition,message):
 if not condition:raise ValueError(message)
def write_result(path,value):
 path=Path(path);temporary=path.with_name(path.name+'.'+secrets.token_hex(6)+'.tmp')
 try:temporary.write_text(json.dumps(value,indent=2)+'\n');temporary.replace(path)
 finally:temporary.unlink(missing_ok=True)
def current_source_hash():
 from rayclmm_dynamic_evidence import source_hash
 return source_hash()

def validate(root,verification,report,digest):
 root=Path(root);out=root/'integration/artifacts/local'
 require(report.get('passed')is True and verification['passed']is True and verification['source_unchanged_during_verification']is True,'failed verification')
 require(digest==report['source_tree_sha256']==verification['source_tree_sha256'],'source changed')
 require(report['verification_sha256']==sha(out/'verification.json'),'verification replaced')
 require(report['fresh_scope_reports']==SCOPES and set(report['scope_hashes'])=={name+'.json'for name in SCOPES},'scope set changed')
 for name in SCOPES:
  path=out/(name+'.json');value=json.loads(path.read_text());stamp=value.get('measured_at_unix_ms')
  require(value.get('passed')is True and isinstance(stamp,int)and not isinstance(stamp,bool)and stamp>=verification['measured_at_unix_ms'],'failed/stale scope '+name)
  require(path.stat().st_mtime*1000>=verification['measured_at_unix_ms']and sha(path)==report['scope_hashes'][path.name],'changed scope '+name)
 inputs=snapshot_inputs(root);require(report['input_hashes']==inputs,'current inputs changed')
 process=json.loads((out/'account-shards-three-service-smoke.json').read_text())
 require(report['process_checks']==len(PROCESS_CHECKS),'process count changed')
 require(process['input_hashes']==inputs and process['checks']==PROCESS_CHECKS,'process execution inputs/coverage changed')
 require(process['pool_count']==16 and process['account_count']==134 and process['max_account_response_bytes']==1_000_000,'process scope changed')
 require(process['header_trust']=='PROVIDER_DECLARED_FINALIZED_CONTEXT_NOT_INDEPENDENT_BANK_PROOF','trust overstated')
 status=process['source_evidence_status']
 require(status['write_poisoned']is False and 0<status['retained_bytes']<=status['max_retained_bytes'],'unhealthy or unbounded WAL status')
 require(status['session_id']>=2 and status['records']>0 and status['evidence_only']is True and status['confers_readiness']is False,'fresh WAL recovery not proven')
 runtime=runtime_fingerprint(root,digest)
 require(process['runtime']==runtime,'execution-time service source/binary binding changed')
 binding_path=out/'account-shards-runtime-binding.json';binding=json.loads(binding_path.read_text())
 require(binding.get('passed')is True and binding['runtime']==runtime,'checked-build runtime binding changed')
 require(binding['input_hashes']==inputs,'runtime binding inputs changed')
 require(verification['measured_at_unix_ms']<=binding['measured_at_unix_ms']<=process['started_at_unix_ms']<=process['measured_at_unix_ms'],'runtime binding must precede process execution')
 require(binding['verification_sha256']==report['verification_sha256']and report['runtime_binding_sha256']==sha(binding_path),'runtime binding belongs to another verification')
 seal_path=out/'account-shards-scope-execution.json';seal=json.loads(seal_path.read_text())
 require(seal.get('passed')is True and seal['runtime_binding_sha256']==sha(binding_path),'scope execution seal belongs to another batch')
 require(report['scope_execution_sha256']==sha(seal_path)and seal['input_hashes']==inputs and seal['scope_hashes']==report['scope_hashes'],'scope execution inputs/reports changed')
 for name in SCOPES:
  value=json.loads((out/(name+'.json')).read_text())
  require(binding['measured_at_unix_ms']<=value['measured_at_unix_ms']<=seal['measured_at_unix_ms'],'scope not executed inside captured batch '+name)

 review=json.loads((out/'account-shards-review.json').read_text());require(report['review_sha256']==sha(out/'account-shards-review.json'),'review changed')
 require(review['source_tree_sha256']==digest and review['input_hashes']==inputs,'stale source/tools review')
 for stage in ('spec_review','quality_review'):
  require(review[stage]['status']=='CLOSED'and all(f['status']=='CLOSED'for f in review[stage].get('findings',[])),'review open '+stage)
 require(report['production_ready']is False and report['full_work_packages_accepted']==0 and bool(report['limits']),'acceptance overstated')
 return True

def evaluate():
 out=ROOT/'integration/artifacts/local';verification=json.loads((out/'verification.json').read_text());digest=current_source_hash()
 report={'passed':True,'source_tree_sha256':digest,'verification_sha256':sha(out/'verification.json'),
         'fresh_scope_reports':SCOPES,'scope_hashes':{n+'.json':sha(out/(n+'.json'))for n in SCOPES},
         'input_hashes':snapshot_inputs(ROOT),'review_sha256':sha(out/'account-shards-review.json'),
         'runtime_binding_sha256':sha(out/'account-shards-runtime-binding.json'),
         'scope_execution_sha256':sha(out/'account-shards-scope-execution.json'),
         'production_ready':False,'full_work_packages_accepted':0,'process_checks':10,
         'limits':['Synthetic RPC fixtures and local PG/Kafka only; no deployed-program/SBF/real-transaction acceptance.',
                   'Provider-declared current finalized bootstrap only, not historical account or independent bank proof.',
                   'At most32 shards,99 non-Clock dependencies each and35 RPC calls per provider attempt; whole-deadline and aggregate byte budget retained.',
                   'No Yellowstone/stream continuity/strict execution/complete EnsurePoolScope or production claim; WAL archival rotation remains open.'],
         'measured_at_unix_ms':int(time.time()*1000)}
 validate(ROOT,verification,report,digest);require(current_source_hash()==digest,'source changed during gate')
 check={'passed':True,'source_tree_sha256':digest,'fresh_scope_reports':len(SCOPES),'process_checks':10,'production_ready':False,'measured_at_unix_ms':int(time.time()*1000)}
 return report,check

def run():
 out=ROOT/'integration/artifacts/local';out.mkdir(parents=True,exist_ok=True)
 paths=[out/'account-shards-acceptance.json',out/'account-shards-evidence-check.json']
 pending={'passed':False,'status':'RUNNING','attempt_id':secrets.token_hex(12),'production_ready':False,'measured_at_unix_ms':int(time.time()*1000)}
 try:
  for path in paths:write_result(path,pending)
  report,check=evaluate();report['attempt_id']=check['attempt_id']=pending['attempt_id']
  write_result(paths[0],report);write_result(paths[1],check);return check
 except BaseException as exc:
  failed={**pending,'status':'FAILED','error_type':type(exc).__name__,'error':str(exc),'measured_at_unix_ms':int(time.time()*1000)}
  for path in paths:
   try:write_result(path,failed)
   except OSError:pass
  raise

def bind_runtime():
 out=ROOT/'integration/artifacts/local';out.mkdir(parents=True,exist_ok=True);path=out/'account-shards-runtime-binding.json'
 pending={'passed':False,'status':'RUNNING','production_ready':False,'attempt_id':secrets.token_hex(12),'measured_at_unix_ms':int(time.time()*1000)}
 try:
  write_result(path,pending);write_result(out/'account-shards-scope-execution.json',pending);verification=json.loads((out/'verification.json').read_text());digest=current_source_hash()
  require(verification['passed']is True and verification['source_unchanged_during_verification']is True and verification['source_tree_sha256']==digest,'checked build source differs')
  runtime=runtime_fingerprint(ROOT,digest);require(current_source_hash()==digest,'sources changed during runtime binding')
  result={**pending,'passed':True,'status':'CHECKED_BUILD','runtime':runtime,'verification_sha256':sha(out/'verification.json'),'input_hashes':snapshot_inputs(ROOT),'measured_at_unix_ms':int(time.time()*1000)}
  write_result(path,result);return {'passed':True,'source_tree_sha256':digest,'binaries':len(BINARIES)}
 except BaseException as exc:
  write_result(path,{**pending,'status':'FAILED','error_type':type(exc).__name__,'error':str(exc)});raise

def seal_scopes():
 out=ROOT/'integration/artifacts/local';path=out/'account-shards-scope-execution.json'
 pending={'passed':False,'status':'RUNNING','production_ready':False,'attempt_id':secrets.token_hex(12),'measured_at_unix_ms':int(time.time()*1000)}
 try:
  write_result(path,pending);binding_path=out/'account-shards-runtime-binding.json';binding=json.loads(binding_path.read_text())
  inputs=snapshot_inputs(ROOT);runtime=runtime_fingerprint(ROOT,current_source_hash())
  require(binding['passed']is True and binding['input_hashes']==inputs and binding['runtime']==runtime,'batch inputs/runtime drifted')
  hashes={}
  for name in SCOPES:
   scope=out/(name+'.json');value=json.loads(scope.read_text());stamp=value.get('measured_at_unix_ms')
   require(value.get('passed')is True and isinstance(stamp,int)and not isinstance(stamp,bool)and stamp>=binding['measured_at_unix_ms'],'scope not freshly executed '+name)
   require(scope.stat().st_mtime*1000>=binding['measured_at_unix_ms'],'scope file predates batch '+name)
   hashes[scope.name]=sha(scope)
  require(snapshot_inputs(ROOT)==inputs and runtime_fingerprint(ROOT,current_source_hash())==runtime,'inputs/runtime changed while sealing')
  result={**pending,'passed':True,'status':'SCOPES_COMPLETE','runtime_binding_sha256':sha(binding_path),'input_hashes':inputs,'scope_hashes':hashes,'measured_at_unix_ms':int(time.time()*1000)}
  write_result(path,result);return {'passed':True,'scoped_process_reports':len(SCOPES),'execution_inputs':len(inputs)}
 except BaseException as exc:
  write_result(path,{**pending,'status':'FAILED','error_type':type(exc).__name__,'error':str(exc)});raise

if __name__=='__main__':
 import sys
 operation=bind_runtime if sys.argv[1:]==['--bind-runtime']else seal_scopes if sys.argv[1:]==['--seal-scopes']else run
 print(json.dumps(operation(),indent=2))
