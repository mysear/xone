#!/usr/bin/env python3
"""Exact cached RPC preparation -> owned worker, hash conservation and bounded cache."""
import json
import os
from pathlib import Path
import sys
import uuid
from validate_evm_dynamic_fees import main
from v2_path_reference_check import check,FIXTURE
if __name__=='__main__':
    root=Path(__file__).resolve().parents[3]
    output=root/'integration/artifacts/local/evm-prepared-api-check'
    target=Path(os.environ.get('CARGO_TARGET_DIR',str(root/'integration/.runtime/design-review-target')))
    if not target.is_absolute():target=root/target
    manifest=['--locked','--manifest-path','dex-router/Cargo.toml']
    checks=[
      ('api-regression',['cargo','+1.90.0','test']+manifest+['-p','xone-quote-api','--test','api','--test','exact_out','--test','local_simulation','--test','evm_preparation']),
      ('cache-lifetimes',['cargo','+1.90.0','test']+manifest+['-p','xone-quote-api','--lib','cache_gc::']),
      ('path-regression',['cargo','+1.90.0','test']+manifest+['-p','xone-simulation','--test','v2_path_execution']),
      ('http-build',['cargo','+1.90.0','build']+manifest+['-p','xone-quote-api','--example','evm_dynamic_fee_harness']),
    ]
    reports={}
    for name,kind,mode in [('prepared-in','EXACT_IN','bank'),('prepared-out','EXACT_OUT','bank'),('funding-mismatch','EXACT_OUT','bank-mismatch')]:
        directory=output/'http-runs'/uuid.uuid4().hex
        reports[name]=str((directory/'report.json').relative_to(root))
        checks.append((name,[sys.executable,'integration/tools/acceptance/evm_prepared_http.py',
            str(target/'debug/examples/evm_dynamic_fee_harness'),str(directory),kind,mode]))
    main(output_base=output,verification_scope='RPC_PREPARED_COMPONENT_ACTUAL_HTTP_OWNED_WORKER_SAME_PROVIDED_BANK',
         extra_checks=checks,required_by_check={'api-context':[
          'local_simulation::tests::prepared_cache_replacement_blocks_late_evidence_and_reclaims_bounded_bytes',
          'local_simulation::tests::prepared_worker_response_is_exclusive_and_its_outer_commitment_is_recomputed',
         ]},reference_inputs=check(json.loads(FIXTURE.read_bytes())),
         extra_report=dict(actual_prepared_http_reports=reports,preparation_cache_payload_budget_bytes=32*1024*1024))
