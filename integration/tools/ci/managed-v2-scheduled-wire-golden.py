#!/usr/bin/env python3
"""C++ protoc scheduled Snapshot syntax/domain contract, without native proof.

Default generates four copies; --check compares bytes without touching mtime.
--self-test uses the real acceptance discovery entry. No application/VM executes.
Static Pool identities are borrowed from the legacy contract as syntax templates;
every schedule, third asset, header and envelope here is explicitly synthetic.
"""
import sys
sys.dont_write_bytecode = True
import argparse
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[3]
_spec = importlib.util.spec_from_file_location('legacy_managed_wire', Path(__file__).with_name('managed-v2-aum-wire-golden.py'))
legacy = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(legacy)
require = legacy.require
ONE = 10**18
PROTOCOL = 'balancer-v2-managed-scheduled'
ADAPTER = 'balancer-v2/20230411-managed-scheduled-1'
SOURCE = 'SYNTHETIC_MANAGED_SCHEDULED_WIRE'
AUTHORITY = 'synthetic-scheduled-state-wire-contract-no-native-execution-proof'
FILENAME = 'managed-v2-scheduled-wire.json'
PROJECTS = ('integration', 'chain-parser-go', 'chain-parser-rust', 'dex-router')
STARTS = [499600399600399601, 300499500499500499, 199900099900099901]
ENDS = [200000000000000000, 300000000000000000, 500000000000000000]
WEIGHTS = [499301098501897704, 300499001498002497, 200199900000099801]


def build_snapshot():
    # Never use the original bank or execute the old native reference. The local
    # legacy JSON supplies static identity syntax only, including code hashes.
    s = copy.deepcopy(json.loads((ROOT / 'integration/test/contracts/managed-v2-aum-wire.json').read_bytes())['snapshot'])
    s['source'] = SOURCE
    p = s['pools'][0]
    p['protocol'], p['adapter_version'] = PROTOCOL, ADAPTER
    multi = p['multi_asset']
    m = multi['managed_v2']
    block = s['reference']['block']
    block.update(number_or_slot=100, hash='0x' + 'a'*64, parent_hash='0x' + 'b'*64, timestamp_ms=1767225662000)
    s['reference']['adapter_set_hash'] = legacy.array_hash([ADAPTER])
    s['reference']['scope_hash'] = legacy.array_hash([p['pool_id']])
    m.update(timestamp=1767225662, block_number=100, block_hash=block['hash'],
             management_aum_fee='0', pending_aum='0', actual_supply=m['virtual_supply'],
             gradual_weight_end=1767226662, gradual_fee_end=0, swaps_enabled=True,
             allowlist_enabled=False, recovery_mode=False)
    third = copy.deepcopy(multi['assets'][0])
    third.update(token='0x' + 'a'*40, balance='1000', scaling=str(ONE))
    multi['assets'].append(third)
    info = copy.deepcopy(m['token_info'][0])
    info.update(cash='1000', managed='0', vault_actual_balance='1000',
                asset_manager='0x' + '0'*40, last_change_block=0, circuit_breaker_enabled=False)
    m['token_info'].append(info)
    multi['weights'] = list(map(str, WEIGHTS))
    word = (1767225661 | (1767226662 << 32) | (10**16 << 128) |
            (10**16 << 190) | (1 << 252) | (1 << 255))
    m['schedule'] = {'pool_state_word': str(word), 'tokens': [
        {'token': a['token'], 'state_word': str(start | (end << 64))}
        for a, start, end in zip(multi['assets'], STARTS, ENDS)]}
    validate_snapshot(s)
    return s


def validate_snapshot(s):
    """Strict contract qualification; public encode is a general text adapter."""
    require(type(s) is dict and type(s.get('pools')) is list and len(s['pools']) == 1, 'one synthetic Pool required')
    p = s['pools'][0]
    require(p.get('protocol') == PROTOCOL and p.get('adapter_version') == ADAPTER, 'scheduled profile required')
    m = p['multi_asset']['managed_v2']
    legacy.exact_keys(m, (*legacy.MANAGED_FIELDS, 'schedule'), 'managed_v2 39 required + schedule40')
    schedule = m['schedule']
    legacy.exact_keys(schedule, ('pool_state_word', 'tokens'), 'schedule')
    legacy.amount(schedule['pool_state_word'], 'pool_state_word')
    tokens = schedule['tokens']
    assets = p['multi_asset']['assets']
    require(type(tokens) is list and 2 <= len(tokens) <= 50 and len(tokens) == len(assets), '2..50 aligned tokens required')
    for token in tokens:
        legacy.exact_keys(token, ('token', 'state_word'), 'scheduled token')
        legacy.address(token['token'], 'scheduled token address')
        legacy.amount(token['state_word'], 'state_word')
    addresses = [t['token'] for t in tokens]
    require(addresses == sorted(set(addresses)) and addresses == [a['token'] for a in assets], 'sorted unique asset order required')
    require(p['pool_id'] not in addresses and p['multi_asset']['vault'] not in addresses, 'own pool/vault forbidden')
    # Reuse only legacy structural validators for the exact original 39 fields.
    # Changes are on a private copy; the scheduled profile and hashes above and
    # below are checked against their own values, never silently downgraded.
    base = copy.deepcopy(s)
    base['pools'][0]['multi_asset']['managed_v2'].pop('schedule')
    base['pools'][0].update(protocol=legacy.projection.PROTOCOL, adapter_version=legacy.projection.ADAPTER)
    base['reference']['adapter_set_hash'] = legacy.array_hash([legacy.projection.ADAPTER])
    base['source'] = legacy.SOURCE
    base['reference']['owner_service'] = 'chain-parser-go'
    legacy.validate_snapshot(base)
    require(s['source'] == SOURCE and s['history_gaps'] == [], 'synthetic golden source required')
    require(s['reference']['owner_service'] in ('test', 'chain-parser-go'), 'golden owner required')
    require(s['reference']['adapter_set_hash'] == legacy.array_hash([ADAPTER]), 'scheduled adapter hash mismatch')
    word = int(schedule['pool_state_word'])
    field = lambda offset, width: (word >> offset) & ((1 << width)-1)
    ws, we, fs, fe = (field(i, 32) for i in (0, 32, 64, 96))
    require(ws <= we and fs <= fe, 'schedule time order')
    fees = [field(128, 62), field(190, 62)]
    require(all(10**12 <= f <= 95*10**16 for f in fees), 'fee range')
    starts, ends, scaling = [], [], []
    for token in tokens:
        tw = int(token['state_word'])
        require(tw >> 133 == 0, 'token padding bits')
        start, end, diff = tw & ((1 << 64)-1), (tw >> 64) & ((1 << 64)-1), (tw >> 128) & 31
        require(10**16 <= start <= ONE and 10**16 <= end <= ONE and diff <= 18, 'weight/decimals range')
        starts.append(start); ends.append(end); scaling.append(str(ONE * 10**diff))
    require(sum(ends) == ONE, 'caller target sum must be ONE')
    def current(a, b, start, end):
        time = m['timestamp']
        progress = ONE if time >= end else 0 if time <= start else (time-start)*ONE//(end-start)
        delta = abs(b-a)*progress//ONE
        return a-delta if a>b else a+delta
    require(p['multi_asset']['weights'] == [str(current(a,b,ws,we)) for a,b in zip(starts, ends)], 'exact source getter weights required; no normalization')
    require([a['scaling'] for a in assets] == scaling, 'raw decimals/scaling mismatch')
    require(p['fee_denominator'] == ONE and p['fee_numerator'] == current(*fees,fs,fe), 'raw fee mismatch')
    require(m['gradual_weight_end'] == we and m['gradual_fee_end'] == fe, 'raw end time mismatch')
    require([m['swaps_enabled'],m['allowlist_enabled'],m['recovery_mode']] == [bool(field(i,1)) for i in (252,253,254)], 'raw flags mismatch')


def encode(snapshot, level=''):
    """Public service adapter. Preserve None omission and Known(0/false)."""
    amounts = legacy.AMOUNTS | {'pool_state_word', 'state_word'}
    def blob(raw):
        return '"' + ''.join('\\x%02x' % b for b in raw) + '"'
    out = []
    for name, value in snapshot.items():
        for item in value if isinstance(value, list) else [value]:
            if item is None:
                continue
            if isinstance(item, dict):
                out.append(name + ' { ' + encode(item, name) + ' }')
            elif name in amounts:
                legacy.amount(item, name)
                out.append(name + ' { uint256_be: ' + blob(int(item).to_bytes(32, 'big')) + ' }')
            elif name in ('hash', 'parent_hash', 'block_hash', 'adapter_set_hash', 'scope_hash') or name.endswith('_sha256') or (name == 'pool_id' and level == 'multi_asset'):
                out.append(name + ': ' + blob(legacy.digest_bytes(item, name)))
            else:
                out.append(name + ': ' + json.dumps(item, ensure_ascii=True, allow_nan=False))
    return ' '.join(out)


def encode_protobuf(snapshot):
    validate_snapshot(snapshot)
    return legacy._protoc('encode', encode(snapshot).encode())


def build_fixture():
    s = build_snapshot()
    raw = encode_protobuf(s)
    f = {'snapshot': s, 'protobuf_hex': raw.hex(), 'protobuf_sha256': hashlib.sha256(raw).hexdigest(),
         'authority': AUTHORITY,
         'raw_layout': {'pool': 'weightStart:u32@0,weightEnd:u32@32,feeStart:u32@64,feeEnd:u32@96,startFee:u62@128,endFee:u62@190,flags@252..255',
                        'token': 'startWeight:u64@0,endWeight:u64@64,decimalsDiff:u5@128,zeroPadding@133..255',
                        'provenance': 'All new fields and metadata are synthetic syntax/domain contracts. Static identity/code hashes are templates, not evidence of deployment, third-token deployment, or native execution.'}}
    validate_fixture(f)
    return f


def validate_fixture(f):
    legacy.exact_keys(f, ('snapshot','protobuf_hex','protobuf_sha256','authority','raw_layout'), 'scheduled wire fixture')
    require(f['authority'] == AUTHORITY, 'synthetic authority required')
    validate_snapshot(f['snapshot'])
    require(f['snapshot'] == build_snapshot(), 'synthetic canonical snapshot drift')
    text = f['protobuf_hex']
    require(type(text) is str and len(text)%2 == 0 and all(c in '0123456789abcdef' for c in text), 'canonical protobuf hex')
    raw = bytes.fromhex(text)
    require(hashlib.sha256(raw).hexdigest() == f['protobuf_sha256'], 'wire sha mismatch')
    require(raw == encode_protobuf(f['snapshot']), 'C++ wire bytes mismatch')
    require(legacy._protoc('encode', legacy._protoc('decode', raw)) == raw, 'C++ roundtrip mismatch')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--check', action='store_true')
    parser.add_argument('--self-test', action='store_true')
    args = parser.parse_args()
    if args.self_test:
        suite = unittest.defaultTestLoader.discover(str(ROOT / 'integration/tools/acceptance/tests'), pattern='test_managed_v2_scheduled_wire.py')
        return 0 if unittest.TextTestRunner(verbosity=2).run(suite).wasSuccessful() else 1
    fixture = build_fixture()
    data = (json.dumps(fixture, indent=2, ensure_ascii=True) + '\n').encode()
    legacy.publish(data, [ROOT / p / 'test/contracts' / FILENAME for p in PROJECTS], check=args.check)
    print(json.dumps({'checked' if args.check else 'generated': 4, 'protobuf_bytes':len(bytes.fromhex(fixture['protobuf_hex'])), 'protobuf_sha256':fixture['protobuf_sha256'], 'authority':AUTHORITY}))
    return 0


if __name__ == '__main__':
    try:
        sys.exit(main())
    except (ValueError, OSError) as error:
        print(str(error), file=sys.stderr)
        sys.exit(1)
