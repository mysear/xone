#!/usr/bin/env python3
"""Original Router ExactIn path binding, prepared execution and normal dual-mode HTTP."""
import json
import os
from pathlib import Path
import sys
import uuid
from validate_evm_dynamic_fees import main
from v2_path_reference_check import check,FIXTURE
if __name__=='__main__':
    root=Path(__file__).resolve().parents[3]
    output=root/'integration/artifacts/local/evm-input-path-check'
    target=Path(os.environ.get('CARGO_TARGET_DIR',str(root/'integration/.runtime/design-review-target')))
    if not target.is_absolute():target=root/target
    reports={kind:output/'http-runs'/uuid.uuid4().hex for kind in ['EXACT_IN','EXACT_OUT']}
    manifest=['--locked','--manifest-path','dex-router/Cargo.toml']
    checks=[
      ('reference-controls',[sys.executable,'-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_v2_path_reference_check.py']),
      ('path-simulation',['cargo','+1.90.0','test']+manifest+['-p','xone-simulation','--test','v2_path_execution']),
      ('plan-regression',['cargo','+1.90.0','test']+manifest+['-p','xone-execution-plan']),
      ('plan-lint',['cargo','+1.90.0','clippy']+manifest+['-p','xone-execution-plan','--all-targets','--','-D','warnings']),
      ('frontend-check',['npm','--prefix','frontend','run','check']),
      ('frontend-model',['npm','--prefix','frontend','test']),
      ('http-build',['cargo','+1.90.0','build']+manifest+['-p','xone-quote-api','--example','evm_dynamic_fee_harness']),
    ]
    for kind,case in [('EXACT_IN','EXACT_IN-3-false'),('EXACT_OUT','EXACT_OUT-3-true')]:
        checks.append(('http-'+kind.lower(),[sys.executable,'integration/tools/acceptance/evm_path_http.py',
            str(target/'debug/examples/evm_dynamic_fee_harness'),str(reports[kind]),kind,case]))
    main(output_base=output,verification_scope='ORIGINAL_ROUTER_EXACT_IN_SIMPLE_PATHS_NORMAL_HTTP_DUAL_MODE',
         extra_checks=checks,required_by_check={'path-simulation':[
          'exact_in_six_original_simple_paths_bind_prepare_and_execute_mined_wire',
          'exact_in_path_rehashed_cycles_wrong_endpoints_and_later_hop_identities_reject',
         ]},reference_inputs=check(json.loads(FIXTURE.read_bytes())),
         extra_report=dict(exact_in_original_mined_paths=6,invalid_path_controls=9,
                           actual_http_reports={k:str((v/'report.json').relative_to(root)) for k,v in reports.items()}))
