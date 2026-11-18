#!/usr/bin/env python3
"""Frozen independent retained-source audit of historical signed OP fee receipts."""
import json
import os
from pathlib import Path
import sys
import uuid
from op_bound_vectors import ROOT, digest, require
from validate_cache_reclamation import runtime_inputs
from validate_evm_dynamic_fees import main


def validate():
    output = ROOT / 'integration/artifacts/local/op-fee-measurements-check'
    prior_path = ROOT / 'integration/artifacts/local/op-worker-check/runs/cd7284662bdd4ed2870c02c17c7bc95c/report.json'
    prior = json.loads(prior_path.read_bytes())
    require(prior.get('passed') is True, 'Completed signed worker reference required')
    refs = dict(prior['reference_inputs'])
    for name, sha in prior['artifact_reports'].items():
        require(digest(ROOT / name) == sha, 'Prior reference changed: ' + name)
    refs[str(prior_path.relative_to(ROOT))] = digest(prior_path)
    # The binary and build log were evidence of the prior worker run, not pinned
    # source inputs of this new audit. Normal HTTP uses this round's fresh build.
    for name in list(refs):
        if name.endswith('/debug/dex-router') or name.endswith('/worker-build.log'):
            del refs[name]
    refs.update(runtime_inputs())
    execution_name = next(n for n in refs if n.endswith('/vectors.json') and '/op-execution-check/original-runs/' in n)
    execution = ROOT / execution_name
    bound_name = next(n for n in prior['artifact_reports'] if n.endswith('/vectors.json'))
    vectors = ROOT / bound_name
    refs[bound_name] = digest(vectors)
    public_name = next(n for n in refs if n.endswith('/public-users.json'))
    target = Path(os.environ.get('CARGO_TARGET_DIR', str(ROOT / 'integration/.runtime/design-review-target')))
    if not target.is_absolute():
        target = ROOT / target
    http = output / 'http-runs' / uuid.uuid4().hex
    replay = output / 'original-runs' / uuid.uuid4().hex / 'replay.json'
    replay.parent.mkdir(parents=True)
    cargo = ['cargo', '+1.90.0']
    manifest = ['--locked', '--manifest-path', 'dex-router/Cargo.toml']
    env = ['env', 'XONE_OP_EXECUTION_VECTORS=' + str(execution),
           'XONE_OP_BOUND_VECTORS=' + str(vectors), 'XONE_OP_PUBLIC_USERS=' + str(ROOT / public_name)]
    guards = ['-m', 'unittest', 'discover', '-s', 'integration/tools/acceptance/tests', '-p', 'test_op_bound_vectors.py']
    main(output_base=output,
         verification_scope='RETAINED_SIGNED_HISTORICAL_OP_FEE_RECEIPT_SOURCE_CONSISTENCY_NO_VM_ATTESTATION_OR_NORMAL_OP_API',
         extra_checks=[
             ('original-bound-replay', [sys.executable, 'integration/tools/acceptance/op_bound_vectors.py', '--replay', str(vectors), str(replay)]),
             ('signed-bound-router', env + cargo + ['test'] + manifest + ['-p', 'xone-simulation', '--test', 'op_bound', '--', '--include-ignored', '--skip', 'export_original_op_bound_inputs', '--skip', 'actual_signed_op_worker_', '--skip', 'karst_bound_router_']),
             ('retained-fee-source-audit', env + cargo + ['test'] + manifest + ['-p', 'xone-simulation', '--test', 'op_execution', '--test', 'op_transaction', '--', '--include-ignored']),
             ('original-bound-guards', env + [sys.executable] + guards),
             ('original-bound-guards-optimized', env + [sys.executable, '-OO'] + guards),
             ('http-build', cargo + ['build'] + manifest + ['-p', 'xone-quote-api', '--example', 'evm_dynamic_fee_harness']),
             ('actual-legacy-http', [sys.executable, 'integration/tools/acceptance/evm_native_cost_http.py', str(target / 'debug/examples/evm_dynamic_fee_harness'), str(http / 'legacy'), '--legacy-payer-cost']),
             ('actual-dynamic-http', [sys.executable, 'integration/tools/acceptance/evm_native_cost_http.py', str(target / 'debug/examples/evm_dynamic_fee_harness'), str(http / 'dynamic'), '--payer-cost']),
             ('router-build', cargo + ['build'] + manifest + ['--bin', 'dex-router']),
         ],
         required_by_check={
             'retained-fee-source-audit': [
                 'retained_signed_op_fee_audit_matches_original_roles_aliases_funding_and_refunds',
                 'retained_op_fee_audit_rejects_report_source_and_signature_substitutions',
                 'retained_op_fee_audit_charges_reads_preserves_failure_usage_and_honors_control',
                 'authenticated_strict_op_execution_matches_original_gas_accounts_nonce_and_storage',
                 'strict_op_context_missing_state_fee_binding_and_resource_guards_fail_closed',
                 'op_native_value_creation_and_internal_transfers_stay_outside_fee_only_scope',
                 'op_runtime_memory_instructions_and_cooperative_cancellation_are_fatal',
                 'signed_envelopes_derive_every_vm_field_and_preserve_complete_wire',
                 'signature_source_identity_budget_and_canonical_rlp_fail_closed',
                 'signed_access_list_width_depth_and_aggregate_allocations_are_bounded',
                 'signature_malleability_parity_scalars_and_field_counts_are_rejected',
                 'archived_normal_sources_authenticate_all_original_public_signed_users'],
             'signed-bound-router': [
                 'bound_signed_op_router_measures_real_tokens_complete_state_and_all_chain_fees',
                 'op_bound_rejects_rehashed_signed_plan_substitutions_and_preserves_probe_budgets',
                 'op_bound_exact_maximum_funding_probes_failures_and_control_are_accounted_once'],
         }, reference_inputs=refs,
         artifact_reports=[str(p.relative_to(ROOT)) for p in [replay, http / 'legacy/report.json', http / 'dynamic/report.json']],
         extra_report=dict(retained_source_fee_audits=896, minimum_receipt_source_mutation_rejections=650,
                           actual_http_worker_executions=8, source_audit_executes_vm=False,
                           normal_op_api_supported=False, historical_bank_verified=False,
                           production_verified_added=False))


if __name__ == '__main__':
    validate()
