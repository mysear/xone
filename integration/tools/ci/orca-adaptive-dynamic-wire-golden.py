#!/usr/bin/env python3
"""Independent protoc snapshot from a pinned native Orca adaptive dynamic bank."""
from pathlib import Path
import hashlib, importlib.util, json, subprocess, sys
ROOT=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from orca_adaptive_dynamic_fixture import corpus,pool
spec=importlib.util.spec_from_file_location('adaptive_wire',Path(__file__).with_name('orca-adaptive-wire-golden.py'))
wire=importlib.util.module_from_spec(spec);spec.loader.exec_module(wire)
def generate():
    scenario = corpus()['scenarios'][0]
    bank = scenario['banks'][0]
    state = pool(scenario['config'], bank['native_state'])
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
        'source': 'SYNTHETIC_ORCA_ADAPTIVE_DYNAMIC_COMPLETE_WIRE',
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
        (ROOT / project / 'test/contracts/orca-adaptive-dynamic-wire.json').write_text(text)
    print('Independent adaptive dynamic wire bytes', len(raw))

if __name__ == '__main__':
    generate()
