#!/usr/bin/env python3
"""Fail-closed evidence for original public ramp/stop and carried exchanges."""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
ROOT = Path(__file__).resolve().parents[3]
REF = ROOT / 'integration/tools/reference/curve-crypto-ramp'
ORIGINAL = ROOT / 'integration/tools/reference/curve-crypto-full'

def require(ok, message):
    if not ok:
        raise ValueError(message)

def parse_json(text):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, 'ramp duplicate reference JSON member: ' + key)
            result[key] = value
        return result
    return json.loads(text, object_pairs_hook=unique)

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def inventory_module():
    spec = importlib.util.spec_from_file_location('ramp_inventory', ROOT / 'integration/tools/ci/curve-crypto-ramp-inventory.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

def validate_inputs(document, metadata, expected_pins=None, expected_inputs=None):
    inventory = inventory_module()
    if expected_pins is None:
        expected_pins = inventory.pins()
    if expected_inputs is None:
        expected_inputs = inventory.expected_inputs()
    require(document['source_pins'] == metadata['source_pins'] == expected_pins, 'ramp mandatory source identities omitted or substituted')
    require(metadata.get('input_inventory_kind') == 'complete-fixed-node-and-compiler-filesystem-v1', 'ramp independent input scope missing')
    require(bool(expected_inputs) and metadata.get('input_inventory') == expected_inputs, 'ramp independent dependency inventory omitted or changed')
    require('loaded_dependencies' not in metadata and 'compiler_dependencies' not in metadata, 'ramp legacy declared-only closure is not sufficient')

def manifest():
    return [f'ramp-{direction}-step{step}-{decimals}' for decimals in ['18-18', '18-6'] for direction, count in [('up', 6), ('down', 6), ('stop', 2)] for step in range(count)]

def validate(document, metadata, receipt):
    cases = document['cases']
    require([c['name'] for c in cases] == manifest(), 'ramp exact case inventory changed')
    require(receipt['valid'] is True and receipt['cases'] == 28 and receipt['reverts'] == 0, 'ramp generator incomplete')
    require(metadata['allow_unlimited_contract_size'] is False and metadata['eip170_limit'] == 24576, 'ramp deployment limit')
    require(len(metadata['deployments']) == 2 and all(d['pool_runtime_bytes'] <= 24576 for d in metadata['deployments']), 'ramp original deployment missing')
    seen = {}
    administrations = set()
    exchange_hashes = set()
    transitions = 0
    def transaction(value):
        require(isinstance(value, dict) and value.get('status') == 1, 'ramp transaction status missing')
        h = value.get('hash', '')
        require(isinstance(h, str) and len(h) == 66 and h.startswith('0x') and h == h.lower() and all(x in '0123456789abcdef' for x in h[2:]), 'ramp transaction hash invalid')
        require(type(value.get('block_number')) is int and value['block_number'] > 0 and int(value.get('gas_used', 0)) > 0, 'ramp mined transaction missing')
        return h
    members = [(f'ramp-{direction}-step{step}-{decimals}', direction + '-' + decimals, step) for decimals in ['18-18', '18-6'] for direction, count in [('up', 6), ('down', 6), ('stop', 2)] for step in range(count)]
    require([(c['name'], c['sequence'], c['step']) for c in cases] == members, 'ramp exact sequence membership changed')
    for c in cases:
        e = c['expected']
        require(c['operation'] == 'exchange' and not e.get('reverted'), 'public ramp original exchange failed')
        schedule_hash = transaction(c['schedule_transaction'])
        exchange_hash = transaction(e['transaction'])
        require(exchange_hash not in exchange_hashes, 'duplicate original exchange transaction')
        exchange_hashes.add(exchange_hash)
        require(c['schedule_transaction']['block_number'] < e['transaction']['block_number'], 'ramp schedule does not precede exchange')
        r = c['ramp']
        require(r['initial_time'] <= c['timestamp'] and 4000 <= int(r['initial_amp']) <= 4_000_000_000 and 10_000_000_000 <= int(r['initial_gamma']) <= 20_000_000_000_000_000, 'ramp origin domain')
        stopping = c['sequence'].startswith('stop-')
        require(('stop_transaction' in c) == stopping, 'ramp stop transaction inventory missing or unexpected')
        if stopping:
            transaction(c['stop_transaction'])
            require(c['schedule_transaction']['block_number'] < c['stop_transaction']['block_number'] < e['transaction']['block_number'], 'stop transaction order invalid')
            require(c['stop_transaction']['status'] == 1 and c['params']['amp'] == r['initial_amp'] and c['params']['gamma'] == r['initial_gamma'], 'stop endpoints do not equal packed current values')
            if c['step'] == 0:
                require(r['initial_time'] == int(c['state']['future_A_gamma_time']), 'stop equal times not preserved')
        previous = seen.get(c['sequence'])
        if previous:
            require(c['schedule_transaction'] == previous['schedule_transaction'] and c.get('stop_transaction') == previous.get('stop_transaction'), 'ramp administration changed within sequence')
            require(c['ramp'] == previous['ramp'] and c['params'] == previous['params'], 'ramp endpoints changed within sequence')
            transitions += 1
            require(c['step'] == previous['step'] + 1, 'ramp sequence missing step')
            require(all(c[k] == previous['expected'][k] for k in ['state', 'balances', 'raw_balances', 'total_supply', 'receiver_balance', 'trader_balances']), 'ramp sequence lost original poststate')
        else:
            require(c['step'] == 0, 'ramp sequence origin missing')
            require(schedule_hash not in administrations, 'duplicate public ramp administration')
            administrations.add(schedule_hash)
            if stopping:
                stop_hash = c['stop_transaction']['hash']
                require(stop_hash not in administrations, 'duplicate public stop administration')
                administrations.add(stop_hash)
        seen[c['sequence']] = c
        i, j = c['index_in'], c['index_out']
        dx, dy = int(c['input']), int(e['amount_out'])
        require(dy > 0 and i != j, 'ramp full positive output')
        raw = list(map(int, c['raw_balances']))
        raw[i] += dx
        raw[j] -= dy
        trader = list(map(int, c['trader_balances']))
        trader[i] -= dx
        trader[j] += dy
        require(raw == list(map(int, e['raw_balances'])) and trader == list(map(int, e['trader_balances'])), 'ramp original token ledger')
        minted = int(e['minted_admin'])
        require(int(e['total_supply']) - int(c['total_supply']) == minted and int(e['receiver_balance']) - int(c['receiver_balance']) == minted, 'ramp LP ledger')
    require(len(seen) == 6, 'ramp sequence count')
    for decimals in ['18-18', '18-6']:
        for direction in ['up', 'down']:
            rows = [c for c in cases if c['sequence'] == direction + '-' + decimals]
            require(int(rows[3]['state']['future_A_gamma_time']) > rows[3]['timestamp'], 'pre-end boundary missing')
            require(int(rows[4]['state']['future_A_gamma_time']) == rows[4]['timestamp'], 'exact end boundary missing')
            require(rows[4]['expected']['state']['future_A_gamma_time'] == '0' and rows[5]['state']['future_A_gamma_time'] == '0', 'end sentinel did not settle')
    require(len(administrations) == 8 and not administrations.intersection(exchange_hashes), 'public ramp/stop transaction inventory differs')
    return {'cases': len(cases), 'sequences': len(seen), 'carried_transitions': transitions, 'public_admin_calls': len(administrations)}

def main():
    target = REF / 'verification.json'
    def write(value):
        tmp = target.with_name(target.name + '.tmp-' + str(os.getpid()))
        tmp.write_text(json.dumps(value, indent=2) + '\n')
        tmp.replace(target)
    write({'valid': False, 'state': 'RUNNING'})
    source = ROOT / 'integration/test/contracts/curve-crypto-ramp-native.json'
    receipt = parse_json((REF / 'generation.json').read_text())
    metadata = parse_json((REF / 'runtime-metadata.json').read_text())
    require(receipt['generator_sha256'] == sha(ROOT / 'integration/tools/ci/curve-crypto-ramp-golden.cjs'), 'ramp generator changed')
    require(receipt['corpus_sha256'] == sha(source) and receipt['runtime_metadata_sha256'] == sha(REF / 'runtime-metadata.json'), 'ramp source artifacts changed')
    document = parse_json(source.read_text())
    validate_inputs(document, metadata)
    result = validate(document, metadata, receipt)
    write({'valid': True, 'reference_only': True, 'corpus_sha256': sha(source), 'verifier_sha256': sha(Path(__file__)), **result})
    print(json.dumps(result))

if __name__ == '__main__':
    main()
