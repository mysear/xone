#!/usr/bin/env python3
"""Actual local API/worker dynamic fees, original Router bank and protocol checks."""
import os
from pathlib import Path
import sys
from validate_evm_dynamic_fees import main

if __name__ == '__main__':
    root = Path(__file__).resolve().parents[3]
    output = root / 'integration/artifacts/local/evm-dynamic-fee-api-check'
    import uuid
    http_output = output / 'http-runs' / uuid.uuid4().hex
    target = Path(os.environ.get('CARGO_TARGET_DIR', str(root / 'integration/.runtime/design-review-target')))
    if not target.is_absolute(): target = root / target
    main(output_base=output,
         verification_scope='EVM_ACTUAL_HTTP_WORKER_EFFECTIVE_FEE_PROVIDED_LOCAL_STATE',
         extra_checks=[
             ('http-build', ['cargo', '+1.90.0', 'build', '--locked', '--manifest-path', 'dex-router/Cargo.toml',
                             '-p', 'xone-quote-api', '--example', 'evm_dynamic_fee_harness']),
             ('http-worker', [sys.executable, 'integration/tools/acceptance/evm_dynamic_fee_http.py',
                              str(target / 'debug/examples/evm_dynamic_fee_harness'), str(http_output)]),
         ],
         required_by_check={'api-context': [
             'local_simulation::tests::dynamic_fee_api_reports_measured_effective_price_and_rejects_inconsistent_fees',
         ]},
         extra_report=dict(actual_http_worker_report=str((http_output/'report.json').relative_to(root))))
