#!/usr/bin/env python3
"""Shanghai EIP-1559 replay acceptance with measured native balances."""
import hashlib
import json
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time
import uuid
import validate_cache_reclamation as shared

ROOT = Path(__file__).resolve().parents[3]
BASE = ROOT / 'integration/artifacts/local/evm-dynamic-fees-check'


def main(*, output_base=None, verification_scope='EVM_SHANGHAI_DYNAMIC_FEE_SIMULATION_PROVIDED_LOCAL_STATE', extra_required=(), extra_report=None, extra_checks=(), required_by_check=None):
    base = output_base or BASE
    out = base / 'runs' / uuid.uuid4().hex
    out.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, PATH=str(Path.home() / '.cargo/bin') + os.pathsep + os.environ['PATH'])
    env.setdefault('CARGO_BUILD_JOBS', '2')
    env.setdefault('CARGO_TARGET_DIR', str(ROOT / 'integration/.runtime/design-review-target'))
    env.setdefault('RUST_TEST_THREADS', '1')
    if sys.platform == 'darwin':
        import platform
        triple = ('aarch64' if platform.machine() == 'arm64' else 'x86_64') + '_APPLE_DARWIN'
        env.setdefault('CARGO_TARGET_' + triple.upper() + '_RUNNER', str(ROOT / 'integration/tools/ci/native-test-runner.py'))
    env.setdefault('XONE_SOLANA_PYTHON_PATH', str(ROOT / 'integration/artifacts/local/r01-solana-rebuild/python'))
    cargo = ['cargo', '+1.90.0']
    manifest = ['--locked', '--manifest-path', 'dex-router/Cargo.toml']
    py = [sys.executable]
    checks = [
        ('format', cargo + ['fmt', '--all', '--check', '--manifest-path', 'dex-router/Cargo.toml']),
        ('contracts', py + ['integration/tools/ci/contracts.py']),
        ('progress', py + ['-m', 'unittest', 'discover', '-s', 'integration/tools/ci/tests', '-p', 'test_progress_document.py']),
        ('progress-optimized', py + ['-OO', '-m', 'unittest', 'discover', '-s', 'integration/tools/ci/tests', '-p', 'test_progress_document.py']),
        ('simulation', cargo + ['test'] + manifest + ['-p', 'xone-simulation', '--lib', '--test', 'evm_fixture', '--test', 'v2_complete', '--test', 'evm_native', '--test', 'revm_execution', '--test', 'revm_database']),
        ('api-context', cargo + ['test'] + manifest + ['-p', 'xone-quote-api', '--lib', 'local_simulation::']),
        ('preparation-regression', cargo + ['test'] + manifest + ['-p', 'xone-execution-preparation', '--test', 'evm_preparation', '--test', 'evm_exact_out', '--test', 'evm_rpc']),
        ('lint', cargo + ['clippy'] + manifest + ['-p', 'xone-simulation', '-p', 'xone-quote-api', '--all-targets', '--', '-D', 'warnings']),
    ]
    checks[-1:-1] = extra_checks
    required = [
        'dynamic_fees_observe_actual_native_balances_and_fee_cap_clipping',
        'dynamic_fee_validation_precedes_state_reads_and_rejects_insufficient_maximum_funding',
        'dynamic_fee_zero_base_and_caller_coinbase_keep_measured_deltas',
        'dynamic_fee_wire_preserves_legacy_commitments_and_rejects_numeric_priority',
        'complete_original_router_eip1559_matches_legacy_overlay_and_measured_native_fees',
    ]
    required.extend(extra_required)
    report = dict(passed=False, state='RUNNING', source_tree_sha256=shared.fingerprint(),
                  started_ms=int(time.time() * 1000), run_directory=str(out.relative_to(ROOT)),
                  checks=[], test_inputs={}, publicTransactionsSubmitted=False,
                  production_ready=False, all_requested_features_complete=False,
                  verification_scope=verification_scope, build_target_directory=env['CARGO_TARGET_DIR'])

    def save():
        text = json.dumps(report, indent=2) + '\n'
        (out / 'report.json').write_text(text)
        temp = base / 'report.tmp'
        temp.write_text(text)
        temp.replace(base / 'report.json')

    def interrupted(signum, frame):
        raise KeyboardInterrupt(f'Interrupted by signal {signum}')

    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    save()
    try:
        report['test_inputs'] = shared.test_inputs()
        save()
        for name, command in checks:
            print('Running ' + name, flush=True)
            with (out / (name + '.log')).open('wb') as log:
                child = subprocess.Popen(command, cwd=ROOT, env=env, stdout=log,
                                         stderr=subprocess.STDOUT, start_new_session=True)
                try:
                    code = child.wait()
                finally:
                    if child.poll() is None:
                        os.killpg(child.pid, signal.SIGTERM)
                        try:
                            child.wait(timeout=2)
                        except subprocess.TimeoutExpired:
                            os.killpg(child.pid, signal.SIGKILL)
                            child.wait()
            raw = (out / (name + '.log')).read_bytes()
            text = raw.decode(errors='replace')
            if b'\0' in raw:
                code = 1
            count = sum(int(n) for n in re.findall(r'test result: ok\. (\d+) passed;', text))
            found = set(re.findall(r'^test ([A-Za-z0-9_:]+) \.\.\. ok$', text, re.M))
            expected = list(required) if name == 'simulation' else []
            expected.extend((required_by_check or {}).get(name, []))
            missing = [test for test in expected if test not in found]
            if sys.platform == 'darwin' and name in ['simulation', 'api-context', 'preparation-regression'] and 'XONE_STAGED_NATIVE sha256=' not in text:
                missing.append('byte-identical native runner execution')
            if missing or (name == 'simulation' and count < len(required)):
                code = 1
            report['checks'].append(dict(name=name, exit_code=code, rust_test_executions=count,
                                         missing_required_tests=missing, log_sha256=hashlib.sha256(raw).hexdigest()))
            save()
            if code:
                raise RuntimeError(f'{name} failed: {out / (name + ".log")}')
        if shared.fingerprint() != report['source_tree_sha256'] or shared.test_inputs() != report['test_inputs']:
            raise ValueError('Source or fixed inputs changed during validation')
        report.update(passed=True, state='LOCAL_SCOPE_VERIFIED', original_router_dynamic_cases=16,
                      rust_test_executions=sum(c['rust_test_executions'] for c in report['checks']))
        report.update(extra_report or {})
    except KeyboardInterrupt:
        report.update(state='CANCELLED', error='INTERRUPTED')
        raise
    except Exception as error:
        report.update(state='FAILED', error=str(error))
        raise
    finally:
        report['finished_ms'] = int(time.time() * 1000)
        save()
    print(json.dumps({key: report[key] for key in ['state', 'rust_test_executions', 'original_router_dynamic_cases', 'run_directory']}))


if __name__ == '__main__':
    main()
