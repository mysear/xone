#!/usr/bin/env python3
"""Finite local callback/API/frontend validation; does not grant execution status."""
import hashlib
import json
import os
import re
from pathlib import Path
import subprocess
import time

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / 'integration/artifacts/local/weighted-exact-out-check'

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
    OUT.mkdir(parents=True, exist_ok=True)
    env = dict(os.environ, PATH=str(Path.home() / '.cargo/bin') + os.pathsep + os.environ['PATH'])
    bundled = Path.home() / '.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright'
    if bundled.exists():
        env.setdefault('PLAYWRIGHT_MODULE', str(bundled))
    env['XONE_WEIGHTED_BROWSER_OUTPUT'] = str(OUT / 'browser')
    cargo = ['cargo', '+1.90.0']
    manifest = ['--locked', '--manifest-path', 'dex-router/Cargo.toml']
    checks = [
        ('reference', ['node', 'integration/tools/ci/weighted-exact-out-golden.cjs']),
        ('contracts', ['python3', 'integration/tools/ci/contracts.py']),
        ('protocol', ['python3', 'common/protoc/check.py']),
        ('go-parser', ['go', '-C', 'chain-parser-go', 'test', './internal/chain/evm', './internal/domain', './internal/config', './internal/journal', '-run', 'Test.*(Weighted|MultiAsset|MultiTokenWeighted)', '-count=1']),
        ('progress-docs', ['python3', '-m', 'unittest', 'discover', '-s', 'integration/tools/ci/tests', '-p', 'test_progress_document.py']),
        ('progress-docs-optimized', ['python3', '-OO', '-m', 'unittest', 'discover', '-s', 'integration/tools/ci/tests', '-p', 'test_progress_document.py']),
        ('format', cargo + ['fmt', '--all', '--check', '--manifest-path', 'dex-router/Cargo.toml']),
        ('math', cargo + ['test'] + manifest + ['-p', 'xone-protocol-math', '--test', 'weighted_exact_out', '--test', 'weighted_differential', '--test', 'weighted_static_differential', '--test', 'balancer_multi_differential', '--test', 'capabilities', '--test', 'exact_out', '--test', 'v3_exact_out']),
        ('routes-allocation', cargo + ['test'] + manifest + ['-p', 'xone-routing', '-p', 'xone-optimizer', '--test', 'weighted_exact_out', '--test', 'exact_out', '--test', 'v3_exact_out']),
        ('api', cargo + ['test'] + manifest + ['-p', 'xone-quote-api', '--test', 'weighted_exact_out', '--test', 'exact_out', '--test', 'v3_exact_out']),
        ('lint', cargo + ['clippy'] + manifest + ['-p', 'xone-protocol-math', '-p', 'xone-routing', '-p', 'xone-optimizer', '-p', 'xone-quote-api', '--lib', '--test', 'weighted_exact_out', '--', '-D', 'warnings']),
        ('plan-lint', cargo + ['clippy'] + manifest + ['-p', 'xone-execution-plan', '--lib', '--', '-D', 'warnings']),
        ('browser-lint', cargo + ['clippy'] + manifest + ['-p', 'xone-quote-api', '--example', 'weighted_frontend_harness', '--', '-D', 'warnings']),
        ('frontend-check', ['npm', '--prefix', 'frontend', 'run', 'check']),
        ('browser-build', cargo + ['build'] + manifest + ['-p', 'xone-quote-api', '--example', 'weighted_frontend_harness']),
        ('frontend-unit', ['npm', '--prefix', 'frontend', 'test']),
        ('browser', ['node', 'frontend/test/weighted-browser.mjs']),
    ]
    report = {'passed': False, 'state': 'RUNNING', 'checks': [], 'source_tree_sha256': fingerprint(), 'started_ms': int(time.time()*1000), 'publicTransactionsSubmitted': False, 'production_ready': False, 'all_requested_features_complete': False}
    target = OUT / 'report.json'
    def save():
        target.write_text(json.dumps(report, indent=2) + '\n')
    save()
    for name, command in checks:
        print(f'Running {name}', flush=True)
        with (OUT / f'{name}.log').open('wb') as log:
            result = subprocess.run(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT)
        log_text = (OUT / f'{name}.log').read_text(errors='replace')
        test_executions = sum(int(n) for n in re.findall(r'test result: ok\. (\d+) passed;', log_text))
        report['checks'].append({'name': name, 'exit_code': result.returncode, 'rust_test_executions': test_executions})
        save()
        if result.returncode:
            report['state'] = 'FAILED'
            save()
            raise SystemExit(f'{name} failed: {OUT / (name + ".log")}')
    if report['source_tree_sha256'] != fingerprint():
        report['state'] = 'SOURCE_CHANGED'
        save()
        raise SystemExit('Source changed during validation; rerun on the final source')
    report.update(passed=True, rust_test_executions=sum(c['rust_test_executions'] for c in report['checks']), state='LOCAL_SCOPE_VERIFIED', verification_scope='PINNED_WEIGHTED_GIVEN_OUT_CALLBACK_API_FRONTEND', finished_ms=int(time.time()*1000), reference=json.loads((ROOT / 'integration/artifacts/local/weighted-exact-out-generation.json').read_text()), browser=json.loads((OUT / 'browser/report.json').read_text()))
    report.update(cases=report['reference']['cases'], shared_sequences=report['reference']['sequences'], sequence_steps=report['reference']['steps'], browser_checks=len(report['browser']['checks']))
    save()
    print(json.dumps({'passed': True, 'checks': len(checks), 'browser_checks': len(report['browser']['checks'])}))

if __name__ == '__main__':
    main()
