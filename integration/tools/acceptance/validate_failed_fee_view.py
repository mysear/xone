#!/usr/bin/env python3
"""Freeze failed receipt fee display and successful settlement control."""
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
BASE = ROOT / 'integration/artifacts/local/failed-fee-view-check'


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
    bundled = Path.home() / '.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright'
    if bundled.exists():
        env.setdefault('PLAYWRIGHT_MODULE', str(bundled))
    env.update(XONE_BROWSER_OUTPUT=str(out / 'browser'), XONE_TEST_RPC_CAPTURE='1',
               XONE_TEST_SUBMISSION_DIR=str(out / 'browser/receipts'),
               XONE_TEST_SBF_MANIFEST_DIR=str(ROOT / 'dex-router/test/contracts/sbf-runtime'),
               XONE_HARNESS_BIN=str(Path(env['CARGO_TARGET_DIR']) / 'debug/examples/frontend_harness'))
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
        ('frontend-syntax', ['npm', 'run', 'check', '--prefix', 'frontend']),
        ('frontend-unit', ['npm', 'test', '--prefix', 'frontend']),
        ('harness-lint', cargo + ['clippy'] + manifest + ['-p', 'xone-quote-api', '--example', 'frontend_harness', '--', '-D', 'warnings']),
        ('harness-build', cargo + ['build'] + manifest + ['-p', 'xone-quote-api', '--example', 'frontend_harness']),
        ('actual-browser', ['node', 'frontend/test/failed-fee-browser.mjs']),
    ]
    required = {}
    report = dict(passed=False, state='RUNNING', source_tree_sha256=shared.fingerprint(), run_directory=str(out.relative_to(ROOT)), started_ms=int(time.time() * 1000), checks=[], publicTransactionsSubmitted=False, production_ready=False, all_requested_features_complete=False, verification_scope='FAILED_FEE_UI_ORIGINAL_SIGNED_LOCAL_TRANSACTIONS_AND_SUCCESS_CONTROL')

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
            if name == 'actual-browser':
                report['browser_harness_sha256_before'] = shared.sha_file(env['XONE_HARNESS_BIN'])
                save()
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
        if report['browser_harness_sha256_before'] != shared.sha_file(env['XONE_HARNESS_BIN']):
            raise ValueError('Harness executable changed during actual browser execution')
        artifact = out / 'browser/result.json'
        audit = json.loads(artifact.read_bytes())
        expected = {f'{name}_{suffix}' for name in ['native-success-control', 'ordinary-token-failed', 'native-ata-failed', 'wrapped-refund-failed'] for suffix in ['actual_sbf', 'receipt_fee_visible', 'reload_without_resend']}
        expected.add('failure_fee_mobile_visible_without_overflow')
        if audit.get('passed') is not True or set(audit.get('checks', [])) != expected:
            raise ValueError('Missing actual browser economics/receipt checks')
        from solana_native_economics_evidence import verify_costs
        economics = []
        artifacts = {}
        for name in ['native-success-control', 'ordinary-token-failed', 'native-ata-failed', 'wrapped-refund-failed']:
            path = out / ('browser/' + name + '.json')
            data = json.loads(path.read_bytes())
            e, p, receipt = (data[k] for k in ['evidence', 'preparation', 'receipt'])
            if name == 'native-success-control':
                economics.append(verify_costs(receipt, p, e, 'RPC_CAPTURE_SBF_EXECUTION', e['transactionSha256']))
                verify_costs(receipt, p, receipt['settlement'], 'RPC_TRANSACTION_RECEIPT', receipt['transactionSha256'])
                if receipt['state'] != 'FINALIZED' or receipt['failureFee'] is not None:
                    raise ValueError('Successful receipt was changed to failure')
            else:
                from solana_native_cost_evidence import amount, require
                f = receipt['failureFee']
                require(receipt['state'] == 'FAILED' and receipt['confirmationStatus'] == 'finalized' and receipt['settlement'] is None, 'Failure promoted to settlement')
                require(f['chainKey'] == receipt['chainKey'] and f['signature'] == receipt['signature'] and f['preparationHash'] == p['preparationHash'] == receipt['preparationHash'] and f['transactionSha256'] == receipt['transactionSha256'] and f['slot'] == receipt['slot'], 'UI fee transaction binding changed')
                require(f['modelVersion'] == 'xone-solana-failed-fee/1' and f['sourceAuthority'] == 'RPC_TRANSACTION_RECEIPT' and f['nativeUnit'] == 'LAMPORTS', 'UI fee source changed')
                require(amount(f['networkFeeNative']) == amount(f['payerNativeBefore']) - amount(f['payerNativeAfter']) == 5000 and f['payerLamportDelta'] == '-5000' and f['retainedRentDepositNative'] == '0', 'Failure fee/native cash changed')
                require(f['onchainError'] == receipt['error']['detail'] and f['netBuyAmountEstimate'] is None and f['netSellAmountEstimate'] is None, 'Failure error/net changed')
                require(f['observedNativeRollbackMatched'] is True and f['observedTokenRollbackMatched'] is True and all(f[k] is False for k in ['settlementApplied', 'quoteRankingApplied', 'historicalBankVerified', 'independentProductionVerification']), 'Failure qualification changed')
            artifacts[str(path.relative_to(ROOT))] = shared.sha_file(path)
            execution_picture = out / ('browser/' + name + '-execution.png')
            artifacts[str(execution_picture.relative_to(ROOT))] = shared.sha_file(execution_picture)
            picture = out / ('browser/' + name + '.png')
            artifacts[str(picture.relative_to(ROOT))] = shared.sha_file(picture)
        mobile = out / 'browser/mobile.png'
        artifacts[str(mobile.relative_to(ROOT))] = shared.sha_file(mobile)
        report.update(passed=True, state='LOCAL_SCOPE_VERIFIED', browser_checks=len(expected), actual_api_scenarios=4, original_sbf_executions=4, signed_local_receipts=4, failed_receipts=3, successful_control_receipts=1, economics=economics, artifacts=artifacts, actual_artifact_report=str(artifact.relative_to(ROOT)), actual_artifact_report_sha256=shared.sha_file(artifact), harness_binary_sha256=shared.sha_file(env['XONE_HARNESS_BIN']))
    except KeyboardInterrupt:
        report.update(state='CANCELLED', error='INTERRUPTED')
        raise
    except Exception as error:
        report.update(state='FAILED', error=str(error))
        raise
    finally:
        report['finished_ms'] = int(time.time() * 1000)
        save()
    print(json.dumps({k: report[k] for k in ['state', 'browser_checks', 'run_directory']}))


if __name__ == '__main__':
    main()
