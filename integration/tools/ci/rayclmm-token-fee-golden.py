#!/usr/bin/env python3
"""Generate/audit token-fee goldens using only the pinned original native program."""
import argparse
from collections import Counter
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tomllib

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[3]
REFERENCE = ROOT / 'integration/tools/reference/raydium-clmm'
OUTPUT = ROOT / 'integration/test/contracts/rayclmm-token-fee-quotes.json'
COPY = ROOT / 'dex-router/test/contracts/rayclmm-token-fee-quotes.json'
ARTIFACTS = ROOT / 'integration/artifacts/local'
RUNTIME = Path('/tmp/xone-raydium-clmm/target2')
PROFILE = 'raydium-clmm-token-fee'
ADAPTER = 'raydium-clmm/6d7a0f82-spl-token-fee-1'
REVISION = '6d7a0f82d2f293c553c69aaf943377367c6c4cc6'
OLD_HASH = 'ef1a0be7c80d6f7f5e815b75a10223a9a7be225d279bc74e057664b5af4f6f31'
REPORT = ARTIFACTS / 'rayclmm-token-fee-golden-generation.json'


def sha(data):
    return hashlib.sha256(data).hexdigest()


def load_script(name):
    spec = importlib.util.spec_from_file_location(name.replace('-', '_'), Path(__file__).with_name(name + '.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def uint(value, bits, string=False):
    if string:
        assert type(value) is str and value.isascii() and value.isdecimal(), (bits, value)
        assert value == str(int(value)), value
    else:
        assert type(value) is int, (bits, value)
    assert 0 <= int(value) < 2**bits, (bits, value)


def audit_pool(pool):
    assert pool['protocol'] == PROFILE
    assert pool['adapter_version'] == ADAPTER
    assert pool['reserve0'] == pool['reserve1'] == '0'
    state = pool['raydium_clmm']
    assert type(state['collect_fee_on']) is int and state['collect_fee_on'] in (1, 2)
    for field in ['sqrt_price_x64', 'liquidity', 'fee_growth_global0', 'fee_growth_global1']:
        uint(state[field], 128, string=True)
    for field in ['raw_vault0', 'raw_vault1', 'protocol_fees0', 'protocol_fees1', 'fund_fees0', 'fund_fees1',
                  'bank_slot', 'bank_unix_timestamp', 'bank_epoch', 'open_time', 'recent_epoch', 'observation_recent_epoch']:
        uint(state[field], 64)
    for field in ['trade_fee_rate', 'protocol_fee_rate', 'fund_fee_rate', 'tick_spacing', 'max_steps',
                  'pool_bump', 'mint0_decimals', 'mint1_decimals', 'observation_index']:
        uint(state[field], 32)
    assert len(state['bitmap']) == 16
    for word in state['bitmap']:
        uint(word, 64)
    assert len(state['observations']) == 100
    for observation in state['observations']:
        uint(observation['block_timestamp'], 32)
        assert type(observation['tick_cumulative']) is int and -(2**63) <= observation['tick_cumulative'] < 2**63
    for array in state['tick_arrays']:
        uint(array['recent_epoch'], 64)
        for tick in array['ticks']:
            net = tick['liquidity_net']
            assert type(net) is str and str(int(net)) == net and -(2**127) <= int(net) < 2**127
            for field in ['liquidity_gross', 'fee_growth_outside0', 'fee_growth_outside1']:
                uint(tick[field], 128, string=True)


def audit_bases(corpus, old_base):
    for mode, field in [(1, 'base_pool'), (2, 'base_pool_mode2')]:
        expected = json.loads(json.dumps(old_base))
        expected['protocol'] = PROFILE
        expected['adapter_version'] = ADAPTER
        expected['raydium_clmm']['collect_fee_on'] = mode
        assert corpus[field] == expected, f'{field} differs from original raw-RPC base beyond profile/adapter/mode'
        audit_pool(corpus[field])
        assert corpus[field]['raydium_clmm']['trade_fee_rate'] == 2500


def audit_result(pool, amount, direction, expected):
    audit_pool(pool)
    uint(amount, 64, string=True)
    assert type(direction) is bool
    if 'reverted' in expected:
        assert expected == {'reverted': True} and expected['reverted'] is True
        return Counter()
    assert set(expected) == {'amount_out', 'next_pool'}
    uint(expected['amount_out'], 64, string=True)
    assert int(expected['amount_out']) > 0 and int(amount) > 0
    after_pool = expected['next_pool']
    audit_pool(after_pool)
    assert {k: v for k, v in pool.items() if k != 'raydium_clmm'} == {k: v for k, v in after_pool.items() if k != 'raydium_clmm'}
    before, after = pool['raydium_clmm'], after_pool['raydium_clmm']
    assert after['collect_fee_on'] == before['collect_fee_on']
    input_token, output_token = (0, 1) if direction else (1, 0)
    fee_token = before['collect_fee_on'] - 1
    assert after[f'raw_vault{input_token}'] == before[f'raw_vault{input_token}'] + int(amount)
    assert after[f'raw_vault{output_token}'] == before[f'raw_vault{output_token}'] - int(expected['amount_out'])
    assert after['recent_epoch'] == before['bank_epoch']
    assert after['observation_recent_epoch'] == before['observation_recent_epoch']
    assert after['bitmap'] == before['bitmap']
    for field in [f'protocol_fees{1-fee_token}', f'fund_fees{1-fee_token}', f'fee_growth_global{1-fee_token}']:
        assert after[field] == before[field], f'non-fee token changed: {field}'
    for field in ['protocol_fees0', 'protocol_fees1', 'fund_fees0', 'fund_fees1']:
        assert after[field] >= before[field]
    counts = Counter(successful_results=1)
    counts['input_fee_results' if fee_token == input_token else 'output_fee_results'] += 1
    if int(after[f'fee_growth_global{fee_token}']) < int(before[f'fee_growth_global{fee_token}']):
        counts['growth_wrap_results'] += 1
    if sum(a['recent_epoch'] != b['recent_epoch'] for a, b in zip(after['tick_arrays'], before['tick_arrays'])) >= 2:
        counts['cross_array_results'] += 1
    if after['observation_index'] != before['observation_index']:
        counts['observation_ring_updates'] += 1
    return counts


def audit(corpus):
    assert len(corpus['cases']) >= 300 and len(corpus['sequences']) >= 8
    assert corpus['reference']['revision'] == REVISION
    counts = Counter()
    names = [case['name'] for case in corpus['cases']]
    assert len(names) == len(set(names))
    cells = Counter()
    vectors = set()
    assert 'independent_vectors' in corpus
    independent = corpus['independent_vectors']
    assert len(independent) == len(set(independent)) == 8
    assert set(independent).issubset(names)
    for case in corpus['cases']:
        assert set(case) == {'name', 'pool', 'input', 'zero_for_one', 'expected'}
        pool, amount, direction, expected = [case[k] for k in ['pool', 'input', 'zero_for_one', 'expected']]
        counts.update(audit_result(pool, amount, direction, expected))
        mode = pool['raydium_clmm']['collect_fee_on']
        cells[(mode, direction)] += 1
        counts[f'mode{mode}_cases'] += 1
        counts['cases'] += 1
        counts['reverted_cases' if expected.get('reverted') else 'successful_cases'] += 1
        counts['zero_for_one' if direction else 'one_for_zero'] += 1
        if case['name'] in independent:
            assert amount in ('1000000', '2000000000')
            assert pool == corpus['base_pool' if mode == 1 else 'base_pool_mode2']
            assert not expected.get('reverted')
            vectors.add((mode, direction, amount))
    assert len(set(cells.values())) == 1 and set(cells) == {(m, d) for m in (1, 2) for d in (True, False)}, cells
    assert vectors == {(m, d, a) for m in (1, 2) for d in (True, False) for a in ('1000000', '2000000000')}, vectors
    seq_names = [sequence['name'] for sequence in corpus['sequences']]
    assert len(seq_names) == len(set(seq_names))
    sequence_shapes = set()
    for sequence in corpus['sequences']:
        assert set(sequence) == {'name', 'pool', 'steps', 'expected_final_pool'}
        current = sequence['pool']
        assert len(sequence['steps']) >= 2
        mode = current['raydium_clmm']['collect_fee_on']
        directions = []
        for step in sequence['steps']:
            assert set(step) == {'input', 'zero_for_one', 'expected'}
            counts.update(audit_result(current, step['input'], step['zero_for_one'], step['expected']))
            assert not step['expected'].get('reverted')
            directions.append(step['zero_for_one'])
            current = step['expected']['next_pool']
            counts['sequence_steps'] += 1
        assert current == sequence['expected_final_pool']
        sequence_shapes.add((mode, 'mixed' if len(set(directions)) == 2 else str(directions[0])))
        counts['sequences'] += 1
        counts[f'mode{mode}_sequences'] += 1
    assert sequence_shapes == {(mode, shape) for mode in (1, 2) for shape in ('mixed', 'True', 'False')}
    for label in ['zero-fee', 'static-fee', 'high-fee', 'protocol-all', 'fund-all', 'lp-only', 'empty-bitmap',
                  'zero-liquidity', 'growth-wrap', 'u128-max', 'u64-max', 'raw-input-overflow', 'raw-output-underflow',
                  'protocol-counter-overflow', 'fund-counter-overflow', 'tick-boundary', 'partial-window-exhaustion',
                  'observation-wrap', 'bitmap-low-edge', 'bitmap-high-edge', 'min-tick', 'max-tick', 'cross-liquidity-delta']:
        assert any(label in name for name in names), label
    for field in ['growth_wrap_results', 'cross_array_results', 'observation_ring_updates', 'input_fee_results', 'output_fee_results']:
        assert counts[field] > 0, field
    counts['independent_base_vectors'] = len(vectors)
    counts.update({f'mode{mode}_{"zero_for_one" if direction else "one_for_zero"}_cases': count for (mode, direction), count in cells.items()})
    return dict(sorted(counts.items()))


def source_checks():
    manifest = tomllib.loads((REFERENCE / 'Cargo.toml').read_text())
    assert manifest['dependencies']['raydium-clmm'] == {'path': 'upstream/programs/amm', 'features': ['no-entrypoint']}
    text = (REFERENCE / 'src/lib.rs').read_text()
    for required in ['instructions::swap::swap_internal', 'let result = swap_internal(', 'RefCell::new(state)',
                     'a.borrow_mut()', 'set_syscall_stubs', 'sol_get_clock_sysvar', '.set_fee_on(']:
        assert required in text, required
    assert PROFILE in text and ADAPTER in text
    assert 'generate-token-fee' in (REFERENCE / 'src/main.rs').read_text()
    for path in (REFERENCE / 'src').glob('*.rs'):
        for forbidden in ['protocol_math', 'protocol-math', 'chain_parser', 'dex_router']:
            assert forbidden not in path.read_text(), (path, forbidden)
    lock = tomllib.loads((REFERENCE / 'Cargo.lock').read_text())
    dependencies = [{'name': p['name'], 'version': p['version'], 'source': p.get('source', 'path'), 'checksum': p.get('checksum')} for p in lock['package']]
    assert {p['name'] for p in dependencies if p['source'] == 'path'} == {'raydium-clmm', 'raydium-clmm-reference'}
    uint_deps = [p for p in dependencies if p['name'] == 'uint']
    assert len(uint_deps) == 1 and uint_deps[0]['source'] == 'git+https://github.com/raydium-io/parity-common#43a0303b9c2a23a00508816bd6d5a2e9381f143c'
    return dependencies


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--offline', action='store_true')
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--report-only', action='store_true', help='Verify existing bytes and refresh generation evidence only')
    args = parser.parse_args()
    sources = load_script('rayclmm-reference-fetch').verify()
    assert sum(len(archive['files']) for archive in sources['archives']) == 225
    dependencies = source_checks()
    old_paths = [ROOT / project / 'test/contracts/rayclmm-quotes.json' for project in ['integration', 'dex-router']]
    for path in old_paths:
        assert sha(path.read_bytes()) == OLD_HASH, f'old 480-case corpus changed: {path}'
    old_data = old_paths[0].read_bytes()
    old_corpus = json.loads(old_data)
    assert len(old_corpus['cases']) == 480
    toolchain = Path.home() / '.rustup/toolchains/1.90.0-x86_64-apple-darwin/bin'
    cargo, rustc, rustdoc = [str(toolchain / name) for name in ['cargo', 'rustc', 'rustdoc']]
    env = dict(os.environ, RUSTC=rustc, RUSTDOC=rustdoc, CARGO_TARGET_DIR=str(RUNTIME))
    commands, transcript = [], []

    def run(command, payload=False):
        commands.append(command)
        completed = subprocess.run(command, cwd=REFERENCE, env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        transcript.append('$ ' + ' '.join(command) + '\n' + completed.stderr.decode() +
                          (('stdout SHA-256: ' + sha(completed.stdout) + '\n') if payload else completed.stdout.decode()))
        if completed.returncode:
            print(transcript[-1], file=sys.stderr)
            raise SystemExit(completed.returncode)
        return completed.stdout

    compiler = run([rustc, '-Vv']).decode().strip()
    assert compiler.startswith('rustc 1.90.0 ')
    cargo_version = run([cargo, '-V']).decode().strip()
    flags = ['--locked', '--config', './cargo-source.toml'] + (['--offline'] if args.offline else [])
    run([sys.executable, str(REFERENCE / 'tests/token_fee_runner.py')])
    run([cargo, 'test', *flags])
    run([cargo, 'build', *flags])
    binary = str(RUNTIME / 'debug/raydium-clmm-reference')
    generated = run([binary, 'generate-token-fee'], payload=True)
    assert generated == run([binary, 'generate-token-fee'], payload=True), 'nondeterministic corpus'
    assert old_data == run([binary, 'generate'], payload=True), 'native generator changed original 480-case corpus'
    corpus = json.loads(generated)
    audit_bases(corpus, old_corpus['base_pool'])
    counts = audit(corpus)
    fixture = run([binary, 'fixture'], payload=True)
    for path in [REFERENCE / 'base-fixture.json', ROOT / 'integration/test/solana/rayclmm-base.json']:
        assert fixture == path.read_bytes(), f'original RPC fixture changed: {path}'
    old_report_path = ARTIFACTS / 'rayclmm-golden-generation.json'
    old_report = json.loads(old_report_path.read_bytes())
    assert old_report['golden_sha256'] == OLD_HASH
    for name, digest in old_report['harness_files'].items():
        assert sha((ROOT / name).read_bytes()) == digest, f'refresh old evidence with rayclmm-golden.py --offline --report-only: {name}'
    harness = sorted(set(list((REFERENCE / 'src').glob('*.rs')) + list((REFERENCE / 'tests').glob('*.rs')) +
                         list((REFERENCE / 'tests').glob('*.py')) + [REFERENCE / 'Cargo.toml', REFERENCE / 'cargo-source.toml',
                         REFERENCE / 'README.md', Path(__file__).resolve(), Path(__file__).with_name('rayclmm-golden.py'),
                         Path(__file__).with_name('rayclmm-reference-fetch.py')]))
    vectors = [{'name': c['name'], 'collect_fee_on': c['pool']['raydium_clmm']['collect_fee_on'],
                'zero_for_one': c['zero_for_one'], 'input': c['input'], 'amount_out': c['expected']['amount_out']}
               for c in corpus['cases'] if c['name'] in corpus['independent_vectors']]
    report = {'schema_version': 1, 'revision': REVISION, 'profile': PROFILE, 'adapter_version': ADAPTER,
              'archives': {a['archive']: a['archive_sha256'] for a in sources['archives']},
              'unchanged_source_files_verified': 225, 'source_adaptations': [],
              'dependency_source_replacement': 'Locked git uint served from byte-identical archived source with generated Cargo checksum metadata',
              'compiler': compiler, 'cargo': cargo_version, 'cargo_lock_sha256': sha((REFERENCE / 'Cargo.lock').read_bytes()),
              'original_cargo_lock_sha256': sha((REFERENCE / 'upstream/Cargo.lock').read_bytes()),
              'source_manifest_sha256': sha((REFERENCE / 'source-metadata.json').read_bytes()),
              'golden_sha256': sha(generated), 'old_golden_sha256': OLD_HASH,
              'old_generation_report_sha256': sha(old_report_path.read_bytes()), 'fixture_sha256': sha(fixture),
              'harness_files': {str(p.relative_to(ROOT)): sha(p.read_bytes()) for p in harness}, 'counts': counts,
              'independent_base_vectors': vectors, 'deterministic_regeneration': True, 'golden_copies_identical': True,
              'old_corpus_byte_identical': True, 'base_modes_match_original_raw_rpc_pool': True, 'raw_pool_fee_on_offset': 390,
              'business_math_imported': False, 'native_swap_internal_executed': True, 'native_clock_syscall_executed': True,
              'observation_poststate_executed': True, 'sbf_transaction_executed': False, 'token_program_executed': False,
              'synthetic_proof_identity': True, 'tests': 'Python audit tests and cargo test --locked passed (native integration tests and doctests)',
              'dependencies': dependencies, 'commands': commands,
              'environment': {'RUSTC': rustc, 'RUSTDOC': rustdoc, 'CARGO_TARGET_DIR': str(RUNTIME)},
              'coverage_limits': old_report['coverage_limits'] + [
                  'Only collect_fee_on 1 (token0) and 2 (token1) admitted for this profile; static profile still admits only 0',
                  'Fee-side selection, per-step rounding, LP growth, protocol/fund counters and tick crossing execute unchanged original source',
                  'Mode1/mode2 base pools differ from the original raw-RPC base only in profile, adapter and collect_fee_on; smoke derives mode bytes at packed offset390',
                  'This runner starts no services, infrastructure or service cargo builds; parser/process/math integration is owned by other workers']}
    if args.check:
        for path in [OUTPUT, COPY]:
            assert path.read_bytes() == generated, f'stale corpus: {path}'
        saved = json.loads(REPORT.read_bytes())
        # Commands may differ in --offline use. All evidence and dependency hashes must match.
        for field in report.keys() - {'commands'}:
            assert saved[field] == report[field], f'stale report: {field}'
    else:
        for path in [OUTPUT, COPY]:
            if args.report_only:
                assert path.read_bytes() == generated, f'report-only would change output: {path}'
            else:
                path.write_bytes(generated)
        REPORT.write_text(json.dumps(report, indent=2) + '\n')
        (ARTIFACTS / 'rayclmm-token-fee-golden-generation.log').write_text('\n'.join(transcript) + '\n' +
            json.dumps({'counts': counts, 'golden_sha256': sha(generated), 'old_golden_sha256': OLD_HASH}, indent=2) + '\n')
    print(json.dumps({'verified': True, 'check_only': args.check, **counts, 'golden_sha256': sha(generated), 'old_golden_sha256': OLD_HASH}, indent=2))


if __name__ == '__main__':
    main()
