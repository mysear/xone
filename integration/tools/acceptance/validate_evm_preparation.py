#!/usr/bin/env python3
"""Finite component acceptance; no RPC authority, signing service or broadcast."""
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
BASE = ROOT / 'integration/artifacts/local/evm-preparation-component-check'


def reference_inputs(inventory):
    inputs = json.loads(Path(inventory).read_bytes())['inputs']
    for path, expected in inputs.items():
        if shared.sha_file(path) != expected:
            raise ValueError('Independent reference input changed: ' + path)
    return inputs


def main():
    out = BASE / 'runs' / uuid.uuid4().hex
    out.mkdir(parents=True, exist_ok=False)
    (out / 'in').mkdir()
    (out / 'out').mkdir()
    env = dict(os.environ, PATH=str(Path.home() / '.cargo/bin') + os.pathsep + os.environ['PATH'])
    env.setdefault('CARGO_BUILD_JOBS', '1')
    env.setdefault('RUST_TEST_THREADS', '1')
    env.setdefault('XONE_EVM_REFERENCE_MODULES', str(ROOT / 'integration/.runtime/stable-reference/node_modules'))
    env.update(XONE_EVM_COMPONENT_RUN=str(out), XONE_EVM_PREPARATION_DIR=str(out / 'in'),
               XONE_EVM_EXACT_OUT_PREPARATION_DIR=str(out / 'out'))
    cargo = ['cargo', '+1.90.0']
    manifest = ['--locked', '--manifest-path', 'dex-router/Cargo.toml']
    py = [sys.executable]
    checks = [
        ('format', cargo + ['fmt', '--all', '--check', '--manifest-path', 'dex-router/Cargo.toml']),
        ('contracts', py + ['integration/tools/ci/contracts.py']),
        ('progress', py + ['-m', 'unittest', 'discover', '-s', 'integration/tools/ci/tests', '-p', 'test_progress_document.py']),
        ('progress-optimized', py + ['-OO', '-m', 'unittest', 'discover', '-s', 'integration/tools/ci/tests', '-p', 'test_progress_document.py']),
        ('reference-inventory', ['node', 'integration/tools/acceptance/evm_preparation_interop.cjs', '--inventory', str(out / 'reference-inputs.json')]),
        ('component', cargo + ['test'] + manifest + ['-p', 'xone-execution-preparation', '--test', 'evm_preparation', '--test', 'evm_exact_out']),
        ('independent-interop', ['node', 'integration/tools/acceptance/evm_preparation_interop.cjs', str(out / 'in'), str(out / 'out'), str(out / 'interop.json')]),
        ('reference-controls', py + ['-m', 'unittest', 'discover', '-s', 'integration/tools/acceptance/tests', '-p', 'test_evm_preparation_interop.py']),
        ('reference-controls-optimized', py + ['-OO', '-m', 'unittest', 'discover', '-s', 'integration/tools/acceptance/tests', '-p', 'test_evm_preparation_interop.py']),
        ('plan-regression', cargo + ['test'] + manifest + ['-p', 'xone-execution-plan']),
        ('capability-boundaries', cargo + ['test'] + manifest + ['-p', 'xone-protocol-math', '--test', 'capabilities']),
        ('api-regression', cargo + ['test'] + manifest + ['-p', 'xone-quote-api', '--test', 'api', '--test', 'exact_out']),
        ('lint', cargo + ['clippy'] + manifest + ['-p', 'xone-execution-plan', '-p', 'xone-execution-preparation', '--all-targets', '--', '-D', 'warnings']),
    ]
    required = [
        'every_original_router_output_case_binds_complete_paths_and_uint256_preparation',
        'unsigned_exact_input_keeps_uint256_amounts_and_full_bound_plan',
        'quantities_are_lossless_strings_and_rpc_uses_canonical_hex',
        'quantity_json_rejects_numbers_overflow_and_noncanonical_strings',
        'nonce_exhaustion_rejects_the_unusable_u64_maximum',
        'nonce_last_usable_value_is_preserved_without_integer_narrowing',
        'nonce_exhaustion_rejects_exact_output_before_producing_wire',
        'transaction_fee_zero_context_is_valid_without_fabricated_network_cost',
        'transaction_fee_context_rejects_gas_below_calldata_intrinsic_lower_bound',
    ]
    report = dict(passed=False, state='RUNNING', source_tree_sha256=shared.fingerprint(),
                  started_ms=int(time.time() * 1000), run_directory=str(out.relative_to(ROOT)),
                  checks=[], test_inputs={}, reference_inputs={}, publicTransactionsSubmitted=False,
                  production_ready=False, all_requested_features_complete=False,
                  verification_scope='EVM_BOUND_PLAN_AND_PREPARATION_COMPONENT_LOCAL')

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
            missing = [test for test in required if test not in found] if name == 'component' else []
            if missing or (name == 'component' and count < 19):
                code = 1
            report['checks'].append(dict(name=name, exit_code=code, rust_test_executions=count,
                                         missing_required_tests=missing, log_sha256=hashlib.sha256(raw).hexdigest()))
            save()
            if code:
                raise RuntimeError(f'{name} failed: {out / (name + ".log")}')
            if name == 'reference-inventory':
                report['reference_inputs'] = reference_inputs(out / 'reference-inputs.json')
                save()
        interop = json.loads((out / 'interop.json').read_bytes())
        if interop.get('passed') is not True or interop['cases'] != 95 or interop['exactInCases'] != 5 or interop['exactOutCases'] != 90:
            raise ValueError('Incomplete original ABI/RLP/signature matrix')
        if interop['inputs'] != report['reference_inputs']:
            raise ValueError('Independent reference closure differs from inventory')
        if shared.fingerprint() != report['source_tree_sha256'] or shared.test_inputs() != report['test_inputs']:
            raise ValueError('Source or fixed inputs changed during validation')
        if reference_inputs(out / 'reference-inputs.json') != report['reference_inputs']:
            raise ValueError('Independent runtime changed during validation')
        report.update(passed=True, state='LOCAL_SCOPE_VERIFIED', independent_cases=95,
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
    print(json.dumps({key: report[key] for key in ['state', 'rust_test_executions', 'independent_cases', 'run_directory']}))


if __name__ == '__main__':
    main()
