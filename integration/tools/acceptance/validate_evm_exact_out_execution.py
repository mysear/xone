#!/usr/bin/env python3
"""Original Router02 single-pool ExactOut independent execution acceptance."""
from pathlib import Path
from validate_evm_dynamic_fees import main

if __name__ == '__main__':
    root = Path(__file__).resolve().parents[3]
    main(output_base=root / 'integration/artifacts/local/evm-exact-out-execution-check',
         verification_scope='EVM_ORIGINAL_ROUTER_SINGLE_POOL_EXACT_OUT_PROVIDED_STATE',
         extra_required=[
             'exact_out_original_router_16_banks_measure_required_input_and_unused_budget',
             'exact_out_original_router_rejects_mutations_and_missing_state',
             'exact_out_original_router_reverts_leave_full_budget_and_charge_real_gas',
             'exact_out_valid_multi_pool_binding_still_rejects_unrelated_execution_bank',
         ],
         extra_report=dict(exact_out_original_router_banks=16, exact_out_revert_controls=3,
                           multi_pool_execution_supported=True))
