#!/usr/bin/env python3
"""Freeze signed Solana fee-context propagation, durable witnesses and funding wallets."""
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
BASE = ROOT / 'integration/artifacts/local/solana-fee-preflight-check'


def runtime_inputs():
    path = ROOT / 'dex-router/test/contracts/sbf-runtime/rpc-capture.json'
    manifest = json.loads(path.read_bytes())
    result = {str(path.relative_to(ROOT)): shared.sha_file(path)}
    declared = [(Path(manifest['python_executable']['path']), manifest['python_executable']['sha256'])]
    declared += [(Path(manifest['runtime_directory']) / name, digest) for name, digest in manifest['runtime_files'].items()]
    for path, expected in declared:
        actual = shared.sha_file(path)
        if actual != expected:
            raise ValueError('Pinned SBF runtime changed: ' + str(path))
        result[str(path)] = actual
    return result


def main():
    out = BASE / 'runs' / uuid.uuid4().hex
    out.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, PATH=str(Path.home() / '.cargo/bin') + os.pathsep + os.environ['PATH'])
    env.setdefault('CARGO_BUILD_JOBS', '2')
    env.setdefault('CARGO_TARGET_DIR', str(ROOT / 'integration/.runtime/design-review-target'))
    env.setdefault('RUST_TEST_THREADS', '1')
    env.setdefault('XONE_SOLANA_PYTHON_PATH', str(ROOT / 'integration/artifacts/local/r01-solana-rebuild/python'))
    if sys.platform == 'darwin':
        import platform
        triple = ('aarch64' if platform.machine() == 'arm64' else 'x86_64') + '_APPLE_DARWIN'
        env.setdefault('CARGO_TARGET_' + triple.upper() + '_RUNNER', str(ROOT / 'integration/tools/ci/native-test-runner.py'))
    cargo = ['cargo', '+1.90.0']
    manifest = ['--locked', '--manifest-path', 'dex-router/Cargo.toml']
    py = [sys.executable]
    checks = [
        ('format', cargo + ['fmt', '--all', '--check', '--manifest-path', 'dex-router/Cargo.toml']),
        ('contracts', py + ['integration/tools/ci/contracts.py']),
        ('progress', py + ['-m', 'unittest', 'discover', '-s', 'integration/tools/ci/tests', '-p', 'test_progress_document.py']),
        ('progress-optimized', py + ['-OO', '-m', 'unittest', 'discover', '-s', 'integration/tools/ci/tests', '-p', 'test_progress_document.py']),
        ('receipt-and-fee-guards', cargo + ['test'] + manifest + ['-p', 'xone-quote-api', '--lib']),
        ('actual-signed-wallet', cargo + ['test'] + manifest + ['-p', 'xone-quote-api', '--test', 'transaction_lifecycle']),
        ('native-and-wrapped-funding-wallets', cargo + ['test'] + manifest + ['-p', 'xone-quote-api', '--test', 'pump_amm_funding', '--test', 'raydium_funding', 'wallet']),
        ('native-curve-signed-receipts', cargo + ['test'] + manifest + ['-p', 'xone-quote-api', '--test', 'pump_curve_preparation', 'signed_submission']),
        ('lint', cargo + ['clippy'] + manifest + ['-p', 'xone-execution-preparation', '-p', 'xone-quote-api', '--all-targets', '--', '-D', 'warnings']),
        ('router-build', cargo + ['build'] + manifest + ['--bin', 'dex-router']),
        ('harness-build', cargo + ['build'] + manifest + ['-p', 'xone-quote-api', '--example', 'frontend_harness']),
    ]
    required = {
        'receipt-and-fee-guards': ['transaction_lifecycle::fee_context_tests::preflight_floor_accepts_legacy_and_uses_newer_bound_fee_context', 'execution_receipts::tests::fee_preflight_is_immutable_durable_and_old_v1_journals_remain_readable'],
        'actual-signed-wallet': ['wallet_submission_requires_signature_evidence_confirmation_and_durable_reconciliation'],
        'native-curve-signed-receipts': ['pump_native_curve_signed_submission_receipt_matches_real_sol_and_fee_accounts'],
        'native-and-wrapped-funding-wallets': ['pump_swap_funding_wallet_receipts_reconcile_actual_rent_and_resume_without_resending', 'raydium_funding_wallet_receipts_reconcile_actual_rent_and_resume_without_resending'],
    }
    report = dict(passed=False, state='RUNNING', source_tree_sha256=shared.fingerprint(), run_directory=str(out.relative_to(ROOT)), started_ms=int(time.time() * 1000), checks=[], publicTransactionsSubmitted=False, production_ready=False, all_requested_features_complete=False, verification_scope='SIGNED_SOLANA_PREFLIGHT_FEE_ACCOUNT_SIMULATION_SEND_CONTEXT_AND_DURABLE_WITNESS_LOCAL_BANKS')

    def save():
        text = json.dumps(report, indent=2) + '\n'
        (out / 'report.json').write_text(text)
        temporary = BASE / 'report.tmp'
        temporary.write_text(text)
        temporary.replace(BASE / 'report.json')

    def interrupted(signum, frame):
        raise KeyboardInterrupt('Interrupted by signal ' + str(signum))

    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    save()
    try:
        report.update(test_inputs=shared.test_inputs(), runtime_inputs=runtime_inputs())
        save()
        for name, command in checks:
            print('Running ' + name, flush=True)
            with (out / (name + '.log')).open('wb') as log:
                child = subprocess.Popen(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
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
            found = set(re.findall(r'^test ([A-Za-z0-9_:]+) \.\.\. ok$', text, re.M))
            missing = [test for test in required.get(name, []) if test not in found]
            if b'\0' in raw or missing:
                code = 1
            count = sum(int(n) for n in re.findall(r'test result: ok\. (\d+) passed;', text))
            report['checks'].append(dict(name=name, exit_code=code, rust_test_executions=count, missing_required_tests=missing, log_sha256=hashlib.sha256(raw).hexdigest()))
            save()
            if code:
                raise RuntimeError(name + ' failed: ' + str(out / (name + '.log')))
        if shared.fingerprint() != report['source_tree_sha256'] or shared.test_inputs() != report['test_inputs'] or runtime_inputs() != report['runtime_inputs']:
            raise ValueError('Source, fixed banks or runtime changed during validation')
        report.update(passed=True, state='LOCAL_SCOPE_VERIFIED', rust_test_executions=sum(c['rust_test_executions'] for c in report['checks']))
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
