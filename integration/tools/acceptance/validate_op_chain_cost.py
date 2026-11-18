#!/usr/bin/env python3
"""OP full fee-only accounting against full original client transitions."""
import os
from pathlib import Path
import sys
import uuid
from op_chain_cost_vectors import ROOT, digest, write
from native_cost_vectors import write as native_vectors
from validate_evm_dynamic_fees import main


def validate():
    output=ROOT/'integration/artifacts/local/op-chain-cost-check'
    actual=output/'original-runs'/uuid.uuid4().hex
    vectors=actual/'vectors.json'
    reference=write(vectors)
    native=output/'native-runs'/uuid.uuid4().hex/'vectors.json'
    n=native_vectors(native)
    refs=reference['references']
    refs[str(vectors.relative_to(ROOT))]=digest(vectors)
    refs[str(native.relative_to(ROOT))]=n['sha256']
    for name in ['op-l1-cost-reference/rollup_cost.go']:
        p=ROOT/'integration/artifacts/local'/name;refs[str(p.relative_to(ROOT))]=digest(p)
    target=Path(os.environ.get('CARGO_TARGET_DIR',str(ROOT/'integration/.runtime/design-review-target')))
    if not target.is_absolute():target=ROOT/target
    http=output/'http-runs'/uuid.uuid4().hex
    cargo,manifest=['cargo','+1.90.0'],['--locked','--manifest-path','dex-router/Cargo.toml']
    env=['env','XONE_OP_CHAIN_COST_VECTORS='+str(vectors)]
    guards=['-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_op_chain_cost_vectors.py']
    native_guards=['-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_native_cost_vectors.py']
    main(output_base=output,
         verification_scope='LOCAL_SIGNED_ZERO_VALUE_FULL_OP_GETH_FEE_ACCOUNTS_MAXIMUM_FUNDING_AND_REFUNDS_NOT_HISTORICAL_BANK_OR_QUOTE_CHAIN_COSTS',
         extra_checks=[
             ('original-runner-format',[sys.executable,'-c','import subprocess,sys;r=subprocess.check_output(["gofmt","-l","integration/tools/acceptance/reference/op_chain_cost.go"],text=True);print(r,end="");sys.exit(bool(r))']),
             ('independent-op-revm-format',cargo+['fmt','--check','--manifest-path','integration/tools/acceptance/reference/op-revm/Cargo.toml']),
             ('original-transition-replay',[sys.executable,'integration/tools/acceptance/op_chain_cost_vectors.py','--replay',str(vectors),str(actual/'replay.json')]),
             ('independent-op-revm',[sys.executable,'integration/tools/acceptance/op_chain_cost_vectors.py','--revm',str(vectors),str(actual/'revm.json')]),
             ('op-chain-cost',env+cargo+['test']+manifest+['-p','xone-execution-preparation','--test','op_chain_cost','--','--include-ignored']),
             ('native-cost-vectors',['env','XONE_NATIVE_COST_VECTORS='+str(native)]+cargo+['test']+manifest+['-p','xone-execution-preparation','--test','evm_native_cost','--','--include-ignored']),
             ('original-reference-guards',env+[sys.executable]+guards),
             ('original-reference-guards-optimized',env+[sys.executable,'-OO']+guards),
             ('native-reference-guards',[sys.executable]+native_guards),
             ('native-reference-guards-optimized',[sys.executable,'-OO']+native_guards),
             ('independent-op-revm-lint',['env','CARGO_BUILD_JOBS=1','CARGO_TARGET_DIR='+str(ROOT/'integration/.runtime/design-review/op-revm-prototype-target')]+cargo+['clippy','--locked','--manifest-path','integration/tools/acceptance/reference/op-revm/Cargo.toml','--all-targets','--','-D','warnings']),
             ('preparation-lint',cargo+['clippy']+manifest+['-p','xone-execution-preparation','--all-targets','--','-D','warnings']),
             ('http-build',cargo+['build']+manifest+['-p','xone-quote-api','--example','evm_dynamic_fee_harness']),
             ('actual-legacy-http',[sys.executable,'integration/tools/acceptance/evm_native_cost_http.py',str(target/'debug/examples/evm_dynamic_fee_harness'),str(http/'legacy'),'--legacy-payer-cost']),
             ('actual-dynamic-http',[sys.executable,'integration/tools/acceptance/evm_native_cost_http.py',str(target/'debug/examples/evm_dynamic_fee_harness'),str(http/'dynamic'),'--payer-cost']),
             ('router-build',cargo+['build']+manifest+['--bin','dex-router']),
         ],
         required_by_check={'op-chain-cost':['original_signed_op_transitions_match_full_fees_vaults_refunds_and_funding','original_fee_accounts_reject_missing_extra_changed_sources_and_burn_substitution','chain_fee_context_rejects_rules_native_value_unknown_fees_and_overflow'],
                            'native-cost-vectors':['current_python_native_cost_vectors_match_all_components_and_rejections']},
         reference_inputs=refs,
         artifact_reports=[str(v.relative_to(ROOT))for v in [vectors,actual/'replay.json',actual/'revm.json',http/'legacy/report.json',http/'dynamic/report.json']],
         extra_report=dict(original_signed_scenarios=reference['cases'],original_transition_executions=955,
                           independent_op_revm_cases=256,independent_op_revm_unsupported_jovian_cases=64,
                           original_post_refund_below_intrinsic_cases=0,independent_native_cases=n['cases'],
                           actual_http_worker_executions=8,op_native_value_supported=False,
                           parser_fee_math_added=False,complete_quote_chain_costs_claimed=False,
                           historical_bank_verified=False,production_verified_added=False))


if __name__=='__main__':validate()
