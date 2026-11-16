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
MODEL = 'balancer-v2-composable-regular'

def require(condition, reason):
    if not condition:
        raise ValueError(reason)

def state_after(case):
    p = deepcopy(case['pool'])
    e = case['expected']
    if e.get('reverted') is True:
        require((re.fullmatch(r'BAL#\d{3}', e.get('reason', '')) is not None or e.get('reason') in ['raw cash','final cash','BPT']), 'UNEXPECTED_ORIGINAL_REVERT')
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
    require(corpus['reference']['revision'] == 'e91a2b643a49856f51a648d175667c1b48cf3377', 'REVISION')
    cases = corpus['cases']
    require(len(cases) == 288, 'CASE_COUNT')
    successful = 0
    for case in cases:
        p = case['pool']
        require(p['protocol'] == MODEL and p['adapter_version'] == 'balancer-v2-composable-regular/no-rates-1', 'MODEL_IDENTITY')
        require(p['chain_key'] == 'evm:31337', 'CHAIN')
        s=p['multi_asset'];bpt=case['bpt']
        require(3<=len(s['assets'])<=6 and 0<=bpt<len(s['assets']),'ASSET_COUNT')
        require(s['assets'][bpt]['token']==p['pool_id'] and s['assets'][bpt]['code_sha256']==s['code_sha256'],'BPT_IDENTITY')
        require(case['i']!=bpt and case['j']!=bpt,'BPT_NOT_UNDERLYING')
        require(int(s['assets'][bpt]['scaling'])==10**18,'BPT_SCALE')
        require(all(int(a['scaling']) in [10**n for n in range(18,37)] for a in s['assets']), 'SCALE_DOMAIN')
        require(not s['weights'] and 1000<=int(s['amplification'])<=5000000,'STABLE_PARAMETERS')
        successful += state_after(case) is not None
    require(successful == 216, 'SUCCESS_REJECT_COUNTS')
    require(len(corpus['sequences']) == 72, 'SEQUENCE_COUNT')
    for seq in corpus['sequences']:
        require(len(seq['steps']) == 3, 'SEQUENCE_LENGTH')
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
    path = ROOT / 'integration/test/contracts/composable-exact-out.json'
    report = json.loads((ROOT / 'integration/artifacts/local/composable-exact-out-generation.json').read_bytes())
    raw = path.read_bytes()
    require(report['passed'] is True, 'REFERENCE_GENERATION_FAILED')
    require(hashlib.sha256(raw).hexdigest() == report['corpus_sha256'], 'CORPUS_HASH')
    require(raw == (ROOT / 'dex-router/test/contracts/composable-exact-out.json').read_bytes(), 'CORPUS_COPY')
    corpus = json.loads(raw)
    require(corpus['reference']['inputs'] == report['inputs'], 'INPUT_MANIFEST')
    for name, expected in report['inputs'].items():
        require(hashlib.sha256((ROOT / name).read_bytes()).hexdigest() == expected, 'REFERENCE_INPUT_CHANGED: ' + name)
    for item in report['loaded_dependencies']:
        require(hashlib.sha256((ROOT / item['path']).read_bytes()).hexdigest() == item['sha256'], 'DEPENDENCY_CHANGED: ' + item['path'])
    validate_corpus(corpus)
    return {'passed': True, 'cases': 288, 'success': 216, 'rejected': 72, 'sequence_steps': 216, 'grids': 3, 'ordered': 3}

if __name__ == '__main__':
    print(json.dumps(verify()))
