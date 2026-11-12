#!/usr/bin/env python3
"""Finite same-source Meteora MM acceptance; preserves failed receipts separately."""
from pathlib import Path
import json,os,re,secrets,subprocess,sys,time
from weighted_evidence import ROOT,OUT,source_hash,write_result,sha
TARGET=OUT/'core-meteora-mm-validation-20261110.json'
def run():
    OUT.mkdir(parents=True,exist_ok=True);env=dict(os.environ);env['PATH']=str(Path.home()/'.cargo/bin')+os.pathsep+env.get('PATH','');env['CARGO_BUILD_JOBS']='2'
    cargo=['cargo','+1.90.0'];router=['--locked','--manifest-path',str(ROOT/'dex-router/Cargo.toml')];parser=['--locked','--manifest-path',str(ROOT/'chain-parser-rust/Cargo.toml')]
    checks=[
        ('router-format',cargo+['fmt','--all','--check','--manifest-path',str(ROOT/'dex-router/Cargo.toml')]),
        ('parser-format',cargo+['fmt','--all','--check','--manifest-path',str(ROOT/'chain-parser-rust/Cargo.toml')]),
        ('protocol-generated',[sys.executable,'common/protoc/check.py']),
        ('contracts',[sys.executable,'integration/tools/ci/contracts.py']),
        ('source-reference',[sys.executable,'integration/tools/acceptance/meteora_mm_reference.py']),
        ('edge-reference',[sys.executable,'integration/tools/acceptance/meteora_mm_edge_reference.py']),
        ('reference-controls',[sys.executable,'-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_meteora_mm_reference.py']),
        ('reference-controls-optimized',[sys.executable,'-OO','-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_meteora_mm_reference.py']),
        ('progress',[sys.executable,'-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py']),
        ('progress-optimized',[sys.executable,'-OO','-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py']),
        ('parser-new-and-existing',cargo+['test']+parser+['-p','xone-solana-ingest','--test','meteora','--test','cpmm_version','--test','raydium_decode','--test','orca_decode','--test','orca_rpc']),
        ('meteora-kernel-routing-optimizer-api',cargo+['test']+router+['-p','xone-protocol-math','-p','xone-routing','-p','xone-optimizer','-p','xone-quote-api','--test','meteora','--test','meteora_differential']),
        ('lfj-original-regression',cargo+['test']+router+['-p','xone-protocol-math','--test','lfj_differential','--test','lfj_exact_out']),
        ('routing-existing',cargo+['test']+router+['-p','xone-routing','--test','search','--test','exact_out','--test','lfj_exact_out']),
        ('optimizer-existing',cargo+['test']+router+['-p','xone-optimizer','--test','grid_oracle','--test','split','--test','lfj_shared','--test','lfj_exact_out']),
        ('api-existing',cargo+['test']+router+['-p','xone-quote-api','--test','api','--test','lfj_exact_out']),
        ('parser-all-targets',cargo+['check']+parser+['--workspace','--all-targets']),
        ('router-all-targets',cargo+['check']+router+['--workspace','--all-targets']),
        ('parser-lint',cargo+['clippy']+parser+['-p','xone-domain','-p','xone-config','-p','xone-solana-ingest','--lib','--test','meteora','--','-D','warnings']),
        ('router-lint',cargo+['clippy']+router+['--workspace','--lib','--bins','--examples','--','-D','warnings']),
        ('router-new-tests-lint',cargo+['clippy']+router+['-p','xone-protocol-math','-p','xone-routing','-p','xone-optimizer','-p','xone-quote-api','--test','meteora','--test','meteora_differential','--','-D','warnings']),
        ('go-domain',['go','-C',str(ROOT/'chain-parser-go'),'test','./internal/domain']),
        ('go-fixture-build',['go','-C',str(ROOT/'chain-parser-go'),'build','-trimpath','-o','bin/fixture-parser-go','./test/cmd/fixture-parser-go']),
        ('parser-build',cargo+['build']+parser+['--bin','chain-parser-rust']),
        ('router-build',cargo+['build']+router+['--bin','dex-router','--example','meteora-plan-projector']),
        ('normal-pg-kafka-original-plan-replay',[sys.executable,'integration/tools/acceptance/meteora_mm_smoke.py','--reuse-if-current']),
    ]
    if TARGET.exists():
        prior=TARGET.read_bytes();preserved=OUT/('core-meteora-mm-validation-prior-'+__import__('hashlib').sha256(prior).hexdigest()+'.json')
        if not preserved.exists():preserved.write_bytes(prior)
    tag=secrets.token_hex(6);report=dict(schema_version=1,passed=False,state='RUNNING',source_tree_sha256=source_hash(),tool_sha256=sha(Path(__file__)),checks=[],all_requested_features_complete=False,production_ready=False)
    write_result(TARGET,report)
    for name,command in checks:
        log=OUT/f'meteora-mm-validation-{tag}-{name}.log';start=time.monotonic()
        with log.open('w')as stream:result=subprocess.run(command,cwd=ROOT,env=env,stdout=stream,stderr=subprocess.STDOUT)
        text=log.read_text(errors='replace');report['checks'].append(dict(name=name,command=command,observed_exit_code=result.returncode,duration_ms=round((time.monotonic()-start)*1000),rust_test_executions=sum(map(int,re.findall(r'test result: ok\. (\d+) passed;',text))),log=dict(path=str(log.relative_to(ROOT)),sha256=sha(log))));write_result(TARGET,report);print(json.dumps(dict(check=name,observed_exit_code=result.returncode)),flush=True)
        if result.returncode:
            report.update(state='FAILED',source_tree_sha256_after=source_hash());write_result(TARGET,report);raise RuntimeError('METEORA_VALIDATION_FAILED:'+name)
    if source_hash()!=report['source_tree_sha256']:
        report.update(state='SOURCE_CHANGED',source_tree_sha256_after=source_hash());write_result(TARGET,report);raise RuntimeError('METEORA_VALIDATION_SOURCE_CHANGED')
    process=ROOT/'integration/artifacts/local/core-meteora-mm-normal-process.json';p=json.loads(process.read_bytes())
    if p.get('passed')is not True or p.get('source_tree_sha256')!=report['source_tree_sha256']:raise ValueError('METEORA_PROCESS_BINDING')
    frozen=OUT/f"core-meteora-mm-process-{report['source_tree_sha256']}-{sha(process)}.json"
    if not frozen.exists():frozen.write_bytes(process.read_bytes())
    report.update(passed=True,state='LOCAL_SCOPE_VERIFIED',source_tree_sha256_after=source_hash(),rust_test_executions=sum(c['rust_test_executions']for c in report['checks']),process_receipt=dict(path=str(frozen.relative_to(ROOT)),sha256=sha(frozen)),measured_at_unix_ms=int(time.time()*1000));write_result(TARGET,report);return report
if __name__=='__main__':
    report=run();print(json.dumps({k:report[k]for k in ['passed','state','source_tree_sha256','rust_test_executions']}))
