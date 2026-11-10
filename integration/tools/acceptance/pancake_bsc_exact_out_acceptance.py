#!/usr/bin/env python3
"""Seal finite BSC4 ExactOut parsing/route/amount scope against exact observations."""
import json
from pathlib import Path
import re
import shutil
import time
from weighted_evidence import ROOT,OUT,require,runtime_fingerprint,sha,write_result
from pancake_bsc4_acceptance import bound
from pancake_bsc_exact_out_reference import validate as validate_reference
TARGET=OUT/'core-bsc4-exact-out-local-q1-20261110.json'


def run():
    current=runtime_fingerprint();path=OUT/'core-bsc4-exact-out-validation-20261110.json';validation=json.loads(path.read_bytes())
    require(validation.get('passed') is True and validation['state']=='PASSED'
            and validation['source_tree_sha256']==validation['source_tree_sha256_after']==current['source_tree_sha256']
            and validation['runtime_fingerprint']==current
            and validation['tool_sha256']==sha(ROOT/'integration/tools/acceptance/validate_pancake_bsc_exact_out.py'),'BSC4_XO_CURRENT_CHECKS_REQUIRED')
    checks={c['name']:c for c in validation['checks']}
    names={'format','reference','reference-controls','reference-controls-optimized','contracts','progress','progress-optimized','bsc-exact-out','v2-exact-out-regression','curve-ledger-regression','routing-regression','optimizer-regression','plan-regression','api-regression','lint','build','bsc-process','v2-process-regression'}
    require(set(checks)==names and len(checks)==len(validation['checks']),'BSC4_XO_CHECK_SET')
    for c in checks.values():
        require(c['observed_exit_code']==0,'BSC4_XO_CHECK_FAILED');bound(c['log']['path'],c['log']['sha256'])
        text=(ROOT/c['log']['path']).read_text(errors='replace')
        require(c['rust_test_executions']==sum(map(int,re.findall(r'test result: ok\. (\d+) passed;',text))),'BSC4_XO_TEST_EXECUTION_COUNT')
    require(checks['bsc-exact-out']['rust_test_executions']==8 and checks['v2-exact-out-regression']['rust_test_executions']==12
            and all(checks[n]['rust_test_executions']>0 for n in ['curve-ledger-regression','routing-regression','optimizer-regression','plan-regression','api-regression']),'BSC4_XO_REQUIRED_REGRESSION_EXECUTIONS')
    bsc_path=OUT/'core-bsc4-exact-out-normal-process-20261110.json';bsc=json.loads(bsc_path.read_bytes())
    require(bsc.get('passed') is True and bsc['source_tree_sha256']==current['source_tree_sha256']
            and bsc['runtime_fingerprint']==current and bsc['actual_api_records']==bsc['original_swaps']==41
            and bsc['actual_tick_migrations']>0 and bsc['negative_controls']['passed'] is True
            and bsc['negative_controls']['mutations']==13,'BSC4_XO_NORMAL_PROGRAM_EXECUTION')
    for relative,digest in bsc['tool_inputs'].items():bound(relative,digest)
    for key in ['actual_api_input','model_projection','original_replay']:bound(bsc[key]['path'],bsc[key]['sha256'])
    native=json.loads((ROOT/bsc['original_replay']['path']).read_bytes())
    require(native.get('passed') is True and native['input_sha256']==bsc['actual_api_input']['sha256']
            and native['model_sha256']==bsc['model_projection']['sha256']
            and native['reference_sha256']==bsc['reference_sha256']
            and native['generator_sha256']==sha(ROOT/'integration/tools/ci/pancake-bsc4-exact-out-plan-replay.cjs')
            and native['original_swaps']==41 and len(native['records'])==41,'BSC4_XO_NATIVE_IDENTITY')
    for entry in native['loaded_dependencies']:
        e=bound(entry['path'],entry['sha256']);require((ROOT/e['path']).stat().st_size==entry['bytes'],'BSC4_XO_NATIVE_DEPENDENCY_SIZE')
    controls=bsc['negative_controls'];require(len(controls['receipts'])==13 and {r['name']for r in controls['receipts']}==set(controls['checks']),'BSC4_XO_NEGATIVE_RECEIPTS')
    for r in controls['receipts']:
        require(r['observed_exit_code']!=0,'BSC4_XO_CORRUPTION_ACCEPTED');bound(r['log']['path'],r['log']['sha256'])
    reference=validate_reference();require(bsc['reference_sha256']==reference['reference']['sha256'],'BSC4_XO_PROCESS_REFERENCE')
    require(sha(ROOT/'dex-router/test/contracts/pancake-bsc4-exact-out.json')==reference['reference']['sha256'],'BSC4_XO_SERVICE_LOCAL_FIXTURE')
    v2_path=OUT/'core-exact-out-normal-process-20261109.json';v2=json.loads(v2_path.read_bytes())
    require(v2.get('passed') is True and v2['source_tree_sha256']==current['source_tree_sha256']
            and v2['runtime_fingerprint']==current and v2['actual_api_records']==45 and v2['original_transactions']==61
            and v2['negative_controls']['passed'] is True and v2['negative_controls']['mutations']==7,'BSC4_XO_V2_NORMAL_REGRESSION')
    for relative,digest in v2['tool_inputs'].items():bound(relative,digest)
    for key in ['actual_api_input','model_projection','original_replay']:bound(v2[key]['path'],v2[key]['sha256'])
    frozen=[]
    for source,name in [(bsc_path,'bsc4'),(v2_path,'v2')]:
        target=OUT/f'core-bsc4-xo-frozen-{name}-{current["source_tree_sha256"][:16]}-{sha(source)[:12]}-20261110.json'
        if target.exists():require(sha(target)==sha(source),'BSC4_XO_FROZEN_RECEIPT_CHANGED')
        else:shutil.copyfile(source,target)
        frozen.append(bound(str(target.relative_to(ROOT))))
    require(runtime_fingerprint()==current,'BSC4_XO_FINAL_SOURCE_CHANGED')
    result=dict(schema_version=1,passed=True,status='LOCAL_SCOPE_VERIFIED',source_tree_sha256=current['source_tree_sha256'],runtime_fingerprint=current,
        design_reference=bound('XONE_SYSTEM_DESIGN.md'),design_clauses=['12.10','12.13','13.1','13.5','14'],validation=bound(str(path.relative_to(ROOT))),
        original_reference=reference,frozen_process_receipts=frozen,rust_test_executions=sum(c['rust_test_executions']for c in checks.values()),
        actual_bsc_api_records=41,original_bsc_swaps=41,actual_tick_migrations=bsc['actual_tick_migrations'],negative_controls=13,
        v2_regression_api_records=45,v2_regression_original_transactions=61,v2_regression_negative_controls=7,
        acceptance_tool_sha256=sha(Path(__file__)),measured_at_unix_ms=int(time.time()*1000),
        boundaries=['Fixed actual BSC fourth-generation profile only; one pool per ExactOut route.',
                    'Controlled original negative-amount execution with standard callback Token funding; no canonical public-bank qualification.',
                    'Quote budget checked; public build/simulation and an on-chain max-input executor are not implemented.',
                    'Shared physical state replay is sequential; no public atomic batch executor claim.',
                    'Net chain costs, other versions/protocols and remaining chains still incomplete.'],all_requested_features_complete=False,production_ready=False)
    write_result(TARGET,result);return result


if __name__=='__main__':
    result=run();print(json.dumps(dict(passed=True,source_tree_sha256=result['source_tree_sha256'],rust_test_executions=result['rust_test_executions'],actual_bsc_api_records=result['actual_bsc_api_records'])))
