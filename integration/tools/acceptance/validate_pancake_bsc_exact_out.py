#!/usr/bin/env python3
"""Current-source BSC4 ExactOut checks, regression, original execution and freeze."""
import json
import os
from pathlib import Path
import re
import secrets
import subprocess
import sys
import time
from weighted_evidence import ROOT,OUT,source_hash,runtime_fingerprint,sha,write_result
TARGET=OUT/'core-bsc4-exact-out-validation-20261110.json'


def run():
    env=dict(os.environ);env['PATH']=str(Path.home()/'.cargo/bin')+os.pathsep+env.get('PATH','');env['CARGO_BUILD_JOBS']='2'
    cargo=['cargo','+1.90.0'];rust=['--locked','--manifest-path',str(ROOT/'dex-router/Cargo.toml')]
    checks=[
        ('format',cargo+['fmt','--all','--check','--manifest-path',str(ROOT/'dex-router/Cargo.toml')]),
        ('reference',[sys.executable,str(ROOT/'integration/tools/acceptance/pancake_bsc_exact_out_reference.py')]),
        ('reference-controls',[sys.executable,'-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_pancake_bsc_exact_out_reference.py']),
        ('reference-controls-optimized',[sys.executable,'-O','-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_pancake_bsc_exact_out_reference.py']),
        ('contracts',[sys.executable,str(ROOT/'integration/tools/ci/contracts.py')]),
        ('progress',[sys.executable,'-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py']),
        ('progress-optimized',[sys.executable,'-O','-m','unittest','discover','-s','integration/tools/ci/tests','-p','test_progress_document.py']),
        ('bsc-exact-out',cargo+['test']+rust+['-p','xone-protocol-math','-p','xone-routing','-p','xone-optimizer','-p','xone-quote-api','--test','pancake_bsc_exact_out']),
        ('v2-exact-out-regression',cargo+['test']+rust+['-p','xone-protocol-math','-p','xone-routing','-p','xone-optimizer','-p','xone-quote-api','--test','exact_out']),
        ('curve-ledger-regression',cargo+['test']+rust+['-p','xone-protocol-math','--test','v3','--test','pancake_lm_differential','--test','pancake_bsc_lm']),
        ('routing-regression',cargo+['test']+rust+['-p','xone-routing','--test','search']),
        ('optimizer-regression',cargo+['test']+rust+['-p','xone-optimizer','--test','split','--test','grid_oracle','--test','pancake_lm_shared','--test','pancake_bsc_lm_shared']),
        ('plan-regression',cargo+['test']+rust+['-p','xone-execution-plan','--tests']),
        ('api-regression',cargo+['test']+rust+['-p','xone-quote-api','--test','api','--test','local_simulation','--test','simulation_request','--test','recovery_metrics']),
        ('lint',cargo+['clippy']+rust+['--workspace','--all-targets','--','-D','warnings']),
        ('build',cargo+['build']+rust+['--bin','dex-router','--example','pancake-bsc-exact-out-plan-projector','--example','exact-out-plan-projector']),
        ('bsc-process',[sys.executable,str(ROOT/'integration/tools/acceptance/pancake_bsc_exact_out_smoke.py')]),
        ('v2-process-regression',[sys.executable,str(ROOT/'integration/tools/acceptance/exact_out_smoke.py')]),
    ]
    tag=secrets.token_hex(6);report=dict(schema_version=1,passed=False,state='RUNNING',source_tree_sha256=source_hash(),tool_sha256=sha(Path(__file__)),checks=[],all_requested_features_complete=False,production_ready=False)
    write_result(TARGET,report)
    for name,command in checks:
        log=OUT/f'core-bsc4-xo-validation-{tag}-{name}.log';start=time.monotonic()
        with log.open('w')as stream:result=subprocess.run(command,cwd=ROOT,env=env,stdout=stream,stderr=subprocess.STDOUT)
        text=log.read_text(errors='replace')
        report['checks'].append(dict(name=name,command=command,observed_exit_code=result.returncode,duration_ms=round((time.monotonic()-start)*1000),rust_test_executions=sum(map(int,re.findall(r'test result: ok\. (\d+) passed;',text))),log=dict(path=str(log.relative_to(ROOT)),sha256=sha(log))))
        write_result(TARGET,report);print(json.dumps(dict(check=name,observed_exit_code=result.returncode)),flush=True)
        if result.returncode!=0:
            report.update(state='FAILED',source_tree_sha256_after=source_hash());write_result(TARGET,report);raise RuntimeError('BSC4_EXACT_OUT_VALIDATION_FAILED:'+name)
    report['source_tree_sha256_after']=source_hash()
    if report['source_tree_sha256_after']!=report['source_tree_sha256']:
        report['state']='SOURCE_CHANGED';write_result(TARGET,report);raise RuntimeError('BSC4_EXACT_OUT_SOURCE_CHANGED')
    report.update(passed=True,state='PASSED',runtime_fingerprint=runtime_fingerprint(),measured_at_unix_ms=int(time.time()*1000));write_result(TARGET,report);return report


if __name__=='__main__':
    result=run();print(json.dumps(dict(passed=True,checks=len(result['checks']),source_tree_sha256=result['source_tree_sha256'])))
