#!/usr/bin/env python3
"""Native Pump ATA creation, prefunding, atomic rollback and wallet settlement."""
from pathlib import Path
import json,os,re,secrets,subprocess,sys,time
from validate_quote_execution import fingerprint
from validate_pump_native_ata import inputs as prior_inputs
from weighted_evidence import ROOT,OUT,sha,write_result
TARGET=OUT/'core-pump-amm-funding-validation-20261113.json'
TOOLS=['integration/tools/acceptance/validate_pump_amm_funding.py','integration/tools/acceptance/pump_amm_funding_reference.py','integration/tools/acceptance/tests/test_pump_amm_funding_reference.py','integration/Makefile']
def inputs():
 result=prior_inputs();result.update({p:sha(ROOT/p)for p in TOOLS});return result
def main():
 env=dict(os.environ,PATH=str(Path.home()/'.cargo/bin')+os.pathsep+os.environ['PATH'],CARGO_BUILD_JOBS='2')
 env.setdefault('RUST_TEST_THREADS','2')
 from r01_public_catalog_solana_bank import native_imports
 native_imports();env.setdefault('XONE_SOLANA_PYTHON_PATH',str(ROOT/'integration/artifacts/local/r01-solana-rebuild/python'))
 bundled=Path.home()/'.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright'
 if 'PLAYWRIGHT_MODULE'not in env and bundled.exists():env['PLAYWRIGHT_MODULE']=str(bundled)
 tag=secrets.token_hex(6);browser=OUT/f'pump-amm-funding-browser-{tag}';env['XONE_BROWSER_OUTPUT']=str(browser)
 for name,folder in [('XONE_PUMP_FUNDING_DIR',browser/'funding-api'),('XONE_PUMP_CURVE_PREPARATION_DIR',browser/'curve-api'),('XONE_PUMP_CURVE_REFERENCE_DIR',browser),('XONE_PUMP_CURVE_ATA_REFERENCE_DIR',browser),('XONE_PUMP_PREPARATION_DIR',browser/'pump-api'),('XONE_PUMP_REFERENCE_DIR',browser),('XONE_RAYDIUM_PREPARATION_DIR',browser/'raydium-api'),('XONE_RAYDIUM_REFERENCE_DIR',browser),('XONE_TOKEN2022_EVIDENCE_DIR',browser/'api-preparations'),('XONE_TOKEN2022_REFERENCE_DIR',browser),('XONE_TEMP_REFERENCE_DIR',browser),('XONE_ATA_REFERENCE_INPUT',browser/'ata-preparation.json'),('XONE_WRAP_REFERENCE_INPUT',browser/'wrap-preparation.json')]:env[name]=str(folder)
 env['XONE_TEST_RPC_CAPTURE']='1';env['XONE_TEST_SUBMISSION_DIR']=str(browser/'receipts');env['XONE_TEST_SBF_MANIFEST_DIR']=str(ROOT/'dex-router/test/contracts/sbf-runtime')
 cargo=['cargo','+1.90.0'];manifest=['--locked','--manifest-path','dex-router/Cargo.toml'];py=[sys.executable]
 checks=[
 ('format',cargo+['fmt','--all','--check','--manifest-path','dex-router/Cargo.toml']),
 ('contracts',py+['integration/tools/ci/contracts.py']),('protocol',py+['common/protoc/check.py']),
 ('capabilities',cargo+['test']+manifest+['-p','xone-protocol-math','--test','capabilities']),
 ('receipt-unit',cargo+['test']+manifest+['-p','xone-quote-api','--lib','funding_tests']),
 ('api-lint',cargo+['clippy']+manifest+['-p','xone-quote-api','--lib','--test','pump_amm_funding','--test','pump_amm_preparation','--test','pump_curve_preparation','--example','frontend_harness','--','-D','warnings']),
 ('core-lint',cargo+['clippy']+manifest+['-p','xone-execution-preparation','-p','xone-protocol-math','--lib','--test','capabilities','--','-D','warnings']),
 ('build',cargo+['build']+manifest+['-p','xone-quote-api','--example','frontend_harness']),('browser',['node','frontend/test/browser.mjs']),
 ('funding-api',cargo+['test']+manifest+['-p','xone-quote-api','--test','pump_amm_funding']),
 ('pump-regression',cargo+['test']+manifest+['-p','xone-quote-api','--test','pump_amm_preparation']),
 ('native-regression',cargo+['test']+manifest+['-p','xone-quote-api','--test','pump_curve_preparation']),
 ('account-regression',cargo+['test']+manifest+['-p','xone-quote-api','--test','execution_preparation']),
 ('receipt-regression',cargo+['test']+manifest+['-p','xone-quote-api','--test','transaction_lifecycle']),
 ('capture-regression',cargo+['test']+manifest+['-p','xone-quote-api','--test','rpc_capture']),
 ('funding-original',py+['integration/tools/acceptance/pump_amm_funding_reference.py','--matrix',str(browser/'funding-api'),str(browser/'pump-funding-original-matrix.json')]),
 *[(f'funding-browser-original-{i}',py+['integration/tools/acceptance/pump_amm_funding_reference.py',str(browser/f'pump-funding-{i}-preparation.json'),str(browser/f'pump-funding-{i}-original.json'),str(i)])for i in [9,21,11,12]],
 ('pump-original',py+['integration/tools/acceptance/pump_amm_preparation_reference.py','--matrix',str(browser/'pump-api'),str(browser/'pump-amm-original-matrix.json')]),
 ('native-original',py+['integration/tools/acceptance/pump_curve_preparation_reference.py','--matrix',str(browser/'curve-api'),str(browser/'pump-curve-original-matrix.json')]),
 ('funding-controls',py+['-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_pump_amm_funding_reference.py']),
 ('funding-controls-optimized',py+['-OO','-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_pump_amm_funding_reference.py']),
 ('native-controls',py+['-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_pump_curve_preparation_reference.py']),
 ('native-controls-optimized',py+['-OO','-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_pump_curve_preparation_reference.py']),
 ('ata-controls',py+['-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_pump_curve_ata_reference.py']),
 ('ata-controls-optimized',py+['-OO','-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_pump_curve_ata_reference.py']),
 ('frontend-unit',['node','--test','frontend/test/model.test.js']),('wallet-unit',['node','--test','frontend/test/wallet.test.js']),
 ('frontend-syntax',['node','--check','frontend/src/app.js']),('wallet-syntax',['node','--check','frontend/src/wallet.js']),
 ('progress',py+['-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py']),
 ('progress-optimized',py+['-OO','-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py'])]
 if TARGET.exists():
  previous=OUT/f'pump-amm-funding-prior-{sha(TARGET)}.json'
  if not previous.exists():previous.write_bytes(TARGET.read_bytes())
 source=fingerprint();helper=inputs();r=dict(passed=False,state='RUNNING',source_tree_sha256=source,helper_sha256=helper,checks=[],all_requested_features_complete=False,production_ready=False);write_result(TARGET,r)
 for name,command in checks:
  log=OUT/f'pump-amm-funding-{tag}-{name}.log';start=time.monotonic()
  with log.open('w')as output:run=subprocess.run(command,cwd=ROOT,env=env,stdout=output,stderr=subprocess.STDOUT)
  r['checks'].append(dict(name=name,command=command,exit_code=run.returncode,duration_ms=round((time.monotonic()-start)*1000),rust_test_executions=sum(map(int,re.findall(r'test result: ok\. (\d+) passed;',log.read_text(errors='replace')))),log=dict(path=str(log.relative_to(ROOT)),sha256=sha(log))));write_result(TARGET,r);print(json.dumps(dict(check=name,exit_code=run.returncode)),flush=True)
  if run.returncode:r['state']='FAILED';write_result(TARGET,r);raise RuntimeError('PUMP_FUNDING_CHECK_FAILED:'+name)
 b=json.loads((browser/'result.json').read_bytes())
 if not b['passed']or fingerprint()!=source or inputs()!=helper:r.update(state='FAILED',error='SOURCE_HELPER_OR_BROWSER_CHANGED');write_result(TARGET,r);raise ValueError(r['error'])
 required=['pump-funding-9-preparation.json','pump-funding-9-receipt.json','pump-funding-21-complete.png','pump-funding-original-matrix.json','pump-curve-ata-preparation.json','pump-curve-ata-receipt.json','pump-curve-ata-complete.png','pump-curve-original-matrix.json','result.json','desktop.png','mobile.png']
 if any(not(browser/name).is_file()for name in required):raise ValueError('PUMP_FUNDING_ARTIFACT_MISSING')
 artifacts=[p for p in browser.iterdir()if p.is_file()and p.suffix in ['.json','.png','.log']and p.name!='harness-info.json']
 for folder in ['funding-api','pump-api','curve-api','receipts']:artifacts+=list((browser/folder).glob('*.json'if folder!='receipts'else'*.ndjson'))
 r.update(passed=True,state='LOCAL_SCOPE_VERIFIED',source_tree_sha256_after=fingerprint(),browser_checks=len(b['checks']),rust_test_executions=sum(c['rust_test_executions']for c in r['checks']),artifacts=[dict(path=str(p.relative_to(ROOT)),sha256=sha(p))for p in sorted(artifacts)],harness_binary_sha256=sha(ROOT/'dex-router/target/debug/examples/frontend_harness'));write_result(TARGET,r);print(json.dumps({k:r[k]for k in ['passed','state','rust_test_executions','browser_checks']}))
if __name__=='__main__':main()
