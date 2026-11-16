#!/usr/bin/env python3
"""Finite original Pool/CLPool output evidence, no Manager settlement claim."""
from copy import deepcopy
import hashlib
import json
from pathlib import Path

ROOT=Path(__file__).resolve().parents[3]

def require(value,reason):
    if not value:
        raise ValueError(reason)

def after(row):
    p=row['pool'];n=row['native'];target=int(row['target'])
    require(p['protocol'] in ['uniswap-v4','pancake-infinity-cl'],'MODEL')
    require(p['chain_key']=='evm:31337','CHAIN')
    s=p['singleton_cl']
    require(s['hooks']=='0x'+'0'*40,'HOOKS')
    require(p['pool_id']==s['manager']+'/'+s['pool_id'],'MANAGER_IDENTITY')
    if n.get('reverted'):
        require(row['passed'] is False and isinstance(n.get('data'),str) and n['data'].startswith('0x'),'ORIGINAL_REVERT')
        return None
    output,remaining,paid=map(int,[n['output'],n['remaining'],n['input']])
    require(output+remaining==target and remaining>=0 and paid>=0,'OUTPUT_CONSERVATION')
    full=remaining==0 and target<2**127 and paid<=2**127
    require(row['passed'] is full,'PARTIAL_OR_SIGNED_RANGE')
    if not full:
        require('after' not in row and 'requiredInput' not in row,'UNPROVED_COMPLETE_OUTPUT')
        return None
    require(int(row['requiredInput'])==paid and paid>0,'NATIVE_INPUT')
    expected=deepcopy(p)
    expected['singleton_cl']['core'].update(sqrt_price_x96=n['sqrt'],tick=n['tick'],liquidity=n['liquidity'])
    require(expected==row['after'],'FULL_PRICING_PROJECTION')
    return expected

def validate(corpus):
    cases=corpus['cases']
    require(len(cases)==644,'CASE_COUNT')
    require(sum(after(r)is not None for r in cases)==424,'SUCCESS_COUNT')
    require(len(corpus['sequences'])==4,'SEQUENCE_COUNT')
    for seq in corpus['sequences']:
        require(len(seq['steps'])==3,'SEQUENCE_LENGTH')
        current=None
        for row in seq['steps']:
            if current is not None:require(row['pool']==current,'SEQUENCE_STATE')
            current=after(row);require(current is not None,'SEQUENCE_PARTIAL')
    require(len(corpus['ordered'])==4,'ORDERED_COUNT')
    for seq in corpus['ordered']:
        current=seq['pool'];target=paid=0
        require(len(seq['steps'])==3,'ORDERED_LENGTH')
        for row in seq['steps']:
            require(row['pool']==current,'ORDERED_PHYSICAL_STATE')
            require(row['zeroForOne']==seq['zeroForOne'],'ORDERED_DIRECTION')
            current=after(row);require(current is not None,'ORDERED_PARTIAL')
            target+=int(row['target']);paid+=int(row['requiredInput'])
        require(target==int(seq['target'])and paid==int(seq['requiredInput']),'ORDERED_CONSERVATION')
    require(len(corpus['grids'])==4,'GRID_COUNT')
    for grid in corpus['grids']:
        require(len(grid['rows'])==5,'GRID_SHAPE')
        pools={p['pool_id']:p for p in grid['pools']}
        require(len(pools)==2,'GRID_PHYSICAL_POOL')
        feasible=[]
        for row in grid['rows']:
            paid=target=0;full=True
            for proposal in row['proposals']:
                require(proposal['pool']==pools[proposal['pool']['pool_id']],'GRID_POOL')
                require(proposal['zeroForOne']==grid['zeroForOne'],'GRID_DIRECTION')
                target+=int(proposal['target'])
                if after(proposal)is None:full=False
                else:paid+=int(proposal['requiredInput'])
            require(target==int(grid['target']),'GRID_TARGET')
            require(row['passed'] is full,'GRID_CAPACITY')
            if full:
                require(paid==int(row['requiredInput']),'GRID_INPUT');feasible.append(paid)
            else:require(row['requiredInput'] is None,'GRID_PARTIAL_INPUT')
        require(len(feasible)==3,'GRID_FEASIBLE_COUNT')
        require(min(feasible)==int(grid['bestRequiredInput']),'GRID_MINIMUM')

def verify():
    raw=(ROOT/'integration/test/contracts/singleton-exact-out.json').read_bytes()
    report=json.loads((ROOT/'integration/artifacts/local/singleton-exact-out-generation.json').read_bytes())
    require(report['passed']is True,'GENERATION_FAILED')
    require(hashlib.sha256(raw).hexdigest()==report['corpus_sha256'],'CORPUS_HASH')
    require(raw==(ROOT/'dex-router/test/contracts/singleton-exact-out.json').read_bytes(),'CORPUS_COPY')
    corpus=json.loads(raw);require(corpus['reference']['inputs']==report['inputs'],'INPUT_MANIFEST')
    for name,h in report['inputs'].items():require(hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==h,'CHANGED_INPUT: '+name)
    for item in report['loaded_dependencies']:require(hashlib.sha256((ROOT/item['path']).read_bytes()).hexdigest()==item['sha256'],'CHANGED_DEPENDENCY: '+item['path'])
    validate(corpus)
    return dict(passed=True,cases=644,success=424,rejected=220,sequences=4,sequence_steps=12,ordered=4,ordered_steps=12,grids=4)

if __name__=='__main__':
    print(json.dumps(verify()))
