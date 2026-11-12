#!/usr/bin/env python3
"""Original SBF OnlyY fee policy and complete exact-in/output banks."""
from pathlib import Path
from contextlib import contextmanager
import copy,hashlib,importlib.util,json,sys
ROOT=Path(__file__).resolve().parents[3];BASE=ROOT/'integration/tools/reference/meteora-dlmm';sys.path.insert(0,str(BASE/'harness'))
from idl_layout import decode,encode
spec=importlib.util.spec_from_file_location('meteora_fixed_original',ROOT/'integration/tools/ci/meteora-exact-out-reference.py');original=importlib.util.module_from_spec(spec);spec.loader.exec_module(original)
PROFILE='meteora-dlmm-mm-only-y';ADAPTER='meteora-dlmm/1532e540-mm-only-y-1'
TARGET=ROOT/'integration/artifacts/local/core-meteora-only-y-original-20261111.json'
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
@contextmanager
def controlled(mode):
    with original.controlled(mode)as overrides:
        p=original.native.BASE/'commons/tests/fixtures'/original.FOLDER/'lb_pair.bin';raw=p.read_bytes();value,_=decode({'defined':{'name':'LbPair'}},raw,8);value['parameters']['collect_fee_mode']=1;data=raw[:8]+encode({'defined':{'name':'LbPair'}},value);p.write_bytes(data)
        yield overrides+[dict(path='lb_pair.bin',before_only_y_sha256=hashlib.sha256(raw).hexdigest(),controlled_sha256=hashlib.sha256(data).hexdigest())]
def execute(mode,zero,target,exact_out,budget=None,followups=()):
    with controlled(mode)as overrides:
        n=original.native.execute(zero=zero,amount=target,minimum=(original.BUDGET if budget is None else budget)if exact_out else 1,exact_out=exact_out,mm_only=True,protocol_share=mode.get('share',2000),dynamic_control=mode.get('control',7500),elapsed=mode.get('elapsed',300),followups=followups)
        return dict(mode=mode,only_y_overrides=overrides,native=n)
def pool(bank,clock,code):
    p=original.projection.pool(bank,clock,code);p['protocol']=PROFILE;p['adapter_version']=ADAPTER;return p
def selectors():
    modes=[dict(name=name,elapsed=elapsed,share=share,control=control)for name,elapsed,share,control in [('static',0,0,0),('same-clock',0,2000,7500),('filter',299,1000,7500),('reduction',300,2000,7500),('decay',1200,2500,7500)]]
    rows=[(m,z,t,out)for m in modes for z in [True,False]for t in [1,2,3,100,28776,28777,28778,1000000,100000000,100000000000]for out in [False,True]]
    rows +=[(dict(name=f'price-{id}',active=id,control=0,elapsed=0),z,t,out)for id,z in [(40,True),(-40,False)]for t in [1,2,3,4,5,10,100000000]for out in [False,True]]
    rows +=[(dict(name='oracle-'+kind,oracle=kind,age=120),True,1000000,out)for kind in ['full','wrap','grow','single']for out in [False,True]]
    return rows
def main():
    records=[];cases=[]
    for index,(mode,zero,amount,out)in enumerate(selectors()):
        followups=(10000,20000)if index<200 and amount==1000000 else()
        entry=execute(mode,zero,amount,out,followups=followups);entry['index']=index;n=entry['native'];entry['input_minus_one']=None
        if out and n['passed']:
            bad=execute(mode,zero,amount,True,budget=int(n['payer_input_delta'])-1)['native']
            if bad['passed']or bad['before']!=bad['after']:raise ValueError('ONLY_Y_HARD_INPUT_LIMIT')
            entry['input_minus_one']=bad
        steps=[]
        for step in n['sequence']if n['passed']else[n]:steps.append(dict(pool=pool(step['before'],n['clock'],n['program_sha256']),specified_amount=step['specified_amount'],zero_for_one=zero,exact_out=out,passed=step['passed'],actual_input=step['payer_input_delta'],actual_output=step['recipient_output_delta'],after=pool(step['after'],n['clock'],n['program_sha256'])))
        records.append(entry);cases.append(dict(index=index,name=mode['name'],steps=steps))
    evidence=dict(schema_version=1,generator_sha256=sha(__file__),native_helper_sha256=sha(BASE/'harness/native_preflight.py'),input_preparation_sha256=sha(original.__file__),records=records,authority='original pinned Meteora SBF with explicit OnlyY fee byte and controlled MM-only banks; not canonical public bank qualification',production_ready=False)
    TARGET.write_text(json.dumps(evidence,indent=2)+'\n');result=dict(schema_version=1,source_sha256=sha(TARGET),generator_sha256=sha(__file__),cases=cases)
    for rel in ['integration/test/contracts/meteora-only-y.json','dex-router/test/contracts/meteora-only-y.json','chain-parser-rust/test/contracts/meteora-only-y.json']:(ROOT/rel).write_text(json.dumps(result,separators=(',',':'))+'\n')
    print(json.dumps(dict(passed=True,cases=len(cases),full=sum(e['native']['passed']for e in records),rejected=sum(not e['native']['passed']for e in records))))
if __name__=='__main__':main()
