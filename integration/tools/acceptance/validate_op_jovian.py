#!/usr/bin/env python3
"""Frozen historical OP Jovian qualification through normal signed HTTP."""
import json
import os
from pathlib import Path
import subprocess
import sys
import uuid
from op_jovian_vectors import ROOT, write, digest, require
from validate_op_api import validate


def main(*, output_base=None, additional_checks=(), additional_references=None, additional_artifacts=(), additional_required=None, additional_report=None, verification_scope='LOCAL_SIGNED_OP_MAINNET_HISTORICAL_JOVIAN_USER_VM_FEES_CRYPTO_NORMAL_HTTP_NOT_KARST_OR_CURRENT_VM_OR_TRUSTED_BANK'):
    output = output_base or ROOT / 'integration/artifacts/local/op-jovian-check'
    directory = output / 'original-runs' / uuid.uuid4().hex
    refs = write(directory)
    refs.update(additional_references or {})
    execution = directory / 'execution/vectors.json'
    crypto = directory / 'precompiles/vectors.json'
    sources = directory / 'jovian-api-sources.json'
    env = dict(os.environ, PATH=str(Path.home() / '.cargo/bin') + os.pathsep + os.environ['PATH'], XONE_OP_JOVIAN_VECTORS=str(execution), XONE_OP_JOVIAN_API_INPUTS=str(sources), RUST_TEST_THREADS='1')
    env.setdefault('CARGO_TARGET_DIR', str(ROOT / 'integration/.runtime/design-review-target'))
    env.setdefault('CARGO_BUILD_JOBS', '2')
    if sys.platform == 'darwin':
        import platform
        triple = ('aarch64' if platform.machine() == 'arm64' else 'x86_64') + '_APPLE_DARWIN'
        env.setdefault('CARGO_TARGET_' + triple + '_RUNNER', str(ROOT / 'integration/tools/ci/native-test-runner.py'))
    cargo = ['cargo', '+1.90.0', 'test', '--locked', '--manifest-path', 'dex-router/Cargo.toml', '-p', 'xone-simulation']
    result = subprocess.run(cargo + ['--test', 'op_bound', 'export_original_op_bound_inputs_for_jovian_normal_api_sources', '--', '--include-ignored', '--exact'], cwd=ROOT, env=env, capture_output=True, text=True, timeout=180)
    log = directory / 'export.log'
    log.write_text(result.stdout + result.stderr)
    require(result.returncode == 0 and 'test export_original_op_bound_inputs_for_jovian_normal_api_sources ... ok' in result.stdout, 'Jovian source export failed: ' + result.stderr[-2000:])
    for p in [sources, log]: refs[str(p.relative_to(ROOT))] = digest(p)
    replay = directory / 'replay.json'
    guard_env = ['env', 'XONE_OP_JOVIAN_REFERENCE=' + str(directory)]
    crypto_env = ['env', 'XONE_OP_PRECOMPILE_VECTORS=' + str(crypto)]
    signed_env = ['env', 'XONE_OP_JOVIAN_VECTORS=' + str(execution)]
    guards = ['-m', 'unittest', 'discover', '-s', 'integration/tools/acceptance/tests', '-p', 'test_op_jovian_vectors.py']
    validate(output_base=output, additional_api_sources=sources, additional_references=refs,
             additional_checks=list(additional_checks) + [
                 ('jovian-reference-guards', guard_env + [sys.executable] + guards),
                 ('jovian-reference-guards-optimized', guard_env + [sys.executable, '-OO'] + guards),
                 ('jovian-original-replay', [sys.executable, 'integration/tools/acceptance/op_jovian_vectors.py', '--replay', str(directory), str(replay)]),
                 ('jovian-signed-and-fee-audit', signed_env + cargo + ['--test', 'op_jovian', 'jovian_', '--', '--include-ignored']),
                 ('jovian-crypto-corpus', crypto_env + cargo + ['--lib', 'op_precompiles::', '--', '--include-ignored']),
                 ('jovian-signed-crypto-transitions', crypto_env + cargo + ['--test', 'op_precompiles', '--', '--include-ignored']),
             ], additional_required={**(additional_required or {}),
                 'simulation': ['revm_backend::execute::jovian_schedule_tests::jovian_admission_uses_only_the_pinned_op_interval_and_rejects_upgrade_blocks'],
                 'jovian-signed-and-fee-audit': ['jovian_signed_transitions_funding_refunds_aliases_and_source_fee_audit_match_original', 'jovian_source_units_fork_boundaries_and_rehashed_fee_substitutions_fail_closed'],
                 'jovian-crypto-corpus': ['revm_backend::op_precompiles::tests::op_precompiles_match_original_corpus_caps_gas_and_fork_dispatch'],
                 'jovian-signed-crypto-transitions': ['signed_op_precompiles_match_full_original_transitions_fees_and_fork_dispatch'],
             }, additional_artifacts=list(additional_artifacts) + [str(replay.relative_to(ROOT)), str(replay.with_suffix('.precompiles.json').relative_to(ROOT))],
             additional_report=dict(additional_report or {}, jovian_original_signed_scenarios=64, jovian_signed_product_executions=128, jovian_insufficient_funding_rejections=63, original_precompile_cases=7056, original_signed_precompile_transitions=160, historical_jovian_chain_id=10, historical_jovian_activation_excluded=1764691201, historical_jovian_end_exclusive=1783526401, base_later_forks_admitted=False, block_da_footprint_consensus_claimed=False),
             verification_scope=verification_scope)


if __name__ == '__main__':
    main()
