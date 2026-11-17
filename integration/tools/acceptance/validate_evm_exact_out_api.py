#!/usr/bin/env python3
"""Normal ExactOut HTTP, same-executable worker, evidence and cache lifecycle."""
import os
from pathlib import Path
import sys
import uuid
from validate_evm_dynamic_fees import main

if __name__ == '__main__':
    root=Path(__file__).resolve().parents[3]
    output=root/'integration/artifacts/local/evm-exact-out-api-check'
    http_output=output/'http-runs'/uuid.uuid4().hex
    legacy_output=output/'http-runs'/uuid.uuid4().hex
    target=Path(os.environ.get('CARGO_TARGET_DIR',str(root/'integration/.runtime/design-review-target')))
    if not target.is_absolute():target=root/target
    manifest=['--locked','--manifest-path','dex-router/Cargo.toml']
    main(output_base=output,verification_scope='EVM_NORMAL_EXACT_OUT_HTTP_OWNED_WORKER_PROVIDED_STATE',
         extra_checks=[
           ('api-regression',['cargo','+1.90.0','test']+manifest+['-p','xone-quote-api','--test','api','--test','exact_out','--test','local_simulation','--test','evm_preparation']),
           ('http-build',['cargo','+1.90.0','build']+manifest+['-p','xone-quote-api','--example','evm_dynamic_fee_harness']),
           ('http-exact-out',[sys.executable,'integration/tools/acceptance/evm_exact_out_http.py',
                str(target/'debug/examples/evm_dynamic_fee_harness'),str(http_output)]),
           ('http-exact-in',[sys.executable,'integration/tools/acceptance/evm_dynamic_fee_http.py',
                str(target/'debug/examples/evm_dynamic_fee_harness'),str(legacy_output)]),
         ],required_by_check={'api-context':[
           'local_simulation::tests::exact_out_worker_request_selects_one_mode_and_preserves_legacy_wire',
           'local_simulation::tests::exact_out_publication_rechecks_expiry_view_and_preserves_quote_evidence',
           'local_simulation::tests::exact_out_http_cancellation_reaps_worker_and_releases_shared_admission',
         ]},extra_report=dict(actual_exact_out_http_report=str((http_output/'report.json').relative_to(root)),
                              actual_exact_in_http_report=str((legacy_output/'report.json').relative_to(root))))
