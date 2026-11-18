#!/usr/bin/env python3
"""Qualify scoped Karst admission through full original VM and normal HTTP."""
import sys
import uuid
from karst_api_vectors import ROOT,prepare
from validate_karst_execution import main as validate


def main(*, output_base=None, prepared=None, extra_checks=(), extra_references=None, extra_required=None, extra_artifacts=(), extra_report=None, verification_scope='SCOPED_KARST_SIGNED_USER_VM_BOUND_WORKER_PARENT_FEES_NORMAL_LOCAL_HTTP_FULL_ORIGINAL_ENGINE_NOT_TRUSTED_BANK_BLOCK_DERIVATION_CONSENSUS_OR_PRODUCTION'):
    output=output_base or ROOT/'integration/artifacts/local/karst-api-check'
    directory=prepared['sources'].parent if prepared is not None else output/'original-runs'/uuid.uuid4().hex
    v=prepared if prepared is not None else prepare(directory);replay=directory/'bound-replay.json'
    guards=['-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_karst_api_vectors.py']
    env=['env','XONE_KARST_API_REFERENCE='+str(directory)]
    bound_env=['env','XONE_OP_JOVIAN_VECTORS='+str(v['jovian']),'XONE_OP_KARST_BOUND_VECTORS='+str(v['bound'])]
    validate(output_base=output,vectors_path=v['execution'],extra_references={**v['references'],**(extra_references or {})},extra_api_sources=v['sources'],extra_api_runners={'karst_transition':v['binary']},extra_checks=list(extra_checks)+[
        ('karst-api-reference-guards',env+[sys.executable]+guards),
        ('karst-api-reference-guards-optimized',env+[sys.executable,'-OO']+guards),
        ('karst-api-original-replay',[sys.executable,'integration/tools/acceptance/karst_api_vectors.py','--replay',str(directory),str(replay)]),
        ('karst-bound-full-original',bound_env+['cargo','+1.90.0','test','--locked','--manifest-path','dex-router/Cargo.toml','-p','xone-simulation','--test','op_bound','karst_bound_router_tokens_fees_and_complete_poststate_match_full_original','--','--include-ignored','--exact']),
    ],extra_required={**(extra_required or {}),
        'karst-bound-full-original':['karst_bound_router_tokens_fees_and_complete_poststate_match_full_original'],
        'karst-signed-execution-components':[
            'revm_backend::execute::karst_execution_tests::karst_public_signed_execution_and_independent_parent_fee_audit_match_original',
            'revm_backend::execute::karst_execution_tests::karst_schedule_width_and_parent_audit_cap_fail_before_source_reads',
        ],
    },extra_artifacts=list(extra_artifacts)+[str(replay.relative_to(ROOT))],extra_report=dict(extra_report or {},normal_karst_local_api_supported=True,karst_current_vm_admitted=True,karst_public_signed_executions=401,karst_parent_source_fee_audits=401,karst_public_signed_rejections=67,karst_original_bound_router_cases=4,karst_mainnet_start_excluded=1783526401,karst_mainnet_end_exclusive=1799683201,current_live_bank_supported=False),verification_scope=verification_scope)


if __name__=='__main__':main()
