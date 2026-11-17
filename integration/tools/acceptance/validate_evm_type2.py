#!/usr/bin/env python3
"""Core execution gates; multi-hop original references and worker/API binding remain explicit."""
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
BASE = ROOT / 'integration/artifacts/local/evm-type2-simulation-check'


def main():
    out = BASE / 'runs' / uuid.uuid4().hex
    out.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, PATH=str(Path.home() / '.cargo/bin') + os.pathsep + os.environ['PATH'])
    env.setdefault('CARGO_BUILD_JOBS', '1')
    env.setdefault('RUST_TEST_THREADS', '1')
    env.setdefault('XONE_SOLANA_PYTHON_PATH', str(ROOT / 'integration/artifacts/local/r01-solana-rebuild/python'))
    cargo = ['cargo', '+1.90.0']
    manifest = ['--locked', '--manifest-path', 'dex-router/Cargo.toml']
    py = [sys.executable]
    checks = [
        ('format', cargo + ['fmt', '--all', '--check', '--manifest-path', 'dex-router/Cargo.toml']),
        ('contracts', py + ['integration/tools/ci/contracts.py']),
        ('progress', py + ['-m', 'unittest', 'discover', '-s', 'integration/tools/ci/tests', '-p', 'test_progress_document.py']),
        ('progress-optimized', py + ['-OO', '-m', 'unittest', 'discover', '-s', 'integration/tools/ci/tests', '-p', 'test_progress_document.py']),
        ('simulation', cargo + ['test'] + manifest + ['-p', 'xone-simulation']),
        ('worker-regression', cargo + ['test'] + manifest + ['-p', 'xone-quote-api', '--lib', 'local_simulation::']),
        ('api-regression', cargo + ['test'] + manifest + ['-p', 'xone-quote-api', '--test', 'local_simulation', '--test', 'simulation_request']),
        ('lint', cargo + ['clippy'] + manifest + ['-p', 'xone-simulation', '--all-targets', '--', '-D', 'warnings']),
    ]
    required = [
        'typed_exact_in_measures_native_fee_and_preserves_original_token_balance_deltas',
        'typed_exact_output_runs_original_router_and_measures_exact_target_and_input',
        'typed_execution_caller_as_beneficiary_separates_credit_from_actual_gas_charge',
        'typed_execution_fee_caps_and_missing_storage_fail_without_source_mutation',
        'typed_revert_charges_only_actual_native_gas_and_rolls_back_token_transfers',
        'typed_zero_fee_counterfactual_does_not_fabricate_a_native_network_charge',
        'type2_returned_gasprice_and_native_balances_use_the_effective_fee',
        'original_24_transactions_match_real_gas_eth_nonces_code_and_all_declared_slots',
    ]
    report = dict(passed=False, state='RUNNING', source_tree_sha256=shared.fingerprint(),
                  test_inputs={}, runtime_inputs={}, checks=[], run_directory=str(out.relative_to(ROOT)),
                  started_ms=int(time.time() * 1000), publicTransactionsSubmitted=False,
                  production_ready=False, all_requested_features_complete=False,
                  multi_hop_original_reference_verified=False, prepared_wire_bound=False,
                  typed_worker_api_integrated=False,
                  verification_scope='SHANGHAI_PROVIDED_STATE_TYPE2_CORE_CHECKS')

    def save():
        text = json.dumps(report, indent=2) + '\n'
        (out / 'report.json').write_text(text)
        temp = BASE / 'report.tmp'
        temp.write_text(text)
        temp.replace(BASE / 'report.json')

    def interrupted(signum, frame):
        raise KeyboardInterrupt(f'Interrupted by signal {signum}')

    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    save()
    try:
        report.update(test_inputs=shared.test_inputs(), runtime_inputs=shared.runtime_inputs())
        save()
        for name, args in checks:
            print('Running ' + name, flush=True)
            with (out / (name + '.log')).open('wb') as log:
                child = subprocess.Popen(args, cwd=ROOT, env=env, stdout=log,
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
            passed = set(re.findall(r'^test ([A-Za-z0-9_:]+) \.\.\. ok$', text, re.M))
            missing = [test for test in required if test not in passed] if name == 'simulation' else []
            if missing:
                code = 1
            report['checks'].append(dict(name=name, exit_code=code, rust_test_executions=count,
                                        missing_required_tests=missing, log_sha256=hashlib.sha256(raw).hexdigest()))
            save()
            if code:
                raise RuntimeError(f'{name} failed: {out / (name + ".log")}')
        if (shared.fingerprint() != report['source_tree_sha256']
                or shared.test_inputs() != report['test_inputs']
                or shared.runtime_inputs() != report['runtime_inputs']):
            raise ValueError('Source or fixed/runtime inputs changed')
        # This gate does not close the explicitly remaining stage-89 obligations.
        report.update(passed=True, state='PARTIAL_LOCAL',
                      rust_test_executions=sum(c['rust_test_executions'] for c in report['checks']))
    except KeyboardInterrupt:
        report.update(state='CANCELLED', error='INTERRUPTED')
        raise
    except Exception as error:
        report.update(state='FAILED', error=str(error))
        raise
    finally:
        report['finished_ms'] = int(time.time() * 1000)
        save()
    print(json.dumps({k: report[k] for k in ['state', 'rust_test_executions', 'run_directory']}))


if __name__ == '__main__':
    main()
