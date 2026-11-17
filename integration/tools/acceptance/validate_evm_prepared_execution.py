#!/usr/bin/env python3
"""Prepared Router02 transaction replay with exact same-bank funding checks."""
from pathlib import Path
from validate_evm_dynamic_fees import main

if __name__ == '__main__':
    root = Path(__file__).resolve().parents[3]
    main(output_base=root / 'integration/artifacts/local/evm-prepared-execution-check',
         verification_scope='EVM_PREPARED_COMPONENT_SAME_BANK_REPLAY_PROVIDED_LOCAL_STATE',
         extra_required=[
             'prepared_original_router_executes_exact_unsigned_component_in_16_banks',
             'prepared_tampering_even_with_rehashed_component_never_starts_execution',
             'prepared_observation_context_limits_and_cancellation_fail_closed',
             'dynamic_fee_balance_probes_observe_effective_gasprice_instead_of_cap',
         ],
         extra_report=dict(prepared_original_router_cases=16, rehashed_tamper_cases=25,
                           observation_context_controls=10))
