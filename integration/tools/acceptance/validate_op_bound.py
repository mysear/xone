#!/usr/bin/env python3
"""Freeze signed historical OP BOUND Router execution and complete-state evidence."""
import json
import os
from pathlib import Path
import sys
import uuid
from op_bound_vectors import ROOT, digest, require, write
from validate_evm_dynamic_fees import main
from validate_cache_reclamation import runtime_inputs


def validate():
    output=ROOT/'integration/artifacts/local/op-bound-check'
    actual=output/'original-runs'/uuid.uuid4().hex;vectors=actual/'vectors.json'
    previous=ROOT/'integration/artifacts/local/op-execution-check/runs/9d39fff694d14521bb8a7103c302283d/report.json'
    prior=json.loads(previous.read_bytes());require(prior.get('passed') is True,'Completed signed OP reference required')
    execution_name=next(n for n in prior['artifact_reports'] if n.endswith('/vectors.json') and '/original-runs/' in n)
    execution=ROOT/execution_name;require(digest(execution)==prior['artifact_reports'][execution_name],'Signed OP reference changed')
    original=write(vectors,execution);refs=original['references']
    refs.update(json.loads(execution.read_bytes())['references']);refs.update(runtime_inputs())
    refs.update({str(previous.relative_to(ROOT)):digest(previous),str(vectors.relative_to(ROOT)):digest(vectors)})
    public_name=next(n for n in prior['artifact_reports'] if n.endswith('/public-users.json'))
    public=ROOT/public_name;require(digest(public)==prior['artifact_reports'][public_name],'Archived user authentication changed')
    refs[public_name]=digest(public)
    target=Path(os.environ.get('CARGO_TARGET_DIR',str(ROOT/'integration/.runtime/design-review-target')))
    if not target.is_absolute():target=ROOT/target
    http=output/'http-runs'/uuid.uuid4().hex
    cargo,manifest=['cargo','+1.90.0'],['--locked','--manifest-path','dex-router/Cargo.toml']
    env=['env','XONE_OP_EXECUTION_VECTORS='+str(execution),'XONE_OP_BOUND_VECTORS='+str(vectors),'XONE_OP_PUBLIC_USERS='+str(public)]
    guards=['-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_op_bound_vectors.py']
    main(output_base=output,
         verification_scope='LOCAL_SIGNED_HISTORICAL_OP_BOUND_ROUTER_ACTUAL_TOKEN_DELTAS_COMPLETE_DECLARED_POSTSTATE_AND_FEES_NOT_TRUSTED_BANK_OR_NORMAL_OP_API',
         extra_checks=[
             ('original-runner-format',[sys.executable,'-c','import subprocess,sys;r=subprocess.check_output(["gofmt","-l","integration/tools/acceptance/reference/op_bound.go"],text=True);print(r,end="");sys.exit(bool(r))']),
             ('original-bound-replay',[sys.executable,'integration/tools/acceptance/op_bound_vectors.py','--replay',str(vectors),str(actual/'replay.json')]),
             ('signed-bound-router',env+cargo+['test']+manifest+['-p','xone-simulation','--test','op_bound','--','--include-ignored','--skip','export_original_op_bound_inputs','--skip','actual_signed_op_worker_','--skip','karst_bound_router_']),
             ('signed-engine-regression',env+cargo+['test']+manifest+['-p','xone-simulation','--test','op_execution','--test','op_transaction','--','--include-ignored']),
             ('original-bound-guards',env+[sys.executable]+guards),
             ('original-bound-guards-optimized',env+[sys.executable,'-OO']+guards),
             ('http-build',cargo+['build']+manifest+['-p','xone-quote-api','--example','evm_dynamic_fee_harness']),
             ('actual-legacy-http',[sys.executable,'integration/tools/acceptance/evm_native_cost_http.py',str(target/'debug/examples/evm_dynamic_fee_harness'),str(http/'legacy'),'--legacy-payer-cost']),
             ('actual-dynamic-http',[sys.executable,'integration/tools/acceptance/evm_native_cost_http.py',str(target/'debug/examples/evm_dynamic_fee_harness'),str(http/'dynamic'),'--payer-cost']),
             ('router-build',cargo+['build']+manifest+['--bin','dex-router']),
         ],
         required_by_check={
             'api-context':['local_simulation::tests::normal_shanghai_worker_rejects_unbound_op_fee_receipts'],
             'simulation':['revm_backend::execute::observation_tests::op_observation_preserves_native_context_without_l1_fee_or_nonce_payment'],
             'signed-bound-router':['bound_signed_op_router_measures_real_tokens_complete_state_and_all_chain_fees','op_bound_rejects_rehashed_signed_plan_substitutions_and_preserves_probe_budgets','op_bound_exact_maximum_funding_probes_failures_and_control_are_accounted_once'],
             'signed-engine-regression':['authenticated_strict_op_execution_matches_original_gas_accounts_nonce_and_storage','strict_op_context_missing_state_fee_binding_and_resource_guards_fail_closed','op_native_value_creation_and_internal_transfers_stay_outside_fee_only_scope','op_runtime_memory_instructions_and_cooperative_cancellation_are_fatal','signed_envelopes_derive_every_vm_field_and_preserve_complete_wire','signature_source_identity_budget_and_canonical_rlp_fail_closed','signed_access_list_width_depth_and_aggregate_allocations_are_bounded','signature_malleability_parity_scalars_and_field_counts_are_rejected','archived_normal_sources_authenticate_all_original_public_signed_users']},
         reference_inputs=refs,
         artifact_reports=[str(p.relative_to(ROOT)) for p in [vectors,actual/'replay.json',http/'legacy/report.json',http/'dynamic/report.json']],
         extra_report=dict(original_bound_router_transitions=448,product_bound_router_matrix=448,product_direct_complete_poststates=448,
                           product_exact_maximum_funding_router_executions=14,product_insufficient_funding_router_rejections=14,
                           original_signed_engine_scenarios=448,actual_http_worker_executions=8,
                           op_worker_or_api_supported=False,current_op_execution_supported=False,historical_bank_verified=False,
                           complete_quote_chain_costs_claimed=False,production_verified_added=False))


if __name__=='__main__':validate()
