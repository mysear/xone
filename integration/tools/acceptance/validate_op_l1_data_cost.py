#!/usr/bin/env python3
"""Original historical OP data-fee kernel and unchanged normal worker regression."""
import os
from pathlib import Path
import sys
import uuid
from op_l1_cost_vectors import write
from validate_evm_dynamic_fees import main


def validate():
    root = Path(__file__).resolve().parents[3]
    output = root/'integration/artifacts/local/op-l1-data-cost-check'
    independent = output/'independent-runs'/uuid.uuid4().hex/'vectors.json'
    vectors = write(independent)
    cargo = ['cargo', '+1.90.0']
    manifest = ['--locked', '--manifest-path', 'dex-router/Cargo.toml']
    target = Path(os.environ.get('CARGO_TARGET_DIR', str(root/'integration/.runtime/design-review-target')))
    if not target.is_absolute():
        target = root/target
    http = output/'http-runs'/uuid.uuid4().hex
    references = dict(vectors['references'])
    references[str(independent.relative_to(root))] = vectors['sha256']
    tests = ['-m', 'unittest', 'discover', '-s', 'integration/tools/acceptance/tests', '-p', 'test_op_l1_cost_vectors.py']
    main(output_base=output,
         verification_scope='HISTORICAL_OP_REGOLITH_ARITHMETIC_AND_RETAINED_RPC_OBSERVATIONS_NOT_SOURCE_QUALIFIED_CHAIN_FEES',
         extra_checks=[
             ('op-l1-kernel', ['env', 'XONE_OP_L1_COST_VECTORS='+str(independent)] + cargo + ['test'] + manifest + ['-p', 'xone-execution-preparation', '--test', 'op_l1_cost', '--', '--include-ignored']),
             ('original-reference-guards', [sys.executable]+tests),
             ('original-reference-guards-optimized', [sys.executable, '-OO']+tests),
             ('preparation-lint', cargo+['clippy']+manifest+['-p', 'xone-execution-preparation', '--all-targets', '--', '-D', 'warnings']),
             ('http-build', cargo+['build']+manifest+['-p', 'xone-quote-api', '--example', 'evm_dynamic_fee_harness']),
             ('normal-dynamic-worker-regression', [sys.executable, 'integration/tools/acceptance/evm_native_cost_http.py', str(target/'debug/examples/evm_dynamic_fee_harness'), str(http/'dynamic'), '--payer-cost']),
             ('normal-legacy-worker-regression', [sys.executable, 'integration/tools/acceptance/evm_native_cost_http.py', str(target/'debug/examples/evm_dynamic_fee_harness'), str(http/'legacy'), '--legacy-payer-cost']),
             ('router-build', cargo+['build']+manifest+['--bin', 'dex-router']),
         ],
         required_by_check={'op-l1-kernel': [
             'regolith_uses_complete_wire_counts_floor_and_explicit_zero_without_signature_padding',
             'attributes_keep_chain_activation_abi_and_l1_time_boundaries_closed',
             'data_fee_rejects_deposit_future_types_and_byte_budget_and_checks_final_uint256',
             'current_original_op_geth_and_public_receipts_match_rust_data_fees']},
         reference_inputs=references,
         artifact_reports=[str((http/model/'report.json').relative_to(root)) for model in ['dynamic', 'legacy']],
         extra_report=dict(original_op_geth_cases=vectors['cases'], public_signed_op_transactions=3,
                           arithmetic_overflow_cases=vectors['overflow_cases'],
                           original_op_geth_source_sha256='35a4e1976fca49cb5f328c6bf3f1513458eda450c9e7425c24a78a8750c51dec',
                           source_qualification_added=False, current_op_fee_model_claimed=False,
                           quote_ranking_changed=False, complete_chain_fees_claimed=False))


if __name__ == '__main__':
    validate()
