#!/usr/bin/env python3
"""Fresh, frozen local-scope gate for the published Managed V2 model.

Requires the ordinary three-service/common checks from verify.py to have passed.
The gate never promotes local reference replay to trusted chain/executor proof.
"""
from pathlib import Path
import hashlib
from collections import Counter
import json
import os
import subprocess
import time

from managed_v2_projection import ADAPTER, BANK_SHA256
from weighted_evidence import checked_runtime, runtime_fingerprint, snapshot_inputs, write_result

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / 'integration/artifacts/local'
TARGET = OUT / 'managed-v2-aum-acceptance.json'
FAULTS = ('missing-getter', 'actual-supply', 'factory-membership', 'provider-cap', 'vault-bpt-cash',
          'token-balance', 'factory-code', 'helper-code', 'fee-id-width', 'future-head')


def require(value, message):
    if not value:
        raise ValueError(message)


def validate_process(report, runtime, inputs):
    require(report.get('passed') is True and report.get('state') == 'PASSED', 'normal service scope failed')
    require(report.get('source_binding') == runtime and report.get('input_hashes') == inputs
            and report.get('inputs_unchanged') is True, 'service scope executed against different inputs/runtime')
    require(report.get('verified') is False and report.get('production_ready') is False
            and report.get('all_requested_features_complete') is False, 'local scope promoted to complete/production')
    require(report.get('original_bank_sha256') == BANK_SHA256, 'service scope bank differs')
    require(report.get('source_frame_count') == 15 and len(report.get('source_frames_checked', [])) == 15,
            'original current-date source inventory incomplete')
    quotes = report.get('quote_checks', [])
    pairs = {(row.get('actor'), row.get('case')) for row in quotes}
    expected_pairs = {(actor, case) for actor in ('router1', 'router2') for case in ('two:swap-forward', 'two:swap-reverse', 'swap-after-vault-buffer', 'recovery-token-swap')}
    require(len(quotes) == 8 and pairs == expected_pairs, 'dual replica native swap inventory incomplete')
    require(len({row.get('header') for row in report['source_frames_checked']}) == 15,
            'duplicated original source frame coverage')
    require(report.get('fault_count') == len(FAULTS), 'source fault inventory incomplete')
    checks = report.get('checks', [])
    for fault in FAULTS:
        require('scope-withdraw-and-new-generation:' + fault in checks, 'missing withdrawal/recovery: ' + fault)
    for check in ('router-restart-restores-complete-native-managed-state', 'verified-and-build-remain-disabled',
                  'selected-api-single-hops-reexecuted-by-original-vault-with-real-balances'):
        require(check in checks, 'missing service obligation: ' + check)
    require(report.get('future_original_cases_excluded'), 'future original boundary silently erased')
    rejected = report.get('paused_quote_rejections', [])
    require(len(rejected) == 6 and len({(row.get('actor'),row.get('header')) for row in rejected}) == 6
            and {row.get('actor') for row in rejected} == {'router1','router2'}
            and all(row.get('status') in (400,404,422) for row in rejected),
            'original paused Vault readiness/quote boundary coverage incomplete')
    require('original-vault-pause-keeps-canonical-state-but-blocks-swap-until-own-buffer-expires' in checks,
            'distinct original Vault pause expiry service boundary missing')
    paused_path = ROOT/'integration/test/contracts/managed-v2-aum-recovery.json'
    require(report.get('paused_branch_sha256') == hashlib.sha256(paused_path.read_bytes()).hexdigest(),
            'paused original branch binding changed')
    recovery = report.get('recovery_source_frames', [])
    require(len(recovery) == 5 and len({row.get('header') for row in recovery}) == 5
            and all(row.get('effective_fee') == '0' for row in recovery), 'Recovery zero-effective-fee source inventory incomplete')
    restored = report.get('recovery_exit_source', {})
    require(restored.get('recovery_mode') is False and restored.get('effective_fee') == '10000000000000000'
            and restored.get('header') in {row.get('header') for row in report['source_frames_checked']},
            'original normal fee was not restored after Recovery')
    require('original-recovery-effective-zero-fee-swap-zero-collection-and-normal-exit' in checks,
            'Recovery normal service transition proof missing')
    native = report.get('independent_native_plan_replay', {})
    require(native.get('passed') is True and native.get('vm_started') is True and native.get('verified') is False
            and native.get('original_bank_sha256') == BANK_SHA256, 'independent original selected-hop replay missing')
    require(native.get('selected_route_count') == 4 and native.get('native_transaction_count') == 28,
            'independent selected-hop transaction inventory incomplete')
    routes = native.get('routes', [])
    require(len(routes) == 4 and Counter(row.get('case') for row in routes)
            == Counter({'two:swap-forward': 2, 'two:swap-reverse': 2}),
            'independent selected route observations incomplete')
    for row in routes:
        for field in ('actual_amount_in', 'quoted_amount_in', 'actual_amount_out', 'quoted_amount_out'):
            value = row.get(field)
            require(type(value) is str and value.isascii() and value.isdecimal() and str(int(value)) == value
                    and 0 < int(value) < 2**256, 'missing or invalid independent amount: ' + field)
        require(row.get('actual_amount_in') == row.get('quoted_amount_in')
                and row.get('actual_amount_out') == row.get('quoted_amount_out')
                and row.get('complete_original_poststate_matches') is True
                and row.get('before_getters_checked', 0) > 70 and row.get('after_getters_checked', 0) > 70,
                'independent original recipient/input/full-state difference')


def validate_review(review, runtime, inputs, verification):
    require(review.get('passed') is True and review.get('authority') == 'independent-agent-source-review'
            and review.get('scope') == 'managed-v2-implemented-local-scope', 'independent source review unavailable')
    require(review.get('runtime') == runtime and review.get('input_hashes') == inputs
            and review.get('verification_sha256') == verification, 'source review is stale or targets different inputs')
    reviewers = set()
    for stage in ('spec','quality'):
        row = review.get(stage,{})
        require(row.get('passed') is True and row.get('unresolved') == [], 'unresolved independent review: '+stage)
        ident = row.get('reviewer')
        require(type(ident) is str and ident and ident not in reviewers, 'reviews must use distinct identified reviewers')
        reviewers.add(ident)


def run():
    started = int(time.time() * 1000)
    report = dict(schema_version=1, passed=False, state='RUNNING', production_ready=False, verified=False,
                  all_requested_features_complete=False, scope='published-managed-v2-implemented-local-two-token-service-scope',
                  public_adapter_version=ADAPTER, original_bank_sha256=BANK_SHA256, commands=[], started_at_unix_ms=started)
    write_result(TARGET, report)
    try:
        verification, runtime = checked_runtime()
        inputs = snapshot_inputs()
        report.update(source_tree_sha256=runtime['source_tree_sha256'], runtime=runtime, input_hashes=inputs,
                      verification_sha256=hashlib.sha256((OUT / 'verification.json').read_bytes()).hexdigest(),
                      checked_binary_sha256=verification['checked_binary_sha256'])
        write_result(TARGET, report)
        env = os.environ.copy()
        env['PATH'] = str(ROOT / 'integration/.runtime/protoc-bin') + os.pathsep + str(Path.home()/'.cargo/bin') + os.pathsep + env['PATH']
        commands = [
            ('progress', ['make', 'progress-document-test']),
            ('reference-guards', ['make', 'managed-v2-aum-reference-guards']),
            ('exact-original', ['python3', 'tools/ci/managed-v2-aum-check.py', '--check']),
            ('paused-vault-original', ['node', 'tools/ci/managed-v2-aum-paused-vault-golden.cjs', '--check']),
            ('recovery-original', ['node', 'tools/ci/managed-v2-aum-recovery-golden.cjs', '--check']),
            ('thin-projection', ['python3', 'tools/ci/managed-v2-aum-service-golden.py', '--check']),
            ('cpp-wire', ['python3', 'tools/ci/managed-v2-aum-wire-golden.py', '--check']),
            ('cpp-wire-tests', ['python3', 'tools/ci/managed-v2-aum-wire-golden.py', '--self-test']),
            ('helpers', ['make', 'managed-v2-aum-helper-test']),
            ('normal-service', ['python3', 'tools/acceptance/managed_v2_smoke.py']),
        ]
        for name, command in commands:
            require(runtime_fingerprint() == runtime and snapshot_inputs() == inputs, 'inputs/runtime changed before ' + name)
            log = OUT / ('managed-v2-aum-final-' + name + '.log')
            stage = dict(stage=name, command=command, started_at_unix_ms=int(time.time()*1000), log=str(log.relative_to(ROOT)))
            report['commands'].append(stage)
            write_result(TARGET, report)
            with log.open('w') as stream:
                process = subprocess.run(command, cwd=ROOT/'integration', env=env, stdout=stream, stderr=subprocess.STDOUT)
            stage.update(exit_code=process.returncode, completed_at_unix_ms=int(time.time()*1000),
                         log_sha256=hashlib.sha256(log.read_bytes()).hexdigest())
            write_result(TARGET, report)
            require(process.returncode == 0, name + ' failed; see ' + stage['log'])
            require(runtime_fingerprint() == runtime and snapshot_inputs() == inputs, 'inputs/runtime changed during ' + name)
        process_path = OUT / 'managed-v2-aum-three-service-smoke.json'
        process = json.loads(process_path.read_text())
        validate_process(process, runtime, inputs)
        reference_path = ROOT / 'integration/tools/reference/balancer-managed-v2-aum/verified.json'
        reference = json.loads(reference_path.read_text())
        require(reference.get('passed') is True and reference.get('fixture_sha256') == BANK_SHA256
                and reference.get('regeneration') == 'exact-temporary-byte-comparison', 'fresh exact original reference missing')
        review_path = OUT/'managed-v2-aum-review.json'
        validate_review(json.loads(review_path.read_bytes()),runtime,inputs,report['verification_sha256'])
        report['review_sha256'] = hashlib.sha256(review_path.read_bytes()).hexdigest()
        report.update(passed=True, state='PASSED', source_unchanged_during_verification=True, inputs_unchanged=True,
                      process_report_sha256=hashlib.sha256(process_path.read_bytes()).hexdigest(),
                      original_reference_sha256=hashlib.sha256(reference_path.read_bytes()).hexdigest(),
                      cases=27, native_transactions=292, original_objects=2285, process_checks=len(process['checks']),
                      quote_checks=8, source_frames=15, paused_source_quote_rejections=6, recovery_source_frames=5, source_fault_checks=10, native_selected_route_checks=4,
                      completed_at_unix_ms=int(time.time()*1000),
                      remaining=['Current-date 2–50-token normal-service corpus beyond the two-token sample',
                                 'Shared Vault global-wallet overlay across different physical Managed pools',
                                 'Trusted live-chain state and public executor/VERIFIED/build',
                                 'BPT routing, dynamic weights/fees, circuit breakers and other Managed versions'])
        write_result(TARGET, report)
        return report
    except BaseException as error:
        report.update(passed=False, state='FAILED', error=str(error), error_type=type(error).__name__,
                      completed_at_unix_ms=int(time.time()*1000))
        write_result(TARGET, report)
        raise


if __name__ == '__main__':
    result = run()
    print(json.dumps(dict(passed=result['passed'], scope=result['scope'], process_checks=result['process_checks'],
                          production_ready=False, all_requested_features_complete=False)))
