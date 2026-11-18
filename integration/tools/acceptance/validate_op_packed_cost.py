#!/usr/bin/env python3
"""Freeze modern packed arithmetic with a reverified actual normal RPC body.

The source is exported before freezing to generate independent expectations,
then re-exported inside the formal run and required to have identical bytes.
"""
from pathlib import Path
import subprocess
import sys
import uuid
from op_l1_cost_vectors import digest, require
from op_packed_cost_vectors import write
from validate_evm_dynamic_fees import main


def validate():
    root = Path(__file__).resolve().parents[3]
    output = root/'integration/artifacts/local/op-packed-cost-check'
    capture = root/'integration/.runtime/design-review/op-current-captures/e7cc59e6c16c4560ad1229b0264993f6'
    expected = {'full-block.json':'7dea5cbffa43bea10eecf8218708238c078af5fc131b82e3cdd04e9f45ee94ef',
                'vector.json':'68cd47b21cdaa542e92d53b01c6cdeaaa3a43860244e5cbfa9dd064e703ae00c'}
    for name, checksum in expected.items():
        require(digest(capture/name)==checksum, 'Current public capture changed: '+name)
    actual = output/'source-runs'/uuid.uuid4().hex
    actual.mkdir(parents=True, exist_ok=False)
    source_input = actual/'normal-packed-inputs.json'
    go_env = ['env', 'XONE_OP_PACKED_PUBLIC_CAPTURE='+str(capture), 'XONE_OP_PACKED_INPUT_OUTPUT='+str(source_input)]
    go_test = ['go','-C','chain-parser-go','test','-race','-count=1','-v','./internal/chain/evm','-run','^TestOPPackedActualPublicNormalBodyReceiptsAndSignedInputs$']
    with (actual/'source-preflight.log').open('wb') as log:
        result = subprocess.run(go_env+go_test, cwd=root, stdout=log, stderr=subprocess.STDOUT, timeout=120)
    require(result.returncode==0, 'Normal current source preflight failed: '+str(actual))
    independent = output/'independent-runs'/uuid.uuid4().hex/'vectors.json'
    vectors = write(independent, source_input)
    references = dict(vectors['references'])
    references[str(independent.relative_to(root))] = vectors['sha256']
    references.update({str((capture/name).relative_to(root)): checksum for name,checksum in expected.items()})
    references[str((capture/'capture.json').relative_to(root))] = digest(capture/'capture.json')
    cargo, manifest = ['cargo','+1.90.0'], ['--locked','--manifest-path','dex-router/Cargo.toml']
    tests = ['-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_op_packed_cost_vectors.py']
    main(output_base=output,
         verification_scope='EXPLICIT_PACKED_OP_FEE_ARITHMETIC_FASTLZ_AND_NORMAL_RPC_COMMITTED_CURRENT_BODY_NOT_AUTOMATIC_FORK_SELECTION_OR_COMPLETE_CHAIN_COST',
         extra_checks=[
             ('go-format',[sys.executable,'-c','import subprocess,sys;r=subprocess.check_output(["gofmt","-l","chain-parser-go/internal/chain/evm"],text=True);print(r,end="");sys.exit(bool(r))']),
             ('go-current-normal-source',go_env+go_test),
             ('go-regolith-regression',['go','-C','chain-parser-go','test','-race','-count=1','-v','./internal/chain/evm','-run','^TestOPRegolith(NormalSource|ArchiveWhitespace|ProjectionChecks|ReplayRejects|NormalSourceRejects)']),
             ('go-vet',['go','-C','chain-parser-go','vet','./internal/chain/evm']),
             ('packed-original-costs',['env','XONE_OP_PACKED_VECTORS='+str(independent)]+cargo+['test']+manifest+['-p','xone-execution-preparation','--test','op_packed_cost','--','--include-ignored','--skip','normal_versioned_op_sources_feed_rust_across_four_real_upgrade_boundaries']),
             ('regolith-arithmetic',cargo+['test']+manifest+['-p','xone-execution-preparation','--test','op_l1_cost']),
             ('original-reference-guards',[sys.executable]+tests),
             ('original-reference-guards-optimized',[sys.executable,'-OO']+tests),
             ('preparation-lint',cargo+['clippy']+manifest+['-p','xone-execution-preparation','--all-targets','--','-D','warnings']),
             ('router-build',cargo+['build']+manifest+['--bin','dex-router']),
         ],
         required_by_check={'packed-original-costs':['original_op_geth_packed_costs_and_current_public_signed_users_match_rust','packed_formats_are_exact_and_operator_unknown_is_not_zero','operator_units_floor_constant_and_max_width_remain_distinct']},
         go_required_by_check={'go-current-normal-source':['TestOPPackedActualPublicNormalBodyReceiptsAndSignedInputs'],
                               'go-regolith-regression':['TestOPRegolithNormalSourceAndHistoryReplayBindPublicSignedBody','TestOPRegolithArchiveWhitespaceCannotChangeHeaderInputCommitment']},
         reference_inputs=references, artifact_reports=[str(source_input.relative_to(root)),str(independent.relative_to(root))],
         extra_report=dict(original_op_geth_cases=vectors['cases'], original_overflow_cases=vectors['overflow_cases'],
                           public_signed_current_users=49, arithmetic_byte_probes=vectors['cases']-49,
                           committed_public_transactions=50, current_source_mutations_rejected=4,
                           normal_archive_replayed=True, automatic_fork_selection_added=False,
                           current_normal_quote_chain_fees_added=False, complete_chain_fees_claimed=False,
                           parser_fee_math_added=False, source_output=str(source_input.relative_to(root))))


if __name__ == '__main__':
    validate()
