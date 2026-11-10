#!/usr/bin/env python3
"""Seal the current finite ExactOut business scope, leaving design gaps open."""
import json
import re
import shutil
import time
from pathlib import Path
from exact_out_reference import validate, validate_grid
from weighted_evidence import ROOT, OUT, require, sha, runtime_fingerprint, write_result
TARGET=OUT/'core-exact-out-local-q1-20261109.json'


def artifact(path, digest=None):
    p=(ROOT/path).resolve()
    require(p.is_relative_to(ROOT) and p.is_file(),'EXACT_OUT_LOCAL_ARTIFACT_REQUIRED')
    value=sha(p)
    require(digest is None or value==digest,'EXACT_OUT_ARTIFACT_CHANGED')
    return dict(path=str(p.relative_to(ROOT)),sha256=value)


def run():
    current=runtime_fingerprint()
    path=OUT/'core-exact-out-validation-20261109.json'
    checked=json.loads(path.read_bytes())
    require(checked.get('passed') is True and checked['state']=='PASSED'
            and checked['source_tree_sha256']==checked['source_tree_sha256_after']==current['source_tree_sha256']
            and checked['runtime_fingerprint']==current
            and checked['tool_sha256']==sha(ROOT/'integration/tools/acceptance/validate_exact_out.py'), 'EXACT_OUT_CURRENT_VALIDATION_REQUIRED')
    checks={c['name']:c for c in checked['checks']}
    names={'format','reference','grid-reference','reference-controls','reference-controls-optimized','exact-out',
           'math-regression','routing-regression','optimizer-regression','plan-regression','api-regression','lint','build','process'}
    require(set(checks)==names and len(checks)==len(checked['checks']),'EXACT_OUT_CHECK_SET')
    for c in checks.values():
        require(c['observed_exit_code']==0,'EXACT_OUT_CHECK_FAILED')
        artifact(c['log']['path'],c['log']['sha256'])
        text=(ROOT/c['log']['path']).read_text(errors='replace')
        require(c['rust_test_executions']==sum(map(int,re.findall(r'test result: ok\. (\d+) passed;',text))),'EXACT_OUT_TEST_COUNT_CHANGED')
    require(checks['exact-out']['rust_test_executions']==12 and all(checks[n]['rust_test_executions']>0
        for n in ['math-regression','routing-regression','optimizer-regression','plan-regression','api-regression']), 'EXACT_OUT_EXPECTED_TEST_EXECUTIONS')
    process_path=OUT/'core-exact-out-normal-process-20261109.json'
    process=json.loads(process_path.read_bytes())
    require(process.get('passed') is True and process['runtime_fingerprint']==current
            and process['source_tree_sha256']==current['source_tree_sha256']
            and process['actual_api_records']==45 and process['original_transactions']>=55
            and process['negative_controls']['passed'] is True and process['negative_controls']['mutations']==7,
            'EXACT_OUT_NORMAL_PROCESS_REQUIRED')
    controls=process['negative_controls']
    require(len(controls['receipts'])==7 and {r['name'] for r in controls['receipts']}==set(controls['checks']),'EXACT_OUT_NEGATIVE_RECEIPTS')
    for r in controls['receipts']:
        require(r['observed_exit_code']!=0,'EXACT_OUT_NEGATIVE_COMMAND_ACCEPTED')
        for key in ['input','log']+(['model'] if 'model' in r else []):artifact(r[key]['path'],r[key]['sha256'])
    for relative,digest in process['tool_inputs'].items(): artifact(relative,digest)
    require(process['reference_sha256']==sha(ROOT/'integration/test/contracts/v2-exact-out.json'), 'EXACT_OUT_PROCESS_REFERENCE_CHANGED')
    for key in ['actual_api_input','model_projection','original_replay']:
        artifact(process[key]['path'],process[key]['sha256'])
    original=json.loads((ROOT/process['original_replay']['path']).read_bytes())
    require(original['passed'] is True and original['input_sha256']==process['actual_api_input']['sha256']
            and original['model_sha256']==process['model_projection']['sha256']
            and original['original_transactions']==process['original_transactions']
            and len(original['records'])==45,'EXACT_OUT_NATIVE_PLAN_IDENTITY')
    for dependency in original['runtime']['loaded_dependencies']:
        item=artifact(dependency['path'],dependency['sha256'])
        require((ROOT/item['path']).stat().st_size==dependency['bytes'],'EXACT_OUT_NATIVE_DEPENDENCY_SIZE')
    red=ROOT/'integration/artifacts/local/core-exact-out-probe-red-20261109.log'
    red_text=red.read_text()
    require('test result: FAILED' in red_text and 'a failed probe discarded a feasible whole order' in red_text
            and 'left: 0' in red_text and 'right: 1' in red_text,'EXACT_OUT_PROBE_COUNTEREXAMPLE')
    test_text=(ROOT/checks['exact-out']['log']['path']).read_text()
    require('test full_order_fallback_survives_an_infeasible_small_probe ... ok' in test_text,'EXACT_OUT_FULL_ORDER_PROBE_REGRESSION')
    require('test exact_out_http_retains_a_feasible_whole_order_when_split_probe_fails ... ok' in test_text,'EXACT_OUT_FULL_ORDER_HTTP_REGRESSION')
    base,grid=validate(),validate_grid()
    fixture=json.loads((ROOT/'integration/test/contracts/v2-exact-out.json').read_bytes())
    require(fixture['original_reference']['sha256']==base['reference_sha256']
            and fixture['grid']['reference']['sha256']==grid['reference_sha256']
            and fixture['generator_sha256']==sha(ROOT/'integration/tools/ci/v2-exact-out-state.py')
            and sha(ROOT/'integration/test/contracts/v2-exact-out.json')==sha(ROOT/'dex-router/test/contracts/v2-exact-out.json'), 'EXACT_OUT_FIXTURE_BINDING')
    frozen=OUT/f'core-exact-out-frozen-{current["source_tree_sha256"][:16]}-{sha(process_path)[:12]}-process-20261109.json'
    if frozen.exists(): require(sha(frozen)==sha(process_path),'EXACT_OUT_FROZEN_RECEIPT_CONFLICT')
    else: shutil.copyfile(process_path,frozen)
    require(runtime_fingerprint()==current,'EXACT_OUT_FINAL_SOURCE_CHANGED')
    result=dict(schema_version=1,passed=True,status='LOCAL_SCOPE_VERIFIED',scope='fixed original UniswapV2 complete-state ExactOut raw-input quoting; finite candidates and ordered output grid',
        source_tree_sha256=current['source_tree_sha256'],runtime_fingerprint=current,
        design_reference=artifact('XONE_SYSTEM_DESIGN.md'),design_clauses=['12.10','12.13','13.1','13.5','14','23.9'],
        validation=artifact(str(path.relative_to(ROOT))),normal_process=artifact(str(frozen.relative_to(ROOT))),
        original_reference=base,original_grid=grid,actual_api_records=45,original_transactions=process['original_transactions'],
        rust_test_executions=sum(c['rust_test_executions'] for c in checks.values()),
        source_files=93,reference_control_executions=8,negative_controls=process['negative_controls'],
        probe_counterexample=artifact(str(red.relative_to(ROOT))),
        probe_boundary='Controlled state admitted by the complete Pair model; not a public historical standard-Token deficit bank.',
        service_local_fixture=artifact('dex-router/test/contracts/v2-exact-out.json'),
        acceptance_tool_sha256=sha(Path(__file__)),measured_at_unix_ms=int(time.time()*1000),
        boundaries=['Original Router pulls actual required input; unused budget is not a refund.',
                    'Other protocols and Token behaviors remain excluded from this ExactOut path.',
                    'No chain-cost/net-input or global-market optimality claim.',
                    'Original multi-route replay uses ordered separate local transactions; public atomic execution remains unsupported.',
                    'Controlled RPC headers/receipts and original getter banks do not establish public canonical history.'],
        all_requested_features_complete=False,production_ready=False)
    write_result(TARGET,result)
    return result


if __name__=='__main__':
    result=run();print(json.dumps(dict(passed=True,source_tree_sha256=result['source_tree_sha256'],actual_api_records=result['actual_api_records'],original_transactions=result['original_transactions'])))
