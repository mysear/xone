#!/usr/bin/env python3
"""Pinned original SwapExactOut2, actual funds, exact credits and rounding dust."""
from pathlib import Path
from contextlib import contextmanager
import base64,copy,hashlib,importlib.util,json,shutil,struct,sys,tempfile
ROOT=Path(__file__).resolve().parents[3];BASE=ROOT/'integration/tools/reference/meteora-dlmm';sys.path.insert(0,str(BASE/'harness'))
import native_preflight as native
from idl_layout import IDL,decode,encode
spec=importlib.util.spec_from_file_location('mm_projection',ROOT/'integration/tools/ci/meteora-mm-state.py');projection=importlib.util.module_from_spec(spec);spec.loader.exec_module(projection)
FOLDER='EtAdVRLFH22rjWh3mcUasKFF27WtHhsaCvK27tPFFWig';TIME=1734392448;BUDGET=10**15
TARGET=ROOT/'integration/artifacts/local/core-meteora-exact-out-original-20261110.json'
def sha(data):return hashlib.sha256(data).hexdigest()
def price(id,step):
    q=1<<64;s=((1<<128)-1)//(q+step*q//10000);r=q
    for bit in range(19):
        if abs(id)&(1<<bit):r=r*s>>64
        if bit!=18:s=s*s>>64
    return ((1<<128)-1)//r if id>0 else q if id==0 else r
@contextmanager
def controlled(mode):
    original=native.BASE
    with tempfile.TemporaryDirectory(prefix='xone-meteora-exact-out-')as temp:
        base=Path(temp);folder=base/'commons/tests/fixtures'/FOLDER;shutil.copytree(BASE/'commons/tests/fixtures'/FOLDER,folder);(base/'commons/tests/artifacts').mkdir(parents=True);binary=(BASE/'commons/tests/artifacts/lb_clmm_prod.so').read_bytes()
        if sha(binary)!='942f609d03440911f15c40e8df1733d808670768fd9042af1de956964fb2f0e6':raise ValueError('METEORA_PROGRAM_CODE')
        (base/'commons/tests/artifacts/lb_clmm_prod.so').write_bytes(binary)
        path=folder/'lb_pair.bin';raw=path.read_bytes();pair,_=decode({'defined':{'name':'LbPair'}},raw,8)
        if 'active'in mode:
            pair['active_id']=mode['active'];pair['v_parameters']['index_reference']=mode['active'];pair['v_parameters']['volatility_accumulator']=pair['v_parameters']['volatility_reference']=0
        if mode.get('zero_fee'):pair['parameters']['base_factor']=pair['parameters']['variable_fee_control']=0
        if mode.get('fee_power')is not None:pair['parameters']['base_fee_power_factor']=mode['fee_power']
        path.write_bytes(raw[:8]+encode({'defined':{'name':'LbPair'}},pair))
        if 'active'in mode:
            arrays=[]
            for filename in ['bin_array_1.bin','bin_array_2.bin']:
                p=folder/filename;r=p.read_bytes();a,_=decode({'defined':{'name':'BinArray'}},r,8);arrays.append((p,r[:8],a))
            template=copy.deepcopy(next(a['bins'][15]for _,_,a in arrays if a['index']==0))
            for p,disc,a in arrays:
                for b in a['bins']:b['amount_x']=b['amount_y']=b['liquidity_supply']=0
                if a['index']==mode['active']//70:a['bins'][mode['active']%70]=dict(template,price=price(mode['active'],pair['bin_step']),amount_x=100000000,amount_y=100000000)
                p.write_bytes(disc+encode({'defined':{'name':'BinArray'}},a))
        if 'oracle'in mode:
            p=folder/'oracle.bin';r=bytearray(p.read_bytes());old=struct.unpack_from('<Q',r,8)[0];sample=r[32+old*32:64+old*32];index=old;active=length=100
            if mode['oracle']=='wrap':index=99
            elif mode['oracle']=='grow':index=0;active=1;r[32:]=bytes(len(r)-32)
            elif mode['oracle']=='single':index=0;active=length=1;r=r[:64]
            r[32+index*32:64+index*32]=sample;struct.pack_into('<QQQ',r,8,index,active,length);struct.pack_into('<qq',r,32+index*32+16,TIME-mode['age'],TIME-mode['age']);p.write_bytes(r)
        overrides=[dict(path=p.name,original_sha256=sha((BASE/'commons/tests/fixtures'/FOLDER/p.name).read_bytes()),controlled_sha256=sha(p.read_bytes()))for p in folder.iterdir()if p.is_file()and p.read_bytes()!=(BASE/'commons/tests/fixtures'/FOLDER/p.name).read_bytes()]
        native.BASE=base
        try:yield overrides
        finally:native.BASE=original

def execute(mode,zero,target,budget=BUDGET,followups=()):
    with controlled(mode)as overrides:
        n=native.execute(zero=zero,amount=target,minimum=budget,exact_out=True,mm_only=True,protocol_share=mode.get('share',2000),dynamic_control=mode.get('control',7500),elapsed=mode.get('elapsed',300),followups=followups)
        return dict(mode=mode,preparation_overrides=overrides,native=n)
def selectors():
    modes=[dict(name=name,elapsed=elapsed,share=share,control=control)for name,elapsed,share,control in [('static',0,0,0),('same-clock',0,2000,7500),('filter',299,1000,7500),('reduction',300,2000,7500),('decay',1200,2500,7500)]]
    selectors=[(m,z,t)for m in modes for z in [True,False]for t in [1,2,3,10,100,28776,28777,28778,1000000,100000000,100000000000,1000000000000000]]
    selectors += [(dict(name=f'oracle-{kind}-{age}',oracle=kind,age=age),True,1000000)for kind in ['full','wrap','grow','single']for age in [0,1,119,120,121]]
    selectors += [(dict(name=f'price-{id}',active=id,control=0,elapsed=0),zero,t)for id,zero in [(40,True),(-40,False)]for t in [1,2,3,4,5,10,20,100,99999999,100000000,100000001]]
    selectors += [(dict(name='zero-fee',zero_fee=True,control=0),True,1),(dict(name='fee-cap',fee_power=3),False,1000000)]
    return selectors
def main():
    records=[];cases=[]
    for index,(mode,zero,target)in enumerate(selectors()):
        followups=(10000,20000)if index<120 and target==1000000 else()
        entry=execute(mode,zero,target,followups=followups);entry['index']=index;n=entry['native'];entry['input_minus_one']=None
        if n['passed']:
            if n['recipient_output_delta']!=str(target):raise ValueError('METEORA_EXACT_TARGET')
            entry['input_minus_one']=execute(mode,zero,target,budget=int(n['payer_input_delta'])-1)['native']
            if entry['input_minus_one']['passed']or entry['input_minus_one']['before']!=entry['input_minus_one']['after']:raise ValueError('METEORA_INPUT_LIMIT_CONTROL')
        elif n['before']!=n['after']:raise ValueError('METEORA_ROLLBACK')
        rows=[]
        for step in n['sequence']if n['passed']else[n]:
            rows.append(dict(pool=projection.pool(step['before'],n['clock'],n['program_sha256']),target=step['specified_amount'],zero_for_one=zero,passed=step['passed'],required_input=step['payer_input_delta'],after=projection.pool(step['after'],n['clock'],n['program_sha256'])))
        records.append(entry);cases.append(dict(index=index,name=mode['name'],steps=rows))
    evidence=dict(schema_version=1,generator_sha256=sha(Path(__file__).read_bytes()),native_helper_sha256=sha((BASE/'harness/native_preflight.py').read_bytes()),source_metadata_sha256=sha((BASE/'source-metadata.json').read_bytes()),authority='original retained SBF and classic token CPI; controlled MM-only banks, synthetic Oracle/price/fee cases and funding; no canonical mainnet bank or public executor qualification',records=records,all_requested_features_complete=False,production_ready=False)
    TARGET.write_text(json.dumps(evidence,indent=2)+'\n');result=dict(schema_version=1,source_sha256=sha(TARGET.read_bytes()),generator_sha256=sha(Path(__file__).read_bytes()),cases=cases)
    for name in ['integration/test/contracts/meteora-exact-out.json','dex-router/test/contracts/meteora-exact-out.json']:(ROOT/name).write_text(json.dumps(result,separators=(',',':'))+'\n')
    print(json.dumps(dict(passed=True,cases=len(cases),full=sum(e['native']['passed']for e in records),rejected=sum(not e['native']['passed']for e in records),sequence_steps=sum(len(e['native']['sequence'])for e in records if len(e['native']['sequence'])>1))))
if __name__=='__main__':main()
