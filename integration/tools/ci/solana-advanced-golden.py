#!/usr/bin/env python3
"""Generate initialized-reward observations through unchanged pinned native programs.

This invokes native swap managers and reward/crossing functions. It never executes
SBF, Anchor instructions, token transfers, or deployed program bytecode.
"""
import argparse
import copy
import hashlib
import json
from pathlib import Path
import subprocess

ROOT = Path(__file__).resolve().parents[3]
REFERENCE = ROOT / 'integration/tools/reference'
OUTPUT = ROOT / 'integration/test/contracts/solana-advanced'
ZERO = '11111111111111111111111111111111'
Q64 = 1 << 64
MAX128 = (1 << 128) - 1
FAMILIES = {
    'orca': ('orca-whirlpools', 'orca_whirlpool', 'orca-whirlpools-rewards', 'orca-whirlpools/cc683ec9-spl-rewards-1',
             REFERENCE / 'orca-whirlpools/target/release/solana-advanced-orca-reference'),
    'raydium-clmm': ('raydium-clmm', 'raydium_clmm', 'raydium-clmm-rewards', 'raydium-clmm/6d7a0f82-spl-rewards-1',
                    Path('/tmp/xone-solana-advanced-ray/debug/solana-advanced-raydium-clmm-reference')),
}


def key(label):
    alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
    data = hashlib.sha256(label.encode()).digest()
    n = int.from_bytes(data, 'big')
    text = ''
    while n:
        n, r = divmod(n, 58)
        text = alphabet[r] + text
    return '1' * (len(data) - len(data.lstrip(b'\0'))) + text


def source_identity(name):
    folder = REFERENCE / name
    metadata = json.loads((folder / 'source-metadata.json').read_text())
    assert metadata['source_adaptations'] == []
    archives = metadata.get('archives', [{'tree': 'upstream', 'files': metadata['files']}] if 'files' in metadata else [])
    observed = []
    for archive in archives:
        for entry in archive['files']:
            path = folder / archive['tree'] / entry['path']
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            assert digest == entry['sha256'], str(path)
            observed.append({'path': archive['tree'] + '/' + entry['path'], 'sha256': digest})
    return {'revision': metadata['revision'], 'verified_source_files': len(observed),
            'source_inventory_sha256': hashlib.sha256(json.dumps(observed, sort_keys=True, separators=(',', ':')).encode()).hexdigest()}


class Oracle:
    def __init__(self, binary):
        self.process = subprocess.Popen([str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)

    def request(self, **request):
        self.process.stdin.write(json.dumps(request, separators=(',', ':')) + '\n')
        self.process.stdin.flush()
        line = self.process.stdout.readline()
        assert line, 'native oracle exited without a response'
        return json.loads(line)

    def close(self):
        self.process.stdin.close()
        assert self.process.wait(timeout=15) == 0


def advanced_pool(family, pool):
    _, state_key, profile, adapter, _ = FAMILIES[family]
    pool['protocol'], pool['adapter_version'] = profile, adapter
    s = pool[state_key]
    now = s['bank_unix_timestamp']
    infos = []
    for i in range(3):
        initialized = i < 2
        if family == 'orca':
            infos.append({'mint': pool['token' + str(i % 2)] if initialized else ZERO,
                          'vault': key(f'{family}-reward-vault-{i}') if initialized else ZERO,
                          'extension': list(bytes.fromhex('13' * 32)) if i == 0 else [0] * 32,
                          'emissions_per_second_x64': str(2 * Q64 + 1 if i == 0 else Q64 if initialized else 0),
                          'growth_global_x64': str(MAX128 - 9 if i == 0 else 20 if initialized else 0)})
        else:
            infos.append({'reward_state': 2 if initialized else 0, 'open_time': now - 100 if initialized else 0,
                          'end_time': now + 100 if initialized else 0, 'last_update_time': now - 10 if initialized else 0,
                          'emissions_per_second_x64': str(2 * Q64 + 1 if i == 0 else Q64 if initialized else 0),
                          'reward_total_emitted': 100 if initialized else 0, 'reward_claimed': 1 if initialized else 0,
                          'token_mint': pool['token' + str(i % 2)] if initialized else ZERO,
                          'token_vault': key(f'{family}-reward-vault-{i}') if initialized else ZERO,
                          'authority': key(f'{family}-reward-authority-{i}'),
                          'reward_growth_global_x64': str(MAX128 - 9 if i == 0 else 20 if initialized else 0)})
    s['reward_infos'] = infos
    for array in s['tick_arrays']:
        for tick in array['ticks']:
            tick['reward_growths_outside'] = ['5', '31', '0']
    return pool


def generate(family):
    source, sk, _, _, binary = FAMILIES[family]
    identity = source_identity(source)
    adapter = REFERENCE / 'solana-advanced' / family
    identity['adapter_files'] = {str(path.relative_to(adapter)): hashlib.sha256(path.read_bytes()).hexdigest()
                                 for path in sorted(adapter.rglob('*'))
                                 if path.is_file() and (path.suffix in ['.rs', '.toml'] or path.name == 'Cargo.lock')}
    oracle = Oracle(binary)
    try:
        base = advanced_pool(family, oracle.request(action='base'))
        fixture = oracle.request(action='fixture', pool=base)
        cases = []
        for amount in ['1', '100000', '4000000000', '12000000000']:
            for direction in [True, False]:
                result = oracle.request(action='quote', pool=base, input=amount, zero_for_one=direction)
                cases.append({'name': f'initialized-{amount}-{direction}', 'pool': base, 'input': amount,
                              'zero_for_one': direction, 'expected': result})
        for name in ['all-rewards', 'paused-emission'] + (['dynamic-tick-array'] if family == 'orca' else []):
            pool = copy.deepcopy(base)
            if name == 'all-rewards':
                r = copy.deepcopy(pool[sk]['reward_infos'][1])
                if family == 'orca':
                    r.update(mint=key('third-reward-mint'), vault=key('third-reward-vault'))
                else:
                    r.update(token_mint=key('third-reward-mint'), token_vault=key('third-reward-vault'))
                pool[sk]['reward_infos'][2] = r
            elif name == 'paused-emission':
                pool[sk]['reward_infos'][0]['emissions_per_second_x64'] = '0'
            else:
                for array in pool[sk]['tick_arrays']: array['layout'] = 'dynamic'
            for direction in [True, False]:
                result = oracle.request(action='quote', pool=pool, input='4000000000', zero_for_one=direction)
                assert not result.get('reverted'), result
                cases.append({'name': name + '-' + str(direction), 'pool': pool, 'input': '4000000000',
                              'zero_for_one': direction, 'expected': result})
        dynamic_fixture = None
        if family == 'orca':
            pool = copy.deepcopy(base)
            for array in pool[sk]['tick_arrays']: array['layout'] = 'dynamic'
            dynamic_fixture = oracle.request(action='fixture', pool=pool)
        sequences = []
        pool = copy.deepcopy(base)
        for i, (amount, direction, elapsed) in enumerate([('4000000000', True, 0), ('4000000000', False, 0), ('100000', True, 15), ('100000', False, 0)]):
            pool[sk]['bank_unix_timestamp'] += elapsed
            result = oracle.request(action='quote', pool=pool, input=amount, zero_for_one=direction)
            assert not result.get('reverted'), result
            sequences.append({'name': f'repeated-swap-{i}', 'pool': copy.deepcopy(pool), 'input': amount,
                              'zero_for_one': direction, 'expected': result})
            pool = copy.deepcopy(result['next_pool'])
        kernels = []
        names = ['ordinary', 'same-timestamp', 'zero-liquidity', 'multiplication-overflow', 'growth-wrap']
        if family == 'raydium-clmm':
            names += ['before-open', 'at-open', 'end-clamped', 'already-ended', 'emission-cap', 'emission-exhausted', 'fractional-ceil', 'quotient-overflow']
        for name in names:
            pool = copy.deepcopy(base)
            s = pool[sk]
            r = s['reward_infos'][0]
            now = s['bank_unix_timestamp']
            if name == 'same-timestamp':
                if family == 'orca': s['reward_last_updated_timestamp'] = now
                else: r['last_update_time'] = now
            if name == 'zero-liquidity': s['liquidity'] = '0'
            if name == 'multiplication-overflow': r['emissions_per_second_x64'] = str(MAX128)
            if name == 'growth-wrap':
                s['liquidity'] = '1'
                r['emissions_per_second_x64'] = str(Q64)
            if name == 'before-open': r.update(open_time=now + 10, end_time=now + 100, last_update_time=now + 10, reward_state=1)
            if name == 'at-open': r.update(open_time=now, last_update_time=now, reward_state=1)
            if name == 'end-clamped': r.update(end_time=now - 3)
            if name == 'already-ended': r.update(end_time=now - 3, last_update_time=now - 3, reward_state=3)
            if name == 'emission-cap': r['reward_total_emitted'] = (1 << 64) - 3
            if name == 'emission-exhausted': r['reward_total_emitted'] = (1 << 64) - 1
            if name == 'fractional-ceil': r['emissions_per_second_x64'] = '1'
            if name == 'quotient-overflow':
                s['liquidity'] = '1'
                r['emissions_per_second_x64'] = str(MAX128)
            outside = [str(MAX128), '31', '0']
            expected = oracle.request(action='reward_update', pool=pool, outside=outside)
            kernels.append({'name': name, 'pool': pool, 'outside': outside, 'expected': expected})
        return {'reference': dict(identity, execution='unchanged original native swap manager/state updater/tick crossing',
                                  source_adaptations=[], business_math_imported=False, sbf_executed=False,
                                  token_program_executed=False, anchor_instruction_executed=False,
                                  synthetic_proof_identity=True),
                'base_pool': base, 'fixture': fixture, 'dynamic_fixture': dynamic_fixture, 'cases': cases, 'sequences': sequences, 'kernels': kernels}
    finally:
        oracle.close()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--family', choices=list(FAMILIES), action='append')
    args = parser.parse_args()
    for family in args.family or FAMILIES:
        result = generate(family)
        output = json.dumps(result, sort_keys=True, indent=2) + '\n'
        path = OUTPUT / (family + '-rewards.json')
        if args.check:
            assert path.read_text() == output, f'stale native evidence: {path}'
        else:
            path.write_text(output)
        print(f'{family}: {len(result["cases"])} swaps, {len(result["sequences"])} sequential swaps, {len(result["kernels"])} reward/crossing cases')


if __name__ == '__main__':
    main()
