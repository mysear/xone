#!/usr/bin/env python3
"""Original pack/read/native service fixture generation, replay, and bound receipt."""
import argparse
import ast
import base64
import hashlib
import json
import os
from pathlib import Path
import struct
import subprocess
import sys
import tomllib

ROOT = Path(__file__).resolve().parents[3]
REF = ROOT / 'integration/tools/reference/orca-adaptive-service'
TARGET = Path('/tmp/xone-orca-adaptive-swap-native')
BINARY = TARGET / 'debug/orca-adaptive-service-reference'
OUTPUTS = [ROOT / p / 'test/contracts/orca-adaptive-service.json'
           for p in ('integration', 'chain-parser-rust', 'dex-router')]
PIN = 'cc683ec92acb5e7606caa3d8e514d9383e9c7c11'
NAMES = [f'pool-{i}-{d}-continuous' for i in range(2) for d in ('a-to-b', 'b-to-a')]
TOKEN = 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'
CLOCK = 'SysvarC1ock11111111111111111111111111111111'
ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'

def require(condition, message):
    if not condition:
        raise RuntimeError(str(message))

def sha(data):
    return hashlib.sha256(data).hexdigest()

def b58(data):
    n = int.from_bytes(data, 'big')
    out = ''
    while n:
        n, r = divmod(n, 58)
        out = ALPHABET[r] + out
    return '1' * (len(data) - len(data.lstrip(b'\0'))) + out

def raw(bank, address):
    a = bank['accounts'][address]
    require(a['data'][1] == 'base64', 'wrong account encoding')
    b = base64.b64decode(a['data'][0], validate=True)
    require(a['space'] == len(b), 'RPC account space differs')
    require(isinstance(a['lamports'], int) and isinstance(a['executable'], bool), 'invalid RPC account')
    return b

class Native:
    def __init__(self, binary):
        self.proc = subprocess.Popen([str(binary)], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
        self.identity = self.request({'action': 'identity'})
        require(self.identity['executed_binary_sha256'] == sha(binary.read_bytes()), 'executed binary identity differs')
        require(self.identity['revision'] == PIN and self.identity['sbf'] is False, 'native source identity differs')
        require(self.identity['execution'] == 'unchanged-original-native-pack-read-swap-manager-and-post-update', 'wrong execution authority')

    def request(self, req):
        self.proc.stdin.write(json.dumps(req, separators=(',', ':')) + '\n')
        self.proc.stdin.flush()
        result = self.proc.stdout.readline()
        require(bool(result), f'native process stopped ({self.proc.poll()})')
        return json.loads(result)

    def close(self):
        self.proc.stdin.close()
        require(self.proc.wait(timeout=20) == 0, 'native process failed')

def validate_bank(config, bank):
    s = bank['native_state']
    require(bank['slot'] == s['bank_slot'] and bank['timestamp'] == s['bank_unix_timestamp'], 'bank context differs')
    require(bank['source_slots'] == {k: bank['slot'] for k in bank['accounts']}, 'source slot coverage differs')
    require(s['source_slots'] == bank['source_slots'], 'native source slots differ')
    require(s['is_initialized_with_adaptive_fee'] is True, 'nonadaptive pool')
    require(s['fee_tier_index'] != s['tick_spacing'], 'tier seed equals spacing')
    require(s['fee_tier_index_seed'] == list(s['fee_tier_index'].to_bytes(2, 'little')), 'tier seed differs')
    p = raw(bank, config['id'])
    require(len(p) == 653 and p[:8] == hashlib.sha256(b'account:Whirlpool').digest()[:8], 'Whirlpool layout differs')
    require(p[40] == s['whirlpool_bump'][0], 'Whirlpool bump differs')
    require(struct.unpack_from('<HHHH', p, 41) == (s['tick_spacing'], s['fee_tier_index'], s['fee_rate'], s['protocol_fee_rate']), 'Whirlpool small fields differ')
    require(int.from_bytes(p[49:65], 'little') == int(s['liquidity']), 'packed liquidity differs')
    require(int.from_bytes(p[65:81], 'little') == int(s['sqrt_price_x64']), 'packed price differs')
    require(struct.unpack_from('<i', p, 81)[0] == s['tick_current_index'], 'packed current tick differs')
    require([b58(p[a:a+32]) for a in (101, 181, 133, 213)] == [s['token_mint_a'], s['token_mint_b'], s['token_vault_a'], s['token_vault_b']], 'pool mint/vault identities differ')
    for index, reward in enumerate(s['reward_infos']):
        require(reward['initialized'] is False and reward['mint'] == '1'*32 and reward['vault'] == '1'*32, 'reward initialized')
        require(reward['emissions_per_second_x64'] == '0' and reward['growth_global_x64'] == '0', 'reward growth nonzero')
        require(list(p[269+index*128+64:269+index*128+96]) == reward['extension'], 'reward extension bytes lost')
        if index:
            require(reward['extension'] == [0]*32, 'reward slot1/2 must remain zero')
        else:
            require(any(reward['extension']), 'reward authority slot0 lost')
    o = raw(bank, config['fixture']['oracle'])
    require(len(o) == 254 and o[:8] == hashlib.sha256(b'account:Oracle').digest()[:8], 'Oracle layout differs')
    require(b58(o[8:40]) == config['id'], 'Oracle pool differs')
    c = s['oracle']['adaptive_fee_info']['constants']
    v = s['oracle']['adaptive_fee_info']['variables']
    require(list(o[66:82]) == c['reserved'] and list(o[110:126]) == v['reserved'] and list(o[126:254]) == s['oracle']['reserved'], 'Oracle reserved bytes lost')
    require(all(c['reserved']) and all(v['reserved']) and all(s['oracle']['reserved']), 'Oracle reserve pattern lost')
    require(len({bytes(c['reserved']), bytes(v['reserved']), bytes(s['oracle']['reserved'])}) == 3, 'reserved patterns not distinct')
    group_size = c['tick_group_size']
    require((-443636-1)//group_size <= v['tick_group_index_reference'] <= 443636//group_size, 'reference group outside supported range')
    arrays = s['tick_arrays']
    starts = [a['start_tick_index'] for a in arrays]
    require(len(arrays) == 3 and starts == config['orca_whirlpool']['tick_array_starts'], 'array manifest differs')
    require(starts[1]-starts[0] == 88*s['tick_spacing'] == starts[2]-starts[1], 'arrays not contiguous')
    for a in arrays:
        require(len(a['ticks']) == 88 and a['layout'] == 'fixed', 'incomplete fixed array')
        b = raw(bank, a['address'])
        require(len(b) == 9988 and b[:8] == hashlib.sha256(b'account:TickArray').digest()[:8], 'fixed array layout differs')
        require(struct.unpack_from('<i', b, 8)[0] == a['start_tick_index'] and b58(b[-32:]) == config['id'], 'array identity differs')
        bitmap = 0
        for i, tick in enumerate(a['ticks']):
            t = b[12+113*i:12+113*(i+1)]
            require(bool(t[0]) == tick['initialized'], 'tick initialized differs')
            require(int.from_bytes(t[1:17], 'little', signed=True) == int(tick['liquidity_net']), 'packed tick net differs')
            require(int.from_bytes(t[17:33], 'little') == int(tick['liquidity_gross']), 'packed tick gross differs')
            require(tick['tick'] == a['start_tick_index'] + i*s['tick_spacing'], 'tick index differs')
            if tick['initialized']:
                bitmap |= 1 << i
        require(bitmap == int(a['initialized_bitmap']), 'bitmap differs')
    for label in ('a', 'b'):
        b = raw(bank, s[f'token_vault_{label}'])
        require(len(b) == 165 and bank['accounts'][s[f'token_vault_{label}']]['owner'] == TOKEN, 'classic SPL vault differs')
        require(b58(b[:32]) == s[f'token_mint_{label}'] and b58(b[32:64]) == config['id'], 'vault identity differs')
        require(str(int.from_bytes(b[64:72], 'little')) == s[f'raw_vault_{label}'] and b[108] == 1, 'raw vault differs')
        require(len(raw(bank, s[f'token_mint_{label}'])) == 82, 'classic SPL mint differs')
    clock = raw(bank, CLOCK)
    require(len(clock) == 40 and struct.unpack('<QqQQq', clock)[::4] == (bank['slot'], bank['timestamp']), 'packed Clock context differs')

def validate(data, native):
    require(data['schema'] == 'orca-adaptive-service-native-v1', 'schema differs')
    require(data['identity'] == native.identity, 'source/compiler/binary/closure binding differs')
    require(data['cargo_dependencies'] == tomllib.loads((REF/'Cargo.lock').read_text())['package'], 'Cargo dependency closure differs')
    scenarios = data['scenarios']
    require([s['name'] for s in scenarios] == NAMES, 'scenario manifest changed')
    require(len({s['config']['id'] for s in scenarios}) == 2, 'expected two actual pool PDAs')
    require(data['metadata']['layouts']['oracle'] == 254, 'Oracle metadata differs')
    fulfilled = rejected = crosses = high_fee = repeated = filter_decay = 0
    for scenario in scenarios:
        config = scenario['config']
        require(config['protocol'] == 'orca-whirlpools-adaptive' and config['adapter_version'] == 'orca-whirlpools/cc683ec9-spl-adaptive-1', 'profile identity differs')
        banks = scenario['banks']
        require(len(banks) == 5, 'bank manifest changed')
        for i, bank in enumerate(banks):
            validate_bank(config, bank)
            observed = native.request({'action': 'state', 'config': config, 'bank': bank})
            require(observed == bank['native_state'], f'{scenario["name"]}/{i}: native packed state differs')
            t = bank['next_trade']
            actual = native.request({'action': 'trade', 'config': config, 'bank': bank, 'params': t['params']})
            require(actual == t['expected'], f'{scenario["name"]}/{i}: native trade differs')
            if i == 4:
                require(actual['status'] == 'native_rejected' and actual['error'] and actual['post_accounts'] == bank['accounts'], 'native exhaustion evidence lost')
                rejected += 1
                continue
            require(actual['status'] == 'fulfilled' and actual['amount_remaining'] == '0', 'unfulfilled planned trade')
            require(actual['amount_in'] == t['params']['amount'] and int(actual['amount_out']) > 0, 'native exact input differs')
            require(int(actual['fee_amount']) == int(actual['lp_fee']) + int(actual['protocol_fee']), 'fee accounting differs')
            next_state = actual['next_state']
            require(len(next_state['tick_arrays']) == 3, 'post arrays missing')
            if actual['initial_total_fee_rate'] >= 60000:
                high_fee += 1
            if i and bank['timestamp'] == banks[i-1]['timestamp']:
                repeated += 1
            before_vars = bank['native_state']['oracle']['adaptive_fee_info']['variables']
            after_vars = next_state['oracle']['adaptive_fee_info']['variables']
            if i == 2:
                require(after_vars['volatility_reference'] == before_vars['volatility_accumulator']//2, 'filter reduction branch not exercised')
                filter_decay += 1
            if i == 3:
                require(after_vars['volatility_reference'] == 0, 'decay reset branch not exercised')
            for a, b in zip(bank['native_state']['tick_arrays'], next_state['tick_arrays']):
                crosses += sum(x['fee_growth_outside_a'] != y['fee_growth_outside_a'] or x['fee_growth_outside_b'] != y['fee_growth_outside_b'] for x, y in zip(a['ticks'], b['ticks']))
            post_bank = dict(bank, accounts=actual['post_accounts'], native_state=next_state)
            validate_bank(config, post_bank)
            next_bank = banks[i+1]
            for address, account in actual['post_accounts'].items():
                if address != CLOCK:
                    require(next_bank['accounts'][address] == account, 'continuous post bank chain broken')
            for delta in actual['transfer_model']['vault_deltas']:
                amount = int(delta['amount'])
                require(int(delta['after'])-int(delta['before']) == (amount if delta['direction'] == 'credit_input' else -amount), 'nominal exact transfer differs')
            fulfilled += 1
    require((fulfilled, rejected) == (16, 4), 'trade manifest changed')
    require(crosses > 0 and high_fee > 0 and repeated == 4 and filter_decay == 4, 'required coverage missing')
    return dict(pools=2, scenarios=4, banks=20, fulfilled=fulfilled, native_rejected=rejected,
                tick_cross_updates=crosses, high_fee_trades=high_fee, repeated_timestamp_sequences=repeated,
                filter_reduction_sequences=filter_decay)

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--binary', type=Path)
    parser.add_argument('--corpus', type=Path, default=OUTPUTS[0])
    parser.add_argument('--parser-corpus', type=Path, default=OUTPUTS[1])
    parser.add_argument('--router-corpus', type=Path, default=OUTPUTS[2])
    parser.add_argument('--receipt', type=Path, default=REF/'receipt.json')
    args = parser.parse_args()
    args.receipt.write_text(json.dumps({'status': 'invalid', 'reason': 'verification/build started'}, indent=2)+'\n')
    native = None
    try:
        for path in [Path(__file__), Path(__file__).with_name('orca-adaptive-service-check.py')]:
            require(not any(isinstance(n, ast.Assert) for n in ast.walk(ast.parse(path.read_text()))), 'optimization-removable guard')
        if args.binary is None:
            compiler = subprocess.check_output(['rustup', 'which', '--toolchain', '1.90.0', 'rustc'], text=True).strip()
            cargo = subprocess.check_output(['rustup', 'which', '--toolchain', '1.90.0', 'cargo'], text=True).strip()
            env = dict(os.environ, CARGO_TARGET_DIR=str(TARGET), RUSTC=compiler)
            result = subprocess.run([cargo, 'build', '--offline', '--locked', '--manifest-path', str(REF/'Cargo.toml')], cwd=ROOT, env=env, text=True, capture_output=True)
            (REF/'build.log').write_text(result.stdout+result.stderr)
            require(result.returncode == 0, 'native build failed; see '+str(REF/'build.log')+'\n'+result.stderr[-6000:])
        binary = args.binary or BINARY
        native = Native(binary)
        if args.check:
            paths = [args.corpus, args.parser_corpus, args.router_corpus]
            require(all(p.is_file() for p in paths), 'missing service fixture/copy')
            require(len({p.read_bytes() for p in paths}) == 1, 'service fixture copies differ')
            data = json.loads(args.corpus.read_text())
        else:
            data = native.request({'action': 'generate'})
            data['cargo_dependencies'] = tomllib.loads((REF/'Cargo.lock').read_text())['package']
        counts = validate(data, native)
        # Metadata is regenerated independently so a weakened scope claim cannot pass replay.
        regenerated = native.request({'action': 'generate'})
        require(data['metadata'] == regenerated['metadata'], 'fixture metadata differs from native authority')
        if not args.check:
            text = json.dumps(data, sort_keys=True, indent=2)+'\n'
            for output in OUTPUTS:
                output.write_text(text)
        native.close()
        native = None
        fixture_bytes = args.corpus.read_bytes() if args.check else OUTPUTS[0].read_bytes()
        receipt = dict(status='verified', schema='orca-adaptive-service-receipt-v1', fixture_sha256=sha(fixture_bytes),
                       source_revision=PIN, source_inventory_sha256=data['identity']['source_inventory_sha256'],
                       compiler=data['identity']['compiler'], binary=str(binary), binary_sha256=sha(binary.read_bytes()),
                       harness_closure=data['identity']['harness_closure'], counts=counts, sbf=False,
                       proof='original packed account/native manager local execution; synthetic code identity; no deployment attestation')
        args.receipt.write_text(json.dumps(receipt, sort_keys=True, indent=2)+'\n')
        print(json.dumps(dict(counts, fixture_sha256=receipt['fixture_sha256'], binary_sha256=receipt['binary_sha256']), sort_keys=True))
    finally:
        if native is not None:
            native.close()

if __name__ == '__main__':
    main()
