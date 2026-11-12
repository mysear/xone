#!/usr/bin/env python3
"""Same-source ATA creation, read-only simulation, exact downloaded wire and browser checks."""
from pathlib import Path
import json,os,re,secrets,subprocess,sys,time
from validate_quote_execution import fingerprint
from weighted_evidence import ROOT,OUT,sha,write_result
TARGET=OUT/'core-account-preparation-validation-20261112.json'
TOOLS=['integration/tools/ci/contracts.py','integration/tools/ci/progress_bindings.py','integration/tools/progress_document.py','integration/tools/acceptance/r01_public_catalog_solana_bank.py','integration/tools/acceptance/validate_account_preparation.py','integration/tools/acceptance/solana_prepare_simulation_worker.py','integration/tools/acceptance/meteora_account_creation_reference.py','integration/tools/acceptance/meteora_unsigned_reference.py','integration/tools/acceptance/tests/test_account_creation_reference.py','integration/tools/acceptance/validate_quote_execution.py','integration/tools/ci/meteora-execution-bank.py','integration/test/contracts/meteora-execution-bank.json','integration/test/source/deployment-catalog/solana-rebuild/runtime-lock.json']
def inputs():return {p:sha(ROOT/p)for p in TOOLS}
def main():
 env=dict(os.environ,PATH=str(Path.home()/'.cargo/bin')+os.pathsep+os.environ['PATH'],CARGO_BUILD_JOBS='2')
 from r01_public_catalog_solana_bank import native_imports
 native_imports()
 env.setdefault('XONE_SOLANA_PYTHON_PATH',str(ROOT/'integration/artifacts/local/r01-solana-rebuild/python'))
 bundled=Path.home()/'.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright'
 if 'PLAYWRIGHT_MODULE'not in env and bundled.exists():env['PLAYWRIGHT_MODULE']=str(bundled)
 tag=secrets.token_hex(6);browser=OUT/f'account-preparation-browser-{tag}';env['XONE_BROWSER_OUTPUT']=str(browser)
 env['XONE_ATA_REFERENCE_INPUT']=str(browser/'ata-preparation.json')
 cargo=['cargo','+1.90.0'];manifest=['--locked','--manifest-path','dex-router/Cargo.toml'];python=[sys.executable]
 checks=[('format',cargo+['fmt','--all','--check','--manifest-path','dex-router/Cargo.toml']),('contracts',python+['integration/tools/ci/contracts.py']),('protocol',python+['common/protoc/check.py']),('wire-tests',cargo+['test']+manifest+['-p','xone-execution-plan','--test','solana_unsigned']),('api-tests',cargo+['test']+manifest+['-p','xone-quote-api','--test','execution_preparation','--test','meteora_extended']),('api-lint',cargo+['clippy']+manifest+['-p','xone-quote-api','--lib','--test','execution_preparation','--test','meteora_extended','--example','frontend_harness','--','-D','warnings']),('plan-lint',cargo+['clippy']+manifest+['-p','xone-execution-plan','--lib','--test','solana_unsigned','--','-D','warnings']),('build',cargo+['build']+manifest+['-p','xone-quote-api','--example','frontend_harness']),('browser',['node','frontend/test/browser.mjs']),('ata-original',python+['integration/tools/acceptance/meteora_account_creation_reference.py',str(browser/'ata-preparation.json')]),('existing-original',python+['integration/tools/acceptance/meteora_unsigned_reference.py',str(browser/'unsigned-preparation.json'),str(OUT/'phase58-existing-original.json')]),('controls',python+['-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_account_creation_reference.py']),('controls-optimized',python+['-OO','-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_account_creation_reference.py']),('frontend-unit',['node','--test','frontend/test/model.test.js']),('frontend-syntax',['node','--check','frontend/src/app.js']),('progress',python+['-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py']),('progress-optimized',python+['-OO','-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py'])]
 if TARGET.exists():
  previous=OUT/f'account-preparation-prior-{sha(TARGET)}.json'
  if not previous.exists():previous.write_bytes(TARGET.read_bytes())
 source=fingerprint();helper=inputs();r=dict(passed=False,state='RUNNING',source_tree_sha256=source,helper_sha256=helper,checks=[],all_requested_features_complete=False,production_ready=False);write_result(TARGET,r)
 for name,command in checks:
  log=OUT/f'account-preparation-{tag}-{name}.log';start=time.monotonic()
  with log.open('w')as f:run=subprocess.run(command,cwd=ROOT,env=env,stdout=f,stderr=subprocess.STDOUT)
  r['checks'].append(dict(name=name,command=command,exit_code=run.returncode,duration_ms=round((time.monotonic()-start)*1000),rust_test_executions=sum(map(int,re.findall(r'test result: ok\. (\d+) passed;',log.read_text(errors='replace')))),log=dict(path=str(log.relative_to(ROOT)),sha256=sha(log))));write_result(TARGET,r);print(json.dumps(dict(check=name,exit_code=run.returncode)),flush=True)
  if run.returncode:r['state']='FAILED';write_result(TARGET,r);raise RuntimeError('ACCOUNT_PREPARATION_CHECK_FAILED:'+name)
 b=json.loads((browser/'result.json').read_bytes());artifacts=[browser/name for name in ['result.json','quotes.json','ata-preparation.json','unsigned-preparation.json','desktop.png','mobile.png','harness.log']]+[OUT/'phase58-ata-original.json',OUT/'phase58-existing-original.json']
 if not b['passed'] or fingerprint()!=source or inputs()!=helper:r.update(state='FAILED',error='SOURCE_HELPER_OR_BROWSER_FAILED');write_result(TARGET,r);raise ValueError(r['error'])
 r.update(passed=True,state='LOCAL_SCOPE_VERIFIED',source_tree_sha256_after=fingerprint(),browser_checks=len(b['checks']),rust_test_executions=sum(c['rust_test_executions']for c in r['checks']),artifacts=[dict(path=str(p.relative_to(ROOT)),sha256=sha(p))for p in artifacts],harness_binary_sha256=sha(ROOT/'dex-router/target/debug/examples/frontend_harness'));write_result(TARGET,r);print(json.dumps({k:r[k]for k in ['passed','state','rust_test_executions','browser_checks']}))
if __name__=='__main__':main()
