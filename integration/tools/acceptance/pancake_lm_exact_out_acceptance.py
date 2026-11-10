#!/usr/bin/env python3
"""Seal finite legacy LM ExactOut parsing/route/amount scope against exact observations."""
import json
from pathlib import Path
import re
import shutil
import time
from weighted_evidence import ROOT,OUT,require,runtime_fingerprint,sha,write_result
from pancake_bsc4_acceptance import bound
from pancake_lm_exact_out_reference import validate as validate_reference
TARGET=OUT/'core-legacy-lm-exact-out-local-q1-20261110.json'


def run():
    current=runtime_fingerprint();path=OUT/'core-legacy-lm-exact-out-validation-20261110.json';validation=json.loads(path.read_bytes())
    require(validation.get('passed') is True and validation['state']=='PASSED'
            and validation['source_tree_sha256']==validation['source_tree_sha256_after']==current['source_tree_sha256']
            and validation['runtime_fingerprint']==current
            and validation['tool_sha256']==sha(ROOT/'integration/tools/acceptance/validate_pancake_lm_exact_out.py'),'LEGACY_LM_XO_CURRENT_CHECKS_REQUIRED')
    checks={c['name']:c for c in validation['checks']}
    names={'format','reference','reference-controls','reference-controls-optimized','contracts','progress','progress-optimized','legacy-exact-out','bsc-exact-out-regression','v2-exact-out-regression','curve-ledger-regression','routing-regression','optimizer-regression','plan-regression','api-regression','lint','build','legacy-process','bsc-process-regression','v2-process-regression'}
    require(set(checks)==names and len(checks)==len(validation['checks']),'LEGACY_LM_XO_CHECK_SET')
    for c in checks.values():
        require(c['observed_exit_code']==0,'LEGACY_LM_XO_CHECK_FAILED');bound(c['log']['path'],c['log']['sha256'])
        text=(ROOT/c['log']['path']).read_text(errors='replace')
        require(c['rust_test_executions']==sum(map(int,re.findall(r'test result: ok\. (\d+) passed;',text))),'LEGACY_LM_XO_TEST_EXECUTION_COUNT')
    require(checks['legacy-exact-out']['rust_test_executions']==9 and checks['bsc-exact-out-regression']['rust_test_executions']==8 and checks['v2-exact-out-regression']['rust_test_executions']==12
            and all(checks[n]['rust_test_executions']>0 for n in ['curve-ledger-regression','routing-regression','optimizer-regression','plan-regression','api-regression']),'LEGACY_LM_XO_REQUIRED_REGRESSION_EXECUTIONS')
    legacy_path=OUT/'core-legacy-lm-exact-out-normal-process-20261110.json';legacy=json.loads(legacy_path.read_bytes())
    require(legacy.get('passed') is True and legacy['source_tree_sha256']==current['source_tree_sha256']
            and legacy['runtime_fingerprint']==current and legacy['actual_api_records']==legacy['original_swaps']==49
            and legacy['negative_controls']['passed'] is True
            and legacy['negative_controls']['mutations']==9,'LEGACY_LM_XO_NORMAL_PROGRAM_EXECUTION')
    for relative,digest in legacy['tool_inputs'].items():bound(relative,digest)
    for key in ['actual_api_input','model_projection','original_replay']:bound(legacy[key]['path'],legacy[key]['sha256'])
    native=json.loads((ROOT/legacy['original_replay']['path']).read_bytes())
    require(native.get('passed') is True and native['input_sha256']==legacy['actual_api_input']['sha256']
            and native['model_sha256']==legacy['model_projection']['sha256']
            and native['reference_sha256']==legacy['reference_sha256']
            and native['generator_sha256']==sha(ROOT/'integration/tools/ci/pancake-lm-exact-out-plan-replay.cjs')
            and native['original_swaps']==49 and len(native['records'])==49,'LEGACY_LM_XO_NATIVE_IDENTITY')
    for entry in native['loaded_dependencies']:
        e=bound(entry['path'],entry['sha256']);require((ROOT/e['path']).stat().st_size==entry['bytes'],'LEGACY_LM_XO_NATIVE_DEPENDENCY_SIZE')
    controls=legacy['negative_controls'];require(len(controls['receipts'])==9 and {r['name']for r in controls['receipts']}==set(controls['checks']),'LEGACY_LM_XO_NEGATIVE_RECEIPTS')
    for r in controls['receipts']:
        require(r['observed_exit_code']!=0,'LEGACY_LM_XO_CORRUPTION_ACCEPTED');bound(r['log']['path'],r['log']['sha256'])
    reference=validate_reference();require(legacy['reference_sha256']==reference['reference']['sha256'],'LEGACY_LM_XO_PROCESS_REFERENCE')
    require(sha(ROOT/'dex-router/test/contracts/pancake-lm-exact-out.json')==reference['reference']['sha256'],'LEGACY_LM_XO_SERVICE_LOCAL_FIXTURE')
    v2_path=OUT/'core-exact-out-normal-process-20261109.json';v2=json.loads(v2_path.read_bytes())
    require(v2.get('passed') is True and v2['source_tree_sha256']==current['source_tree_sha256']
            and v2['runtime_fingerprint']==current and v2['actual_api_records']==45 and v2['original_transactions']==61
            and v2['negative_controls']['passed'] is True and v2['negative_controls']['mutations']==7,'LEGACY_LM_XO_V2_NORMAL_REGRESSION')
    for relative,digest in v2['tool_inputs'].items():bound(relative,digest)
    for key in ['actual_api_input','model_projection','original_replay']:bound(v2[key]['path'],v2[key]['sha256'])
    bsc_regression_path=OUT/'core-bsc4-exact-out-normal-process-20261110.json'
    regression=json.loads(bsc_regression_path.read_bytes())
    require(regression.get('passed') is True and regression['source_tree_sha256']==current['source_tree_sha256']
            and regression['runtime_fingerprint']==current and regression['actual_api_records']==regression['original_swaps']==41
            and regression['actual_tick_migrations']>0 and regression['negative_controls']['passed'] is True
            and regression['negative_controls']['mutations']==13,'LEGACY_LM_XO_BSC_REGRESSION_REQUIRED')
    for relative,digest in regression['tool_inputs'].items():bound(relative,digest)
    for key in ['actual_api_input','model_projection','original_replay']:bound(regression[key]['path'],regression[key]['sha256'])
    frozen=[]
    for source,name in [(legacy_path,'legacy'),(bsc_regression_path,'bsc4'),(v2_path,'v2')]:
        target=OUT/f'core-legacy-lm-xo-frozen-{name}-{current["source_tree_sha256"][:16]}-{sha(source)[:12]}-20261110.json'
        if target.exists():require(sha(target)==sha(source),'LEGACY_LM_XO_FROZEN_RECEIPT_CHANGED')
        else:shutil.copyfile(source,target)
        frozen.append(bound(str(target.relative_to(ROOT))))
    require(runtime_fingerprint()==current,'LEGACY_LM_XO_FINAL_SOURCE_CHANGED')
    result=dict(schema_version=1,passed=True,status='LOCAL_SCOPE_VERIFIED',source_tree_sha256=current['source_tree_sha256'],runtime_fingerprint=current,
        design_reference=bound('XONE_SYSTEM_DESIGN.md'),design_clauses=['12.10','12.13','13.1','13.5','14'],validation=bound(str(path.relative_to(ROOT))),
        original_reference=reference,frozen_process_receipts=frozen,rust_test_executions=sum(c['rust_test_executions']for c in checks.values()),
        actual_legacy_api_records=49,original_legacy_swaps=49,negative_controls=9,
        bsc_regression_api_records=41,bsc_regression_original_swaps=41,bsc_regression_tick_migrations=regression['actual_tick_migrations'],bsc_regression_negative_controls=13,
        v2_regression_api_records=45,v2_regression_original_transactions=61,v2_regression_negative_controls=7,
        acceptance_tool_sha256=sha(Path(__file__)),measured_at_unix_ms=int(time.time()*1000),
        boundaries=['Fixed 98684794 old LM profile only; one pool per ExactOut route; no predecessor migration semantics.',
                    'Controlled original negative-amount execution with standard callback Token funding; no canonical public-bank qualification.',
                    'Quote budget checked; public build/simulation and an on-chain max-input executor are not implemented.',
                    'Shared physical state replay is sequential; no public atomic batch executor claim.',
                    'Net chain costs, other versions/protocols and remaining chains still incomplete.'],all_requested_features_complete=False,production_ready=False)
    write_result(TARGET,result);return result


if __name__=='__main__':
    result=run();print(json.dumps(dict(passed=True,source_tree_sha256=result['source_tree_sha256'],rust_test_executions=result['rust_test_executions'],actual_legacy_api_records=result['actual_legacy_api_records'])))
