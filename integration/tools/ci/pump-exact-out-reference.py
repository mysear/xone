#!/usr/bin/env python3
"""Exact token buy hard SOL controls and completion transition from fixed Pump SBF."""
from pathlib import Path
import hashlib,importlib.util,json,sys
ROOT=Path(__file__).resolve().parents[3];sys.path.insert(0,str(ROOT/'integration/tools/reference/pump-bonding/harness'))
import native
spec=importlib.util.spec_from_file_location('pump_exact_projection',ROOT/'integration/tools/ci/pump-bonding-reference.py');projection=importlib.util.module_from_spec(spec);spec.loader.exec_module(projection)
TARGET=ROOT/'integration/artifacts/local/core-pump-exact-out-original-20261111.json'
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def main():
    records=[];cases=[]
    for complete in [False,True]:
        for target in [1,2,1000,1000000,1000000000,620100000000000-1,620100000000000,620100000000001,2**64-1]:
            n=native.execute(kind='buy',amount=target,limit=10**15,complete=complete);control=None
            if n['passed']:
                control=native.execute(kind='buy',amount=target,limit=-n['user_sol_delta']-1,complete=complete)
                if control['passed']or control['before']!=n['before']or control['before']!=control['after']:raise ValueError('HARD_SOL_BOUND_CONTROL')
                if n['user_token_delta']!=target:raise ValueError('EXACT_TOKEN_CREDIT')
            elif n['before']!=n['after']:raise ValueError('FAILURE_BANK_MUTATION')
            index=len(records);records.append(dict(index=index,target=str(target),complete=complete,native=n,input_minus_one=control));cases.append(dict(index=index,target=str(target),complete=complete,passed=n['passed'],required_input=str(-n['user_sol_delta']),pool=projection.pool(n['before'],n['ids']),after=projection.pool(n['after'],n['ids'])))
    TARGET.write_text(json.dumps(dict(schema_version=1,passed=True,generator_sha256=sha(__file__),native_sha256=sha(native.__file__),records=records,authority='fixed original buy instruction in constructed classic/SOL bank, hard max SOL controls; completion is not migration'),indent=2)+'\n')
    data=dict(schema_version=1,source_sha256=sha(TARGET),cases=cases)
    for service in ['integration','dex-router']:(ROOT/service/'test/contracts/pump-exact-out.json').write_text(json.dumps(data,separators=(',',':'))+'\n')
    print(json.dumps(dict(passed=True,cases=len(cases),successful=sum(c['passed']for c in cases),hard_input_controls=sum(r['input_minus_one']is not None for r in records))))
if __name__=='__main__':main()
