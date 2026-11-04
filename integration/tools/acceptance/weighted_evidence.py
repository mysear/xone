#!/usr/bin/env python3
"""Fail-closed Weighted local evidence with pre-execution inputs and binaries."""
from pathlib import Path
import hashlib,json,os,secrets,shutil,time
ROOT=Path(__file__).resolve().parents[3];OUT=ROOT/'integration/artifacts/local'
BINARIES=['chain-parser-go/bin/chain-parser-go','chain-parser-go/bin/fixture-parser-go','chain-parser-rust/target/debug/chain-parser-rust','chain-parser-rust/target/debug/fixture-parser-rust','dex-router/target/debug/dex-router']
SCOPES=['three-service-smoke.json','stable-three-service-smoke.json','weighted-three-service-smoke.json']
def require(value,message):
 if not value:raise ValueError(message)
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def write_result(p,v):
 p=Path(p);p.parent.mkdir(parents=True,exist_ok=True);tmp=p.with_name(p.name+'.'+secrets.token_hex(6)+'.tmp')
 try:tmp.write_text(json.dumps(v,indent=2)+'\n');tmp.replace(p)
 finally:tmp.unlink(missing_ok=True)
def source_hash():
 h=hashlib.sha256()
 for project in ['chain-parser-go','chain-parser-rust','dex-router','common/protoc']:
  files=[]
  for directory,dirs,names in os.walk(ROOT/project):
   dirs[:]=[d for d in dirs if d not in {'target','bin','.runtime','secrets','.git','__pycache__'}]
   for name in names:
    p=Path(directory)/name
    if p.suffix in ['.go','.rs','.toml','.mod','.sum','.lock','.proto','.yaml','.json','.py'] or name=='Makefile':files.append(p)
  for p in sorted(files):h.update(str(p.relative_to(ROOT)).encode());h.update(p.read_bytes())
 return h.hexdigest()

def snapshot_inputs():
 paths={ROOT/'integration/Makefile',ROOT/'integration/postgres/schema.sql'}
 for folder in ['integration/tools/acceptance','integration/tools/ci','integration/tools/reference/weighted-model','integration/test']:
  for p in (ROOT/folder).rglob('*'):
   if p.is_file() and '__pycache__' not in p.parts and p.name!='.DS_Store':paths.add(p)
 # The oracle uses the existing isolated, lock-pinned Solidity runtime.
 paths.update([ROOT/'integration/tools/reference/stable-models/package.json',ROOT/'integration/tools/reference/stable-models/package-lock.json'])
 # Additional model inputs and original native reference binaries participate
 # in the same pre-execution commitment; never traverse build directories.
 for base in ['balancer-multi','curve-models','curve-ng-state','curve-meta','balancer-managed-fees','singleton-advanced','solana-advanced','curve-crypto-full','orca-adaptive','infinity-bin-multi','orca-adaptive-swap','orca-adaptive-service','infinity-bin-service','infinity-bin-detached','curve-crypto-ramp','orca-adaptive-rewards-service','orca-adaptive-dynamic-service','evm-execution','v2-complete']:
  folder=ROOT/'integration/tools/reference'/base
  for directory,dirs,names in os.walk(folder):
   dirs[:]=[d for d in dirs if d not in {'target','.git','__pycache__','node_modules','.runtime'}]
   for name in names:
    p=Path(directory)/name
    if base=='v2-complete' and name in {'verified.json','file-manifest.json'}:continue
    if base=='curve-crypto-ramp' and name=='verification.json':continue
    if base in {'orca-adaptive-rewards-service','orca-adaptive-dynamic-service'} and name in {'receipt.json','guards-receipt.json'}:continue
    if base in {'infinity-bin-service','infinity-bin-detached'} and name.startswith('fault-') and name.endswith('-receipt.json'):continue
    if base=='infinity-bin-detached' and name in {'check-receipt.json','check-node-receipt.json','guards-receipt.json','guards-optimized-receipt.json','guards-node-receipt.json','guard-node-receipt.json','receipt-test-receipt.json','manager-limit-receipt.json'}:continue
    if base=='infinity-bin-service' and name in {'check-receipt.json','check-node-receipt.json','guards-receipt.json','guards-optimized-receipt.json','guards-node-receipt.json','guard-node-receipt.json','replay-receipt.json','replay-test-receipt.json','receipt-test-receipt.json'}:continue
    if p.suffix in ['.rs','.sol','.vy','.toml','.lock','.json','.py','.cjs','.md','.txt','.hex','.ts','.js'] and name!='build.log':paths.add(p)
 # Commit exactly the original compiler/Node dependencies used by NG generation.
 closure=ROOT/'integration/tools/reference/curve-ng-state/execution-closure.json'
 if closure.is_file():
  ng=json.loads(closure.read_text());node=ROOT/'integration/.runtime/stable-reference/node_modules';compiler=ROOT/'integration/tools/reference/curve-models/.runtime/vyper-0.3.10-py313'
  paths.add(node.parent/'package-lock.json');paths.add(compiler/'bin/vyper')
  node_executable=shutil.which('node')
  require(node_executable is not None,'NG original Node runtime missing')
  paths.add(Path(node_executable))
  paths.update(node/item['path'] for item in ng['node_modules'])
  paths.update(compiler/'lib/python3.13/site-packages'/item['path'] for item in ng['compiler_packages'])
 meta_closure=ROOT/'integration/tools/reference/curve-meta/generation.json'
 if meta_closure.is_file():
  meta=json.loads(meta_closure.read_text())
  require(meta.get('generation_complete') is True,'Meta original closure incomplete')
  paths.update(ROOT/item['path'] for item in meta['files'])
  for name in ['compiler_executable','python_executable','node_executable','node_lock','javascript_compiler']:
   paths.add(Path(meta[name]['path']))
  paths.update(Path(meta['node_modules_root'])/item['path'] for item in meta['node_modules'])
  paths.update(Path(item['path']) for item in meta['compiler_packages'])
 managed_closure=ROOT/'integration/tools/reference/balancer-managed-fees/generation.json'
 if managed_closure.is_file():
  managed=json.loads(managed_closure.read_text())
  require(managed.get('passed') is True and managed.get('service_admission') is False,'Managed original reference closure incomplete')
  for relative in managed['source_and_dependency_sha256']:
   path=ROOT/relative
   require(path.resolve().is_relative_to(ROOT.resolve()),'Managed dependency escapes workspace')
   paths.add(path)
 # Bin reference/compiler artifacts bind their exact loaded JavaScript and native
 # executable closure; mutable derived check receipts are output, not inputs.
 for family in ['infinity-bin-service','infinity-bin-detached']:
  metadata=ROOT/'integration/tools/reference'/family/'source-metadata.json'
  if metadata.is_file():
   reference=json.loads(metadata.read_text())
   for entry in reference.get('loaded_dependencies',[])+reference.get('binary_dependencies',[]):
    path=Path(entry['path']);paths.add(path if path.is_absolute()else ROOT/path)
   manifest=ROOT/'integration/.runtime/singleton-reference/package-lock.json'
   if manifest.is_file():paths.add(manifest)
   for entry in reference.get('files',[]):paths.add(ROOT/'integration/.runtime/singleton-reference/sources'/entry['local'])
 # Public Curve ramp original reference dependency closure.
 ramp_metadata=ROOT/'integration/tools/reference/curve-crypto-ramp/runtime-metadata.json'
 if ramp_metadata.is_file():
  reference=json.loads(ramp_metadata.read_text())
  for path in reference.get('input_inventory',{}):paths.add(Path(path))
 # Published original V2 artifacts, native execution scripts and exact Node closure.
 evm_fixture=ROOT/'integration/test/contracts/evm-execution.json'
 if evm_fixture.is_file():
  evm=json.loads(evm_fixture.read_text())
  paths.add(ROOT/'dex-router/test/contracts/evm-execution.json')
  for item in evm['reference']['scripts']:
   declared=Path(item['path']);paths.add(declared if declared.is_absolute() else ROOT/declared)
  for item in evm['reference']['loaded_dependencies']:paths.add(ROOT/item['path'])
  paths.add(Path(evm['reference']['node']['executable']))
  pins=ROOT/'integration/tools/reference/evm-execution/source-pins.json'
  for package in json.loads(pins.read_text())['packages']:
   for item in package['files']:paths.add(pins.parent/item['path'])
 # Complete V2 retains all compressed opcode observations, exact native scripts,
 # packages (including extensionless licenses), and both service-local copies.
 complete_fixture=ROOT/'integration/test/contracts/v2-complete.json'
 if complete_fixture.is_file():
  complete=json.loads(complete_fixture.read_text())
  paths.update(ROOT/service/'test/contracts/v2-complete.json' for service in ['chain-parser-go','dex-router'])
  for item in complete['reference']['scripts']+complete['reference']['loaded_dependencies']:
   declared=Path(item['path']);paths.add(declared if declared.is_absolute() else ROOT/declared)
  paths.add(Path(complete['reference']['node']['executable']))
  for transaction in complete['reference']['transactions']:
   filename=transaction['trace']['file']
   require(Path(filename).name==filename and filename.endswith('.json.gz'),'invalid complete V2 trace input path')
   paths.add(ROOT/'integration/tools/reference/v2-complete/traces'/filename)
  for package in complete['source_pins']['packages']:
   for item in package['files']:paths.add(ROOT/'integration/tools/reference/evm-execution'/item['path'])
 # Published Managed V2 binds its complete original build/compiler and loaded
 # execution dependency closure. Generated success receipts are outputs only.
 managed_v2_fixture=ROOT/'integration/test/contracts/managed-v2-aum.json'
 if managed_v2_fixture.is_file():
  managed_v2=json.loads(managed_v2_fixture.read_text())
  paths.add(ROOT/'integration/tools/reference/balancer-managed-v2-aum/file-manifest.json')
  paths.update(ROOT/service/'test/contracts/managed-v2-aum-paused-vault.json' for service in ['integration','chain-parser-go','dex-router'])
  paths.update(ROOT/service/'test/contracts/managed-v2-aum-recovery.json' for service in ['integration','chain-parser-go','dex-router'])
  for item in managed_v2['reference']['inputs']['files']+managed_v2['reference']['node_closure']['files']:
   declared=Path(item['path'])
   require(not declared.is_absolute() and '..' not in declared.parts,'Managed V2 input path escapes workspace')
   paths.add(ROOT/declared)
  for service in ['integration','chain-parser-go','dex-router']:
   paths.update(ROOT/service/'test/contracts'/name for name in ['managed-v2-aum.json','managed-v2-aum-pools.json'])
  node_executable=shutil.which('node')
  require(node_executable is not None,'Managed V2 original Node runtime missing')
  paths.add(Path(node_executable).resolve())
  managed_wire=ROOT/'integration/test/contracts/managed-v2-aum-wire.json'
  if managed_wire.is_file():
   paths.add(Path(shutil.which('protoc')).resolve())
   paths.update(ROOT/service/'test/contracts/managed-v2-aum-wire.json' for service in ['integration','chain-parser-go','chain-parser-rust','dex-router'])
 # Current-date catalog and shared-Vault references bind their own producer
 # closures as inputs, separately from mutable original verifier outputs.
 catalog_file=ROOT/'integration/test/contracts/managed-v2-aum-catalog.json'
 if catalog_file.is_file():
  catalog=json.loads(catalog_file.read_bytes())
  for item in catalog['source_preservation']['before']:
   declared=Path(item['path']);require(not declared.is_absolute() and '..' not in declared.parts,'catalog input escapes workspace');paths.add(ROOT/declared)
  paths.update(ROOT/service/'test/contracts/managed-v2-aum-catalog.json' for service in ['integration','chain-parser-go','dex-router'])
  paths.update(ROOT/service/'test/contracts/managed-v2-aum-catalog-pools.json' for service in ['integration','dex-router'])
 shared_file=ROOT/'integration/test/contracts/managed-v2-shared-vault.json'
 if shared_file.is_file():
  shared=json.loads(shared_file.read_bytes())
  for item in shared['script_closure']+shared['source_preservation']['before']:
   declared=Path(item['path']);require(not declared.is_absolute() and '..' not in declared.parts,'shared Vault input escapes workspace');paths.add(ROOT/declared)
  paths.update(ROOT/service/'test/contracts/managed-v2-shared-vault.json' for service in ['integration','chain-parser-go','chain-parser-rust','dex-router'])
 # The independent C++ wire encoder and its thin service-local vectors are
 # inputs, distinct from the Node execution fixture and mutable verifier output.
 complete_wire=ROOT/'integration/test/contracts/v2-complete-wire.json'
 if complete_wire.is_file():
  wire=json.loads(complete_wire.read_text())
  paths.add(Path(wire['protoc']['executable']))
  paths.update(ROOT/service/'test/contracts/v2-complete-wire.json' for service in ['chain-parser-go','chain-parser-rust','dex-router'])
 # Bind the exact original Orca sources and both newly executed native oracles.
 orca_metadata=ROOT/'integration/tools/reference/orca-whirlpools/source-metadata.json'
 if orca_metadata.is_file():
  original=json.loads(orca_metadata.read_text())
  paths.add(orca_metadata)
  paths.update(ROOT/'integration/tools/reference/orca-whirlpools/upstream'/item['path'] for item in original['files'])
 for native in [Path('/tmp/xone-orca-adaptive-native/debug/orca-adaptive-reference'),Path('/tmp/xone-orca-adaptive-swap-native/debug/orca-adaptive-swap-reference'),Path('/tmp/xone-orca-adaptive-swap-native/debug/orca-adaptive-service-reference'),Path('/tmp/xone-orca-adaptive-swap-native/debug/orca-adaptive-rewards-service-reference'),Path('/tmp/xone-orca-adaptive-swap-native/debug/orca-adaptive-dynamic-service-reference')]:
  if native.is_file():paths.add(native)
 for path in [ROOT/'integration/tools/reference/orca-whirlpools/target/release/solana-advanced-orca-reference',Path('/tmp/xone-solana-advanced-ray/debug/solana-advanced-raydium-clmm-reference')]:
  if path.is_file():paths.add(path)
 def key(path):
  try:return str(path.relative_to(ROOT))
  except ValueError:return str(path)
 return {key(p):sha(p) for p in sorted(paths)}
def oracle_runtime_hash():
 h=hashlib.sha256();base=ROOT/'integration/.runtime/stable-reference/node_modules'
 def walk(folder):
  for p in sorted(folder.iterdir()):
   if p.is_symlink():continue
   if p.is_dir():walk(p)
   elif p.is_file() and p.suffix in ['.js','.cjs','.json','.wasm','.node']:h.update(str(p.relative_to(base)).encode());h.update(p.read_bytes())
 walk(base);return h.hexdigest()
def runtime_fingerprint():return {'source_tree_sha256':source_hash(),'binaries':{name:sha(ROOT/name) for name in BINARIES}}
def load(name):return json.loads((OUT/name).read_text())
def checked_runtime():
 v=load('verification.json');require(v['passed'] is True and v['source_unchanged_during_verification'] is True,'checked build failed');runtime=runtime_fingerprint()
 require(runtime['source_tree_sha256']==v['source_tree_sha256'],'source changed since checks')
 require(runtime['binaries']==v['checked_binary_sha256'],'binary changed since checked builds')
 require(v['tool_sha256']==sha(ROOT/'integration/tools/acceptance/verify.py'),'verification producer changed')
 return v,runtime

def bind_bundle(prefix):
 target=OUT/(prefix+'-runtime-binding.json');write_result(target,{'passed':False,'state':'RUNNING'})
 try:
  v,runtime=checked_runtime()
  value={'passed':True,'attempt_id':secrets.token_hex(12),'runtime':runtime,'input_hashes':snapshot_inputs(),'verification_sha256':sha(OUT/'verification.json'),'measured_at_unix_ms':int(time.time()*1000)};write_result(target,value);return value
 except BaseException as e:write_result(target,{'passed':False,'error':str(e)});raise

def seal_bundle(prefix,scopes):
 target=OUT/(prefix+'-scope-execution.json');write_result(target,{'passed':False,'state':'RUNNING'})
 try:
  binding=load(prefix+'-runtime-binding.json');require(binding['passed'] is True,'runtime not bound');require(binding['runtime']==runtime_fingerprint() and binding['input_hashes']==snapshot_inputs(),'inputs/runtime changed')
  binding_hash=sha(OUT/(prefix+'-runtime-binding.json'));stamps=[]
  for name in scopes:
   r=load(name)
   require(r['runtime']==binding['runtime'] and r['input_hashes']==binding['input_hashes'] and r['execution_binding_sha256']==binding_hash,'scope not executed against bound inputs/build '+name)
   start=r['started_at_unix_ms'];end=r['measured_at_unix_ms']
   require(r['passed'] is True and isinstance(start,int) and not isinstance(start,bool) and isinstance(end,int) and not isinstance(end,bool) and binding['measured_at_unix_ms']<=start<=end,'failed/stale scope '+name);stamps.append(end)
  value={'passed':True,'attempt_id':binding['attempt_id'],'runtime_binding_sha256':binding_hash,'runtime':binding['runtime'],'input_hashes':binding['input_hashes'],'scope_hashes':{n:sha(OUT/n) for n in scopes},'measured_at_unix_ms':max(int(time.time()*1000),*stamps)};write_result(target,value);return value
 except BaseException as e:write_result(target,{'passed':False,'error':str(e)});raise

def validate_bundle(prefix,scopes):
 v,runtime=checked_runtime();inputs=snapshot_inputs();b=load(prefix+'-runtime-binding.json');s=load(prefix+'-scope-execution.json');binding_hash=sha(OUT/(prefix+'-runtime-binding.json'))
 require(b['passed'] is True and b['runtime']==runtime and b['input_hashes']==inputs and b['verification_sha256']==sha(OUT/'verification.json'),'binding changed')
 require(s['passed'] is True and s['runtime']==runtime and s['input_hashes']==inputs and s['attempt_id']==b['attempt_id'] and s['runtime_binding_sha256']==binding_hash,'seal changed')
 require(s['scope_hashes']=={n:sha(OUT/n) for n in scopes},'scope report replaced')
 require(v['measured_at_unix_ms']<=b['measured_at_unix_ms']<=s['measured_at_unix_ms'],'execution chronology')
 for name in scopes:
  r=load(name);start=r['started_at_unix_ms'];end=r['measured_at_unix_ms']
  require(r['passed'] is True and r['runtime']==runtime and r['input_hashes']==inputs and r['execution_binding_sha256']==binding_hash and isinstance(start,int) and not isinstance(start,bool) and isinstance(end,int) and not isinstance(end,bool) and b['measured_at_unix_ms']<=start<=end<=s['measured_at_unix_ms'],'failed/stale scope '+name)
 return runtime,inputs

def bind_runtime():return bind_bundle('weighted')
def seal_scopes():return seal_bundle('weighted',SCOPES)

def validate():
 from weighted_smoke import PROCESS_CHECKS
 v=load('verification.json');b=load('weighted-runtime-binding.json');s=load('weighted-scope-execution.json');p=load('weighted-three-service-smoke.json');review=load('weighted-review.json');runtime=runtime_fingerprint();inputs=snapshot_inputs()
 require(v['passed'] is True and v['source_unchanged_during_verification'] is True and v['source_tree_sha256']==runtime['source_tree_sha256'] and v['checked_binary_sha256']==runtime['binaries'] and v['tool_sha256']==sha(ROOT/'integration/tools/acceptance/verify.py'),'failed/current source/binary verification')
 require(b['passed'] is True and b['runtime']==runtime and b['input_hashes']==inputs and b['verification_sha256']==sha(OUT/'verification.json'),'binding changed')
 require(s['passed'] is True and s['runtime']==runtime and s['input_hashes']==inputs and s['attempt_id']==b['attempt_id'] and s['runtime_binding_sha256']==sha(OUT/'weighted-runtime-binding.json'),'seal changed')
 require(s['scope_hashes']=={n:sha(OUT/n) for n in SCOPES},'scope report replaced')
 for name in SCOPES:
  r=load(name);require(r['passed'] is True and r['runtime']==runtime and r['input_hashes']==inputs and r['execution_binding_sha256']==sha(OUT/'weighted-runtime-binding.json') and b['measured_at_unix_ms']<=r['started_at_unix_ms']<=r['measured_at_unix_ms']<=s['measured_at_unix_ms'],'failed/stale scope '+name)
 require(p['runtime']==runtime and p['input_hashes']==inputs and p['checks']==PROCESS_CHECKS and p['pool_count']==3,'process scope changed')
 require(v['measured_at_unix_ms']<=b['measured_at_unix_ms']<=p['started_at_unix_ms']<=p['measured_at_unix_ms'],'execution chronology')
 require(p['trust']=='SYNTHETIC_RPC_NOT_DEPLOYMENT_PROOF' and p['production_ready'] is False,'trust overstated')
 generation=load('weighted-golden-generation.json');corpus=ROOT/'integration/test/contracts/weighted-quotes.json';data=json.loads(corpus.read_text())
 require(generation['passed'] is True and generation['corpus_sha256']==sha(corpus) and generation['cases']==len(data['cases'])==765 and generation['sequences']==len(data['sequences'])==45 and generation['steps']==180,'oracle corpus mismatch')
 require(generation['runtime_sha256']==oracle_runtime_hash(),'oracle installed runtime changed')
 require(generation['oracle_inputs']=={n:sha(ROOT/n) for n in ['integration/tools/ci/weighted-golden.cjs','integration/tools/reference/weighted-model/Harness.sol','integration/tools/reference/weighted-model/source-metadata.json','integration/tools/reference/stable-models/package.json','integration/tools/reference/stable-models/package-lock.json']},'oracle execution inputs changed')
 manifest=json.loads((ROOT/'integration/tools/reference/weighted-model/source-metadata.json').read_text())
 require(manifest['revision']==data['reference']['revision'] and len(manifest['files'])==data['reference']['source_files']==40,'reference source closure changed')
 for item in manifest['files']:require(sha(ROOT/'integration/.runtime/weighted-reference/sources'/item['path'])==item['sha256'],'reference source changed '+item['path'])
 require(generation['reference']==data['reference'] and generation['reference']['harness_sha256']==sha(ROOT/'integration/tools/reference/weighted-model/Harness.sol'),'oracle harness changed')
 require(review['source_tree_sha256']==runtime['source_tree_sha256'] and review['input_hashes']==inputs,'review stale')
 for stage in ['spec_review','quality_review']:require(review[stage]['status']=='CLOSED' and review[stage]['open_important_findings']==0,'open review '+stage)
 return {'passed':True,'production_ready':False,'source_tree_sha256':runtime['source_tree_sha256'],'input_hashes':inputs,'verification_sha256':sha(OUT/'verification.json'),'scope_execution_sha256':sha(OUT/'weighted-scope-execution.json'),'review_sha256':sha(OUT/'weighted-review.json'),'fresh_scope_reports':SCOPES,'math_cases':765,'sequences':45,'sequence_steps':180,'process_checks':len(PROCESS_CHECKS),'measured_at_unix_ms':int(time.time()*1000)}
def run():
 target=OUT/'weighted-acceptance.json';write_result(target,{'passed':False,'state':'RUNNING'})
 try:r=validate();write_result(target,r);return r
 except BaseException as e:write_result(target,{'passed':False,'error':str(e),'error_type':type(e).__name__});raise
if __name__=='__main__':
 import argparse
 a=argparse.ArgumentParser();a.add_argument('--bind-runtime',action='store_true');a.add_argument('--seal-scopes',action='store_true');args=a.parse_args();r=bind_runtime() if args.bind_runtime else seal_scopes() if args.seal_scopes else run();print(json.dumps({'passed':r['passed']}))
