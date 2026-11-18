#!/usr/bin/env python3
"""Frozen Karst component audit plus all existing historical signed/API gates."""
import sys
import uuid
from karst_precompile_vectors import ROOT, write, digest
from validate_op_jovian import main as validate


def main(*, output_base=None, extra_checks=(), extra_references=None, extra_artifacts=(), extra_required=None, extra_report=None, verification_scope='ORIGINAL_KARST_PRECOMPILE_COMPONENTS_PLUS_ALL_HISTORICAL_OP_SIGNED_HTTP_NOT_FULL_KARST_VM_OR_CURRENT_BANK_OR_CONSENSUS'):
    output = output_base or ROOT / 'integration/artifacts/local/karst-precompiles-check'
    vectors = output / 'original-runs' / uuid.uuid4().hex / 'vectors.json'
    refs = write(vectors); refs[str(vectors.relative_to(ROOT))] = digest(vectors)
    refs.update(extra_references or {})
    replay = vectors.parent / 'replay.json'
    env = ['env', 'XONE_KARST_PRECOMPILE_VECTORS=' + str(vectors)]
    guards = ['-m', 'unittest', 'discover', '-s', 'integration/tools/acceptance/tests', '-p', 'test_karst_precompile_vectors.py']
    validate(output_base=output, additional_references=refs,
             additional_checks=list(extra_checks) + [
                 ('karst-wrapper-format', ['rustfmt', '+1.90.0', '--edition', '2024', '--config', 'skip_children=true', '--check', 'integration/tools/acceptance/reference/op_karst_precompiles.rs']),
                 ('karst-reference-guards', env + [sys.executable] + guards),
                 ('karst-reference-guards-optimized', env + [sys.executable, '-OO'] + guards),
                 ('karst-original-component-replay', [sys.executable, 'integration/tools/acceptance/karst_precompile_vectors.py', '--replay', str(vectors), str(replay)]),
                 ('karst-component-differential', env + ['cargo', '+1.90.0', 'test', '--locked', '--manifest-path', 'dex-router/Cargo.toml', '-p', 'xone-simulation', '--lib', 'karst_precompiles::', '--', '--include-ignored']),
             ], additional_required={**(extra_required or {}),
                 'karst-component-differential': ['revm_backend::karst_precompiles::tests::karst_components_do_not_admit_a_partial_current_vm', 'revm_backend::karst_precompiles::tests::karst_precompiles_match_original_components_gas_limits_and_error_order'],
             }, additional_artifacts=list(extra_artifacts) + [str(replay.relative_to(ROOT))],
             additional_report=dict(extra_report or {}, karst_original_precompile_components=9452, karst_modexp_cases=1248, karst_p256_cases=6249, karst_current_vm_admitted=False, original_modern_op_commit='60360273aada79572fe2212497de578ad2fa249b', current_registry_commit='0ca0a93ca0aebb05c90c4dae3f1c66927f8db0af'),
             verification_scope=verification_scope)


if __name__ == '__main__':
    main()
