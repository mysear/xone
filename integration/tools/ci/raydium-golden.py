#!/usr/bin/env python3
"""Build the unchanged original native program, test, generate and audit Raydium goldens.

No service/business crate is imported. JSON is emitted by Rust and audited with
Python's arbitrary precision integers; never pass these fixtures through JS Number.
"""
import argparse
from collections import Counter
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys

sys.dont_write_bytecode = True
import tomllib

ROOT = Path(__file__).resolve().parents[3]
REFERENCE = ROOT / 'integration/tools/reference/raydium-cpmm'
RUNTIME = ROOT / 'integration/.runtime/raydium-reference'
OUTPUT = ROOT / 'integration/test/contracts/raydium-quotes.json'
COPY = ROOT / 'dex-router/test/contracts/raydium-quotes.json'
FIXTURE = ROOT / 'integration/test/solana/raydium-base.json'
ARTIFACTS = ROOT / 'integration/artifacts/local'
TOOLCHAIN = '1.90.0'


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def net(state, side):
    return state[f'raw_vault{side}'] - sum(state[f'{kind}_fees{side}'] for kind in ['protocol', 'fund', 'creator'])


def audit(corpus):
    """Accounting/conservation/coverage checks, not a second swap-math oracle."""
    assert corpus['reference']['revision'] == 'f4b9b26342afe9122d5ff67cd7a4f19ded4ac0a5'
    cases = corpus['cases']
    assert len(cases) >= 200
    coverage = Counter()
    changed_fields = {'raw_vault0', 'raw_vault1', 'protocol_fees0', 'protocol_fees1', 'fund_fees0', 'fund_fees1', 'creator_fees0', 'creator_fees1', 'recent_epoch'}
    total_success = 0
    checked = []
    for c in cases + [step for seq in corpus['sequences'] for step in seq['steps']]:
        p, e = c['pool'], c['expected']
        assert isinstance(c['input'], str)
        assert p['protocol'] == 'raydium-cpmm-creator' and p['chain_key'] == 'solana:localnet'
        assert p['adapter_version'] == 'raydium-cpmm/f4b9b263-spl-1'
        s = p['raydium_cpmm']
        coverage[(c['zero_for_one'], s['creator_fee_on'], s['creator_fee_enabled'], bool(p['fee_numerator']), bool(s['creator_fee_rate']))] += 1
        if e.get('reverted'):
            assert set(e) == {'reverted'}
            continue
        total_success += 1
        n, f = e['raydium_cpmm'], c['raw_reference']
        assert set(n) == set(s), 'poststate must preserve every field'
        assert int(e['amount_out']) > 0
        i = 0 if c['zero_for_one'] else 1
        o = 1 - i
        assert n[f'raw_vault{i}'] == s[f'raw_vault{i}'] + int(c['input'])
        assert n[f'raw_vault{o}'] == s[f'raw_vault{o}'] - int(e['amount_out'])
        assert n['recent_epoch'] == s['bank_epoch']
        for field in set(s) - changed_fields:
            assert n[field] == s[field], f'unrelated field changed: {field}'
        for side in [0, 1]:
            assert str(net(n, side)) == e[f'reserve{side}']
            for kind in ['protocol', 'fund']:
                assert n[f'{kind}_fees{side}'] - s[f'{kind}_fees{side}'] == (f[f'{kind}_fee'] if side == i else 0)
            creator_side = i if f['creator_fee_on_input'] else o
            assert n[f'creator_fees{side}'] - s[f'creator_fees{side}'] == (f['creator_fee'] if side == creator_side else 0)
        if not s['creator_fee_enabled']:
            assert f['creator_fee'] == 0
        for field, value in n.items():
            if isinstance(value, int) and not isinstance(value, bool):
                assert 0 <= value <= 2**64 - 1, field
        assert int(e['reserve0']) * int(e['reserve1']) >= int(p['reserve0']) * int(p['reserve1'])
        checked.append(c)
    assert len(corpus['sequences']) >= 8
    for seq in corpus['sequences']:
        p = seq['pool']
        assert len(seq['steps']) >= 4
        for step in seq['steps']:
            assert step['pool'] == p, 'shared-pool state does not chain'
            e = step['expected']
            assert not e.get('reverted')
            p = dict(p, reserve0=e['reserve0'], reserve1=e['reserve1'], raydium_cpmm=e['raydium_cpmm'])
        assert seq['expected_final_pool'] == p
    for direction in [True, False]:
        for mode in range(3):
            for enabled in [True, False]:
                assert any(k[:3] == (direction, mode, enabled) for k in coverage)
                assert any(c['zero_for_one'] == direction and c['pool']['raydium_cpmm']['creator_fee_on'] == mode and c['pool']['raydium_cpmm']['creator_fee_enabled'] == enabled for c in checked)
    assert any(c['pool']['raydium_cpmm']['raw_vault0'] > 2**53 for c in checked)
    assert any(c['expected']['raydium_cpmm']['raw_vault0'] == 2**64 - 1 for c in checked)
    assert any(c['expected']['raydium_cpmm']['raw_vault1'] == 2**64 - 1 for c in checked)
    assert any(c['original_outcome'] == 'panic' for c in corpus['core_cases'])
    assert any(c['original_outcome'] == 'None' for c in corpus['core_cases'])
    assert any(c['expected'].get('reverted') for c in corpus['helper_cases'])
    names = [c['name'] for c in cases]
    assert len(names) == len(set(names))
    return {'cases': len(cases), 'successful_cases': sum(not c['expected'].get('reverted', False) for c in cases), 'reverted_cases': sum(c['expected'].get('reverted', False) for c in cases), 'sequences': len(corpus['sequences']), 'sequence_steps': sum(len(s['steps']) for s in corpus['sequences']), 'core_u128_cases': len(corpus['core_cases']), 'direct_helper_cases': len(corpus['helper_cases']), 'accounting_checks': total_success, 'all_cases_have_full_poststate_or_reverted': True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true', help='Rebuild/test and compare all saved goldens without modifying them')
    parser.add_argument('--offline', action='store_true', help='Require Cargo dependencies already in the local cache')
    args = parser.parse_args()
    spec = importlib.util.spec_from_file_location('raydium_fetch', Path(__file__).with_name('raydium-reference-fetch.py'))
    fetch = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(fetch)
    sources = fetch.verify()
    rustup = shutil.which('rustup') or str(Path.home() / '.cargo/bin/rustup')
    def tool(name):
        return subprocess.check_output([rustup, 'which', '--toolchain', TOOLCHAIN, name], text=True).strip()
    cargo, rustc, rustdoc = (tool(t) for t in ['cargo', 'rustc', 'rustdoc'])
    env = dict(os.environ, RUSTC=rustc, RUSTDOC=rustdoc, CARGO_TARGET_DIR=str(RUNTIME / 'target'))
    RUNTIME.mkdir(parents=True, exist_ok=True)
    commands = []
    transcript = []
    def run(cmd, *, native_json=False):
        commands.append(cmd)
        completed = subprocess.run(cmd, cwd=ROOT, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        transcript.append('$ ' + ' '.join(cmd) + '\n' + ('' if native_json else completed.stdout.decode()) + completed.stderr.decode())
        if completed.returncode:
            print(transcript[-1], end='')
            if not args.check:
                ARTIFACTS.mkdir(parents=True, exist_ok=True)
                (ARTIFACTS / 'raydium-golden-generation.log').write_text('\n'.join(transcript))
            raise SystemExit(completed.returncode)
        return completed.stdout
    version = run([rustc, '-Vv']).decode().strip()
    assert version.startswith('rustc 1.90.0 '), version
    cargo_version = run([cargo, '-V']).decode().strip()
    cmd = [cargo, 'test', '--locked', '--manifest-path', str(REFERENCE / 'Cargo.toml')]
    if args.offline:
        cmd.append('--offline')
    run(cmd)
    build = [cargo, 'build', '--locked', '--manifest-path', str(REFERENCE / 'Cargo.toml')]
    if args.offline:
        build.append('--offline')
    run(build)
    binary = str(RUNTIME / 'target/debug/raydium-cpmm-reference')
    generated = run([binary, 'generate'], native_json=True)
    repeat = run([binary, 'generate'], native_json=True)
    assert generated == repeat, 'native generation is nondeterministic'
    corpus = json.loads(generated)
    counts = audit(corpus)
    fixture = run([binary, 'fixture'], native_json=True)
    base = json.loads(fixture)
    assert base['pool'] == corpus['base_pool']
    assert base['metadata']['synthetic_proof_identity'] is True
    locks = tomllib.loads((REFERENCE / 'Cargo.lock').read_text())
    dependencies = [{'name': p['name'], 'version': p['version'], 'checksum': p.get('checksum'), 'source': p.get('source', 'path')} for p in locks['package']]
    assert not any(p['source'] == 'path' and p['name'] not in ['raydium-cpmm-reference', 'raydium-cp-swap'] for p in dependencies)
    assert all(p['version'] == '1.1.2' for p in dependencies if p['name'] in ['anchor-syn', 'anchor-derive-accounts', 'anchor-attribute-program'])
    assert all(p['version'] == '1.0.2' for p in dependencies if p['name'] in ['anchor-lang', 'anchor-spl'])
    report = {'schema_version': 1, 'revision': fetch.REVISION, 'archive_sha256': sources['archive_sha256'], 'unchanged_source_files_verified': len(sources['files']), 'source_adaptations': [], 'execution': 'native unchanged upstream crate with no-entrypoint; original helpers plus documented standard SPL balance wrapper', 'business_math_imported': False, 'sbf_transaction_executed': False, 'token_program_executed': False, 'synthetic_proof_identity': True, 'authority': base['pool']['raydium_cpmm']['authority'], 'authority_bump': base['pool']['raydium_cpmm']['authority_bump'], 'compiler': version, 'cargo': cargo_version, 'cargo_lock_sha256': sha256((REFERENCE / 'Cargo.lock').read_bytes()), 'original_cargo_lock_sha256': sha256((REFERENCE / 'upstream/Cargo.lock').read_bytes()), 'golden_sha256': sha256(generated), 'fixture_sha256': sha256(fixture), 'source_manifest_sha256': sha256((REFERENCE / 'source-metadata.json').read_bytes()), 'harness_files': {str(p.relative_to(ROOT)): sha256(p.read_bytes()) for p in sorted(list((REFERENCE / 'src').glob('*.rs')) + list((REFERENCE / 'tests').glob('*.rs')) + [REFERENCE / 'Cargo.toml', Path(__file__).resolve(), Path(__file__).with_name('raydium-reference-fetch.py').resolve()])}, 'dependencies': dependencies, 'tests': 'cargo test --locked (including doctests), all passed', 'counts': counts, 'deterministic_regeneration': True, 'golden_copies_identical': True, 'commands': commands, 'environment': {'RUSTC': rustc, 'RUSTDOC': rustdoc, 'CARGO_TARGET_DIR': str(RUNTIME / 'target')}, 'coverage_limits': ['No SBF/bank execution, CPI, token program, observation update, token account decoding, owner/signature or account constraints are executed by this oracle', 'Proof code hashes are synthetic fixture identities, not hashes of deployed mainnet bytecode', 'Core u128 and standalone ledger-overflow cases can exceed the reachable SPL u64 domain', 'Failed helper mutations are discarded; successful pool cases preserve the full shared State API, not unmodeled observation/LP/account bytes']}
    targets = [(OUTPUT, generated), (COPY, generated), (FIXTURE, fixture), (REFERENCE / 'base-fixture.json', fixture)]
    if args.check:
        for path, data in targets:
            assert path.read_bytes() == data, f'saved output is stale: {path}'
        saved = json.loads((ARTIFACTS / 'raydium-golden-generation.json').read_text())
        for field in ['cargo_lock_sha256', 'original_cargo_lock_sha256', 'golden_sha256', 'fixture_sha256', 'source_manifest_sha256', 'harness_files', 'counts']:
            assert saved[field] == report[field], f'saved report is stale: {field}'
    else:
        for path, data in targets:
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        ARTIFACTS.mkdir(parents=True, exist_ok=True)
        (ARTIFACTS / 'raydium-golden-generation.json').write_text(json.dumps(report, indent=2) + '\n')
        transcript.append(json.dumps({'audit': counts, 'golden_sha256': sha256(generated), 'fixture_sha256': sha256(fixture), 'copies_identical': OUTPUT.read_bytes() == COPY.read_bytes()}, indent=2))
        (ARTIFACTS / 'raydium-golden-generation.log').write_text('\n'.join(transcript) + '\n')
    print(json.dumps({'verified': True, 'check_only': args.check, **counts, 'golden_sha256': sha256(generated)}, indent=2))


if __name__ == '__main__':
    main()
