#!/usr/bin/env python3
"""Freeze the Karst execution additions, full original VM and historical APIs."""
import sys
import uuid
from karst_execution_vectors import ROOT,write,digest
from validate_karst_precompiles import main as validate


def main():
    output=ROOT/'integration/artifacts/local/karst-execution-check'
    vectors=output/'original-runs'/uuid.uuid4().hex/'vectors.json'
    refs=write(vectors);refs[str(vectors.relative_to(ROOT))]=digest(vectors)
    replay=vectors.parent/'replay.json'
    env=['env','XONE_KARST_EXECUTION_VECTORS='+str(vectors)]
    guards=['-m','unittest','discover','-s','integration/tools/acceptance/tests','-p','test_karst_execution_vectors.py']
    validate(output_base=output,extra_references=refs,extra_checks=[
        ('karst-execution-wrapper-format',['rustfmt','+1.90.0','--edition','2024','--check','integration/tools/acceptance/reference/op_karst_execution.rs']),
        ('karst-execution-reference-guards',env+[sys.executable]+guards),
        ('karst-execution-reference-guards-optimized',env+[sys.executable,'-OO']+guards),
        ('karst-full-original-replay',[sys.executable,'integration/tools/acceptance/karst_execution_vectors.py','--replay',str(vectors),str(replay)]),
        ('karst-signed-execution-components',env+['cargo','+1.90.0','test','--locked','--manifest-path','dex-router/Cargo.toml','-p','xone-simulation','--lib','karst_execution_tests::','--','--include-ignored']),
    ],extra_required={'karst-signed-execution-components':[
        'revm_backend::execute::karst_execution_tests::karst_execution_components_match_full_original_signed_gas_fees_and_poststate',
        'revm_backend::execute::karst_execution_tests::karst_clz_never_bypasses_instruction_control_or_transaction_cap',
    ]},extra_artifacts=[str(replay.relative_to(ROOT))],extra_report=dict(karst_full_original_signed_scenarios=468,karst_signed_product_executions=401,karst_signed_rejections=67,karst_clz_results=257,karst_current_vm_admitted=False),verification_scope='KARST_EXECUTION_COMPONENTS_FULL_ORIGINAL_SIGNED_LOCAL_BANK_PLUS_HISTORICAL_APIS_NOT_KARST_ADMISSION_OR_CURRENT_BANK_DERIVATION_CONSENSUS')


if __name__=='__main__':main()
