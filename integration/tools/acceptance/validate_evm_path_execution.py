#!/usr/bin/env python3
"""Original-mined simple-path ExactOut and full declared pre/post states."""
import json
from pathlib import Path
import sys
from validate_evm_dynamic_fees import main
from v2_path_reference_check import check, FIXTURE
if __name__=='__main__':
    root=Path(__file__).resolve().parents[3]
    references=check(json.loads(FIXTURE.read_bytes()))
    main(output_base=root/'integration/artifacts/local/evm-path-execution-check',
         verification_scope='ORIGINAL_MINED_ROUTER_SIMPLE_PATH_EXACT_OUT_COMPLETE_DECLARED_BANKS',
         extra_checks=[
          ('reference-controls',[sys.executable,'-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_v2_path_reference_check.py']),
          ('reference-controls-optimized',[sys.executable,'-OO','-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_v2_path_reference_check.py']),
          ('path-simulation',['cargo','+1.90.0','test','--locked','--manifest-path','dex-router/Cargo.toml','-p','xone-simulation','--test','v2_path_execution']),
         ],required_by_check={'path-simulation':[
          'original_twelve_path_receipts_match_all_native_accounts_and_declared_storage',
          'exact_out_six_original_simple_paths_and_preparations_match_mined_transactions',
          'exact_out_every_hop_bank_identity_code_state_and_shared_budgets_are_checked',
         ]},reference_inputs=references,
         extra_report=dict(original_mined_transactions=12,exact_out_path_cases=6,max_tested_hops=3))
