#!/usr/bin/env python3
"""Validate fixed original exact-output observations; no quote formula is used."""
import argparse
import hashlib
import json
import itertools
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
DEFAULT = ROOT / 'integration/artifacts/local/core-v2-exact-out-original-20261109.json'


def require(condition, reason):
    if not condition:
        raise ValueError(reason)


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def pairs(values):
    result = {}
    for key, value in values:
        require(key not in result, 'EXACT_OUT_DUPLICATE_JSON')
        result[key] = value
    return result


def local(value):
    path = (ROOT / value).resolve()
    require(path.is_relative_to(ROOT) and path.is_file(), 'EXACT_OUT_INPUT_LOCALITY')
    return path


def validate_bank(bank, code_ledger):
    """Join every parsed canonical field to original raw getter/code bytes."""
    calls, slots, codes = {}, {}, {}
    def put(table, key, value):
        require(key not in table or table[key] == value, 'EXACT_OUT_CONTRADICTORY_RAW_READ')
        table[key] = value
    for read in bank['reads']:
        method, params = read['method'], read['params']
        if method == 'eth_call':
            put(calls, (params[0]['to'].lower(), params[0]['data'].lower()), read['result'])
        elif method == 'eth_getStorageAt':
            put(slots, (params[0].lower(), int(params[1], 16)), read['result'])
        elif method == 'eth_getCode':
            key = read['result_code_sha256']
            require(key in code_ledger, 'EXACT_OUT_RAW_CODE_MISSING')
            put(codes, params[0].lower(), key)
    word = lambda n: f'{int(n):064x}'
    for pool in bank['canonical_pools']:
        identity, state, factory = pool['pool_id'], pool['v2']['swap'], pool['v2']['factory']
        expected = {
            (identity, '0x0dfe1681'): '0x'+word(int(pool['token0'],16)),
            (identity, '0xd21220a7'): '0x'+word(int(pool['token1'],16)),
            (identity, '0xc45a0155'): '0x'+word(int(factory,16)),
            (identity, '0x0902f1ac'): '0x'+word(pool['reserve0'])+word(pool['reserve1'])+word(state['block_timestamp_last']),
            (identity, '0x5909c0d5'): '0x'+word(state['price0_cumulative_last']),
            (identity, '0x5a3d5493'): '0x'+word(state['price1_cumulative_last']),
            (pool['token0'], '0x70a08231'+word(int(identity,16))): '0x'+word(state['raw_balance0']),
            (pool['token1'], '0x70a08231'+word(int(identity,16))): '0x'+word(state['raw_balance1']),
        }
        for a,b in [(pool['token0'],pool['token1']),(pool['token1'],pool['token0'])]:
            expected[(factory, '0xe6a43905'+word(int(a,16))+word(int(b,16)))] = '0x'+word(int(identity,16))
        require(all(calls.get(key)==value for key,value in expected.items()), 'EXACT_OUT_RAW_GETTER_PROJECTION')
        require(codes.get(identity)==pool['v2']['code_sha256']
                and codes.get(factory)==state['factory_code_sha256'], 'EXACT_OUT_RAW_CODE_PROJECTION')
        require(slots.get((identity,12))=='0x'+word(1) and state['unlocked'] is True, 'EXACT_OUT_RAW_LOCK_PROJECTION')
    canonical = {p['pool_id']:p for p in bank['canonical_pools']}
    require(len(canonical)==len(bank['pools']), 'EXACT_OUT_PROJECTED_BANK_COUNT')
    for pool in bank['pools']:
        observed = json.loads(json.dumps(pool))
        identity = pool['pool_id']
        require(identity in canonical, 'EXACT_OUT_PROJECTED_BANK_ID')
        for key in ['bank_block_number','bank_block_hash','bank_unix_timestamp']:
            observed['v2']['swap'][key] = canonical[identity]['v2']['swap'][key]
        require(observed==canonical[identity], 'EXACT_OUT_PROJECTED_BANK_FIELDS')


def validate(path=DEFAULT):
    document = json.loads(Path(path).read_bytes(), object_pairs_hook=pairs)
    require(document.get('passed') is True and document['schema_version'] == 1,
            'EXACT_OUT_ORIGINAL_REPORT')
    require(document['generator_sha256'] == digest(local('integration/tools/ci/v2-exact-out-reference.cjs')),
            'EXACT_OUT_GENERATOR_CHANGED')
    source = document['source_pins']
    pin_path = local(source['path'])
    require(digest(pin_path) == source['sha256'], 'EXACT_OUT_SOURCE_PINS_CHANGED')
    pin_set = json.loads(pin_path.read_bytes(), object_pairs_hook=pairs)
    seen = set()
    for package in pin_set['packages']:
        for entry in package['files']:
            require(entry['path'] not in seen and not Path(entry['path']).is_absolute()
                    and '..' not in Path(entry['path']).parts, 'EXACT_OUT_ORIGINAL_SOURCE_PATH')
            seen.add(entry['path'])
            require(digest(local(str(pin_path.parent.relative_to(ROOT) / entry['path'])))
                    == entry['sha256'], 'EXACT_OUT_ORIGINAL_SOURCE_CHANGED')
    require(len(seen) == 93, 'EXACT_OUT_ORIGINAL_SOURCE_COUNT')
    for dependency in document['runtime']['loaded_dependencies']:
        item = local(dependency['path'])
        require(item.stat().st_size == dependency['bytes'] and digest(item) == dependency['sha256'],
                'EXACT_OUT_RUNTIME_DEPENDENCY_CHANGED')
    require(len(document['cases']) == 90 and len(document['invalid']) == 2,
            'EXACT_OUT_ORIGINAL_CASE_COUNT')
    require(document.get('code_ledger_encoding') == 'sha256-hex-result/v1', 'EXACT_OUT_CODE_LEDGER_ENCODING')
    for code_hash, code in document['code_ledger'].items():
        require(code.startswith('0x') and hashlib.sha256(bytes.fromhex(code[2:])).hexdigest() == code_hash,
                'EXACT_OUT_CODE_LEDGER_DIGEST')
    changes, cumulative_changes, divisions = 0, 0, 0
    for index, case in enumerate(document['cases']):
        require(case['id'] == index, 'EXACT_OUT_CASE_ID')
        expected = case['expected']
        require(expected['max_input_minus_one_rejected'] is True
                and int(expected['recipient_delta']) == int(case['desired_output']) > 0
                and int(expected['required_input']) > 0
                and int(case['max_input']) - int(expected['required_input'])
                == int(expected['unused_input_budget']) == 17, 'EXACT_OUT_BUDGET_AND_RECIPIENT')
        amounts = list(map(int, expected['amounts']))
        route = case['path']
        require(len(amounts) == len(route) and amounts[0] == int(expected['required_input'])
                and amounts[-1] == int(case['desired_output']), 'EXACT_OUT_HOP_AMOUNT_SCOPE')
        for bank in [case['source'], expected['after']]:
            validate_bank(bank, document['code_ledger'])
            header = bank['header']
            require(len(bank['reads']) >= 30 and len(bank['canonical_pools']) == 6,
                    'EXACT_OUT_RAW_SOURCE_READS')
            for pool in bank['canonical_pools']:
                state = pool['v2']['swap']
                require(state['bank_block_number'] == header['number']
                        and state['bank_block_hash'] == header['hash']
                        and state['bank_unix_timestamp'] == header['timestamp'],
                        'EXACT_OUT_CANONICAL_SOURCE_ANCHOR')
            for read in bank['reads']:
                method, params = read['method'], read['params']
                if method == 'eth_getCode':
                    require(read.get('result_code_sha256') in document['code_ledger'] and 'result' not in read,
                            'EXACT_OUT_CODE_RESPONSE_REFERENCE')
                if method in ['eth_call', 'eth_getCode', 'eth_getStorageAt']:
                    index = 2 if method == 'eth_getStorageAt' else 1
                    require(int(params[index], 16) == header['number'], 'EXACT_OUT_UNPINNED_GETTER')
        require(expected['after']['header']['timestamp'] == case['source']['header']['timestamp'],
                'EXACT_OUT_EXECUTION_CLOCK')
        if route[0] == document['identities']['token3']['address'] and case['desired_output'] == '1000':
            require(int(expected['required_input']) == 1001, 'EXACT_OUT_ORIGINAL_DIVISIBLE_ROUND_UP')
        before = {p['pool_id']: p for p in case['source']['pools']}
        after = {p['pool_id']: p for p in expected['after']['pools']}
        require(len(before) == len(case['source']['pools']) == len(after) == 6
                and set(before) == set(after), 'EXACT_OUT_PHYSICAL_SCOPE')
        touched = set()
        for hop, (token_in, token_out) in enumerate(zip(route, route[1:])):
            candidates = [p for p in before.values() if {p['token0'], p['token1']} == {token_in, token_out}]
            require(len(candidates) == 1, 'EXACT_OUT_HOP_POOL_IDENTITY')
            p = candidates[0]
            require(p['pool_id'] not in touched, 'EXACT_OUT_REPEATED_POOL_NOT_DECLARED')
            touched.add(p['pool_id'])
            q = after[p['pool_id']]
            source_state, after_state = p['v2']['swap'], q['v2']['swap']
            zero = p['token0'] == token_in
            input_key, output_key = ('raw_balance0', 'raw_balance1') if zero else ('raw_balance1', 'raw_balance0')
            require(int(after_state[input_key]) == int(source_state[input_key]) + amounts[hop]
                    and int(after_state[output_key]) == int(source_state[output_key]) - amounts[hop+1],
                    'EXACT_OUT_PER_HOP_TOKEN_CONSERVATION')
            require(int(q['reserve0']) == int(after_state['raw_balance0'])
                    and int(q['reserve1']) == int(after_state['raw_balance1']), 'EXACT_OUT_COMPLETE_RESERVE_WRITE')
            require(q['v2']['factory'] == p['v2']['factory'] and q['v2']['code_sha256'] == p['v2']['code_sha256']
                    and q['token0'] == p['token0'] and q['token1'] == p['token1'], 'EXACT_OUT_READONLY_IDENTITIES')
            for key in ['bank_block_number', 'bank_block_hash', 'bank_unix_timestamp', 'factory_code_sha256', 'unlocked']:
                require(source_state[key] == after_state[key], 'EXACT_OUT_CARRIED_SOURCE_CONTEXT')
            changes += 1
            cumulative_changes += source_state['price0_cumulative_last'] != after_state['price0_cumulative_last']
            divisions += case['path'][0] == document['identities']['token3']['address'] and case['desired_output'] == '1000'
        for identity in set(before) - touched:
            require(before[identity] == after[identity], 'EXACT_OUT_UNVISITED_POOL_CHANGED')
    require(all(item['rejected'] is True for item in document['invalid']), 'EXACT_OUT_INVALID_TARGET_ACCEPTED')
    require(cumulative_changes > 0 and divisions > 0, 'EXACT_OUT_CLOCK_AND_ROUNDING_SCOPE')
    return dict(schema_version=1, passed=True, authority='original-runtime observation integrity and token-flow validation, no inverse quote formula',
                reference_sha256=digest(Path(path)), original_transactions=90, hop_transitions=changes,
                cumulative_updates=cumulative_changes, invalid_targets=2, source_files=len(seen),
                all_requested_features_complete=False, production_ready=False)


def validate_grid(path=None):
    path = Path(path or ROOT / 'integration/artifacts/local/core-v2-exact-out-grid-original-20261109.json')
    d = json.loads(path.read_bytes(), object_pairs_hook=pairs)
    require(d.get('passed') is True and d['schema_version'] == 1, 'EXACT_OUT_GRID_REPORT')
    require(d['generator_sha256'] == digest(local('integration/tools/ci/v2-exact-out-reference.cjs')),
            'EXACT_OUT_GRID_GENERATOR_CHANGED')
    source = d['source_pins']
    require(digest(local(source['path'])) == source['sha256'], 'EXACT_OUT_GRID_SOURCE_PINS')
    require(d['code_ledger_encoding'] == 'sha256-hex-result/v1', 'EXACT_OUT_GRID_CODE_ENCODING')
    for key, code in d['code_ledger'].items():
        require(hashlib.sha256(bytes.fromhex(code[2:])).hexdigest() == key, 'EXACT_OUT_GRID_CODE_DIGEST')
    for entry in d['loaded_dependencies']:
        file = local(entry['path'])
        require(file.stat().st_size == entry['bytes'] and digest(file) == entry['sha256'],
                'EXACT_OUT_GRID_DEPENDENCY_CHANGED')
    validate_bank(d['bank'], d['code_ledger'])
    target, parts, splits = int(d['target_output']), d['grid_parts'], d['max_splits']
    require(parts == 4 and splits == 3 and len(d['paths']) == 3 and len(d['bank']['pools']) == 6,
            'EXACT_OUT_GRID_BOUNDARY')
    expected = set()
    for count in range(1, splits+1):
        for order in itertools.permutations(range(len(d['paths'])), count):
            for cuts in itertools.combinations(range(1, parts), count-1):
                bounds = (0,) + cuts + (parts,)
                expected.add(tuple((order[i], str(target*bounds[i+1]//parts-target*bounds[i]//parts))
                                   for i in range(count)))
    actual = set()
    successes = []
    for index, plan in enumerate(d['plans']):
        validate_bank(plan['after'], d['code_ledger'])
        require(plan['id'] == index, 'EXACT_OUT_GRID_ID')
        key = tuple((s['path_index'], s['amount_out']) for s in plan['proposal'])
        require(key not in actual, 'EXACT_OUT_GRID_DUPLICATE')
        actual.add(key)
        require(sum(int(s['amount_out']) for s in plan['proposal']) == target, 'EXACT_OUT_GRID_TARGET')
        if not plan['feasible']:
            continue
        require(len(plan['observations']) == len(plan['proposal'])
                and int(plan['recipient_delta']) == target
                and int(plan['required_input']) <= int(d['max_input']), 'EXACT_OUT_GRID_FULL_RECIPIENT')
        require(sum(int(o['amounts'][0]) for o in plan['observations'])
                == int(plan['required_input']) == int(plan['observed_prefix_input']), 'EXACT_OUT_GRID_INPUT')
        before = {p['pool_id']:p for p in d['bank']['pools']}
        deltas = {key:[0,0] for key in before}
        for step, observation in zip(plan['proposal'], plan['observations']):
            route = d['paths'][step['path_index']]
            amounts = list(map(int, observation['amounts']))
            require(observation['path'] == route and observation['path_index'] == step['path_index']
                    and observation['amount_out'] == step['amount_out']
                    and amounts[-1] == int(step['amount_out']) and len(amounts) == len(route),
                    'EXACT_OUT_GRID_ROUTE_OBSERVATION')
            for hop, (token_in, token_out) in enumerate(zip(route,route[1:])):
                candidates = [p for p in before.values() if {p['token0'],p['token1']} == {token_in,token_out}]
                require(len(candidates)==1, 'EXACT_OUT_GRID_PHYSICAL_ROUTE')
                pool=candidates[0]; side=0 if pool['token0']==token_in else 1
                deltas[pool['pool_id']][side]+=amounts[hop]
                deltas[pool['pool_id']][1-side]-=amounts[hop+1]
        after={p['pool_id']:p for p in plan['after']['pools']}
        require(set(after)==set(before), 'EXACT_OUT_GRID_COMPLETE_BANK')
        for identity,p in before.items():
            q=after[identity]
            for side in [0,1]:
                field='raw_balance'+str(side)
                require(int(q['v2']['swap'][field])==int(p['v2']['swap'][field])+deltas[identity][side],
                        'EXACT_OUT_GRID_PHYSICAL_CONSERVATION')
            if deltas[identity]==[0,0]:
                require(q==p,'EXACT_OUT_GRID_UNVISITED_POOL')
        successes.append(plan)
    require(actual==expected and len(actual)==39 and successes, 'EXACT_OUT_GRID_EXHAUSTIVENESS')
    best=min(successes,key=lambda p:(int(p['required_input']),len(p['proposal'])))
    require(d['best_required_input']==best['required_input']
            and d['plans'][d['best_plan_id']]['required_input']==best['required_input'], 'EXACT_OUT_GRID_BEST')
    return dict(schema_version=1,passed=True,authority='all finite ordered output allocations checked against original-runtime transaction observations, no quote formula',
                reference_sha256=digest(path),grid_plans=39,feasible=len(successes),
                best_required_input=best['required_input'],all_requested_features_complete=False,production_ready=False)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path, default=DEFAULT)
    parser.add_argument('--grid', action='store_true')
    arguments = parser.parse_args()
    print(json.dumps(validate_grid() if arguments.grid else validate(arguments.input.resolve()), indent=2))
