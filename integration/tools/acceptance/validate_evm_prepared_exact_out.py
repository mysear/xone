#!/usr/bin/env python3
"""ExactOut inner/outer preparation commitments and same-bank independent execution."""
from pathlib import Path
from validate_evm_dynamic_fees import main

if __name__ == '__main__':
    root = Path(__file__).resolve().parents[3]
    main(output_base=root/'integration/artifacts/local/evm-prepared-exact-out-check',
         verification_scope='EVM_PREPARED_EXACT_OUT_SAME_BANK_PROVIDED_LOCAL_STATE',
         extra_required=[
             'prepared_exact_out_16_original_banks_bind_both_commitments_and_unused_input',
             'prepared_exact_out_rehashed_outer_inner_tampering_and_funding_fail_closed',
             'prepared_exact_out_limits_and_cancellation_precede_bank_execution',
         ], extra_report=dict(prepared_exact_out_banks=16, rehashed_and_funding_controls=16,
                              budget_cancellation_controls=4))
