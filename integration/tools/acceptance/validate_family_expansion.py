#!/usr/bin/env python3
"""Finite, same-source OnlyY/Pump/combination checks; never global completion."""
from pathlib import Path
import json,os,re,secrets,subprocess,sys,time
from weighted_evidence import ROOT,OUT,source_hash,sha,write_result
TARGET=OUT/'core-family-expansion-validation-20261111.json'
def main():
 env=dict(os.environ,PATH=str(Path.home()/'.cargo/bin')+os.pathsep+os.environ['PATH'],CARGO_BUILD_JOBS='2');cargo=['cargo','+1.90.0'];router=['--locked','--manifest-path','dex-router/Cargo.toml'];parser=['--locked','--manifest-path','chain-parser-rust/Cargo.toml'];checks=[
 ('router-format',cargo+['fmt','--all','--check','--manifest-path','dex-router/Cargo.toml']),
 ('parser-format',cargo+['fmt','--all','--check','--manifest-path','chain-parser-rust/Cargo.toml']),
 ('protocol',[sys.executable,'common/protoc/check.py']),('contracts',[sys.executable,'integration/tools/ci/contracts.py']),
 ('reference',[sys.executable,'integration/tools/acceptance/family_reference.py','--reexecute']),
 ('reference-tests',[sys.executable,'-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_family_reference.py']),
 ('reference-tests-optimized',[sys.executable,'-OO','-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_family_reference.py']),
 ('progress',[sys.executable,'-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py']),
 ('progress-optimized',[sys.executable,'-OO','-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py']),
 ('parser-tests',cargo+['test']+parser+['-p','xone-solana-ingest','--test','pump','--test','meteora_only_y','--test','meteora','--test','cpmm_version']),
 ('math-new-and-existing',cargo+['test']+router+['-p','xone-protocol-math','--test','pump_bonding','--test','meteora_only_y','--test','meteora_differential','--test','meteora_exact_out']),
 ('api-new-and-existing',cargo+['test']+router+['-p','xone-quote-api','--test','pump_bonding','--test','meteora_only_y','--test','meteora','--test','meteora_exact_out']),
 ('combination-routing',cargo+['test']+router+['-p','xone-routing','--test','family_combinations','--test','meteora','--test','meteora_exact_out','--test','search']),
 ('parser-all-targets',cargo+['check']+parser+['--workspace','--all-targets']),
 ('router-all-targets',cargo+['check']+router+['--workspace','--all-targets']),
 ('parser-lint',cargo+['clippy']+parser+['-p','xone-solana-ingest','-p','xone-domain','-p','xone-config','--lib','--test','pump','--test','meteora_only_y','--','-D','warnings']),
 ('router-lint',cargo+['clippy']+router+['--workspace','--lib','--bins','--example','pump-plan-projector','--','-D','warnings']),
 ('new-tests-lint',cargo+['clippy']+router+['-p','xone-protocol-math','-p','xone-routing','-p','xone-quote-api','--test','pump_bonding','--test','meteora_only_y','--test','family_combinations','--','-D','warnings']),
 ('go-domain',['go','-C','chain-parser-go','test','./internal/domain']),
 ('go-fixture-build',['go','-C','chain-parser-go','build','-trimpath','-o','bin/fixture-parser-go','./test/cmd/fixture-parser-go']),
 ('parser-build',cargo+['build']+parser+['--bin','chain-parser-rust']),
 ('router-build',cargo+['build']+router+['--bin','dex-router','--example','pump-plan-projector']),
 ('pump-normal-process',[sys.executable,'integration/tools/acceptance/pump_bonding_smoke.py']),
 ]
 if TARGET.exists():
  old=OUT/f'core-family-expansion-validation-prior-{sha(TARGET)}.json'
  if not old.exists():old.write_bytes(TARGET.read_bytes())
 tag=secrets.token_hex(6);report=dict(schema_version=1,passed=False,state='RUNNING',source_tree_sha256=source_hash(),tool_sha256=sha(Path(__file__)),checks=[],all_requested_features_complete=False,production_ready=False);write_result(TARGET,report)
 for name,command in checks:
  path=OUT/f'family-expansion-{tag}-{name}.log';start=time.monotonic()
  with path.open('w')as out:r=subprocess.run(command,cwd=ROOT,env=env,stdout=out,stderr=subprocess.STDOUT)
  text=path.read_text(errors='replace');report['checks'].append(dict(name=name,command=command,exit_code=r.returncode,duration_ms=round((time.monotonic()-start)*1000),rust_test_executions=sum(map(int,re.findall(r'test result: ok\. (\d+) passed;',text))),log=dict(path=str(path.relative_to(ROOT)),sha256=sha(path))));write_result(TARGET,report);print(json.dumps(dict(check=name,exit_code=r.returncode)),flush=True)
  if r.returncode:
   report.update(state='FAILED',source_tree_sha256_after=source_hash());write_result(TARGET,report);raise RuntimeError('FAMILY_EXPANSION_FAILED:'+name)
 source=source_hash();process_path=OUT/'core-pump-bonding-normal-process.json';process=json.loads(process_path.read_bytes())
 if not(process['passed']is True and source==report['source_tree_sha256']==process['source_tree_sha256']==process['source_tree_sha256_after']):raise ValueError('FAMILY_FINAL_SOURCE_BINDING')
 frozen=OUT/f'core-pump-bonding-process-{source}-{sha(process_path)}.json'
 if not frozen.exists():frozen.write_bytes(process_path.read_bytes())
 report.update(passed=True,state='LOCAL_SCOPE_VERIFIED',source_tree_sha256_after=source,rust_test_executions=sum(c['rust_test_executions']for c in report['checks']),process_receipt=dict(path=str(frozen.relative_to(ROOT)),sha256=sha(frozen)));write_result(TARGET,report);print(json.dumps({k:report[k]for k in ['passed','state','source_tree_sha256','rust_test_executions']}))
if __name__=='__main__':main()
