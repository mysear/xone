#!/usr/bin/env python3
"""Independent protoc snapshot from a pinned native Orca adaptive dynamic bank."""
from pathlib import Path
import hashlib, importlib.util, json, subprocess, sys
ROOT=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from orca_adaptive_dynamic_fixture import corpus,pool
spec=importlib.util.spec_from_file_location('adaptive_wire',Path(__file__).with_name('orca-adaptive-wire-golden.py'))
wire=importlib.util.module_from_spec(spec);spec.loader.exec_module(wire)
def write_fixture(name, state, bank, source):
    reference = {
        'block': {
            'chain_key': state['chain_key'],
            'number_or_slot': bank['slot'],
            'hash': '11111111111111111111111111111112',
            'parent_hash': '11111111111111111111111111111113',
            'timestamp_ms': bank['timestamp'] * 1000,
        },
        'canonical_epoch': 1,
        'journal_seq': 1,
        'schema_version': 1,
        'online_generation': 1,
        'writer_epoch': 1,
        'owner_service': 'chain-parser-rust',
        'owner_assignment_version': 1,
    }
    for key, field in [('scope_hash', 'pool_id'), ('adapter_set_hash', 'adapter_version')]:
        scope = json.dumps([state[field]], separators=(',', ':')).encode()
        reference[key] = hashlib.sha256(scope).hexdigest()
    snapshot = {
        'reference': reference,
        'pools': [state],
        'source': source,
        'history_gaps': [],
    }
    proto = ROOT / 'common/protoc/proto'
    raw = subprocess.run(
        ['protoc', '-I', str(proto), '--encode=dex.v1.OnlineSnapshot', str(proto / 'dex/v1/state.proto')],
        input=wire.encode(snapshot).encode(), capture_output=True, check=True,
    ).stdout
    fixture = {'snapshot': snapshot, 'protobuf_hex': raw.hex(), 'protobuf_sha256': hashlib.sha256(raw).hexdigest()}
    text = json.dumps(fixture, indent=2) + '\n'
    for project in ['integration', 'chain-parser-go', 'chain-parser-rust', 'dex-router']:
        (ROOT / project / 'test/contracts' / (name + '.json')).write_text(text)
    print(name, len(raw), 'bytes')

def generate():
    scenario = corpus()['scenarios'][0]
    bank = scenario['banks'][0]
    state = pool(scenario['config'], bank['native_state'])
    write_fixture('orca-adaptive-dynamic-wire', state, bank, 'SYNTHETIC_ORCA_ADAPTIVE_DYNAMIC_COMPLETE_WIRE')

    s = state['orca_whirlpool']
    state['protocol'] = 'orca-whirlpools-adaptive-token2022'
    state['adapter_version'] = 'orca-whirlpools/cc683ec9-token2022-adaptive-1'
    s['token_2022'] = {
        'programdata': 'DoU57AYuPFu2QU514RktNPG22QhApEjnKxnBcu4BHDTY',
        'deployed_slot': 300,
        'program_code_sha256': hashlib.sha256(b'\x7fELFsynthetic Token-2022 proof').hexdigest(),
    }
    for i, (older_bps, newer_bps, cap) in enumerate([(100, 250, 20), (300, 50, 100)]):
        mint = bytearray(278)
        mint[44:46] = bytes([s[f'mint{i}_decimals'], 1])
        mint[165:170] = bytes([1, 1, 0, 108, 0])
        rollover = s['bank_epoch'] + 1
        for offset, epoch, bps in [(242, 0, older_bps), (260, rollover, newer_bps)]:
            mint[offset:offset + 8] = epoch.to_bytes(8, 'little')
            mint[offset + 8:offset + 16] = cap.to_bytes(8, 'little')
            mint[offset + 16:offset + 18] = bps.to_bytes(2, 'little')
        vault = bytearray(182)
        vault[:32] = wire.pubkey(state[f'token{i}'])
        vault[32:64] = wire.pubkey(state['pool_id'])
        vault[64:72] = s[f'raw_vault{i}'].to_bytes(8, 'little')
        vault[108] = 1
        vault[165:] = bytes([2, 2, 0, 8, 0]) + (17 + i).to_bytes(8, 'little') + bytes([7, 0, 0, 0])
        s['token_2022'][f'token{i}'] = {'mint_data': mint.hex(), 'vault_data': vault.hex()}
    write_fixture('orca-adaptive-token2022-wire', state, bank, 'SYNTHETIC_ORCA_ADAPTIVE_TOKEN2022_WIRE')

if __name__ == '__main__':
    generate()
