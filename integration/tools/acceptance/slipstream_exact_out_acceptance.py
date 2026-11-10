#!/usr/bin/env python3
"""Seal the fixed origin-bound Slipstream output-target scope and regressions."""
import json
from pathlib import Path
import re
import shutil
import time
from weighted_evidence import ROOT, OUT, require, runtime_fingerprint, sha, write_result
from pancake_bsc4_acceptance import bound
from slipstream_exact_out_reference import validate as validate_reference
TARGET=OUT/'core-slipstream-exact-out-local-q1-20261110.json'


def run():
    current=runtime_fingerprint()
    path=OUT/'core-slipstream-exact-out-validation-20261110.json'
    validation=json.loads(path.read_bytes())
    require(validation.get('passed') is True and validation['state']=='PASSED' and validation['source_tree_sha256']==validation['source_tree_sha256_after']==current['source_tree_sha256'] and validation['runtime_fingerprint']==current and validation['tool_sha256']==sha(ROOT/'integration/tools/acceptance/validate_slipstream_exact_out.py'),'SLIPSTREAM_XO_CURRENT_CHECKS_REQUIRED')
    checks={c['name']:c for c in validation['checks']}
    names={'format','reference','reference-controls','reference-controls-optimized','contracts','progress','progress-optimized','slipstream-exact-out','legacy-exact-out','bsc-exact-out-regression','v2-exact-out-regression','curve-ledger-regression','routing-regression','optimizer-regression','plan-regression','api-regression','lint','lint-new-tests','build','slipstream-process','slipstream-exact-in-process-regression','legacy-process','bsc-process-regression','v2-process-regression'}
    require(set(checks)==names and len(checks)==len(validation['checks']),'SLIPSTREAM_XO_CHECK_SET')
    for check in checks.values():
        require(check['observed_exit_code']==0,'SLIPSTREAM_XO_CHECK_FAILED');bound(check['log']['path'],check['log']['sha256'])
        text=(ROOT/check['log']['path']).read_text(errors='replace')
        require(check['rust_test_executions']==sum(map(int,re.findall(r'test result: ok\. (\d+) passed;',text))),'SLIPSTREAM_XO_TEST_EXECUTION_COUNT')
    require(all(checks[name]['rust_test_executions']==count for name,count in [('slipstream-exact-out',9),('legacy-exact-out',9),('bsc-exact-out-regression',8),('v2-exact-out-regression',12)]) and all(checks[name]['rust_test_executions']>0 for name in ['curve-ledger-regression','routing-regression','optimizer-regression','plan-regression','api-regression']),'SLIPSTREAM_XO_REQUIRED_EXECUTIONS')
    def process(name,filename,api_count,swaps_key,swap_count,controls):
        source=OUT/filename;body=json.loads(source.read_bytes())
        require(body.get('passed') is True and body['source_tree_sha256']==current['source_tree_sha256'] and body['runtime_fingerprint']==current and body['actual_api_records']==api_count and body[swaps_key]==swap_count and body['negative_controls']['passed'] is True and body['negative_controls']['mutations']==controls,'SLIPSTREAM_XO_CURRENT_PROCESS:'+name)
        for relative,digest in body.get('tool_inputs',{}).items():bound(relative,digest)
        for key in ['actual_api_input','model_projection','original_replay']:bound(body[key]['path'],body[key]['sha256'])
        target=OUT/f'core-slipstream-xo-frozen-{name}-{current["source_tree_sha256"][:16]}-{sha(source)[:12]}-20261110.json'
        if target.exists():require(sha(target)==sha(source),'SLIPSTREAM_XO_FROZEN_CHANGED')
        else:shutil.copyfile(source,target)
        return body,bound(str(target.relative_to(ROOT)))
    slip,frozen_slip=process('slipstream','core-slipstream-exact-out-normal-process-20261110.json',49,'original_swaps',49,9)
    legacy,frozen_legacy=process('legacy','core-legacy-lm-exact-out-normal-process-20261110.json',49,'original_swaps',49,9)
    bsc,frozen_bsc=process('bsc4','core-bsc4-exact-out-normal-process-20261110.json',41,'original_swaps',41,13)
    v2,frozen_v2=process('v2','core-exact-out-normal-process-20261109.json',45,'original_transactions',61,7)
    exact_in,frozen_exact_in=process('slipstream-exact-in','core-slipstream-context-normal-process.json',13,'original_swaps',13,8)
    require(bsc['actual_tick_migrations']>0,'SLIPSTREAM_XO_MIGRATION_REGRESSION')
    native=json.loads((ROOT/slip['original_replay']['path']).read_bytes())
    require(native.get('passed') is True and native['actual_api_input_sha256']==slip['actual_api_input']['sha256'] and native['model_projection_sha256']==slip['model_projection']['sha256'] and native['original_reference_sha256']==slip['reference_sha256'] and native['generator_sha256']==sha(ROOT/'integration/tools/ci/slipstream-exact-out-plan-replay.cjs') and native['original_swaps']==len(native['records'])==49,'SLIPSTREAM_XO_NATIVE_IDENTITY')
    for entry in native['loaded_dependencies']:
        item=bound(entry['path'],entry['sha256']);require((ROOT/item['path']).stat().st_size==entry['bytes'],'SLIPSTREAM_XO_DEPENDENCY_SIZE')
    controls=slip['negative_controls']
    require(len(controls['receipts'])==9 and {r['name']for r in controls['receipts']}==set(controls['checks']),'SLIPSTREAM_XO_CONTROL_RECEIPTS')
    for receipt in controls['receipts']:
        require(receipt['observed_exit_code']!=0,'SLIPSTREAM_XO_CORRUPTION_ACCEPTED');bound(receipt['log']['path'],receipt['log']['sha256'])
    reference=validate_reference()
    require(slip['reference_sha256']==reference['reference']['sha256']==sha(ROOT/'dex-router/test/contracts/slipstream-exact-out.json'),'SLIPSTREAM_XO_SERVICE_LOCAL_FIXTURE')
    require(runtime_fingerprint()==current,'SLIPSTREAM_XO_FINAL_SOURCE_CHANGED')
    result=dict(schema_version=1,passed=True,status='LOCAL_SCOPE_VERIFIED',source_tree_sha256=current['source_tree_sha256'],runtime_fingerprint=current,design_reference=bound('XONE_SYSTEM_DESIGN.md'),design_clauses=['12.10','12.13','13.1','13.5'],validation=bound(str(path.relative_to(ROOT))),original_reference=reference,frozen_process_receipts=[frozen_slip,frozen_exact_in,frozen_legacy,frozen_bsc,frozen_v2],rust_test_executions=sum(c['rust_test_executions']for c in checks.values()),actual_slipstream_api_records=49,original_slipstream_swaps=49,negative_controls=9,slipstream_exact_in_regression_records=13,legacy_regression_records=49,bsc_regression_records=41,bsc_regression_tick_migrations=bsc['actual_tick_migrations'],v2_regression_records=45,v2_regression_original_transactions=61,acceptance_tool_sha256=sha(Path(__file__)),measured_at_unix_ms=int(time.time()*1000),boundaries=['Fixed Slipstream profile, three module policies and standard Tokens; one physical pool per ExactOut path.','Caller is the recorded tx.origin context; controlled callback contract pays actual input.','Controlled original code/storage and synthetic RPC headers do not qualify public Base banks or production deployment.','Quote budgets are checked off-chain; public build/simulation, an on-chain input-limit guard and atomic split executor remain unsupported.','Minimum raw input within the evaluated grid; net chain fees, other profiles and remaining chains are incomplete.'],all_requested_features_complete=False,production_ready=False)
    write_result(TARGET,result);return result


if __name__=='__main__':
    r=run();print(json.dumps(dict(passed=True,source_tree_sha256=r['source_tree_sha256'],rust_test_executions=r['rust_test_executions'],actual_slipstream_api_records=r['actual_slipstream_api_records'])))
