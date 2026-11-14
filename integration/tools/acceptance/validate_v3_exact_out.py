#!/usr/bin/env python3
"""Same-source standard V3 original EVM target/routing/API/frontend checks."""
from pathlib import Path
import json,os,re,secrets,subprocess,sys,time
from validate_quote_execution import fingerprint
from validate_rayclmm_exact_out import inputs as prior_inputs
from weighted_evidence import ROOT,OUT,sha,write_result
TARGET=OUT/'core-v3-exact-out-validation-20261114.json'
TOOLS=['integration/tools/acceptance/validate_v3_exact_out.py','integration/tools/acceptance/v3_exact_out_reference.cjs','integration/tools/acceptance/verify_v3_exact_out.py','integration/tools/acceptance/verify_v3_exact_out_api.py','integration/tools/acceptance/tests/test_v3_exact_out_reference.py','integration/tools/reference/v3-exact-out/Reference.sol','integration/tools/reference/v3-exact-out/source-pins.json','integration/tools/reference/v3-exact-out/README.md','integration/test/contracts/v3-exact-out.json','dex-router/test/contracts/v3-exact-out.json','integration/artifacts/local/v3-exact-out-original-20261114.json','integration/tools/reference/v3-sdk/package.json','integration/tools/reference/v3-sdk/package-lock.json','integration/tools/reference/pancake-v3-lm/source-metadata.json','integration/.runtime/stable-reference/package.json','integration/.runtime/stable-reference/package-lock.json']
def inputs():
 r=prior_inputs();paths=list(TOOLS);base='integration/tools/reference/pancake-v3-lm/';meta=json.loads((ROOT/(base+'source-metadata.json')).read_bytes());paths += [base+x['path']for x in meta['files']];paths += [base+x['path']for x in meta['dependencies']['openzeppelin']['files']]
 core='integration/.runtime/v3-sdk-oracle/node_modules/@uniswap/v3-staker/node_modules/@uniswap/v3-core/'
 paths += [core+p for p in ['package.json','artifacts/contracts/UniswapV3Pool.sol/UniswapV3Pool.json','artifacts/contracts/UniswapV3Factory.sol/UniswapV3Factory.json']]
 for package in ['ethers','ganache','solc07']:
  base=ROOT/'integration/.runtime/stable-reference/node_modules'/package
  for path in sorted(base.rglob('*')):
   if path.is_file()and 'node_modules'not in path.relative_to(base).parts and path.suffix in ['.js','.json','.node']:paths.append(str(path.relative_to(ROOT)))
 r.update({p:sha(ROOT/p)for p in paths})
 observed=json.loads((OUT/'v3-exact-out-original-20261114.json').read_bytes())
 for item in observed['loadedDependencies']:r[item['path']]=sha(item['path'])
 return r

def main():
 env=dict(os.environ,PATH=str(Path.home()/'.cargo/bin')+os.pathsep+os.environ['PATH'],CARGO_BUILD_JOBS='2');env.setdefault('RUST_TEST_THREADS','2');env.setdefault('XONE_SOLANA_PYTHON_PATH',str(ROOT/'integration/artifacts/local/r01-solana-rebuild/python'))
 bundled=Path.home()/'.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright'
 if 'PLAYWRIGHT_MODULE'not in env and bundled.exists():env['PLAYWRIGHT_MODULE']=str(bundled)
 tag=secrets.token_hex(6);browser=OUT/f'v3-exact-out-browser-{tag}';env['XONE_BROWSER_OUTPUT']=str(browser);env['XONE_V3_EXACT_OUT_DIR']=str(browser/'v3-api');env['XONE_TEST_RPC_CAPTURE']='1';env['XONE_TEST_SUBMISSION_DIR']=str(browser/'receipts');env['XONE_TEST_SBF_MANIFEST_DIR']=str(ROOT/'dex-router/test/contracts/sbf-runtime')
 cargo=['cargo','+1.90.0'];manifest=['--locked','--manifest-path','dex-router/Cargo.toml'];py=[sys.executable]
 checks=[
 ('format',cargo+['fmt','--all','--check','--manifest-path','dex-router/Cargo.toml']),('contracts',py+['integration/tools/ci/contracts.py']),('protocol',py+['common/protoc/check.py']),
 ('api-lint',cargo+['clippy']+manifest+['-p','xone-quote-api','--lib','--test','v3_exact_out','--example','frontend_harness','--','-D','warnings']),
 ('core-lint',cargo+['clippy']+manifest+['-p','xone-protocol-math','-p','xone-routing','-p','xone-optimizer','-p','xone-execution-plan','--lib','--test','v3_exact_out','--','-D','warnings']),
 ('build',cargo+['build']+manifest+['-p','xone-quote-api','--example','frontend_harness']),('browser',['node','frontend/test/browser.mjs']),
 ('math-routes-grid',cargo+['test']+manifest+['-p','xone-protocol-math','-p','xone-routing','-p','xone-optimizer','--test','v3_exact_out']),
 ('legacy-price-kernels',cargo+['test']+manifest+['-p','xone-protocol-math','--test','v3','--test','capabilities','--test','orca_exact_out','--test','rayclmm_exact_out']),
 ('api',cargo+['test']+manifest+['-p','xone-quote-api','--test','v3_exact_out']),('legacy-api',cargo+['test']+manifest+['-p','xone-quote-api','--test','exact_out']),
 ('legacy-exact-out',cargo+['test']+manifest+['-p','xone-protocol-math','-p','xone-routing','-p','xone-optimizer','--test','exact_out']),
 ('plan',cargo+['test']+manifest+['-p','xone-execution-plan','--test','plan','--test','v2_complete']),
 ('go-v3-parser',['go','-C','chain-parser-go','test','./internal/chain/evm','-run','V3|Pancake']),
 ('original-replay',py+['integration/tools/acceptance/verify_v3_exact_out.py','--reexecute','--output',str(browser/'v3-original-replay.json')]),
 ('actual-api',py+['integration/tools/acceptance/verify_v3_exact_out_api.py',str(browser/'v3-api'),'--output',str(browser/'v3-api-verification.json')]),
 ('controls',py+['-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_v3_exact_out_reference.py']),
 ('controls-optimized',py+['-OO','-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_v3_exact_out_reference.py']),
 ('frontend-unit',['node','--test','frontend/test/model.test.js']),('wallet-unit',['node','--test','frontend/test/wallet.test.js']),('frontend-syntax',['node','--check','frontend/src/app.js']),('wallet-syntax',['node','--check','frontend/src/wallet.js']),
 ('progress',py+['-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py']),('progress-optimized',py+['-OO','-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py'])]
 if TARGET.exists():
  p=OUT/f'v3-exact-out-prior-{sha(TARGET)}.json'
  if not p.exists():p.write_bytes(TARGET.read_bytes())
 source=fingerprint();helper=inputs();report=dict(passed=False,state='RUNNING',source_tree_sha256=source,helper_sha256=helper,checks=[],all_requested_features_complete=False,production_ready=False);write_result(TARGET,report)
 for name,command in checks:
  log=OUT/f'v3-exact-out-{tag}-{name}.log';start=time.monotonic()
  with log.open('w')as out:run=subprocess.run(command,cwd=ROOT,env=env,stdout=out,stderr=subprocess.STDOUT)
  report['checks'].append(dict(name=name,command=command,exit_code=run.returncode,duration_ms=round((time.monotonic()-start)*1000),rust_test_executions=sum(map(int,re.findall(r'test result: ok\. (\d+) passed;',log.read_text(errors='replace')))),log=dict(path=str(log.relative_to(ROOT)),sha256=sha(log))));write_result(TARGET,report);print(json.dumps(dict(check=name,exit_code=run.returncode)),flush=True)
  if run.returncode:report['state']='FAILED';write_result(TARGET,report);raise RuntimeError('V3_EXACT_OUT_CHECK_FAILED:'+name)
 b=json.loads((browser/'result.json').read_bytes());catalog=b['metadata']['protocolCatalog'];counts=(len(catalog),sum('EXACT_OUT'in c['quoteModes']for c in catalog.values()),sum(c['executionPreparation']for c in catalog.values()))
 if not b['passed']or counts!=(61,29,10)or fingerprint()!=source or inputs()!=helper:report.update(state='FAILED',error='SOURCE_HELPER_BROWSER_OR_SCOPE_CHANGED');write_result(TARGET,report);raise ValueError(report['error'])
 required=['uniswap-v3-exact-out.png','pancake-v3-exact-out.png','v3-original-replay.json','v3-api-verification.json','quotes.json','mobile.png']
 if any(not(browser/p).exists()for p in required):raise ValueError('V3_ACTUAL_ARTIFACT_MISSING')
 artifacts=[p for p in browser.iterdir()if p.is_file()and p.name!='harness-info.json'and p.suffix in ['.json','.png','.log']]+list((browser/'v3-api').glob('*.json'))+list((browser/'receipts').glob('*.ndjson'))
 report.update(passed=True,state='LOCAL_SCOPE_VERIFIED',source_tree_sha256_after=fingerprint(),browser_checks=len(b['checks']),rust_test_executions=sum(x['rust_test_executions']for x in report['checks']),protocol_models=61,exact_out_models=29,preparation_models=10,artifacts=[dict(path=str(p.relative_to(ROOT)),sha256=sha(p))for p in sorted(artifacts)],harness_binary_sha256=sha(ROOT/'dex-router/target/debug/examples/frontend_harness'));write_result(TARGET,report);print(json.dumps({k:report[k]for k in ['passed','state','rust_test_executions','browser_checks']}))
if __name__=='__main__':main()
