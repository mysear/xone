#!/usr/bin/env python3
"""Source-H observed base fee and H+1 execution fee must retain distinct commitments."""
from pathlib import Path
from validate_evm_dynamic_fees import main
if __name__=='__main__':
    root=Path(__file__).resolve().parents[3]
    main(output_base=root/'integration/artifacts/local/evm-source-fees-check',
         verification_scope='EVM_OBSERVED_H_VS_EXECUTION_H_PLUS_ONE_FEE_CONTEXTS',
         extra_required=[
          'prepared_source_h_fee_and_execution_h_plus_one_fee_are_distinct_in_16_banks',
          'wrong_or_missing_source_fee_cannot_reuse_prepared_commitment_and_execution_cap_still_applies',
         ],extra_report=dict(original_banks=16,target_modes=2,source_and_cap_controls=3))
