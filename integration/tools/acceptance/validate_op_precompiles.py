#!/usr/bin/env python3
"""Frozen original OP crypto/cap and signed transition/API qualification."""
from pathlib import Path
import json
import sys
import uuid
from op_precompile_vectors import ROOT, write, digest
from validate_op_api import validate


def main():
    output = ROOT / 'integration/artifacts/local/op-precompiles-check'
    vectors = output / 'original-runs' / uuid.uuid4().hex / 'vectors.json'
    refs = write(vectors)
    # Retain the failed frozen qualification and the original host diagnosis.
    for name in [
        'integration/artifacts/local/op-precompiles-check/runs/360c6062766e460491ca1a9159e8a0fa/report.json',
        'integration/.runtime/design-review/op-precompile-hash-profile/before.json',
        'integration/.runtime/design-review/op-precompile-hash-profile/after.json',
        'integration/.runtime/design-review/op-precompile-hash-profile/worker-sample.txt',
    ]:
        path = ROOT / name
        if path.exists():
            refs[name] = digest(path)
    refs[str(vectors.relative_to(ROOT))] = digest(vectors)
    data = json.loads(vectors.read_bytes())
    replay = vectors.parent / 'replay.json'
    env = ['env', 'XONE_OP_PRECOMPILE_VECTORS=' + str(vectors)]
    cargo = ['cargo', '+1.90.0', 'test', '--locked', '--manifest-path', 'dex-router/Cargo.toml', '-p', 'xone-simulation']
    validate(output_base=output, additional_references=refs,
             additional_checks=[
                 ('original-precompile-guards', env + [sys.executable, '-m', 'unittest', 'discover', '-s', 'integration/tools/acceptance/tests', '-p', 'test_op_precompile_vectors.py']),
                 ('original-precompile-guards-optimized', env + [sys.executable, '-OO', '-m', 'unittest', 'discover', '-s', 'integration/tools/acceptance/tests', '-p', 'test_op_precompile_vectors.py']),
                 ('original-precompile-replay', [sys.executable, 'integration/tools/acceptance/op_precompile_vectors.py', '--replay', str(vectors), str(replay)]),
                 ('precompile-crypto-corpus', env + cargo + ['--lib', 'op_precompiles::', '--', '--include-ignored']),
                 ('signed-precompile-transitions', env + cargo + ['--test', 'op_precompiles', '--', '--include-ignored']),
             ], additional_required={
                 'precompile-crypto-corpus': ['revm_backend::op_precompiles::tests::op_precompiles_match_original_corpus_caps_gas_and_fork_dispatch', 'revm_backend::op_precompiles::tests::op_precompile_registries_include_required_crypto_and_preserve_fork_admission'],
                 'signed-precompile-transitions': ['signed_op_precompiles_match_full_original_transitions_fees_and_fork_dispatch'],
             }, additional_artifacts=[str(replay.relative_to(ROOT))],
             additional_report=dict(original_precompile_cases=data['case_count'], original_signed_precompile_transitions=data['signed_transitions'],
                                    identity_digest_admission='FULL_FILE_PARENT_AND_CHILD_NO_CACHE',
                                    identity_hash_dev_test_optimization=3,
                                    op_precompile_scope='KZG_BLS_CRYPTO_AND_ISTHMUS_CAPS_NOT_JOVIAN_OR_COMPLETE_FORK_PROOF'),
             verification_scope='LOCAL_OP_KZG_BLS_ORIGINAL_CORPUS_ISTHMUS_LIMITS_SIGNED_TRANSITIONS_NORMAL_API_NOT_TRUSTED_BANK_OR_CURRENT_VM')


if __name__ == '__main__':
    main()
