#!/usr/bin/env python3
"""Verify full original bank projections, actual transfers, controls and fresh SBF replay."""
from pathlib import Path
import json,sys
from solana_expanded_reference import module,require,sha,tokens,delta_token,ROOT
OUT=ROOT/'integration/artifacts/local/core-pump-amm-sell-target-original-20261112.json'
FIXTURE=ROOT/'integration/test/contracts/pump-amm-sell-target.json'
def verify(reexecute=False,evidence=None,fixture=None):
    gen=module('amm_sell_target_reference','integration/tools/ci/pump-amm-sell-target-reference.py')
    e=json.loads(OUT.read_bytes())if evidence is None else evidence;f=json.loads(FIXTURE.read_bytes())if fixture is None else fixture
    require(e['generator_sha256']==sha(gen.__file__)and e['native_sha256']==sha(gen.ref.native.__file__)and f['source_sha256']==sha(OUT),'amm-target-source')
    require(len(e['records'])==len(f['cases'])==48,'amm-target-coverage')
    expected=[(b,h,str(t))for b,h in [(False,False),(True,False),(True,True)]for t in [0,1,2,10,100,999,1000,1001,9899,9900,9901,10000,1000000,1000000000,3000000000,None]]
    for (b,h,target),r,c in zip(expected,e['records'],f['cases']):require((r['boost'],r['high_price'],r['target'])==(b,h,str(int(c['pool']['reserve1'])+1)if target=='None'else target),'amm-target-exact-matrix')
    full=surplus=0
    for i,(r,c)in enumerate(zip(e['records'],f['cases'])):
        require(r['index']==c['index']==i and r['target']==c['target']and r['boost']==c['boost']and r['high_price']==c['high_price'],'amm-target-identity')
        n=r['native'];require(c['passed']==(n is not None),'amm-target-result')
        if n is None:require(int(c['target'])==0 or int(c['target'])>int(c['pool']['reserve1']),'amm-target-rejected-bound');continue
        require(n['passed']and'err'not in n['native'],'amm-original-result')
        require(gen.ref.pool(n['before'],n['ids'])==c['pool']and gen.ref.pool(n['after'],n['ids'])==c['after'],'amm-target-full-projection')
        require(str(-delta_token(n,n['ids']['base_mint'],n['ids']['user']))==c['required_input']and str(delta_token(n,n['ids']['quote_mint'],n['ids']['user']))==c['actual_output']and int(c['actual_output'])>=int(c['target']),'amm-target-actual-funds');tokens(n)
        control=r['input_minus_one'];require(not control['passed']and'err'in control['native']and control['before']==control['after']==n['before'],'amm-target-minimum-control')
        if reexecute:
            fresh=gen.execute(r['boost'],int(c['required_input']),int(c['target']),r['high_price']);require(fresh['passed']and fresh['before']==n['before']and fresh['after']==n['after'],'amm-target-fresh-original')
            fresh=gen.execute(r['boost'],int(c['required_input'])-1,int(c['target']),r['high_price']);require(not fresh['passed']and fresh['before']==fresh['after']==n['before'],'amm-target-fresh-budget-control')
        full+=1;surplus+=c['actual_output']!=c['target']
    require((full,surplus)==(42,14),'amm-target-counts')
    return dict(passed=True,cases=48,successful_targets=full,actual_surplus_cases=surplus,original_budget_controls=full,reexecuted=reexecute,all_requested_features_complete=False,production_ready=False)
if __name__=='__main__':print(json.dumps(verify('--reexecute'in sys.argv)))
