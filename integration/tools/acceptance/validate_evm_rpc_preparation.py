#!/usr/bin/env python3
"""Finite fixed-RPC/API/browser acceptance, independent execution stays false."""
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
BASE = ROOT / 'integration/artifacts/local/evm-rpc-preparation-check'


def main():
    out = BASE / 'runs' / uuid.uuid4().hex
    out.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, PATH=str(Path.home() / '.cargo/bin') + os.pathsep + os.environ['PATH'])
    env.setdefault('CARGO_BUILD_JOBS', '1')
    env.setdefault('RUST_TEST_THREADS', '1')
    env.setdefault('XONE_SOLANA_PYTHON_PATH', str(ROOT / 'integration/artifacts/local/r01-solana-rebuild/python'))
    playwright = Path.home() / '.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright'
    if playwright.exists():
        env.setdefault('PLAYWRIGHT_MODULE', str(playwright))
    env['XONE_EVM_RPC_BROWSER_OUTPUT'] = str(out / 'browser')
    cargo = ['cargo', '+1.90.0']
    manifest = ['--locked', '--manifest-path', 'dex-router/Cargo.toml']
    py = [sys.executable]
    checks = [
        ('format', cargo + ['fmt', '--all', '--check', '--manifest-path', 'dex-router/Cargo.toml']),
        ('contracts', py + ['integration/tools/ci/contracts.py']),
        ('progress', py + ['-m', 'unittest', 'discover', '-s', 'integration/tools/ci/tests', '-p', 'test_progress_document.py']),
        ('progress-optimized', py + ['-OO', '-m', 'unittest', 'discover', '-s', 'integration/tools/ci/tests', '-p', 'test_progress_document.py']),
        ('config', cargo + ['test'] + manifest + ['-p', 'xone-config', '--test', 'evm_preparation', '--test', 'config']),
        ('strict-json', cargo + ['test'] + manifest + ['-p', 'xone-execution-preparation', '--lib', 'evm_rpc_json']),
        ('rpc', cargo + ['test'] + manifest + ['-p', 'xone-execution-preparation', '--test', 'evm_rpc']),
        ('evm-api', cargo + ['test'] + manifest + ['-p', 'xone-quote-api', '--test', 'evm_preparation']),
        ('component-regression', cargo + ['test'] + manifest + ['-p', 'xone-execution-preparation', '--test', 'evm_preparation', '--test', 'evm_exact_out']),
        ('api-regression', cargo + ['test'] + manifest + ['-p', 'xone-quote-api', '--test', 'api', '--test', 'exact_out', '--test', 'execution_preparation']),
        ('frontend-check', ['npm', '--prefix', 'frontend', 'run', 'check']),
        ('frontend-unit', ['npm', '--prefix', 'frontend', 'test']),
        ('lint', cargo + ['clippy'] + manifest + ['-p', 'xone-config', '-p', 'xone-execution-preparation', '-p', 'xone-quote-api', '--all-targets', '--', '-D', 'warnings']),
        ('browser-build', cargo + ['build'] + manifest + ['-p', 'xone-quote-api', '--example', 'evm_preparation_frontend_harness']),
        ('browser', ['node', 'frontend/test/evm-preparation-browser.mjs']),
    ]
    required = {
        'strict-json': ['evm_rpc_json::tests::duplicate_members_and_trailing_data_are_rejected'],
        'rpc': ['fixed_original_state_is_checked_and_rpc_commitment_is_bound',
                'identity_state_nonce_and_envelope_faults_never_emit_transactions',
                'insufficient_funding_is_captured_without_unsigned_wire', 'budgets_and_deadlines_fail_closed'],
        'evm-api': ['explicit_evm_preparation_binds_quote_view_and_downloadable_wire',
                    'evm_prepare_auth_unknown_context_and_reorg_do_not_call_rpc',
                    'evm_rpc_rejection_and_funding_blockers_preserve_api_semantics',
                    'exact_output_preparation_uses_its_own_target_and_maximum_input'],
    }
    report = dict(passed=False, state='RUNNING', source_tree_sha256=shared.fingerprint(),
                  test_inputs={}, runtime_inputs={}, checks=[], started_ms=int(time.time() * 1000),
                  run_directory=str(out.relative_to(ROOT)), publicTransactionsSubmitted=False,
                  independentExecutionVerified=False, chainFeesComplete=False,
                  production_ready=False, all_requested_features_complete=False,
                  verification_scope='RETAINED_ORIGINAL_STATE_FIXED_RPC_API_BROWSER_LOCAL')

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
        report['runtime_inputs'] = shared.runtime_inputs()
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
            found = set(re.findall(r'^test ([A-Za-z0-9_:]+) \.\.\. ok$', text, re.M))
            missing = [test for test in required.get(name, []) if test not in found]
            if missing:
                code = 1
            report['checks'].append(dict(name=name, exit_code=code, rust_test_executions=count,
                                        missing_required_tests=missing, log_sha256=hashlib.sha256(raw).hexdigest()))
            save()
            if code:
                raise RuntimeError(f'{name} failed: {out / (name + ".log")}')
            if name == 'browser-build':
                target = Path(env.get('CARGO_TARGET_DIR', str(ROOT / 'dex-router/target')))
                binary = target / 'debug/examples/evm_preparation_frontend_harness'
                report['browser_binary_sha256'] = shared.sha_file(binary)
                retained = out / 'binaries'
                retained.mkdir()
                import shutil
                shutil.copy2(binary, retained / binary.name)
                if shared.sha_file(retained / binary.name) != report['browser_binary_sha256']:
                    raise ValueError('Retained browser executable differs')
                save()
        browser = json.loads((out / 'browser/report.json').read_bytes())
        if browser.get('passed') is not True or len(browser['checks']) != 9:
            raise ValueError('Browser qualification is incomplete')
        for kind in ['EXACT_IN', 'EXACT_OUT']:
            preparation = json.loads((out / f'browser/{kind}-preparation.json').read_bytes())
            if (preparation['ready'] is not True or preparation['quoteKind'] != kind
                    or preparation['signed'] is not False or preparation['submitted'] is not False
                    or preparation['independentExecutionVerified'] is not False):
                raise ValueError('Invalid actual browser/API preparation')
        if (shared.fingerprint() != report['source_tree_sha256']
                or shared.test_inputs() != report['test_inputs']
                or shared.runtime_inputs() != report['runtime_inputs']):
            raise ValueError('Source or test inputs changed during validation')
        artifacts = {str(path.relative_to(ROOT)): shared.sha_file(path)
                     for path in out.rglob('*') if path.is_file() and path.name != 'report.json'}
        report.update(passed=True, state='LOCAL_SCOPE_VERIFIED', browser_checks=9,
                      rust_test_executions=sum(c['rust_test_executions'] for c in report['checks']), artifacts=artifacts)
    except KeyboardInterrupt:
        report.update(state='CANCELLED', error='INTERRUPTED')
        raise
    except Exception as error:
        report.update(state='FAILED', error=str(error))
        raise
    finally:
        report['finished_ms'] = int(time.time() * 1000)
        save()
    print(json.dumps({k: report[k] for k in ['state', 'rust_test_executions', 'browser_checks', 'run_directory']}))


if __name__ == '__main__':
    main()
