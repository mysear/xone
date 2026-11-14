#!/usr/bin/env python3
"""Same-source Raydium CLMM exact-output math, routing, API and browser verification."""
from pathlib import Path
import json,os,re,secrets,subprocess,sys,time
from validate_quote_execution import fingerprint
from validate_orca_exact_out import inputs as prior_inputs
from weighted_evidence import ROOT,OUT,sha,write_result
TARGET=OUT/'core-rayclmm-exact-out-validation-20261114.json'
TOOLS=['integration/tools/acceptance/validate_rayclmm_exact_out.py','integration/tools/acceptance/rayclmm_exact_out_reference.py','integration/tools/acceptance/verify_rayclmm_exact_out.py','integration/tools/acceptance/verify_rayclmm_exact_out_api.py','integration/tools/acceptance/tests/test_rayclmm_exact_out_reference.py','integration/test/contracts/rayclmm-exact-out.json','dex-router/test/contracts/rayclmm-exact-out.json','integration/test/contracts/rayclmm-exact-out-frames.json','chain-parser-rust/test/contracts/rayclmm-exact-out-frames.json','integration/artifacts/local/rayclmm-exact-out-original-20261114.json','integration/tools/reference/solana-advanced/README.md','integration/tools/reference/solana-advanced/raydium-clmm/Cargo.toml','integration/tools/reference/solana-advanced/raydium-clmm/Cargo.lock','integration/tools/reference/solana-advanced/raydium-clmm/cargo-source.toml','integration/tools/reference/solana-advanced/raydium-clmm/src/lib.rs','integration/tools/reference/solana-advanced/raydium-clmm/src/main.rs','integration/tools/reference/solana-advanced/raydium-clmm/src/seed.rs','integration/tools/reference/solana-advanced/raydium-clmm/src/bitmap.rs','integration/tools/reference/solana-advanced/raydium-clmm/src/dynamic.rs','integration/tools/reference/raydium-clmm/source-metadata.json']
def inputs():
 result=prior_inputs();paths=list(TOOLS);base='integration/tools/reference/raydium-clmm/';metadata=json.loads((ROOT/(base+'source-metadata.json')).read_bytes())
 for archive in metadata['archives']:paths += [base+archive['tree']+'/'+item['path']for item in archive['files']]
 for service in ['integration','dex-router','chain-parser-rust']:
  paths.append(service+'/test/contracts/solana-advanced/raydium-clmm-rewards.json')
  for name in ['rayclmm-quotes.json','rayclmm-token-fee-quotes.json','rayclmm-bitmap-quotes.json','rayclmm-seed-quotes.json','rayclmm-dynamic-quotes.json']:
   path=service+'/test/contracts/'+name
   if(ROOT/path).is_file():paths.append(path)
 result.update({p:sha(ROOT/p)for p in paths});binary=Path('/tmp/xone-solana-advanced-ray/debug/solana-advanced-raydium-clmm-reference');result[str(binary)]=sha(binary);return result

def main():
 env=dict(os.environ,PATH=str(Path.home()/'.cargo/bin')+os.pathsep+os.environ['PATH'],CARGO_BUILD_JOBS='2');env.setdefault('RUST_TEST_THREADS','2')
 env.setdefault('XONE_SOLANA_PYTHON_PATH',str(ROOT/'integration/artifacts/local/r01-solana-rebuild/python'))
 bundled=Path.home()/'.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright'
 if 'PLAYWRIGHT_MODULE'not in env and bundled.exists():env['PLAYWRIGHT_MODULE']=str(bundled)
 tag=secrets.token_hex(6);browser=OUT/f'rayclmm-exact-out-browser-{tag}';env['XONE_BROWSER_OUTPUT']=str(browser);env['XONE_RAYCLMM_EXACT_OUT_DIR']=str(browser/'rayclmm-api');env['XONE_TEST_RPC_CAPTURE']='1';env['XONE_TEST_SUBMISSION_DIR']=str(browser/'receipts');env['XONE_TEST_SBF_MANIFEST_DIR']=str(ROOT/'dex-router/test/contracts/sbf-runtime')
 cargo=['cargo','+1.90.0'];manifest=['--locked','--manifest-path','dex-router/Cargo.toml'];py=[sys.executable]
 legacy=['rayclmm_steps','rayclmm_quotes','rayclmm_differential','rayclmm_token_fee','rayclmm_token_fee_differential','rayclmm_bitmap_differential','rayclmm_bitmap_quotes','rayclmm_seed_differential','rayclmm_dynamic_differential','rayclmm_dynamic','rayclmm_dynamic_helpers','solana_advanced_rewards','orca_exact_out','capabilities']
 checks=[
 ('parser-format',cargo+['fmt','--all','--check','--manifest-path','chain-parser-rust/Cargo.toml']),
 ('format',cargo+['fmt','--all','--check','--manifest-path','dex-router/Cargo.toml']),
 ('contracts',py+['integration/tools/ci/contracts.py']),('protocol',py+['common/protoc/check.py']),
 ('api-lint',cargo+['clippy']+manifest+['-p','xone-quote-api','--lib','--test','rayclmm_exact_out','--example','frontend_harness','--','-D','warnings']),
 ('core-lint',cargo+['clippy']+manifest+['-p','xone-protocol-math','-p','xone-routing','-p','xone-optimizer','-p','xone-execution-plan','--lib','--test','rayclmm_exact_out','--','-D','warnings']),
 ('build',cargo+['build']+manifest+['-p','xone-quote-api','--example','frontend_harness']),
 ('browser',['node','frontend/test/browser.mjs']),
 ('native-math-routes-grid',cargo+['test']+manifest+['-p','xone-protocol-math','-p','xone-routing','-p','xone-optimizer','--test','rayclmm_exact_out']),
 ('legacy-ray-and-orca',cargo+['test']+manifest+['-p','xone-protocol-math']+sum((['--test',name]for name in legacy),[])),
 ('api',cargo+['test']+manifest+['-p','xone-quote-api','--test','rayclmm_exact_out']),
 ('legacy-api-exact-out',cargo+['test']+manifest+['-p','xone-quote-api','--test','exact_out']),
 ('legacy-exact-out',cargo+['test']+manifest+['-p','xone-protocol-math','-p','xone-routing','-p','xone-optimizer','--test','exact_out']),
 ('plan-regression',cargo+['test']+manifest+['-p','xone-execution-plan','--test','plan','--test','v2_complete']),
 ('parser',cargo+['test','--locked','--manifest-path','chain-parser-rust/Cargo.toml','-p','xone-solana-ingest','--test','rayclmm_exact_out','--test','rayclmm_decode','--test','rayclmm_bitmap','--test','rayclmm_seed','--test','rayclmm_dynamic']),
 ('parser-lint',cargo+['clippy','--locked','--manifest-path','chain-parser-rust/Cargo.toml','-p','xone-solana-ingest','--lib','--test','rayclmm_exact_out','--','-D','warnings']),
 ('native-replay',py+['integration/tools/acceptance/verify_rayclmm_exact_out.py','--reexecute','--output',str(browser/'rayclmm-native-replay.json')]),
 ('actual-api-evidence',py+['integration/tools/acceptance/verify_rayclmm_exact_out_api.py',str(browser/'rayclmm-api'),'--output',str(browser/'rayclmm-api-verification.json')]),
 ('native-controls',py+['-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_rayclmm_exact_out_reference.py']),
 ('native-controls-optimized',py+['-OO','-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_rayclmm_exact_out_reference.py']),
 ('static-source-projection',py+['integration/tools/ci/solana-advanced-golden.py','--family','raydium-clmm','--check']),

 ('frontend-unit',['node','--test','frontend/test/model.test.js']),('wallet-unit',['node','--test','frontend/test/wallet.test.js']),
 ('frontend-syntax',['node','--check','frontend/src/app.js']),('wallet-syntax',['node','--check','frontend/src/wallet.js']),
 ('progress',py+['-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py']),
 ('progress-optimized',py+['-OO','-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py'])]
 if TARGET.exists():
  previous=OUT/f'rayclmm-exact-out-prior-{sha(TARGET)}.json'
  if not previous.exists():previous.write_bytes(TARGET.read_bytes())
 source=fingerprint();helper=inputs();report=dict(passed=False,state='RUNNING',source_tree_sha256=source,helper_sha256=helper,checks=[],all_requested_features_complete=False,production_ready=False);write_result(TARGET,report)
 for name,command in checks:
  log=OUT/f'rayclmm-exact-out-{tag}-{name}.log';start=time.monotonic()
  with log.open('w')as output:run=subprocess.run(command,cwd=ROOT,env=env,stdout=output,stderr=subprocess.STDOUT)
  report['checks'].append(dict(name=name,command=command,exit_code=run.returncode,duration_ms=round((time.monotonic()-start)*1000),rust_test_executions=sum(map(int,re.findall(r'test result: ok\. (\d+) passed;',log.read_text(errors='replace')))),log=dict(path=str(log.relative_to(ROOT)),sha256=sha(log))));write_result(TARGET,report);print(json.dumps(dict(check=name,exit_code=run.returncode)),flush=True)
  if run.returncode:report['state']='FAILED';write_result(TARGET,report);raise RuntimeError('RAYCLMM_EXACT_OUT_CHECK_FAILED:'+name)
 result=json.loads((browser/'result.json').read_bytes());catalog=result['metadata']['protocolCatalog'];require_counts=(len(catalog),sum('EXACT_OUT'in c['quoteModes']for c in catalog.values()),sum(c['executionPreparation']for c in catalog.values()))
 if not result['passed']or fingerprint()!=source or inputs()!=helper or require_counts!=(61,27,10):report.update(state='FAILED',error='SOURCE_HELPER_BROWSER_OR_MODEL_SCOPE_CHANGED');write_result(TARGET,report);raise ValueError(report['error'])
 required=['rayclmm-native-replay.json','rayclmm-api-verification.json','quotes.json','mobile.png']+[name+'-exact-out.png'for name in ['raydium-clmm-static','raydium-clmm-rewards','raydium-clmm-token-fee','raydium-clmm-bitmap','raydium-clmm-custom-seed','raydium-clmm-dynamic']]
 if any(not(browser/p).is_file()for p in required):raise ValueError('RAYCLMM_ARTIFACT_MISSING')
 artifacts=[p for p in browser.iterdir()if p.is_file()and p.name!='harness-info.json'and p.suffix in ['.json','.png','.log']]+list((browser/'rayclmm-api').glob('*.json'))+list((browser/'receipts').glob('*.ndjson'))
 report.update(passed=True,state='LOCAL_SCOPE_VERIFIED',source_tree_sha256_after=fingerprint(),browser_checks=len(result['checks']),rust_test_executions=sum(c['rust_test_executions']for c in report['checks']),protocol_models=61,exact_out_models=27,preparation_models=10,artifacts=[dict(path=str(p.relative_to(ROOT)),sha256=sha(p))for p in sorted(artifacts)],harness_binary_sha256=sha(ROOT/'dex-router/target/debug/examples/frontend_harness'));write_result(TARGET,report);print(json.dumps({k:report[k]for k in ['passed','state','rust_test_executions','browser_checks','exact_out_models','preparation_models']}))
if __name__=='__main__':main()
