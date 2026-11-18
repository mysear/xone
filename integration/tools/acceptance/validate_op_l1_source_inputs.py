#!/usr/bin/env python3
"""Normal native Go fee facts/replay, Rust arithmetic and original references."""
from pathlib import Path
import sys
import uuid
from op_l1_cost_vectors import write
from op_deposit_source_vectors import write as write_source_hash
from validate_evm_dynamic_fees import main


def validate():
    root = Path(__file__).resolve().parents[3]
    output = root/'integration/artifacts/local/op-l1-source-inputs-check'
    independent = output/'independent-runs'/uuid.uuid4().hex/'vectors.json'
    vectors = write(independent)
    source_vectors = output/'source-reference-runs'/uuid.uuid4().hex/'vectors.json'
    source_reference = write_source_hash(source_vectors)
    actual = output/'source-runs'/uuid.uuid4().hex
    cargo, manifest = ['cargo', '+1.90.0'], ['--locked', '--manifest-path', 'dex-router/Cargo.toml']
    go = ['go', '-C', 'chain-parser-go']
    reference_inputs = dict(vectors['references'])
    reference_inputs.update(source_reference['references'])
    reference_inputs[str(independent.relative_to(root))] = vectors['sha256']
    main(output_base=output,
         verification_scope='NORMAL_GO_RPC_HEADER_BODY_REGOLITH_FEE_INPUTS_REPLAY_AND_RUST_ARITHMETIC_NOT_CONSENSUS_OR_COMPLETE_CHAIN_COSTS',
         extra_checks=[
             ('go-format', [sys.executable, '-c', 'import subprocess,sys;r=subprocess.check_output(["gofmt","-l","chain-parser-go/internal/chain/evm"],text=True);print(r,end="");sys.exit(bool(r))']),
             ('go-evm', ['env', 'XONE_OP_L1_INPUT_OUTPUT='+str(actual), 'XONE_OP_DEPOSIT_SOURCE_VECTORS='+str(source_vectors)]+go+['test', '-race', '-count=1', '-timeout=20m', '-v', './internal/chain/evm']),
             ('go-normal-native', go+['test', '-race', '-count=1', '-v', './internal/service', '-run', 'Native|V2Complete|RuntimeFactoryNormalArchived']),
             ('go-vet', go+['vet', './internal/chain/evm', './internal/service']),
             ('go-build', go+['build', '-trimpath', '-o', str(actual/'chain-parser-go'), './cmd/chain-parser']),
             ('normal-source-rust-costs', ['env', 'XONE_OP_L1_COST_VECTORS='+str(independent), 'XONE_OP_L1_SOURCE_INPUTS='+str(actual/'normal-op-fee-inputs.json')]+cargo+['test']+manifest+['-p', 'xone-execution-preparation', '--test', 'op_l1_cost', '--', '--include-ignored']),
             ('original-reference-guards', [sys.executable, '-m', 'unittest', 'discover', '-s', 'integration/tools/acceptance/tests', '-p', 'test_op_l1_cost_vectors.py']),
             ('original-reference-guards-optimized', [sys.executable, '-OO', '-m', 'unittest', 'discover', '-s', 'integration/tools/acceptance/tests', '-p', 'test_op_l1_cost_vectors.py']),
             ('preparation-lint', cargo+['clippy']+manifest+['-p', 'xone-execution-preparation', '--all-targets', '--', '-D', 'warnings']),
             ('router-build', cargo+['build']+manifest+['--bin', 'dex-router']),
         ],
         required_by_check={'normal-source-rust-costs': ['current_normal_go_committed_inputs_feed_rust_fee_kernel_without_rpc_fee_fields', 'current_original_op_geth_and_public_receipts_match_rust_data_fees']},
         go_required_by_check={'go-evm': [
             'TestOPRegolithNormalSourceAndHistoryReplayBindPublicSignedBody',
             'TestOPRegolithReplayRejectsRawBodyCorruptionCancellationAndBudget',
             'TestOPRegolithNormalSourceRejectsChangedBodyAndFinalAnchorBeforePublication',
             'TestOPRegolithProjectionChecksDepositIdentityABIAndUserSignature',
             'TestOPRegolithSourceHashMatchesPinnedOriginalOptimism',
             'TestOPRegolithArchiveWhitespaceCannotChangeHeaderInputCommitment',
             'TestNormalNativeHistoryExactHeightKeepsSuccessorEvidence'],
             'go-normal-native': ['TestNormalHistoryRuntimeInstallsNativeEvidenceResponsibilitySink', 'TestNativeFactoryActualPublicProofNormalAndReplay']},
         reference_inputs=reference_inputs,
         artifact_reports=[str((actual/'normal-op-fee-inputs.json').relative_to(root))],
         extra_report=dict(original_op_geth_cases=vectors['cases'], public_signed_op_transactions=3,
                           source_projection_mutations_rejected=12, source_fault_cases=6,
                           normal_publication_rejections=2, semantic_projection_controls=15,
                           original_deposit_source_hash_cases=source_reference['cases'],
                           source_output=str((actual/'normal-op-fee-inputs.json').relative_to(root)),
                           parser_fee_math_added=False, consensus_qualification_added=False,
                           complete_chain_fees_claimed=False, current_op_fee_model_claimed=False))


if __name__ == '__main__':
    validate()
