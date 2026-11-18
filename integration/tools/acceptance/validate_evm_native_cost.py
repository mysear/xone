#!/usr/bin/env python3
"""Native cost arithmetic/disclosure over actual original Router fee executions."""
import os
from pathlib import Path
import sys
import uuid
from native_cost_vectors import write
from validate_evm_dynamic_fees import main

def validate(*, balance_audit=False, payer_cost=False):
    root=Path(__file__).resolve().parents[3]
    output=root/'integration/artifacts/local'/('evm-native-payer-cost-check' if payer_cost else 'evm-native-balance-audit-check' if balance_audit else 'evm-native-cost-model-check')
    independent=output/'independent-runs'/uuid.uuid4().hex/'native-vectors.json'
    vectors=write(independent)
    http_output=output/'http-runs'/uuid.uuid4().hex
    target=Path(os.environ.get('CARGO_TARGET_DIR',str(root/'integration/.runtime/design-review-target')))
    if not target.is_absolute():target=root/target
    cargo=['cargo','+1.90.0'];manifest=['--locked','--manifest-path','dex-router/Cargo.toml']
    main(output_base=output,
         verification_scope='DECLARED_LOCAL_EVM_MAXIMUM_FUNDING_AND_AUDITED_PAYER_NET_DEBIT' if payer_cost else 'DECLARED_LOCAL_EVM_NATIVE_COST_AND_SOURCE_BOUND_BALANCE_RECONCILIATION' if balance_audit else 'DECLARED_LOCAL_EVM_NATIVE_COST_COMPONENTS_ACTUAL_HTTP_WORKER_NOT_COMPLETE_CHAIN_FEES',
         extra_required=['prepared_source_h_fee_and_execution_h_plus_one_fee_are_distinct_in_16_banks',
                         'wrong_or_missing_source_fee_cannot_reuse_prepared_commitment_and_execution_cap_still_applies'],
         extra_checks=[
             ('native-cost-vectors',['env','XONE_NATIVE_COST_VECTORS='+str(independent)]+cargo+['test']+manifest+['-p','xone-execution-preparation','--test','evm_native_cost','--','--include-ignored']),
             ('independent-vector-guards',[sys.executable,'-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_native_cost_vectors.py']),
             ('preparation-lint',cargo+['clippy']+manifest+['-p','xone-execution-preparation','--all-targets','--','-D','warnings']),
             ('http-build',cargo+['build']+manifest+['-p','xone-quote-api','--example','evm_dynamic_fee_harness']),
             ('actual-http-costs',[sys.executable,'integration/tools/acceptance/evm_native_cost_http.py',str(target/'debug/examples/evm_dynamic_fee_harness'),str(http_output)]+(['--payer-cost'] if payer_cost else ['--balance-audit'] if balance_audit else [])),
             ('router-build',cargo+['build']+manifest+['--bin','dex-router']),
         ],
         required_by_check={
             'native-cost-vectors':['native_cost_uses_effective_price_and_keeps_reserve_separate_from_charge',
                                    'native_cost_rejects_impossible_receipts_context_and_full_width_overflow',
                                    'current_python_native_cost_vectors_match_all_components_and_rejections']+(['native_balance_conservation_handles_distinct_and_alias_beneficiary_without_double_fee'] if balance_audit else [])+(['payer_cost_requires_maximum_funding_even_if_the_actual_fee_or_alias_debit_is_smaller'] if payer_cost else []),
             'api-context':['local_simulation::tests::native_cost_breakdown_discloses_unknown_components_without_double_counting_or_net_claim',
                            'local_simulation::tests::native_cost_receipt_cannot_exceed_signed_gas_or_use_invalid_base_price',
                            'local_simulation::tests::dynamic_fee_api_reports_measured_effective_price_and_rejects_inconsistent_fees']+(['local_simulation::tests::dynamic_fee_balance_reconciliation_rejects_changed_measurements_and_handles_alias'] if balance_audit else [])+(['local_simulation::tests::fee_funding_source_must_cover_maximum_even_with_low_spend_and_legacy_net_stays_unknown'] if payer_cost else []),
         },
         reference_inputs={str(independent.relative_to(root)):vectors['sha256']},
         artifact_reports=[str((http_output/'report.json').relative_to(root))],
         extra_report={'native_cost_model_version':'xone-evm-native-cost/1','independent_integer_cases':vectors['cases'],
                       'independent_valid_cases':vectors['valid'],'actual_http_worker_executions':4 if balance_audit else 2,'native_balance_audit_required':balance_audit,'payer_cost_required':payer_cost,
                       'actual_http_worker_report':str((http_output/'report.json').relative_to(root)),
                       'complete_chain_fees_claimed':False,'net_token_price_conversion_claimed':False})

if __name__=='__main__':validate()
