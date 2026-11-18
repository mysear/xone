#!/usr/bin/env python3
"""Freeze message-bound Solana native fee/funding evidence and regressions."""
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
BASE = ROOT / 'integration/artifacts/local/solana-native-cost-check'


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
    env['XONE_SOLANA_NATIVE_COST_DIR'] = str(out / 'actual')
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
        ('native-cost', cargo + ['test'] + manifest + ['-p', 'xone-execution-preparation', '--lib', '--test', 'contract']),
        ('actual-http-sbf', cargo + ['test'] + manifest + ['-p', 'xone-quote-api', '--test', 'solana_native_cost']),
        ('independent-artifact-audit', py + ['integration/tools/acceptance/solana_native_cost_evidence.py', str(out / 'actual')]),
        ('audit-guards', py + ['-m', 'unittest', 'discover', '-s', 'integration/tools/acceptance/tests', '-p', 'test_solana_native_cost_evidence.py']),
        ('audit-guards-optimized', py + ['-OO', '-m', 'unittest', 'discover', '-s', 'integration/tools/acceptance/tests', '-p', 'test_solana_native_cost_evidence.py']),
        ('funding-regression', cargo + ['test'] + manifest + ['-p', 'xone-quote-api', '--test', 'execution_preparation', '--test', 'raydium_funding', '--test', 'pump_amm_funding', '--test', 'pump_curve_preparation']),
        ('capture-wallet-regression', cargo + ['test'] + manifest + ['-p', 'xone-quote-api', '--test', 'rpc_capture', '--test', 'transaction_lifecycle']),
        ('lint', cargo + ['clippy'] + manifest + ['-p', 'xone-execution-preparation', '-p', 'xone-quote-api', '--all-targets', '--', '-D', 'warnings']),
        ('router-build', cargo + ['build'] + manifest + ['--bin', 'dex-router']),
        ('harness-build', cargo + ['build'] + manifest + ['-p', 'xone-quote-api', '--example', 'frontend_harness']),
    ]
    required = {
        'native-cost': ['solana_native_cost::tests::solana_native_cost_binds_rpc_message_and_separates_fee_rent_and_principal', 'solana_native_cost::tests::solana_native_cost_native_input_is_principal_and_blocked_preparation_is_unavailable', 'solana_native_cost::tests::solana_native_cost_rejects_changed_wire_amounts_source_balances_and_fee_slots'],
        'actual-http-sbf': ['solana_native_cost_actual_http_binds_message_fee_funding_and_original_sbf'],
    }
    report = dict(passed=False, state='RUNNING', source_tree_sha256=shared.fingerprint(), run_directory=str(out.relative_to(ROOT)), started_ms=int(time.time() * 1000), checks=[], publicTransactionsSubmitted=False, production_ready=False, all_requested_features_complete=False, verification_scope='MESSAGE_BOUND_SOLANA_RPC_FEE_AND_NATIVE_FUNDING_CLASSIFICATION_PROVIDED_LOCAL_BANKS')

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
        artifact = out / 'actual/report.json'
        audit = json.loads(artifact.read_bytes())
        if audit.get('passed') is not True or audit['current_api_preparations'] != 20 or audit['original_sbf_executions'] != 20:
            raise ValueError('Missing current actual API/SBF audit')
        report.update(passed=True, state='LOCAL_SCOPE_VERIFIED', rust_test_executions=sum(c['rust_test_executions'] for c in report['checks']), actual_api_preparations=20, original_sbf_executions=20, actual_artifact_report=str(artifact.relative_to(ROOT)), actual_artifact_report_sha256=shared.sha_file(artifact))
    except KeyboardInterrupt:
        report.update(state='CANCELLED', error='INTERRUPTED')
        raise
    except Exception as error:
        report.update(state='FAILED', error=str(error))
        raise
    finally:
        report['finished_ms'] = int(time.time() * 1000)
        save()
    print(json.dumps({k: report[k] for k in ['state', 'rust_test_executions', 'actual_api_preparations', 'original_sbf_executions', 'run_directory']}))


if __name__ == '__main__':
    main()
