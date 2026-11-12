#!/usr/bin/env python3
"""Same-source finite quote / wallet preparation / unsigned-wire / browser verification."""
from pathlib import Path
import hashlib,json,os,re,secrets,subprocess,sys,time
from weighted_evidence import ROOT,OUT,source_hash,sha,write_result
TARGET=OUT/'core-quote-execution-validation-20261112.json'
TOOLS=['integration/tools/acceptance/validate_quote_execution.py','integration/tools/acceptance/pump_amm_sell_target_reference.py','integration/tools/acceptance/meteora_unsigned_reference.py','integration/tools/acceptance/tests/test_pump_amm_sell_target_reference.py','integration/tools/ci/meteora-execution-bank.py','integration/tools/ci/pump-amm-sell-target-reference.py','integration/tools/reference/meteora-dlmm/harness/native_extended.py','integration/tools/reference/pump-amm/harness/native.py','integration/tools/ci/pump-amm-reference.py','integration/artifacts/local/core-pump-amm-sell-target-original-20261112.json','integration/artifacts/local/core-pump-migration-original-20261111.json']
def fingerprint():
 h=hashlib.sha256(source_hash().encode())
 for p in sorted((ROOT/'frontend').rglob('*')):
  if p.is_file() and 'node_modules'not in p.parts and p.suffix in ['.html','.js','.mjs','.css','.json','.md']:h.update(str(p.relative_to(ROOT)).encode());h.update(p.read_bytes())
 return h.hexdigest()
def inputs():return {p:sha(ROOT/p)for p in TOOLS}
def main():
 env=dict(os.environ,PATH=str(Path.home()/'.cargo/bin')+os.pathsep+os.environ['PATH'],CARGO_BUILD_JOBS='2')
 cargo=['cargo','+1.90.0'];manifest=['--locked','--manifest-path','dex-router/Cargo.toml'];python=[sys.executable]
 tag=secrets.token_hex(6);browser=OUT/f'quote-execution-browser-{tag}';env['XONE_BROWSER_OUTPUT']=str(browser)
 if 'PLAYWRIGHT_MODULE'not in env:
  bundled=Path.home()/'.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright'
  if bundled.exists():env['PLAYWRIGHT_MODULE']=str(bundled)
 checks=[('router-format',cargo+['fmt','--all','--check','--manifest-path','dex-router/Cargo.toml']),('parser-format',cargo+['fmt','--all','--check','--manifest-path','chain-parser-rust/Cargo.toml']),('contracts',python+['integration/tools/ci/contracts.py']),('protocol',python+['common/protoc/check.py']),('config-tests',cargo+['test']+manifest+['-p','xone-config','--test','config','--test','cors']),('math',cargo+['test']+manifest+['-p','xone-protocol-math','--test','pump_amm_sell_target','--test','pump_amm','--test','pump_bonding','--test','meteora_orders']),('api',cargo+['test']+manifest+['-p','xone-quote-api','--test','execution_preparation','--test','pump_amm_sell_target','--test','meteora_extended']),('routing',cargo+['test']+manifest+['-p','xone-routing','--test','meteora_orders_exact_out','--test','pump_migration']),('optimizer',cargo+['test']+manifest+['-p','xone-optimizer','--test','meteora_orders_exact_out']),('parser-config',cargo+['check','--locked','--manifest-path','chain-parser-rust/Cargo.toml','-p','xone-config']),('go-config',['go','-C','chain-parser-go','test','./internal/config']),('lint',cargo+['clippy']+manifest+['-p','xone-quote-api','--lib','--test','execution_preparation','--test','pump_amm_sell_target','--test','meteora_extended','--example','frontend_harness','--','-D','warnings']),('math-lint',cargo+['clippy']+manifest+['-p','xone-protocol-math','--lib','--test','pump_amm_sell_target','--','-D','warnings']),('plan-lint',cargo+['clippy']+manifest+['-p','xone-execution-plan','--lib','--','-D','warnings']),('build',cargo+['build']+manifest+['-p','xone-quote-api','--example','frontend_harness']),('browser',['node','frontend/test/browser.mjs']),('unsigned-original',python+['integration/tools/acceptance/meteora_unsigned_reference.py',str(browser/'unsigned-preparation.json')]),('original',python+['integration/tools/acceptance/pump_amm_sell_target_reference.py','--reexecute']),('controls',python+['-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_pump_amm_sell_target_reference.py']),('controls-optimized',python+['-OO','-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_pump_amm_sell_target_reference.py']),('frontend-unit',['node','--test','frontend/test/model.test.js']),('frontend-syntax',['node','--check','frontend/src/app.js']),('progress',python+['-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py']),('progress-optimized',python+['-OO','-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py'])]
 if TARGET.exists():
  previous=OUT/f'quote-execution-prior-{sha(TARGET)}.json'
  if not previous.exists():previous.write_bytes(TARGET.read_bytes())
 source=fingerprint();helper=inputs();report=dict(schema_version=1,passed=False,state='RUNNING',source_tree_sha256=source,helper_sha256=helper,checks=[],all_requested_features_complete=False,production_ready=False);write_result(TARGET,report)
 for name,command in checks:
  log=OUT/f'quote-execution-{tag}-{name}.log';start=time.monotonic()
  with log.open('w')as f:run=subprocess.run(command,cwd=ROOT,env=env,stdout=f,stderr=subprocess.STDOUT)
  report['checks'].append(dict(name=name,command=command,exit_code=run.returncode,duration_ms=round((time.monotonic()-start)*1000),rust_test_executions=sum(map(int,re.findall(r'test result: ok\. (\d+) passed;',log.read_text(errors='replace')))),log=dict(path=str(log.relative_to(ROOT)),sha256=sha(log))));write_result(TARGET,report);print(json.dumps(dict(check=name,exit_code=run.returncode)),flush=True)
  if run.returncode:
   report.update(state='FAILED');write_result(TARGET,report);raise RuntimeError('QUOTE_EXECUTION_CHECK_FAILED:'+name)
 b=json.loads((browser/'result.json').read_bytes())
 if not b['passed']:raise ValueError('BROWSER_NOT_VERIFIED')
 artifacts=[browser/name for name in ['result.json','quotes.json','unsigned-preparation.json','desktop.png','mobile.png','harness.log']]+[OUT/'phase57-unsigned-original.json']
 if fingerprint()!=source or inputs()!=helper:
  report.update(state='FAILED',error='SOURCE_OR_HELPER_CHANGED');write_result(TARGET,report);raise ValueError('SOURCE_OR_HELPER_CHANGED')
 report.update(passed=True,state='LOCAL_SCOPE_VERIFIED',source_tree_sha256_after=fingerprint(),rust_test_executions=sum(c['rust_test_executions']for c in report['checks']),browser_checks=len(b['checks']),artifacts=[dict(path=str(p.relative_to(ROOT)),sha256=sha(p))for p in artifacts],harness_binary_sha256=sha(ROOT/'dex-router/target/debug/examples/frontend_harness'));write_result(TARGET,report);print(json.dumps({k:report[k]for k in ['passed','state','rust_test_executions','browser_checks']}))
if __name__=='__main__':main()
