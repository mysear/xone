#!/usr/bin/env python3
"""Same-source finite acceptance of order ExactOut, including real service plans."""
from pathlib import Path
import json
import os
import re
import secrets
import subprocess
import sys
import time
from weighted_evidence import ROOT, OUT, source_hash, sha, write_result

TARGET = OUT / 'core-meteora-orders-exact-out-validation-20261111.json'
TOOLS = [
    'integration/tools/acceptance/validate_meteora_orders_exact_out.py',
    'integration/tools/acceptance/meteora_orders_exact_out_reference.py',
    'integration/tools/acceptance/tests/test_meteora_orders_exact_out_reference.py',
    'integration/tools/acceptance/solana_expanded_smoke.py',
    'integration/tools/acceptance/solana_expanded_reference.py',
    'integration/tools/ci/meteora-orders-exact-out-reference.py',
    'integration/tools/ci/meteora-orders-reference.py',
    'integration/tools/ci/meteora-orders-preflight.py',
    'integration/tools/ci/meteora-mm-state.py',
    'integration/tools/reference/meteora-dlmm/harness/native_extended.py',
    'integration/tools/reference/meteora-dlmm/source-metadata.json',
    'integration/artifacts/local/core-meteora-orders-exact-out-original-20261111.json',
    'integration/test/contracts/meteora-orders-exact-out.json',
]

def inputs():
    return {p: sha(ROOT / p) for p in TOOLS}

def main():
    env = dict(os.environ, PATH=str(Path.home() / '.cargo/bin') + os.pathsep + os.environ['PATH'], CARGO_BUILD_JOBS='2')
    python = [sys.executable]
    def cargo(command, service='dex-router'):
        return ['cargo', '+1.90.0', command, '--locked', '--manifest-path', service + '/Cargo.toml']
    def tests(crate, *names, service='dex-router', lint=False):
        args = cargo('clippy' if lint else 'test', service) + ['-p', crate]
        for name in names:
            args += ['--test', name]
        return args + (['--', '-D', 'warnings'] if lint else [])
    checks = [
        ('router-format', ['cargo', '+1.90.0', 'fmt', '--all', '--check', '--manifest-path', 'dex-router/Cargo.toml']),
        ('parser-format', ['cargo', '+1.90.0', 'fmt', '--all', '--check', '--manifest-path', 'chain-parser-rust/Cargo.toml']),
        ('protocol', python + ['common/protoc/check.py']),
        ('contracts', python + ['integration/tools/ci/contracts.py']),
        ('pinned-source', python + ['integration/tools/reference/meteora-dlmm/harness/verify_source.py']),
        ('original-reexecution', python + ['integration/tools/acceptance/meteora_orders_exact_out_reference.py', '--reexecute']),
        ('reference-controls', python + ['-m', 'unittest', 'discover', '-s', 'integration/tools/acceptance/tests', '-p', 'test_meteora_orders_exact_out_reference.py']),
        ('reference-controls-optimized', python + ['-OO', '-m', 'unittest', 'discover', '-s', 'integration/tools/acceptance/tests', '-p', 'test_meteora_orders_exact_out_reference.py']),
        ('progress', python + ['-m', 'unittest', 'discover', '-s', 'integration/tools/ci/tests', '-p', 'test_progress_document.py']),
        ('progress-optimized', python + ['-OO', '-m', 'unittest', 'discover', '-s', 'integration/tools/ci/tests', '-p', 'test_progress_document.py']),
        ('parser-tests', tests('xone-solana-ingest', 'meteora_orders', 'meteora', service='chain-parser-rust')),
        ('math-tests', tests('xone-protocol-math', 'meteora_orders', 'meteora_exact_out', 'meteora_differential', 'meteora_only_y', 'meteora_bitmap', 'meteora_rewards', 'meteora_token2022')),
        ('routing-tests', tests('xone-routing', 'meteora_orders_exact_out', 'meteora_exact_out', 'search')),
        ('optimizer-tests', tests('xone-optimizer', 'meteora_orders_exact_out', 'meteora_exact_out', 'meteora')),
        ('api-tests', tests('xone-quote-api', 'meteora_extended', 'meteora_exact_out', 'meteora_only_y')),
        ('parser-all-targets', cargo('check', 'chain-parser-rust') + ['--workspace', '--all-targets']),
        ('router-all-targets', cargo('check') + ['--workspace', '--all-targets']),
        ('parser-lint', tests('xone-solana-ingest', 'meteora_orders', service='chain-parser-rust', lint=True)),
        ('router-lint', cargo('clippy') + ['--workspace', '--lib', '--bins', '--example', 'meteora-exact-out-plan-projector', '--', '-D', 'warnings']),
        ('math-tests-lint', tests('xone-protocol-math', 'meteora_orders', lint=True)),
        ('routing-tests-lint', tests('xone-routing', 'meteora_orders_exact_out', lint=True)),
        ('optimizer-tests-lint', tests('xone-optimizer', 'meteora_orders_exact_out', lint=True)),
        ('api-tests-lint', tests('xone-quote-api', 'meteora_extended', lint=True)),
        ('go-domain', ['go', '-C', 'chain-parser-go', 'test', './internal/domain']),
        ('go-build', ['go', '-C', 'chain-parser-go', 'build', '-trimpath', '-o', 'bin/fixture-parser-go', './test/cmd/fixture-parser-go']),
        ('parser-build', cargo('build', 'chain-parser-rust') + ['--bin', 'chain-parser-rust']),
        ('router-build', cargo('build') + ['--bin', 'dex-router', '--example', 'meteora-exact-out-plan-projector']),
        ('normal-orders-exact-out', python + ['integration/tools/acceptance/solana_expanded_smoke.py', '--family', 'orders-exact-out']),
    ]
    if TARGET.exists():
        backup = TARGET.with_name('phase53-validation-prior-' + sha(TARGET) + '.json')
        if not backup.exists():
            backup.write_bytes(TARGET.read_bytes())
    tag = secrets.token_hex(6)
    source = source_hash()
    helpers = inputs()
    report = dict(schema_version=1, passed=False, state='RUNNING', source_tree_sha256=source,
        helper_sha256=helpers, checks=[], all_requested_features_complete=False, production_ready=False)
    write_result(TARGET, report)
    for name, command in checks:
        log = OUT / f'phase53-{tag}-{name}.log'
        started = time.monotonic()
        with log.open('w') as out:
            run = subprocess.run(command, cwd=ROOT, env=env, stdout=out, stderr=subprocess.STDOUT)
        check = dict(name=name, command=command, exit_code=run.returncode,
            duration_ms=round((time.monotonic() - started) * 1000),
            rust_test_executions=sum(map(int, re.findall(r'test result: ok\. (\d+) passed;', log.read_text(errors='replace')))),
            log=dict(path=str(log.relative_to(ROOT)), sha256=sha(log)))
        report['checks'].append(check)
        write_result(TARGET, report)
        print(json.dumps(dict(check=name, exit_code=run.returncode)), flush=True)
        if run.returncode:
            report.update(state='FAILED', source_tree_sha256_after=source_hash())
            write_result(TARGET, report)
            raise RuntimeError('ORDERS_EXACT_OUT_CHECK_FAILED:' + name)
    path = OUT / 'core-solana-expanded-orders-exact-out-normal-process.json'
    normal = json.loads(path.read_bytes())
    if not (normal['passed'] is True and normal['normal_exact_out_exercised'] is True and
            normal['source_tree_sha256'] == normal['source_tree_sha256_after'] == source and
            normal['api_records'] == normal['original_sbf_swaps'] == 9):
        raise ValueError('ORDERS_EXACT_OUT_NORMAL_RECEIPT')
    for artifact in normal['artifacts'].values():
        if sha(ROOT / artifact['path']) != artifact['sha256']:
            raise ValueError('ORDERS_EXACT_OUT_ARTIFACT_CHANGED')
    frozen = OUT / ('phase53-normal-frozen-' + sha(path) + '.json')
    frozen.write_bytes(path.read_bytes())
    if source_hash() != source or inputs() != helpers:
        report.update(state='FAILED', error='SOURCE_OR_HELPER_CHANGED')
        write_result(TARGET, report)
        raise ValueError('SOURCE_OR_HELPER_CHANGED')
    report.update(passed=True, state='LOCAL_SCOPE_VERIFIED', source_tree_sha256_after=source,
        normal_process=dict(path=str(frozen.relative_to(ROOT)), sha256=sha(frozen)),
        rust_test_executions=sum(c['rust_test_executions'] for c in report['checks']))
    write_result(TARGET, report)
    print(json.dumps({k: report[k] for k in ['passed', 'state', 'rust_test_executions']}))

if __name__ == '__main__':
    main()
