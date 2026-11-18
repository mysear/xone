#!/usr/bin/env python3
"""Freeze the Karst execution additions, full original VM and historical APIs."""
import sys
import uuid
from karst_execution_vectors import ROOT,write,digest
from validate_karst_precompiles import main as validate


def main(*, output_base=None, vectors_path=None, extra_checks=(), extra_references=None, extra_artifacts=(), extra_required=None, extra_report=None, extra_api_sources=None, extra_api_runners=None, verification_scope='KARST_EXECUTION_COMPONENTS_FULL_ORIGINAL_SIGNED_LOCAL_BANK_PLUS_HISTORICAL_APIS_NOT_KARST_ADMISSION_OR_CURRENT_BANK_DERIVATION_CONSENSUS'):
    output=output_base or ROOT/'integration/artifacts/local/karst-execution-check'
    vectors=vectors_path or output/'original-runs'/uuid.uuid4().hex/'vectors.json'
    if vectors_path is None: refs=write(vectors)
    else:
        import json
        refs=json.loads(vectors.read_bytes())['references']
    refs[str(vectors.relative_to(ROOT))]=digest(vectors)
    refs.update(extra_references or {})
    replay=vectors.parent/'replay.json'
    env=['env','XONE_KARST_EXECUTION_VECTORS='+str(vectors)]
    guards=['-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_karst_execution_vectors.py']
    validate(output_base=output,extra_references=refs,extra_api_sources=extra_api_sources,extra_api_runners=extra_api_runners,extra_checks=list(extra_checks)+[
        ('karst-execution-wrapper-format',['rustfmt','+1.90.0','--edition','2024','--check','integration/tools/acceptance/reference/op_karst_execution.rs']),
        ('karst-execution-reference-guards',env+[sys.executable]+guards),
        ('karst-execution-reference-guards-optimized',env+[sys.executable,'-OO']+guards),
        ('karst-full-original-replay',[sys.executable,'integration/tools/acceptance/karst_execution_vectors.py','--replay',str(vectors),str(replay)]),
        ('karst-signed-execution-components',env+['cargo','+1.90.0','test','--locked','--manifest-path','dex-router/Cargo.toml','-p','xone-simulation','--lib','karst_execution_tests::','--','--include-ignored']),
    ],extra_required={'karst-signed-execution-components':[
        'revm_backend::execute::karst_execution_tests::karst_execution_components_match_full_original_signed_gas_fees_and_poststate',
        'revm_backend::execute::karst_execution_tests::karst_clz_never_bypasses_instruction_control_or_transaction_cap',
    ]+list((extra_required or {}).get('karst-signed-execution-components',[])),**{k:v for k,v in (extra_required or {}).items() if k!='karst-signed-execution-components'}},extra_artifacts=list(extra_artifacts)+[str(replay.relative_to(ROOT))],extra_report=dict(extra_report or {},karst_full_original_signed_scenarios=468,karst_signed_product_executions=401,karst_signed_rejections=67,karst_clz_results=257,karst_current_vm_admitted=bool((extra_report or {}).get('karst_current_vm_admitted', False))),verification_scope=verification_scope)


if __name__=='__main__':main()
