#!/usr/bin/env python3
"""Original BinPool output/tree/burn evidence; app ledger is explicit projection."""
from copy import deepcopy
import hashlib
import json
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]

def require(value,reason):
    if not value:raise ValueError(reason)

def after(row):
    p=row['pool'];s=p['infinity_bin'];n=row['native'];zero=row['zeroForOne'];target=int(row['target'])
    require(p['protocol']=='pancake-infinity-bin'and p['adapter_version']=='pancake-infinity-bin/3c5ef1be-static-1','MODEL')
    require(s['hooks']=='0x'+'0'*40,'HOOKS')
    require(p['pool_id']==s['manager']+'/'+s['pool_id'],'MANAGER_IDENTITY')
    pre,post=n['pre'],n['post']
    require(s['active_id']==pre['active_id']and s['bins']==pre['bins']and s.get('detached_active')==pre['detached_active'],'NATIVE_PRESTATE')
    for state in [pre,post]:
        root=0;one={};two={}
        for b in state['bins']:
            k=b['id'];root|=1<<(k>>16);one[k>>16]=one.get(k>>16,0)|(1<<((k>>8)&255));two[k>>8]=two.get(k>>8,0)|(1<<(k&255))
        require(int(state['tree']['level0'],16)==root,'TREE_ROOT')
        for key,v in state['tree']['level1'].items():require(int(v,16)==one.get(int(key),0),'TREE_LEVEL1')
        for key,v in state['tree']['level2'].items():require(int(v,16)==two.get(int(key),0),'TREE_LEVEL2')
    if not n['passed']:
        require(row['passed']is False and pre==post and n['revert_data'].startswith('0x')and n['revert_data']!='0x','NATIVE_ROLLBACK')
        require('after'not in row and 'requiredInput'not in row,'REVERTED_SUCCESS')
        return None
    require(row['passed']is True,'RESULT')
    d0,d1=int(n['delta0']),int(n['delta1']);paid,out=(-d0,d1)if zero else(-d1,d0)
    require(out==target and paid==int(row['requiredInput'])and paid>0,'NATIVE_TARGET_AND_INPUT')
    protocol=int(n['protocol_amount']);require(protocol>=0,'PROTOCOL_AMOUNT')
    sums=lambda state:[sum(int(b['reserve'+str(i)])for b in state['raw_bins'])for i in [0,1]]
    before_bal,after_bal=sums(pre),sums(post);i,j=(0,1)if zero else(1,0)
    require(after_bal[i]-before_bal[i]==paid-protocol and before_bal[j]-after_bal[j]==target,'BIN_CASH_CONSERVATION')
    before_ids={b['id']:b for b in pre['raw_bins']};after_ids={b['id']:b for b in post['raw_bins']}
    require(set(before_ids)==set(after_ids),'NATIVE_BIN_MEMBERS')
    for k in before_ids:require(before_ids[k]['total_shares']==after_ids[k]['total_shares'],'SWAP_SHARES')
    expected=deepcopy(p);next_state=expected['infinity_bin'];next_state['active_id']=post['active_id'];next_state['bins']=post['bins'];next_state['detached_active']=post['detached_active']
    next_state['app_reserve'+str(i)]=str(int(s['app_reserve'+str(i)])+paid)
    next_state['app_reserve'+str(j)]=str(int(s['app_reserve'+str(j)])-target)
    next_state['protocol_fees'+str(i)]=str((int(s['protocol_fees'+str(i)])+protocol)%2**256)
    require(expected==row['after'],'FULL_PROJECTION')
    return expected

def validate(corpus):
    require(corpus['reference']['revision']=='3c5ef1be1f71442d8bc60cc6fd11f9c677b714df','REVISION')
    require(len(corpus['cases'])==264,'CASE_COUNT')
    require(sum(after(c)is not None for c in corpus['cases'])==219,'SUCCESS_COUNT')
    require(sum('burn'in c for c in corpus['cases'])==48,'BURN_COVERAGE')
    require(len(corpus['sequences'])==18,'SEQUENCE_COUNT')
    for seq in corpus['sequences']:
        require(len(seq['steps'])==4,'SEQUENCE_LENGTH');current=None
        for row in seq['steps']:
            if current is not None:require(row['pool']==current,'SEQUENCE_STATE')
            current=after(row);require(current is not None,'SEQUENCE_REVERT')
    require(len(corpus['ordered'])==2,'ORDERED_COUNT')
    for seq in corpus['ordered']:
        current=seq['pool'];paid=target=0;require(len(seq['steps'])==3,'ORDERED_LENGTH')
        for row in seq['steps']:
            require(row['pool']==current,'ORDERED_STATE');current=after(row);require(current is not None,'ORDERED_REVERT')
            paid+=int(row['requiredInput']);target+=int(row['target'])
        require(paid==int(seq['requiredInput'])and target==int(seq['target']),'ORDERED_CONSERVATION')
    require(len(corpus['grids'])==2,'GRID_COUNT')
    for grid in corpus['grids']:
        require(len(grid['rows'])==5,'GRID_SHAPE');pools={p['pool_id']:p for p in grid['pools']};require(len(pools)==2,'GRID_POOL_ID');inputs=[]
        for row in grid['rows']:
            paid=target=0
            for proposal in row['proposals']:
                require(proposal['pool']==pools[proposal['pool']['pool_id']],'GRID_POOL')
                require(after(proposal)is not None,'GRID_REVERT');paid+=int(proposal['requiredInput']);target+=int(proposal['target'])
            require(paid==int(row['requiredInput'])and target==int(grid['target']),'GRID_CONSERVATION');inputs.append(paid)
        require(min(inputs)==int(grid['bestRequiredInput']),'GRID_MINIMUM')

def verify():
    raw=(ROOT/'integration/test/contracts/infinity-bin-exact-out.json').read_bytes();r=json.loads((ROOT/'integration/artifacts/local/infinity-bin-exact-out-generation.json').read_bytes());require(r['passed']is True,'GENERATION_FAILED')
    require(hashlib.sha256(raw).hexdigest()==r['corpus_sha256'],'CORPUS_HASH');require(raw==(ROOT/'dex-router/test/contracts/infinity-bin-exact-out.json').read_bytes(),'CORPUS_COPY');f=json.loads(raw);require(f['reference']['inputs']==r['inputs'],'INPUT_MANIFEST')
    for name,h in r['inputs'].items():require(hashlib.sha256((ROOT/name).read_bytes()).hexdigest()==h,'CHANGED_INPUT: '+name)
    for item in r['loaded_dependencies']:require(hashlib.sha256((ROOT/item['path']).read_bytes()).hexdigest()==item['sha256'],'CHANGED_DEPENDENCY: '+item['path'])
    validate(f);return dict(passed=True,cases=264,success=219,rejected=45,burn_cases=48,sequences=18,sequence_steps=72,ordered=2,ordered_steps=6,grids=2)
if __name__=='__main__':print(json.dumps(verify()))
