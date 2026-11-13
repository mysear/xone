#!/usr/bin/env python3
"""Same-source Orca exact-output math, routing, API and browser verification."""
from pathlib import Path
import json,os,re,secrets,subprocess,sys,time
from validate_quote_execution import fingerprint
from validate_pump_amm_funding import inputs as prior_inputs
from weighted_evidence import ROOT,OUT,sha,write_result
TARGET=OUT/'core-orca-exact-out-validation-20261114.json'
TOOLS=['integration/tools/acceptance/validate_orca_exact_out.py','integration/tools/acceptance/orca_exact_out_reference.py','integration/tools/acceptance/verify_orca_exact_out.py','integration/tools/acceptance/verify_orca_exact_out_api.py','integration/tools/acceptance/tests/test_orca_exact_out_reference.py','integration/test/contracts/orca-exact-out.json','dex-router/test/contracts/orca-exact-out.json','integration/artifacts/local/orca-exact-out-original-20261114.json','integration/tools/reference/solana-advanced/README.md','integration/tools/reference/solana-advanced/orca/Cargo.toml','integration/tools/reference/solana-advanced/orca/Cargo.lock','integration/tools/reference/solana-advanced/orca/src/lib.rs','integration/tools/reference/solana-advanced/orca/src/main.rs','integration/tools/reference/orca-whirlpools/source-metadata.json','integration/tools/reference/orca-whirlpools/target/release/solana-advanced-orca-reference','integration/tools/ci/solana-advanced-golden.py']
def inputs():
 result=prior_inputs();paths=list(TOOLS)
 metadata=json.loads((ROOT/'integration/tools/reference/orca-whirlpools/source-metadata.json').read_bytes())
 paths += ['integration/tools/reference/orca-whirlpools/upstream/'+item['path']for item in metadata['files']]
 for stem in ['orca-adaptive-service','orca-adaptive-rewards-service','orca-adaptive-dynamic-service']:
  base='integration/tools/reference/'+stem+'/'
  paths += [base+p for p in ['Cargo.toml','Cargo.lock','README.md','identity.rs','build.rs','src/main.rs']]
  paths += ['integration/tools/ci/'+stem+'-golden.py','integration/tools/ci/'+stem+'-check.py','integration/tools/acceptance/'+stem.replace('-service','').replace('-','_')+'_fixture.py']
  for service in ['integration','chain-parser-rust','dex-router']:paths += [service+'/test/contracts/'+stem+'.json',service+'/test/contracts/'+stem.replace('-service','-pools')+'.json']
  binary=Path('/tmp/xone-orca-adaptive-swap-native/debug')/(stem+'-reference');result[str(binary)]=sha(binary)
 for service in ['integration','dex-router','chain-parser-rust']:paths.append(service+'/test/contracts/solana-advanced/orca-rewards.json')
 result.update({p:sha(ROOT/p)for p in paths});return result

def main():
 env=dict(os.environ,PATH=str(Path.home()/'.cargo/bin')+os.pathsep+os.environ['PATH'],CARGO_BUILD_JOBS='2');env.setdefault('RUST_TEST_THREADS','2')
 env.setdefault('XONE_SOLANA_PYTHON_PATH',str(ROOT/'integration/artifacts/local/r01-solana-rebuild/python'))
 bundled=Path.home()/'.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright'
 if 'PLAYWRIGHT_MODULE'not in env and bundled.exists():env['PLAYWRIGHT_MODULE']=str(bundled)
 tag=secrets.token_hex(6);browser=OUT/f'orca-exact-out-browser-{tag}';env['XONE_BROWSER_OUTPUT']=str(browser);env['XONE_ORCA_EXACT_OUT_DIR']=str(browser/'orca-api');env['XONE_TEST_RPC_CAPTURE']='1';env['XONE_TEST_SUBMISSION_DIR']=str(browser/'receipts');env['XONE_TEST_SBF_MANIFEST_DIR']=str(ROOT/'dex-router/test/contracts/sbf-runtime')
 cargo=['cargo','+1.90.0'];manifest=['--locked','--manifest-path','dex-router/Cargo.toml'];py=[sys.executable]
 legacy=['orca_steps','orca_quotes','orca_differential','orca_adaptive','orca_adaptive_swap','orca_adaptive_online','orca_adaptive_rewards','orca_adaptive_dynamic','orca_dispatch','capabilities']
 checks=[
 ('format',cargo+['fmt','--all','--check','--manifest-path','dex-router/Cargo.toml']),
 ('contracts',py+['integration/tools/ci/contracts.py']),('protocol',py+['common/protoc/check.py']),
 ('api-lint',cargo+['clippy']+manifest+['-p','xone-quote-api','--lib','--test','orca_exact_out','--example','frontend_harness','--','-D','warnings']),
 ('core-lint',cargo+['clippy']+manifest+['-p','xone-protocol-math','-p','xone-routing','-p','xone-optimizer','-p','xone-execution-plan','--lib','--test','orca_exact_out','--','-D','warnings']),
 ('build',cargo+['build']+manifest+['-p','xone-quote-api','--example','frontend_harness']),
 ('browser',['node','frontend/test/browser.mjs']),
 ('native-math-routes-grid',cargo+['test']+manifest+['-p','xone-protocol-math','-p','xone-routing','-p','xone-optimizer','--test','orca_exact_out']),
 ('legacy-orca',cargo+['test']+manifest+['-p','xone-protocol-math']+sum((['--test',name]for name in legacy),[])),
 ('api',cargo+['test']+manifest+['-p','xone-quote-api','--test','orca_exact_out']),
 ('legacy-api-exact-out',cargo+['test']+manifest+['-p','xone-quote-api','--test','exact_out']),
 ('legacy-exact-out',cargo+['test']+manifest+['-p','xone-protocol-math','-p','xone-routing','-p','xone-optimizer','--test','exact_out']),
 ('plan-regression',cargo+['test']+manifest+['-p','xone-execution-plan','--test','plan','--test','v2_complete']),
 ('parser',cargo+['test','--locked','--manifest-path','chain-parser-rust/Cargo.toml','-p','xone-solana-ingest','--test','orca_decode','--test','orca_adaptive','--test','orca_adaptive_rewards','--test','orca_adaptive_dynamic']),
 ('native-replay',py+['integration/tools/acceptance/verify_orca_exact_out.py','--reexecute','--output',str(browser/'orca-native-replay.json')]),
 ('actual-api-evidence',py+['integration/tools/acceptance/verify_orca_exact_out_api.py',str(browser/'orca-api'),'--output',str(browser/'orca-api-verification.json')]),
 ('native-controls',py+['-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_orca_exact_out_reference.py']),
 ('native-controls-optimized',py+['-OO','-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_orca_exact_out_reference.py']),
 ('static-source-projection',py+['integration/tools/ci/solana-advanced-golden.py','--family','orca','--check']),
 *[(stem+'-source',py+['integration/tools/ci/'+stem+'-golden.py','--check','--binary',str(Path('/tmp/xone-orca-adaptive-swap-native/debug')/(stem+'-reference'))])for stem in ['orca-adaptive-service','orca-adaptive-rewards-service','orca-adaptive-dynamic-service']],
 ('frontend-unit',['node','--test','frontend/test/model.test.js']),('wallet-unit',['node','--test','frontend/test/wallet.test.js']),
 ('frontend-syntax',['node','--check','frontend/src/app.js']),('wallet-syntax',['node','--check','frontend/src/wallet.js']),
 ('progress',py+['-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py']),
 ('progress-optimized',py+['-OO','-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py'])]
 if TARGET.exists():
  previous=OUT/f'orca-exact-out-prior-{sha(TARGET)}.json'
  if not previous.exists():previous.write_bytes(TARGET.read_bytes())
 source=fingerprint();helper=inputs();report=dict(passed=False,state='RUNNING',source_tree_sha256=source,helper_sha256=helper,checks=[],all_requested_features_complete=False,production_ready=False);write_result(TARGET,report)
 for name,command in checks:
  log=OUT/f'orca-exact-out-{tag}-{name}.log';start=time.monotonic()
  with log.open('w')as output:run=subprocess.run(command,cwd=ROOT,env=env,stdout=output,stderr=subprocess.STDOUT)
  report['checks'].append(dict(name=name,command=command,exit_code=run.returncode,duration_ms=round((time.monotonic()-start)*1000),rust_test_executions=sum(map(int,re.findall(r'test result: ok\. (\d+) passed;',log.read_text(errors='replace')))),log=dict(path=str(log.relative_to(ROOT)),sha256=sha(log))));write_result(TARGET,report);print(json.dumps(dict(check=name,exit_code=run.returncode)),flush=True)
  if run.returncode:report['state']='FAILED';write_result(TARGET,report);raise RuntimeError('ORCA_EXACT_OUT_CHECK_FAILED:'+name)
 result=json.loads((browser/'result.json').read_bytes());catalog=result['metadata']['protocolCatalog'];require_counts=(len(catalog),sum('EXACT_OUT'in c['quoteModes']for c in catalog.values()),sum(c['executionPreparation']for c in catalog.values()))
 if not result['passed']or fingerprint()!=source or inputs()!=helper or require_counts!=(61,21,10):report.update(state='FAILED',error='SOURCE_HELPER_BROWSER_OR_MODEL_SCOPE_CHANGED');write_result(TARGET,report);raise ValueError(report['error'])
 required=['orca-native-replay.json','orca-api-verification.json','quotes.json','mobile.png','orca-whirlpools-static-exact-out.png','orca-whirlpools-rewards-exact-out.png','orca-whirlpools-adaptive-exact-out.png','orca-whirlpools-adaptive-rewards-exact-out.png','orca-whirlpools-adaptive-dynamic-exact-out.png']
 if any(not(browser/p).is_file()for p in required):raise ValueError('ORCA_ARTIFACT_MISSING')
 artifacts=[p for p in browser.iterdir()if p.is_file()and p.name!='harness-info.json'and p.suffix in ['.json','.png','.log']]+list((browser/'orca-api').glob('*.json'))+list((browser/'receipts').glob('*.ndjson'))
 report.update(passed=True,state='LOCAL_SCOPE_VERIFIED',source_tree_sha256_after=fingerprint(),browser_checks=len(result['checks']),rust_test_executions=sum(c['rust_test_executions']for c in report['checks']),protocol_models=61,exact_out_models=21,preparation_models=10,artifacts=[dict(path=str(p.relative_to(ROOT)),sha256=sha(p))for p in sorted(artifacts)],harness_binary_sha256=sha(ROOT/'dex-router/target/debug/examples/frontend_harness'));write_result(TARGET,report);print(json.dumps({k:report[k]for k in ['passed','state','rust_test_executions','browser_checks','exact_out_models','preparation_models']}))
if __name__=='__main__':main()
