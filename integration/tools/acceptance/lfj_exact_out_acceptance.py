#!/usr/bin/env python3
"""Seal the fixed origin-bound Slipstream output-target scope and regressions."""
import json
from pathlib import Path
import re
import shutil
import time
from weighted_evidence import ROOT, OUT, require, runtime_fingerprint, sha, write_result
from pancake_bsc4_acceptance import bound
from lfj_exact_out_reference import validate as validate_reference
TARGET=OUT/'core-lfj-exact-out-local-q1-20261110.json'


def run():
    current=runtime_fingerprint()
    path=OUT/'core-lfj-exact-out-validation-20261110.json'
    validation=json.loads(path.read_bytes())
    require(validation.get('passed') is True and validation['state']=='PASSED' and validation['source_tree_sha256']==validation['source_tree_sha256_after']==current['source_tree_sha256'] and validation['runtime_fingerprint']==current and validation['tool_sha256']==sha(ROOT/'integration/tools/acceptance/validate_lfj_exact_out.py'),'LFJ_XO_CURRENT_CHECKS_REQUIRED')
    checks={c['name']:c for c in validation['checks']}
    names={'format','reference','reference-controls','reference-controls-optimized','contracts','progress','progress-optimized','lfj-exact-out','slipstream-exact-out','legacy-exact-out','bsc-exact-out-regression','v2-exact-out-regression','curve-ledger-regression','routing-regression','optimizer-regression','plan-regression','api-regression','lint','lint-new-tests','build','lfj-process','lfj-exact-in-process-regression','slipstream-process','slipstream-exact-in-process-regression','legacy-process','bsc-process-regression','v2-process-regression'}
    require(set(checks)==names and len(checks)==len(validation['checks']),'LFJ_XO_CHECK_SET')
    for check in checks.values():
        require(check['observed_exit_code']==0,'LFJ_XO_CHECK_FAILED');bound(check['log']['path'],check['log']['sha256'])
        text=(ROOT/check['log']['path']).read_text(errors='replace')
        require(check['rust_test_executions']==sum(map(int,re.findall(r'test result: ok\. (\d+) passed;',text))),'LFJ_XO_TEST_EXECUTION_COUNT')
    require(all(checks[name]['rust_test_executions']==count for name,count in [('lfj-exact-out',10),('slipstream-exact-out',9),('legacy-exact-out',9),('bsc-exact-out-regression',8),('v2-exact-out-regression',12)]) and all(checks[name]['rust_test_executions']>0 for name in ['curve-ledger-regression','routing-regression','optimizer-regression','plan-regression','api-regression']),'LFJ_XO_REQUIRED_EXECUTIONS')
    def process(name,filename,api_count,swaps_key,swap_count,controls):
        source=OUT/filename;body=json.loads(source.read_bytes())
        require(body.get('passed') is True and body['source_tree_sha256']==current['source_tree_sha256'] and body['runtime_fingerprint']==current and body['actual_api_records']==api_count and body[swaps_key]==swap_count and body['negative_controls']['passed'] is True and body['negative_controls']['mutations']==controls,'LFJ_XO_CURRENT_PROCESS:'+name)
        for relative,digest in body.get('tool_inputs',{}).items():bound(relative,digest)
        for key in ['actual_api_input','model_projection','original_replay']:bound(body[key]['path'],body[key]['sha256'])
        target=OUT/f'core-lfj-xo-frozen-{name}-{current["source_tree_sha256"][:16]}-{sha(source)[:12]}-20261110.json'
        if target.exists():require(sha(target)==sha(source),'LFJ_XO_FROZEN_CHANGED')
        else:shutil.copyfile(source,target)
        return body,bound(str(target.relative_to(ROOT)))
    lfj,frozen_lfj=process('lfj','core-lfj-exact-out-normal-process-20261110.json',181,'original_swaps',181,9)
    old_lfj_path=OUT/'core-lfj-normal-process.json';old_lfj=json.loads(old_lfj_path.read_bytes())
    require(old_lfj.get('passed') is True and old_lfj['runtime_fingerprint']==current and old_lfj['source_tree_sha256']==current['source_tree_sha256'] and old_lfj['captured_api_records']==19 and old_lfj['original_native_swaps']==38 and old_lfj['negative_controls']['passed'] is True and old_lfj['negative_controls']['mutations']==10,'LFJ_XO_EXACT_IN_REGRESSION')
    for key in ['actual_api_input','model_projection','original_native_replay']:bound(old_lfj[key]['path'],old_lfj[key]['sha256'])
    for relative,digest in old_lfj['tool_digests'].items():bound(relative,digest)
    old_lfj_frozen=OUT/f'core-lfj-xo-frozen-lfj-exact-in-{current["source_tree_sha256"][:16]}-{sha(old_lfj_path)[:12]}-20261110.json'
    if old_lfj_frozen.exists():require(sha(old_lfj_frozen)==sha(old_lfj_path),'LFJ_XO_FROZEN_CHANGED')
    else:shutil.copyfile(old_lfj_path,old_lfj_frozen)
    frozen_old_lfj=bound(str(old_lfj_frozen.relative_to(ROOT)))
    slip,frozen_slip=process('slipstream','core-slipstream-exact-out-normal-process-20261110.json',49,'original_swaps',49,9)
    legacy,frozen_legacy=process('legacy','core-legacy-lm-exact-out-normal-process-20261110.json',49,'original_swaps',49,9)
    bsc,frozen_bsc=process('bsc4','core-bsc4-exact-out-normal-process-20261110.json',41,'original_swaps',41,13)
    v2,frozen_v2=process('v2','core-exact-out-normal-process-20261109.json',45,'original_transactions',61,7)
    exact_in,frozen_exact_in=process('slipstream-exact-in','core-slipstream-context-normal-process.json',13,'original_swaps',13,8)
    require(bsc['actual_tick_migrations']>0,'LFJ_XO_MIGRATION_REGRESSION')
    native=json.loads((ROOT/lfj['original_replay']['path']).read_bytes())
    require(native.get('passed') is True and native['actual_api_input_sha256']==lfj['actual_api_input']['sha256'] and native['model_projection_sha256']==lfj['model_projection']['sha256'] and native['reference_sha256']==lfj['reference_sha256'] and native['generator_sha256']==sha(ROOT/'integration/tools/ci/lfj-v21-exact-out-plan-replay.cjs') and native['native_swaps']==len(native['records'])==181,'LFJ_XO_NATIVE_IDENTITY')
    captured=json.loads((ROOT/lfj['actual_api_input']['path']).read_bytes())
    surplus_records=sum(int(r['quote']['outputSurplus'])>0 for r in captured['records'])
    require(surplus_records==24,'LFJ_XO_ACTUAL_API_SURPLUS_SCOPE')
    for record,observed in zip(captured['records'],native['records']):
        q=record['quote'];received=observed['gross_recipient_delta'];target=record['request']['buyAmount']
        require(received==q['grossBuyAmount'] and int(received)>=int(target) and str(int(received)-int(target))==q['outputSurplus'],'LFJ_XO_ACTUAL_SURPLUS_DELTA')
    for entry in native['loaded_dependencies']:
        item=bound(entry['path'],entry['sha256']);require((ROOT/item['path']).stat().st_size==entry['bytes'],'LFJ_XO_DEPENDENCY_SIZE')
    controls=lfj['negative_controls']
    require(len(controls['receipts'])==9 and {r['name']for r in controls['receipts']}==set(controls['checks']),'LFJ_XO_CONTROL_RECEIPTS')
    for receipt in controls['receipts']:
        require(receipt['observed_exit_code']!=0,'LFJ_XO_CORRUPTION_ACCEPTED');bound(receipt['log']['path'],receipt['log']['sha256'])
    reference=validate_reference()
    require(lfj['reference_sha256']==reference['reference']['sha256']==sha(ROOT/'dex-router/test/contracts/lfj-exact-out.json'),'LFJ_XO_SERVICE_LOCAL_FIXTURE')
    require(runtime_fingerprint()==current,'LFJ_XO_FINAL_SOURCE_CHANGED')
    result=dict(schema_version=1,passed=True,status='LOCAL_SCOPE_VERIFIED',source_tree_sha256=current['source_tree_sha256'],runtime_fingerprint=current,design_reference=bound('XONE_SYSTEM_DESIGN.md'),design_clauses=['12.10','12.13','13.1','13.5'],validation=bound(str(path.relative_to(ROOT))),original_reference=reference,frozen_process_receipts=[frozen_lfj,frozen_old_lfj,frozen_slip,frozen_exact_in,frozen_legacy,frozen_bsc,frozen_v2],rust_test_executions=sum(c['rust_test_executions']for c in checks.values()),actual_lfj_api_records=181,original_lfj_swaps=181,actual_api_surplus_records=surplus_records,negative_controls=9,lfj_exact_in_regression_records=19,lfj_exact_in_original_swaps=38,slipstream_regression_records=49,slipstream_exact_in_regression_records=13,legacy_regression_records=49,bsc_regression_records=41,bsc_regression_tick_migrations=bsc['actual_tick_migrations'],v2_regression_records=45,v2_regression_original_transactions=61,acceptance_tool_sha256=sha(Path(__file__)),measured_at_unix_ms=int(time.time()*1000),boundaries=['Fixed LFJ v2.1.0 standard Token profile; one physical pool per minimum-target route.','Actual output/surplus and minimum target are distinct; existing precise-output serialization stays compatible.','Controlled clone/factory sentinel, original views/Pair and separately prefunded Token transfers; no original Router or atomic funding executor.','Controlled original code/storage and synthetic RPC headers do not qualify public Avalanche banks or production deployment.','Quote budgets are checked off-chain; public build/simulation, an on-chain input-limit guard and atomic split executor remain unsupported.','Minimum raw input within the evaluated grid; net chain fees, other profiles and remaining chains are incomplete.'],all_requested_features_complete=False,production_ready=False)
    write_result(TARGET,result);return result


if __name__=='__main__':
    r=run();print(json.dumps(dict(passed=True,source_tree_sha256=r['source_tree_sha256'],rust_test_executions=r['rust_test_executions'],actual_lfj_api_records=r['actual_lfj_api_records'])))
