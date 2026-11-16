#!/usr/bin/env python3
"""Finite local callback/API/frontend validation; does not grant execution status."""
import hashlib
import json
import os
import re
from pathlib import Path
import subprocess
import time
import signal
import uuid

ROOT = Path(__file__).resolve().parents[3]
BASE = ROOT / 'integration/artifacts/local/composable-exact-out-check'
OUT = BASE

def fingerprint():
    paths = subprocess.check_output(['git', 'ls-files', '-co', '--exclude-standard', '-z'], cwd=ROOT).split(b'\0')
    h = hashlib.sha256()
    for raw in sorted(set(paths)):
        if raw:
            p = ROOT / os.fsdecode(raw)
            if p.is_file() and p.suffix != '.md':
                h.update(raw + b'\0' + p.read_bytes())
    return h.hexdigest()

def main():
    global OUT
    OUT=BASE/'runs'/uuid.uuid4().hex
    OUT.mkdir(parents=True, exist_ok=False)
    def interrupted(signum,frame):
        raise KeyboardInterrupt(f'Interrupted by signal {signum}')
    signal.signal(signal.SIGTERM,interrupted)
    env = dict(os.environ, PATH=str(Path.home() / '.cargo/bin') + os.pathsep + os.environ['PATH'])
    env.setdefault('CARGO_BUILD_JOBS', '2')
    bundled = Path.home() / '.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright'
    if bundled.exists():
        env.setdefault('PLAYWRIGHT_MODULE', str(bundled))
    env['XONE_COMPOSABLE_BROWSER_OUTPUT'] = str(OUT / 'browser')
    env['XONE_COMPOSABLE_API_DIR'] = str(OUT / 'api')
    cargo = ['cargo', '+1.90.0']
    manifest = ['--locked', '--manifest-path', 'dex-router/Cargo.toml']
    checks = [
        ('reference', ['node', 'integration/tools/ci/composable-exact-out-golden.cjs']),
        ('reference-evidence', ['python3', 'integration/tools/acceptance/verify_composable_exact_out.py']),
        ('evidence-controls', ['python3', '-m', 'unittest', 'discover', '-s', 'integration/tools/acceptance/tests', '-p', 'test_composable_exact_out.py']),
        ('evidence-controls-optimized', ['python3', '-OO', '-m', 'unittest', 'discover', '-s', 'integration/tools/acceptance/tests', '-p', 'test_composable_exact_out.py']),
        ('contracts', ['python3', 'integration/tools/ci/contracts.py']),
        ('protocol', ['python3', 'common/protoc/check.py']),
        ('go-parser', ['go', '-C', 'chain-parser-go', 'test', './internal/chain/evm', './internal/domain', './internal/config', './internal/journal', '-run', 'Test.*(Weighted|Stable|MultiAsset|MultiTokenWeighted|StandardClone)' , '-count=1']),
        ('progress-docs', ['python3', '-m', 'unittest', 'discover', '-s', 'integration/tools/ci/tests', '-p', 'test_progress_document.py']),
        ('progress-docs-optimized', ['python3', '-OO', '-m', 'unittest', 'discover', '-s', 'integration/tools/ci/tests', '-p', 'test_progress_document.py']),
        ('format', cargo + ['fmt', '--all', '--check', '--manifest-path', 'dex-router/Cargo.toml']),
        ('math', cargo + ['test'] + manifest + ['-p', 'xone-protocol-math', '--test', 'composable_exact_out', '--test', 'balancer_composable_differential', '--test', 'managed_snapshot_exact_out', '--test', 'weighted_exact_out', '--test', 'capabilities', '--test', 'exact_out', '--test', 'v3_exact_out']),
        ('routes-allocation', cargo + ['test'] + manifest + ['-p', 'xone-routing', '-p', 'xone-optimizer', '--test', 'composable_exact_out', '--test', 'managed_snapshot_exact_out', '--test', 'weighted_exact_out', '--test', 'exact_out', '--test', 'v3_exact_out']),
        ('api', cargo + ['test'] + manifest + ['-p', 'xone-quote-api', '--test', 'composable_exact_out', '--test', 'managed_snapshot_exact_out', '--test', 'weighted_exact_out', '--test', 'exact_out', '--test', 'v3_exact_out']),
        ('lint', cargo + ['clippy'] + manifest + ['-p', 'xone-protocol-math', '-p', 'xone-routing', '-p', 'xone-optimizer', '-p', 'xone-quote-api', '--lib', '--test', 'composable_exact_out', '--', '-D', 'warnings']),
        ('plan-lint', cargo + ['clippy'] + manifest + ['-p', 'xone-execution-plan', '--lib', '--', '-D', 'warnings']),
        ('browser-lint', cargo + ['clippy'] + manifest + ['-p', 'xone-quote-api', '--example', 'composable_frontend_harness', '--', '-D', 'warnings']),
        ('frontend-check', ['npm', '--prefix', 'frontend', 'run', 'check']),
        ('browser-build', cargo + ['build'] + manifest + ['-p', 'xone-quote-api', '--example', 'composable_frontend_harness']),
        ('frontend-unit', ['npm', '--prefix', 'frontend', 'test']),
        ('browser', ['node', 'frontend/test/composable-browser.mjs']),
    ]
    report = {'passed': False, 'state': 'RUNNING', 'checks': [], 'source_tree_sha256': fingerprint(), 'started_ms': int(time.time()*1000), 'run_directory':str(OUT.relative_to(ROOT)), 'publicTransactionsSubmitted': False, 'production_ready': False, 'all_requested_features_complete': False}
    target = BASE / 'report.json'
    def save():
        text=json.dumps(report,indent=2)+'\n'
        (OUT/'report.json').write_text(text)
        temporary=target.with_suffix('.tmp')
        temporary.write_text(text)
        temporary.replace(target)
    save()
    for name, command in checks:
        print(f'Running {name}', flush=True)
        with (OUT / f'{name}.log').open('wb') as log:
            child=subprocess.Popen(command,cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
            try:
                returncode=child.wait()
            finally:
                if child.poll() is None:
                    os.killpg(child.pid,signal.SIGTERM)
                    try:child.wait(timeout=2)
                    except subprocess.TimeoutExpired:
                        os.killpg(child.pid,signal.SIGKILL)
                        child.wait()
        log_bytes=(OUT/f'{name}.log').read_bytes()
        if bytes([0]) in log_bytes:
            report.update(state='INVALID_LOG',error=f'NUL bytes in {name}')
            save()
            raise SystemExit(report['error'])
        log_text = (OUT / f'{name}.log').read_text(errors='replace')
        test_executions = sum(int(n) for n in re.findall(r'test result: ok\. (\d+) passed;', log_text))
        report['checks'].append({'name': name, 'exit_code': returncode, 'rust_test_executions': test_executions})
        save()
        if returncode:
            report['state'] = 'FAILED'
            save()
            raise SystemExit(f'{name} failed: {OUT / (name + ".log")}')
    if report['source_tree_sha256'] != fingerprint():
        report['state'] = 'SOURCE_CHANGED'
        save()
        raise SystemExit('Source changed during validation; rerun on the final source')
    capture_paths=sorted((OUT/'api').glob('case-*.json'))
    if len(capture_paths)!=288:
        report.update(state='CAPTURE_FAILED',error=f'Expected 288 actual HTTP captures, found {len(capture_paths)}')
        save()
        raise SystemExit(report['error'])
    captures=[json.loads(p.read_bytes()) for p in capture_paths]
    for row in captures:
        valid=(row['status']==200 and row['budgetControl']['status']==400 and row['budgetControl']['response']['error']==('INVALID_REQUEST' if row['budgetControl']['request']['maxSellAmount']=='0' else 'EXACT_OUT_MAX_INPUT')) if row['passed'] else (row['status']==404 and row['response']['error']=='NO_ROUTE')
        if not valid:
            report.update(state='CAPTURE_FAILED',error='Actual HTTP status/control mismatch')
            save()
            raise SystemExit(report['error'])
    if sum(row['passed']for row in captures)!=216:
        report.update(state='CAPTURE_FAILED',error='Actual HTTP success count mismatch')
        save()
        raise SystemExit(report['error'])
    report['http_captures']={'cases':288,'success':216,'rejected':72,'budget_controls':216}
    report.update(passed=True, rust_test_executions=sum(c['rust_test_executions'] for c in report['checks']), state='LOCAL_SCOPE_VERIFIED', verification_scope='PINNED_COMPOSABLE_GIVEN_OUT_CALLBACK_API_FRONTEND', finished_ms=int(time.time()*1000), reference=json.loads((ROOT / 'integration/artifacts/local/composable-exact-out-generation.json').read_text()), browser=json.loads((OUT / 'browser/report.json').read_text()))
    artifact_paths=sorted(p for p in OUT.rglob('*') if p.is_file() and p.name != 'report.json')
    artifact_paths += [ROOT/'dex-router/test/contracts/composable-exact-out.json', ROOT/'integration/test/contracts/composable-exact-out.json', ROOT/'integration/artifacts/local/composable-exact-out-generation.json', ROOT/'dex-router/target/debug/examples/composable_frontend_harness']
    report['artifacts']=[{'path':str(p.relative_to(ROOT)), 'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in artifact_paths]
    report.update(cases=report['reference']['cases'], shared_sequences=report['reference']['sequences'], sequence_steps=report['reference']['sequence_steps'], browser_checks=len(report['browser']['checks']))
    save()
    print(json.dumps({'passed': True, 'checks': len(checks), 'browser_checks': len(report['browser']['checks'])}))

if __name__ == '__main__':
    main()
