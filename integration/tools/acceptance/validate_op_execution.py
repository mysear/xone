#!/usr/bin/env python3
"""Frozen historical OP execution/signature binding with independent references."""
import json
import os
from pathlib import Path
import sys
import uuid
from op_execution_vectors import ROOT, digest, public_inputs, require, write
from native_cost_vectors import write as native_vectors
from validate_evm_dynamic_fees import main


def validate():
    output=ROOT/'integration/artifacts/local/op-execution-check'
    actual=output/'original-runs'/uuid.uuid4().hex;vectors=actual/'vectors.json'
    original=write(vectors);refs=original['references']
    refs[str(vectors.relative_to(ROOT))]=digest(vectors)
    public=actual/'public-users.json';refs.update(public_inputs(public))
    previous=ROOT/'integration/artifacts/local/op-chain-cost-check/runs/9a33d71e7fe4404290e5823f5f38393d/report.json'
    old=json.loads(previous.read_bytes());require(old.get('passed') is True,'Prior completed OP fee reference required')
    refs[str(previous.relative_to(ROOT))]=digest(previous)
    fee_path=next(name for name in old['artifact_reports'] if name.endswith('/vectors.json')and '/original-runs/' in name)
    fees=ROOT/fee_path;require(digest(fees)==old['artifact_reports'][fee_path],'Prior OP fee reference changed')
    refs[fee_path]=digest(fees);refs.update(json.loads(fees.read_bytes())['references'])
    native=actual/'native'/'vectors.json';n=native_vectors(native);refs[str(native.relative_to(ROOT))]=n['sha256']
    target=Path(os.environ.get('CARGO_TARGET_DIR',str(ROOT/'integration/.runtime/design-review-target')))
    if not target.is_absolute():target=ROOT/target
    http=output/'http-runs'/uuid.uuid4().hex
    cargo,manifest=['cargo','+1.90.0'],['--locked','--manifest-path','dex-router/Cargo.toml']
    env=['env','XONE_OP_EXECUTION_VECTORS='+str(vectors),'XONE_OP_PUBLIC_USERS='+str(public)]
    guards=['-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_op_execution_vectors.py']
    main(output_base=output,
         verification_scope='LOCAL_AUTHENTICATED_SIGNED_ZERO_VALUE_HISTORICAL_OP_STRICT_VM_AND_FEE_ACCOUNTS_NOT_CURRENT_OP_OR_TRUSTED_BANK_OR_QUOTE_CHAIN_COSTS',
         extra_checks=[
             ('original-runner-format',[sys.executable,'-c','import subprocess,sys;r=subprocess.check_output(["gofmt","-l","integration/tools/acceptance/reference/op_execution.go"],text=True);print(r,end="");sys.exit(bool(r))']),
             ('original-transition-replay',[sys.executable,'integration/tools/acceptance/op_execution_vectors.py','--replay',str(vectors),str(actual/'replay.json')]),
             ('authenticated-op-execution',env+cargo+['test']+manifest+['-p','xone-simulation','--test','op_execution','--test','op_transaction','--','--include-ignored']),
             ('original-reference-guards',env+[sys.executable]+guards),
             ('original-reference-guards-optimized',env+[sys.executable,'-OO']+guards),
             ('op-fee-regression',['env','XONE_OP_CHAIN_COST_VECTORS='+str(fees)]+cargo+['test']+manifest+['-p','xone-execution-preparation','--test','op_chain_cost','--','--include-ignored']),
             ('native-cost-regression',['env','XONE_NATIVE_COST_VECTORS='+str(native)]+cargo+['test']+manifest+['-p','xone-execution-preparation','--test','evm_native_cost','--','--include-ignored']),
             ('preparation-lint',cargo+['clippy']+manifest+['-p','xone-execution-preparation','--all-targets','--','-D','warnings']),
             ('http-build',cargo+['build']+manifest+['-p','xone-quote-api','--example','evm_dynamic_fee_harness']),
             ('actual-legacy-http',[sys.executable,'integration/tools/acceptance/evm_native_cost_http.py',str(target/'debug/examples/evm_dynamic_fee_harness'),str(http/'legacy'),'--legacy-payer-cost']),
             ('actual-dynamic-http',[sys.executable,'integration/tools/acceptance/evm_native_cost_http.py',str(target/'debug/examples/evm_dynamic_fee_harness'),str(http/'dynamic'),'--payer-cost']),
             ('router-build',cargo+['build']+manifest+['--bin','dex-router']),
         ],
         required_by_check={'authenticated-op-execution':['authenticated_strict_op_execution_matches_original_gas_accounts_nonce_and_storage','strict_op_context_missing_state_fee_binding_and_resource_guards_fail_closed','op_native_value_creation_and_internal_transfers_stay_outside_fee_only_scope','op_runtime_memory_instructions_and_cooperative_cancellation_are_fatal','signed_envelopes_derive_every_vm_field_and_preserve_complete_wire','signature_source_identity_budget_and_canonical_rlp_fail_closed','signed_access_list_width_depth_and_aggregate_allocations_are_bounded','signature_malleability_parity_scalars_and_field_counts_are_rejected','archived_normal_sources_authenticate_all_original_public_signed_users'],
                            'op-fee-regression':['original_signed_op_transitions_match_full_fees_vaults_refunds_and_funding','original_fee_accounts_reject_missing_extra_changed_sources_and_burn_substitution','chain_fee_context_rejects_rules_native_value_unknown_fees_and_overflow'],
                            'native-cost-regression':['current_python_native_cost_vectors_match_all_components_and_rejections']},
         reference_inputs=refs,
         artifact_reports=[str(p.relative_to(ROOT))for p in [vectors,actual/'replay.json',public,http/'legacy/report.json',http/'dynamic/report.json']],
         extra_report=dict(original_signed_scenarios=448,original_transition_executions=1337,
                           product_op_accepted_executions=896,product_op_insufficient_funding_rejections=441,
                           archived_public_signed_users_authenticated=180,actual_http_worker_executions=8,
                           historical_op_forks=['regolith','canyon','ecotone','fjord','granite','holocene','isthmus'],
                           jovian_or_later_execution_supported=False,op_native_value_supported=False,
                           parser_fee_math_added=False,complete_quote_chain_costs_claimed=False,
                           historical_bank_verified=False,production_verified_added=False))


if __name__=='__main__':validate()
