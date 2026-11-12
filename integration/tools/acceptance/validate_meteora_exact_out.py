#!/usr/bin/env python3
"""Run the bounded, same-source Meteora ExactOut acceptance and freeze its receipt."""
import json
import os
from pathlib import Path
import re
import secrets
import subprocess
import sys
import time

from weighted_evidence import ROOT, OUT, source_hash, sha, write_result
from meteora_exact_out_smoke import TARGET as PROCESS

TARGET = OUT / 'core-meteora-exact-out-validation-20261111.json'


def validate():
    env = dict(os.environ, PATH=str(Path.home() / '.cargo/bin') + os.pathsep + os.environ['PATH'], CARGO_BUILD_JOBS='2')
    cargo = ['cargo', '+1.90.0']
    router = ['--locked', '--manifest-path', 'dex-router/Cargo.toml']
    packages = ['-p', 'xone-protocol-math', '-p', 'xone-routing', '-p', 'xone-optimizer', '-p', 'xone-quote-api']
    reference_test = ['-m', 'unittest', 'discover', '-s', 'integration/tools/acceptance/tests', '-p', 'test_meteora_exact_out_reference.py']
    progress_test = ['-m', 'unittest', 'discover', '-s', 'integration/tools/ci/tests', '-p', 'test_progress_document.py']
    checks = [
        ('format', cargo + ['fmt', '--all', '--check', '--manifest-path', 'dex-router/Cargo.toml']),
        ('contracts', [sys.executable, 'integration/tools/ci/contracts.py']),
        ('reference-original', [sys.executable, 'integration/tools/acceptance/meteora_exact_out_reference.py', '--reexecute']),
        ('reference-controls', [sys.executable] + reference_test),
        ('reference-controls-optimized', [sys.executable, '-OO'] + reference_test),
        ('progress', [sys.executable] + progress_test),
        ('progress-optimized', [sys.executable, '-OO'] + progress_test),
        ('new-and-meteora-exact-in', cargo + ['test'] + router + packages + ['--test', 'meteora_exact_out', '--test', 'meteora', '--test', 'meteora_differential']),
    ]
    for target in ['exact_out', 'lfj_exact_out', 'slipstream_exact_out', 'pancake_lm_exact_out', 'pancake_bsc_exact_out']:
        checks.append((target, cargo + ['test'] + router + packages + ['--test', target]))
    checks.extend([
        ('production-and-projector-lint', cargo + ['clippy'] + router + ['--workspace', '--lib', '--bins', '--example', 'meteora-exact-out-plan-projector', '--', '-D', 'warnings']),
        ('new-tests-lint', cargo + ['clippy'] + router + packages + ['--test', 'meteora_exact_out', '--', '-D', 'warnings']),
        ('go-fixture-build', ['go', '-C', 'chain-parser-go', 'build', '-trimpath', '-o', 'bin/fixture-parser-go', './test/cmd/fixture-parser-go']),
        ('parser-build', cargo + ['build', '--locked', '--manifest-path', 'chain-parser-rust/Cargo.toml', '--bin', 'chain-parser-rust']),
        ('router-build', cargo + ['build'] + router + ['--bin', 'dex-router', '--example', 'meteora-exact-out-plan-projector']),
        ('normal-process-original-plan-replay', [sys.executable, 'integration/tools/acceptance/meteora_exact_out_smoke.py']),
    ])
    OUT.mkdir(parents=True, exist_ok=True)
    if TARGET.is_file():
        previous = OUT / f'core-meteora-exact-out-validation-prior-{sha(TARGET)}.json'
        if not previous.exists():
            previous.write_bytes(TARGET.read_bytes())
    tag = secrets.token_hex(6)
    report = dict(passed=False, state='RUNNING', checks=[], source_tree_sha256=source_hash(), tool_sha256=sha(Path(__file__)), all_requested_features_complete=False, production_ready=False)
    write_result(TARGET, report)
    for name, command in checks:
        log = OUT / f'meteora-xo-validation-{tag}-{name}.log'
        started = time.monotonic()
        with log.open('w') as stream:
            result = subprocess.run(command, cwd=ROOT, env=env, stdout=stream, stderr=subprocess.STDOUT)
        text = log.read_text(errors='replace')
        report['checks'].append(dict(name=name, command=command, exit_code=result.returncode,
                                     duration_ms=round((time.monotonic() - started) * 1000),
                                     rust_test_executions=sum(map(int, re.findall(r'test result: ok\. (\d+) passed;', text))),
                                     log=dict(path=str(log.relative_to(ROOT)), sha256=sha(log))))
        if result.returncode:
            report.update(state='FAILED', source_tree_sha256_after=source_hash())
        write_result(TARGET, report)
        print(json.dumps(dict(check=name, exit_code=result.returncode)), flush=True)
        if result.returncode:
            raise RuntimeError('METEORA_EXACT_OUT_VALIDATION_FAILED:' + name)
    process = json.loads(PROCESS.read_bytes())
    after = source_hash()
    if not (process.get('passed') is True and after == report['source_tree_sha256'] == process['source_tree_sha256'] == process['source_tree_sha256_after']):
        report.update(state='EVIDENCE_MISMATCH', source_tree_sha256_after=after)
        write_result(TARGET, report)
        raise ValueError('METEORA_EXACT_OUT_FINAL_BINDING')
    receipt = OUT / f'core-meteora-exact-out-process-{after}-{sha(PROCESS)}.json'
    if not receipt.exists():
        with receipt.open('xb') as handle:
            handle.write(PROCESS.read_bytes())
    report.update(passed=True, state='LOCAL_SCOPE_VERIFIED', source_tree_sha256_after=after,
                  rust_test_executions=sum(c['rust_test_executions'] for c in report['checks']),
                  process_receipt=dict(path=str(receipt.relative_to(ROOT)), sha256=sha(receipt)))
    write_result(TARGET, report)
    return report


if __name__ == '__main__':
    result = validate()
    print(json.dumps({k: result[k] for k in ['passed', 'state', 'source_tree_sha256', 'rust_test_executions']}))
