#!/usr/bin/env python3
"""Verify retained original math-wrapper corpus integrity and conservation.

Rust differential tests establish kernel parity. These checks protect source
identity, full-state chaining and recorded grids; they do not prove ERC20 execution.
"""
from copy import deepcopy
import hashlib
import json
from pathlib import Path
import re

ROOT = Path(__file__).resolve().parents[3]
MODEL = 'balancer-v2-managed-snapshot'

def require(condition, reason):
    if not condition:
        raise ValueError(reason)

def state_after(case):
    p = deepcopy(case['pool'])
    e = case['expected']
    if e.get('reverted') is True:
        require(re.fullmatch(r'BAL#\d{3}', e.get('reason', '')) is not None, 'UNEXPECTED_ORIGINAL_REVERT')
        return None
    require('reverted' not in e, 'AMBIGUOUS_ORIGINAL_RESULT')
    i, j = case['i'], case['j']
    require(i!=j and 0<=i<len(p['multi_asset']['assets']) and 0<=j<len(p['multi_asset']['assets']), 'PAIR_INDEX')
    b = [int(a['balance']) for a in p['multi_asset']['assets']]
    target, required = int(case['target']), int(e['amount_in'])
    require(target > 0 and required > 0, 'NONPOSITIVE_TARGET_OR_INPUT')
    b[i] += required
    b[j] -= target
    require(all(0 <= n < 2**112 for n in b), 'CASH_RANGE')
    require(e['next_balances'] == list(map(str, b)), 'TARGET_CASH_CONSERVATION')
    p['reserve0'], p['reserve1'] = map(str, b[:2])
    for asset,value in zip(p['multi_asset']['assets'],e['next_balances']):
        asset['balance']=value
    return p

def validate_corpus(corpus):
    require(corpus['reference']['revision'] == 'e281ff40583c39bf1d46c322334b2b13936e0e33', 'REVISION')
    cases = corpus['cases']
    require(len(cases) == 180, 'CASE_COUNT')
    successful = 0
    for case in cases:
        p = case['pool']
        require(p['protocol'] == MODEL and p['adapter_version'] == 'balancer-v2-managed-snapshot/no-bpt-fees-1', 'MODEL_IDENTITY')
        require(p['chain_key'] == 'evm:31337', 'CHAIN')
        s=p['multi_asset']
        require(len(s['assets']) in [3,8,38], 'ASSET_COUNT')
        require(all(int(a['scaling']) in [10**n for n in range(18,37)] for a in s['assets']), 'SCALE_DOMAIN')
        snapshots=[r for r in corpus['snapshots'] if r['count']==len(s['assets']) and r['timestamp']==case['timestamp']]
        require(len(snapshots)==1,'SNAPSHOT_IDENTITY')
        source=snapshots[0]
        require(source['weights']==s['weights'] and int(source['fee'])==p['fee_numerator'],'OBSERVED_WEIGHT_AND_FEE')
        require(sum(map(int,s['weights']))==int(source['normalized_weight_sum']) and int(source['normalized_weight_sum'])!=10**18,'COMPRESSED_WEIGHT_SUM')
        require(case['block']==source['block'] and case['block']['timestamp_ms']==case['timestamp']*1000,'BLOCK_CONTEXT')
        successful += state_after(case) is not None
    require(successful == 108, 'SUCCESS_REJECT_COUNTS')
    require(len(corpus['guards'])==3,'GUARD_COUNT')
    for guard in corpus['guards']:
        require(guard['expected'].get('reverted') is True and re.fullmatch(r'BAL#\d{3}',guard['expected']['reason']) is not None,'DISABLED_SWAP_GUARD')
    require(len(corpus['snapshots'])==9,'SNAPSHOT_COUNT')
    require(len(corpus['sequences']) == 9, 'SEQUENCE_COUNT')
    for seq in corpus['sequences']:
        require(len(seq['steps']) == 4, 'SEQUENCE_LENGTH')
        previous = None
        for step in seq['steps']:
            if previous is not None:
                require(previous == step['pool'], 'SEQUENCE_FULL_STATE')
            previous = state_after(step)
            require(previous is not None, 'SEQUENCE_REVERTED')
    require(len(corpus['grids']) == 3, 'GRID_COUNT')
    for grid in corpus['grids']:
        require(len(grid['rows']) == 5 and len(grid['pools']) == 2, 'GRID_SHAPE')
        initial = {p['pool_id']: p for p in grid['pools']}
        require(len(initial) == 2, 'GRID_PHYSICAL_POOL_ID')
        for row in grid['rows']:
            total, required = 0, 0
            for proposal in row['proposals']:
                require(proposal['pool'] == initial[proposal['pool']['pool_id']], 'GRID_POOL_STATE')
                require(state_after(proposal) is not None, 'GRID_REVERTED')
                total += int(proposal['target'])
                required += int(proposal['expected']['amount_in'])
            require(total == int(grid['target']), 'GRID_TARGET_CONSERVATION')
            require(required == int(row['requiredInput']), 'GRID_INPUT_CONSERVATION')
        require(int(grid['bestRequiredInput']) == min(int(row['requiredInput']) for row in grid['rows']), 'GRID_MINIMUM')
    require(len(corpus['ordered']) == 3, 'ORDERED_COUNT')
    for ordered in corpus['ordered']:
        require(len(ordered['steps']) == 3, 'ORDERED_LENGTH')
        previous = ordered['pool']
        total, required = 0, 0
        for step in ordered['steps']:
            require(step['pool'] == previous, 'ORDERED_PHYSICAL_STATE')
            previous = state_after(step)
            require(previous is not None, 'ORDERED_REVERTED')
            total += int(step['target'])
            required += int(step['expected']['amount_in'])
        require(total == int(ordered['target']) and required == int(ordered['requiredInput']), 'ORDERED_CONSERVATION')

def verify():
    path = ROOT / 'integration/test/contracts/managed-snapshot-exact-out.json'
    report = json.loads((ROOT / 'integration/artifacts/local/managed-snapshot-exact-out-generation.json').read_bytes())
    raw = path.read_bytes()
    require(report['passed'] is True, 'REFERENCE_GENERATION_FAILED')
    require(hashlib.sha256(raw).hexdigest() == report['corpus_sha256'], 'CORPUS_HASH')
    require(raw == (ROOT / 'dex-router/test/contracts/managed-snapshot-exact-out.json').read_bytes(), 'CORPUS_COPY')
    corpus = json.loads(raw)
    require(corpus['reference']['inputs'] == report['inputs'], 'INPUT_MANIFEST')
    for name, expected in report['inputs'].items():
        require(hashlib.sha256((ROOT / name).read_bytes()).hexdigest() == expected, 'REFERENCE_INPUT_CHANGED: ' + name)
    for item in report['loaded_dependencies']:
        require(hashlib.sha256((ROOT / item['path']).read_bytes()).hexdigest() == item['sha256'], 'DEPENDENCY_CHANGED: ' + item['path'])
    validate_corpus(corpus)
    return {'passed': True, 'cases': 180, 'success': 108, 'rejected': 72, 'sequence_steps': 36, 'grids': 3, 'ordered': 3}

if __name__ == '__main__':
    print(json.dumps(verify()))
