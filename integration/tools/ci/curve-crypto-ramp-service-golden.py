#!/usr/bin/env python3
"""Project public original ramp/stop/exchange observations into synthetic RPC banks."""
from pathlib import Path
import copy, hashlib, importlib.util, json, subprocess
ROOT = Path(__file__).resolve().parents[3]
spec = importlib.util.spec_from_file_location('crypto_service', Path(__file__).with_name('curve-crypto-service-golden.py'))
base = importlib.util.module_from_spec(spec)
spec.loader.exec_module(base)
PROTOCOL = 'curve-crypto2-ramp'
ADAPTER = PROTOCOL + '/d7d04cd9-ramp-1'

def project(case, template):
    pool = base.make_pool(case, template)
    pool.update(protocol=PROTOCOL, adapter_version=ADAPTER)
    pool['multi_asset']['curve_crypto']['ramp'] = copy.deepcopy(case['ramp'])
    return pool

def generate():
    source = ROOT / 'integration/test/contracts/curve-crypto-ramp-native.json'
    corpus = json.loads(source.read_text())
    template = json.loads((ROOT / 'integration/test/contracts/curve-crypto-runtime-template.json').read_text())
    cases = []
    for c in corpus['cases']:
        p = project(c, template)
        s = p['multi_asset']
        config = {'program_id': '', 'accounts': [], 'id': p['pool_id'], 'protocol': PROTOCOL, 'token0': p['token0'], 'token1': p['token1'], 'fee_numerator': p['fee_numerator'], 'fee_denominator': p['fee_denominator'], 'multi_asset': {'expected_code_sha256': s['code_sha256'], 'expected_lp_token': s['curve_crypto']['lp_token'], 'expected_lp_token_code_sha256': base.LP, 'token_policy': 'standard-no-bpt', 'tokens': [{'token': a['token'], 'expected_code_sha256': a['code_sha256']} for a in s['assets']]}}
        cases.append({'name': c['name'], 'sequence': c['sequence'], 'step': c['step'], 'pool': p, 'config': config, 'input': c['input'], 'index_in': c['index_in'], 'index_out': c['index_out'], 'expected': copy.deepcopy(c['expected'])})
    document = {'source': 'LOCAL_RPC_REPLAY_OF_ORIGINAL_PUBLIC_RAMP_STOP_AND_EXCHANGE', 'source_corpus_sha256': hashlib.sha256(source.read_bytes()).hexdigest(), 'profile': ADAPTER, 'scope': 'standard nominal transfers; public ramp and stop; synthetic RPC; INDICATIVE only', 'cases': cases}
    data = json.dumps(document, indent=2) + '\n'
    for service in ['integration', 'chain-parser-go', 'chain-parser-rust', 'dex-router']:
        (ROOT / service / 'test/contracts/curve-crypto-ramp-service.json').write_text(data)
    spec = importlib.util.spec_from_file_location('crypto_wire', Path(__file__).with_name('curve-crypto-wire-golden.py'))
    wire = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(wire)
    wire.base.AMOUNTS |= {'initial_amp', 'initial_gamma'}
    snapshot = json.loads((ROOT / 'integration/test/contracts/curve-crypto-wire.json').read_text())['snapshot']
    pool = copy.deepcopy(cases[0]['pool'])
    snapshot['pools'] = [pool]
    snapshot['source'] = 'SYNTHETIC_CRYPTO_RAMP_WIRE_CONTRACT'
    c = pool['multi_asset']['curve_crypto']
    snapshot['reference']['block'].update(number_or_slot=c['block_number'], hash=c['block_hash'], timestamp_ms=c['timestamp'] * 1000)
    for key, values in [('adapter_set_hash', [ADAPTER]), ('scope_hash', [pool['pool_id']])]:
        snapshot['reference'][key] = hashlib.sha256(json.dumps(values, separators=(',', ':')).encode()).hexdigest()
    raw = subprocess.run(['protoc', '-I', str(ROOT / 'common/protoc/proto'), '--encode=dex.v1.OnlineSnapshot', str(ROOT / 'common/protoc/proto/dex/v1/state.proto')], input=wire.base.encode(snapshot).encode(), capture_output=True, check=True).stdout
    data = json.dumps({'snapshot': snapshot, 'protobuf_hex': raw.hex(), 'protobuf_sha256': hashlib.sha256(raw).hexdigest()}, indent=2) + '\n'
    for service in ['integration', 'chain-parser-go', 'chain-parser-rust', 'dex-router']:
        (ROOT / service / 'test/contracts/curve-crypto-ramp-wire.json').write_text(data)
    print(json.dumps({'cases': len(cases), 'wire_bytes': len(raw)}))

if __name__ == '__main__':
    generate()
