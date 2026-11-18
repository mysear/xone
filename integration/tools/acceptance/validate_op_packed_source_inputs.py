#!/usr/bin/env python3
"""Normal modern OP source/replay and real upgrade-boundary fee arithmetic."""
import json
from pathlib import Path
import subprocess
import sys
import uuid
from op_l1_cost_vectors import ROOT, digest, require, write as regolith_vectors
from op_deposit_source_vectors import write as deposit_vectors
from op_packed_cost_vectors import write as packed_vectors
from op_upgrade_cost_vectors import write as upgrade_vectors
from validate_evm_dynamic_fees import main


def validate():
    output = ROOT/'integration/artifacts/local/op-packed-source-inputs-check'
    actual = output/'source-runs'/uuid.uuid4().hex
    actual.mkdir(parents=True, exist_ok=False)
    current_capture = ROOT/'integration/.runtime/design-review/op-current-captures/e7cc59e6c16c4560ad1229b0264993f6'
    upgrade_capture = ROOT/'integration/.runtime/design-review/op-upgrade-captures/c391bf7c033944b5af1269643ec9479d'
    current, upgrades = actual/'normal-packed-inputs.json', actual/'normal-upgrade-inputs.json'
    go_env = ['env','XONE_OP_PACKED_PUBLIC_CAPTURE='+str(current_capture),'XONE_OP_PACKED_INPUT_OUTPUT='+str(current),
              'XONE_OP_UPGRADE_PUBLIC_CAPTURE='+str(upgrade_capture),'XONE_OP_UPGRADE_INPUT_OUTPUT='+str(upgrades)]
    go = ['go','-C','chain-parser-go']
    with (actual/'source-preflight.log').open('wb') as log:
        result = subprocess.run(go_env+go+['test','-race','-count=1','-v','./internal/chain/evm','-run','^TestOP(Packed|Upgrade)'],cwd=ROOT,stdout=log,stderr=subprocess.STDOUT,timeout=180)
    require(result.returncode==0,'Normal source preflight failed: '+str(actual))
    packed = output/'packed-reference-runs'/uuid.uuid4().hex/'vectors.json'
    p = packed_vectors(packed,current)
    upgrade = output/'upgrade-reference-runs'/uuid.uuid4().hex/'vectors.json'
    u = upgrade_vectors(upgrade,upgrades,packed)
    old = output/'regolith-reference-runs'/uuid.uuid4().hex/'vectors.json'
    r = regolith_vectors(old)
    deposit = output/'deposit-reference-runs'/uuid.uuid4().hex/'vectors.json'
    d = deposit_vectors(deposit)
    refs = {}
    for v in [p,u,r,d]: refs.update(v['references'])
    for path in [packed,upgrade,old]: refs[str(path.relative_to(ROOT))]=digest(path)
    for capture in [current_capture,upgrade_capture]:
        for path in capture.rglob('*.json'):
            refs[str(path.relative_to(ROOT))]=digest(path)
    marshal = ROOT/'integration/artifacts/local/op-l1-cost-reference/transaction_marshalling.go'
    require(digest(marshal)=='2bd82ecba3cd08c3edf444e8436d54480bca062241a1030d8c6112aa5053a3b6','Pinned native JSON source changed')
    refs[str(marshal.relative_to(ROOT))]=digest(marshal)
    rust_env = ['env','XONE_OP_PACKED_VECTORS='+str(packed),'XONE_OP_UPGRADE_INPUTS='+str(upgrades),'XONE_OP_UPGRADE_VECTORS='+str(upgrade)]
    cargo, manifest = ['cargo','+1.90.0'],['--locked','--manifest-path','dex-router/Cargo.toml']
    guards = ['-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_op_packed_cost_vectors.py']
    main(output_base=output,
         verification_scope='NORMAL_RPC_HEADER_BODY_RECEIPTS_MODERN_OP_FEE_INPUTS_REAL_UPGRADE_REPLAY_RUST_ARITHMETIC_NOT_CONSENSUS_OR_COMPLETE_QUOTE_COSTS',
         extra_checks=[
             ('go-format',[sys.executable,'-c','import subprocess,sys;r=subprocess.check_output(["gofmt","-l","chain-parser-go/internal/chain/evm"],text=True);print(r,end="");sys.exit(bool(r))']),
             ('go-evm',go_env+['XONE_OP_L1_INPUT_OUTPUT='+str(actual),'XONE_OP_DEPOSIT_SOURCE_VECTORS='+str(deposit)]+go+['test','-race','-count=1','-timeout=20m','-v','./internal/chain/evm']),
             ('go-normal-native',go+['test','-race','-count=1','-v','./internal/service','-run','Native|V2Complete|RuntimeFactoryNormalArchived']),
             ('go-vet',go+['vet','./internal/chain/evm','./internal/service']),
             ('go-build',go+['build','-trimpath','-o',str(actual/'chain-parser-go'),'./cmd/chain-parser']),
             ('packed-source-rust',rust_env+cargo+['test']+manifest+['-p','xone-execution-preparation','--test','op_packed_cost','--','--include-ignored']),
             ('regolith-source-regression',['env','XONE_OP_L1_SOURCE_INPUTS='+str(actual/'normal-op-fee-inputs.json'),'XONE_OP_L1_COST_VECTORS='+str(old)]+cargo+['test']+manifest+['-p','xone-execution-preparation','--test','op_l1_cost','--','--include-ignored']),
             ('original-reference-guards',[sys.executable]+guards),
             ('original-reference-guards-optimized',[sys.executable,'-OO']+guards),
             ('preparation-lint',cargo+['clippy']+manifest+['-p','xone-execution-preparation','--all-targets','--','-D','warnings']),
             ('router-build',cargo+['build']+manifest+['--bin','dex-router']),
         ],
         required_by_check={'packed-source-rust':['normal_versioned_op_sources_feed_rust_across_four_real_upgrade_boundaries','original_op_geth_packed_costs_and_current_public_signed_users_match_rust'],
                            'regolith-source-regression':['current_normal_go_committed_inputs_feed_rust_fee_kernel_without_rpc_fee_fields']},
         go_required_by_check={'go-evm':['TestOPPackedActualPublicNormalBodyReceiptsAndSignedInputs','TestOPPackedUpgradeNormalSourcesAndHistoryRecreateVersionedInputs','TestOPPackedProjectionSemanticAndBudgetFailuresRemainClosed','TestOPUpgradeContractCreationMissingDestinationMatchesNullButCannotEraseCallTarget','TestOPUpgradeBlocksRejectUsersAndPackedSourceRejectsInvalidSignature','TestOPRegolithSourceHashMatchesPinnedOriginalOptimism'],
                               'go-normal-native':['TestNormalHistoryRuntimeInstallsNativeEvidenceResponsibilitySink']},
         reference_inputs=refs,
         artifact_reports=[str(x.relative_to(ROOT))for x in [current,upgrades,packed,upgrade,actual/'normal-op-fee-inputs.json']],
         extra_report=dict(current_public_signed_users=49,upgrade_public_signed_users=128,
                           original_packed_cases=p['cases'], original_upgrade_cases=u['cases'],
                           actual_upgrade_blocks=8,upgrade_deposits_only_blocks=4,
                           upgrade_projection_mutations_rejected=60,packed_semantic_faults_rejected=15,
                           parser_fee_math_added=False,consensus_qualification_added=False,
                           complete_quote_chain_costs_claimed=False,production_verified_added=False))


if __name__ == '__main__':
    validate()
