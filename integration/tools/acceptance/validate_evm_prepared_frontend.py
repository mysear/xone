#!/usr/bin/env python3
"""Actual Chrome prepared-EVM wire validation and independent execution UI."""
import json
import os
from pathlib import Path
import sys
import uuid
from validate_evm_dynamic_fees import main
from v2_path_reference_check import check,FIXTURE
if __name__=='__main__':
    root=Path(__file__).resolve().parents[3]
    output=root/'integration/artifacts/local/evm-prepared-frontend-check'
    browser_output=output/'browser-runs'/uuid.uuid4().hex
    legacy_output=output/'browser-runs'/uuid.uuid4().hex
    os.environ['XONE_EVM_PREPARED_BROWSER_OUTPUT']=str(browser_output)
    os.environ['XONE_EVM_RPC_BROWSER_OUTPUT']=str(legacy_output)
    module=Path.home()/'.cache/codex-runtimes/codex-primary-runtime/dependencies/node/node_modules/playwright'
    if module.exists():os.environ.setdefault('PLAYWRIGHT_MODULE',str(module))
    manifest=['--locked','--manifest-path','dex-router/Cargo.toml']
    main(output_base=output,verification_scope='ACTUAL_CHROME_PREPARED_EVM_ORIGINAL_WIRE_DUAL_MODE_LOCAL_BANK',
         extra_checks=[
          ('api-regression',['cargo','+1.90.0','test']+manifest+['-p','xone-quote-api','--test','api','--test','exact_out','--test','local_simulation','--test','evm_preparation']),
          ('cache-lifetimes',['cargo','+1.90.0','test']+manifest+['-p','xone-quote-api','--lib','cache_gc::']),
          ('path-regression',['cargo','+1.90.0','test']+manifest+['-p','xone-simulation','--test','v2_path_execution']),
          ('frontend-check',['npm','--prefix','frontend','run','check']),
          ('frontend-model',['npm','--prefix','frontend','test']),
          ('browser-build',['cargo','+1.90.0','build']+manifest+['-p','xone-quote-api','--example','evm_dynamic_fee_harness','--example','evm_preparation_frontend_harness']),
          ('prepared-browser',['node','frontend/test/evm-prepared-browser.mjs']),
          ('legacy-browser',['node','frontend/test/evm-preparation-browser.mjs']),
         ],required_by_check={'api-context':[
          'local_simulation::tests::runtime_evm_independent_capability_requires_matching_bank_and_rpc_preparation',
         ]},reference_inputs=check(json.loads(FIXTURE.read_bytes())),
         extra_report=dict(prepared_browser_report=str((browser_output/'report.json').relative_to(root)),
                           legacy_browser_report=str((legacy_output/'report.json').relative_to(root))))
