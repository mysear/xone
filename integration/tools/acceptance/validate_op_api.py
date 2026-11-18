#!/usr/bin/env python3
"""Frozen normal HTTP historical OP signing and independent parent fee audits."""
import json
import os
from pathlib import Path
import subprocess
import sys
import uuid
from op_bound_vectors import ROOT, digest, require
from op_api_http import prepare
from validate_cache_reclamation import runtime_inputs
from validate_evm_dynamic_fees import main


def validate(*, output_base=None, additional_checks=(), additional_references=None,
             additional_artifacts=(), additional_required=None, additional_report=None,
             additional_api_sources=None,
             verification_scope='NORMAL_HTTP_BUILD_FIXED_LOCAL_WALLET_SIGNED_HISTORICAL_OP_BOUND_EXECUTION_PARENT_SOURCE_FEES_NOT_CURRENT_VM_OR_TRUSTED_BANK'):
    output = output_base or ROOT / 'integration/artifacts/local/op-api-check'
    original = output / 'original-runs' / uuid.uuid4().hex
    original.mkdir(parents=True)
    prior_path = ROOT / 'integration/artifacts/local/op-fee-measurements-check/runs/e7b46c8919bd487c97254bdc5d91b37a/report.json'
    prior = json.loads(prior_path.read_bytes())
    require(prior.get('passed') is True, 'Completed retained fee audit required')
    refs = dict(prior['reference_inputs'])
    refs[str(prior_path.relative_to(ROOT))] = digest(prior_path)
    execution = ROOT / next(n for n in refs if n.endswith('/vectors.json') and '/op-execution-check/original-runs/' in n)
    public = ROOT / next(n for n in refs if n.endswith('/public-users.json'))
    vectors = ROOT / next(n for n in refs if n.endswith('/vectors.json') and '/op-worker-check/original-runs/' in n)
    sources = original / 'api-sources.json'
    env = dict(os.environ, PATH=str(Path.home() / '.cargo/bin') + os.pathsep + os.environ['PATH'],
               XONE_OP_EXECUTION_VECTORS=str(execution), XONE_OP_API_INPUTS=str(sources), RUST_TEST_THREADS='1')
    env.setdefault('CARGO_BUILD_JOBS', '2')
    target = Path(env.setdefault('CARGO_TARGET_DIR', str(ROOT / 'integration/.runtime/design-review-target')))
    if not target.is_absolute():
        target = ROOT / target
    if sys.platform == 'darwin':
        import platform
        triple = ('aarch64' if platform.machine() == 'arm64' else 'x86_64') + '_APPLE_DARWIN'
        env.setdefault('CARGO_TARGET_' + triple.upper() + '_RUNNER', str(ROOT / 'integration/tools/ci/native-test-runner.py'))
    result = subprocess.run(['cargo', '+1.90.0', 'test', '--locked', '--manifest-path', 'dex-router/Cargo.toml', '-p', 'xone-simulation', '--test', 'op_bound', 'export_original_op_bound_inputs_for_normal_op_api_sources', '--', '--include-ignored', '--exact'], cwd=ROOT, env=env, capture_output=True, text=True, timeout=180)
    (original / 'export.log').write_text(result.stdout + result.stderr)
    require(result.returncode == 0 and 'test export_original_op_bound_inputs_for_normal_op_api_sources ... ok' in result.stdout, 'Normal API source export failed: ' + result.stderr[-1500:])
    require(len(json.loads(sources.read_bytes())) == 28, 'Normal API source matrix changed')
    normal_count = 28
    if additional_api_sources is not None:
        extra_path = Path(additional_api_sources)
        extra = json.loads(extra_path.read_bytes())
        require(len(extra) == 4 and all(r['request']['fixture']['execution_environment']['fork'] == 'op-jovian' for r in extra), 'Jovian API sources changed')
        rows = json.loads(sources.read_bytes()) + extra
        require(len({r['id'] for r in rows}) == 32, 'API source identities overlap')
        sources.write_text(json.dumps(rows, indent=2) + '\n')
        refs[str(extra_path.relative_to(ROOT))] = digest(extra_path)
        normal_count = 32
    for p in [sources, original / 'export.log']:
        refs[str(p.relative_to(ROOT))] = digest(p)
    runners, inputs = prepare(original / 'original-client')
    refs.update(inputs)
    refs.update(runtime_inputs())
    refs.update(additional_references or {})
    http = output / 'http-runs' / uuid.uuid4().hex
    replay = original / 'replay.json'
    cargo, manifest = ['cargo', '+1.90.0'], ['--locked', '--manifest-path', 'dex-router/Cargo.toml']
    test_env = ['env', 'XONE_OP_EXECUTION_VECTORS=' + str(execution), 'XONE_OP_BOUND_VECTORS=' + str(vectors), 'XONE_OP_PUBLIC_USERS=' + str(public)]
    guards = ['-m', 'unittest', 'discover', '-s', 'integration/tools/acceptance/tests', '-p', 'test_op_bound_vectors.py']
    main(output_base=output,
         verification_scope=verification_scope,
         extra_checks=list(additional_checks) + [
             ('original-signer-format', [sys.executable, '-c', 'import subprocess,sys;r=subprocess.check_output(["gofmt","-l","integration/tools/acceptance/reference/op_api_sign.go"],text=True);print(r,end="");sys.exit(bool(r))']),
             ('original-bound-replay', [sys.executable, 'integration/tools/acceptance/op_bound_vectors.py', '--replay', str(vectors), str(replay)]),
             ('signed-bound-router', test_env + cargo + ['test'] + manifest + ['-p', 'xone-simulation', '--test', 'op_bound', '--', '--include-ignored', '--skip', 'export_original_op_bound_inputs', '--skip', 'actual_signed_op_worker_']),
             ('retained-fee-source-audit', test_env + cargo + ['test'] + manifest + ['-p', 'xone-simulation', '--test', 'op_execution', '--test', 'op_transaction', '--', '--include-ignored']),
             ('original-bound-guards', test_env + [sys.executable] + guards),
             ('original-bound-guards-optimized', test_env + [sys.executable, '-OO'] + guards),
             ('http-build', cargo + ['build'] + manifest + ['-p', 'xone-quote-api', '--example', 'evm_dynamic_fee_harness']),
             ('normal-signed-op-http', [sys.executable, 'integration/tools/acceptance/op_api_http.py', str(target / 'debug/examples/evm_dynamic_fee_harness'), str(sources), str(runners['signer']), str(runners['transition']), str(http / 'op')]),
             ('actual-legacy-http', [sys.executable, 'integration/tools/acceptance/evm_native_cost_http.py', str(target / 'debug/examples/evm_dynamic_fee_harness'), str(http / 'legacy'), '--legacy-payer-cost']),
             ('actual-dynamic-http', [sys.executable, 'integration/tools/acceptance/evm_native_cost_http.py', str(target / 'debug/examples/evm_dynamic_fee_harness'), str(http / 'dynamic'), '--payer-cost']),
             ('router-build', cargo + ['build'] + manifest + ['--bin', 'dex-router']),
         ], required_by_check={**(additional_required or {}),
             'api-context': ['local_simulation::tests::normal_shanghai_worker_rejects_unbound_op_fee_receipts'],
             'signed-bound-router': ['bound_signed_op_router_measures_real_tokens_complete_state_and_all_chain_fees',
                                     'op_bound_rejects_rehashed_signed_plan_substitutions_and_preserves_probe_budgets',
                                     'op_bound_exact_maximum_funding_probes_failures_and_control_are_accounted_once',
                                     'retained_op_worker_receipt_commitment_audits_modes_and_reserves_parent_reads'],
             'retained-fee-source-audit': ['retained_signed_op_fee_audit_matches_original_roles_aliases_funding_and_refunds',
                                         'retained_op_fee_audit_rejects_report_source_and_signature_substitutions',
                                         'retained_op_fee_audit_charges_reads_preserves_failure_usage_and_honors_control',
                                         'authenticated_strict_op_execution_matches_original_gas_accounts_nonce_and_storage',
                                         'archived_normal_sources_authenticate_all_original_public_signed_users'],
         }, reference_inputs=refs,
         artifact_reports=list(additional_artifacts) + [str(p.relative_to(ROOT)) for p in [replay, http / 'op/report.json', http / 'legacy/report.json', http / 'dynamic/report.json']],
         extra_report=dict(additional_report or {}, normal_signed_op_http_executions=normal_count, normal_signed_op_http_rejections=normal_count*6,
                           original_client_signed_and_executed_normal_calldata=normal_count,
                           parent_receipt_commitment_cases=28, parent_rehashed_receipt_mutations=252,
                           legacy_shanghai_http_worker_executions=8,
                           normal_historical_op_api_supported=True, current_op_execution_supported=False,
                           complete_quote_economic_costs_claimed=False, historical_bank_verified=False,
                           production_verified_added=False))


if __name__ == '__main__':
    validate()
