#!/usr/bin/env python3
"""Legacy native fee source measurements, payer audit and dynamic fee regression."""
import os
from pathlib import Path
import sys
import uuid
from native_cost_vectors import write
from validate_evm_dynamic_fees import main


def validate():
    root = Path(__file__).resolve().parents[3]
    output = root / 'integration/artifacts/local/evm-legacy-native-fee-check'
    independent = output / 'independent-runs' / uuid.uuid4().hex / 'native-vectors.json'
    vectors = write(independent)
    http = output / 'http-runs' / uuid.uuid4().hex
    target = Path(os.environ.get('CARGO_TARGET_DIR', str(root / 'integration/.runtime/design-review-target')))
    if not target.is_absolute():
        target = root / target
    cargo, manifest = ['cargo', '+1.90.0'], ['--locked', '--manifest-path', 'dex-router/Cargo.toml']
    binary = str(target / 'debug/examples/evm_dynamic_fee_harness')
    main(output_base=output,
         verification_scope='PROVIDED_LOCAL_SHANGHAI_LEGACY_NATIVE_BALANCE_FEE_SOURCE_AND_PAYER_DEBIT_NOT_COMPLETE_CHAIN_FEES',
         extra_required=['legacy_fees_measure_gas_price_burn_and_alias_for_success_revert_and_halt',
                         'prepared_source_h_fee_and_execution_h_plus_one_fee_are_distinct_in_16_banks',
                         'wrong_or_missing_source_fee_cannot_reuse_prepared_commitment_and_execution_cap_still_applies'],
         extra_checks=[
             ('native-cost-vectors', ['env', 'XONE_NATIVE_COST_VECTORS='+str(independent)] + cargo + ['test'] + manifest + ['-p', 'xone-execution-preparation', '--test', 'evm_native_cost', '--', '--include-ignored']),
             ('independent-vector-guards', [sys.executable, '-m', 'unittest', 'discover', '-s', 'integration/tools/acceptance/tests', '-p', 'test_native_cost_vectors.py']),
             ('http-build', cargo + ['build'] + manifest + ['-p', 'xone-quote-api', '--example', 'evm_dynamic_fee_harness']),
             ('actual-legacy-http', [sys.executable, 'integration/tools/acceptance/evm_native_cost_http.py', binary, str(http / 'legacy'), '--legacy-payer-cost']),
             ('actual-dynamic-http-regression', [sys.executable, 'integration/tools/acceptance/evm_native_cost_http.py', binary, str(http / 'dynamic'), '--payer-cost']),
             ('router-build', cargo + ['build'] + manifest + ['--bin', 'dex-router']),
         ],
         required_by_check={
             'api-context': ['local_simulation::tests::legacy_fee_balance_reconciliation_binds_source_and_rejects_shifted_or_forged_native_cost',
                             'local_simulation::tests::dynamic_fee_api_reports_measured_effective_price_and_rejects_inconsistent_fees',
                             'local_simulation::tests::dynamic_fee_balance_reconciliation_rejects_changed_measurements_and_handles_alias',
                             'local_simulation::tests::fee_funding_source_must_cover_maximum_even_with_low_spend_and_legacy_net_stays_unknown'],
             'native-cost-vectors': ['current_python_native_cost_vectors_match_all_components_and_rejections',
                                    'native_balance_conservation_handles_distinct_and_alias_beneficiary_without_double_fee',
                                    'payer_cost_requires_maximum_funding_even_if_the_actual_fee_or_alias_debit_is_smaller'],
         },
         reference_inputs={str(independent.relative_to(root)): vectors['sha256']},
         artifact_reports=[str((http / model / 'report.json').relative_to(root)) for model in ['legacy', 'dynamic']],
         extra_report={'native_cost_model_version': 'xone-evm-native-cost/1', 'independent_integer_cases': vectors['cases'],
                       'independent_valid_cases': vectors['valid'], 'actual_http_worker_executions': 8,
                       'legacy_http_worker_executions': 4, 'dynamic_http_worker_regressions': 4,
                       'actual_http_reports': [str((http / model / 'report.json').relative_to(root)) for model in ['legacy', 'dynamic']],
                       'complete_chain_fees_claimed': False, 'net_token_price_conversion_claimed': False})


if __name__ == '__main__':
    validate()
