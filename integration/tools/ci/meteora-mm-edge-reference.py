#!/usr/bin/env python3
"""Actual retained-SBF Oracle/fee/uncached-price boundary banks in isolated copies."""
from pathlib import Path
import base64,copy,hashlib,importlib.util,json,shutil,struct,sys,tempfile
ROOT=Path(__file__).resolve().parents[3];sys.path.insert(0,str(ROOT/'integration/tools/reference/meteora-dlmm/harness'))
import native_preflight as native
from idl_layout import IDL,decode,encode
spec=importlib.util.spec_from_file_location('mm_projection',ROOT/'integration/tools/ci/meteora-mm-state.py');projection=importlib.util.module_from_spec(spec);spec.loader.exec_module(projection)
ORIGINAL=native.BASE;FOLDER='EtAdVRLFH22rjWh3mcUasKFF27WtHhsaCvK27tPFFWig';TIME=1734392448
def sha(raw):return hashlib.sha256(raw).hexdigest()
def main():
    cases=[];records=[];base_fixture=ORIGINAL/'commons/tests/fixtures'/FOLDER
    modes=[dict(name=f'oracle-{mode}-{age}',mode=mode,age=age)for mode in ['full','wrap','grow','single']for age in [0,1,119,120,121]]
    modes += [dict(name='uncached-price-x-y',mode='full',age=120,uncached=True,zero=True),dict(name='uncached-price-y-x',mode='wrap',age=120,uncached=True,zero=False),dict(name='zero-fee-one-unit',mode='full',age=120,base_factor=0,amount=1),dict(name='capped-fee-power',mode='wrap',age=120,fee_power=3),dict(name='volatility-cap',mode='grow',age=120,cap=True)]
    with tempfile.TemporaryDirectory(prefix='xone-meteora-edge-')as temp:
        base=Path(temp);(base/'commons/tests/artifacts').mkdir(parents=True);binary=(ORIGINAL/'commons/tests/artifacts/lb_clmm_prod.so').read_bytes()
        if sha(binary)!='942f609d03440911f15c40e8df1733d808670768fd9042af1de956964fb2f0e6':raise ValueError('PROGRAM_IDENTITY')
        (base/'commons/tests/artifacts/lb_clmm_prod.so').write_bytes(binary);native.BASE=base
        for mode in modes:
            folder=base/'commons/tests/fixtures'/FOLDER;shutil.rmtree(folder,ignore_errors=True);shutil.copytree(base_fixture,folder)
            original=(folder/'oracle.bin').read_bytes();raw=bytearray(original);old_index=struct.unpack_from('<Q',raw,8)[0];sample=raw[32+old_index*32:64+old_index*32];index=old_index;active=100;length=100
            if mode['mode']=='wrap':index=99
            if mode['mode']=='grow':index=0;active=1;raw[32:]=bytes(len(raw)-32)
            if mode['mode']=='single':index=0;active=length=1;raw=raw[:64]
            raw[32+index*32:64+index*32]=sample;struct.pack_into('<QQQ',raw,8,index,active,length);struct.pack_into('<qq',raw,32+index*32+16,TIME-mode['age'],TIME-mode['age']);(folder/'oracle.bin').write_bytes(raw)
            pair_raw=(folder/'lb_pair.bin').read_bytes();pair,_=decode({'defined':{'name':'LbPair'}},pair_raw,8)
            if 'base_factor'in mode:
                pair['parameters']['base_factor']=mode['base_factor'];pair['parameters']['variable_fee_control']=0
            if 'fee_power'in mode:pair['parameters']['base_fee_power_factor']=mode['fee_power']
            if mode.get('cap'):
                pair['parameters']['max_volatility_accumulator']=1;pair['parameters']['variable_fee_control']=7500;pair['v_parameters']['volatility_accumulator']=0;pair['v_parameters']['volatility_reference']=0;pair['v_parameters']['index_reference']=pair['active_id']
            (folder/'lb_pair.bin').write_bytes(pair_raw[:8]+encode({'defined':{'name':'LbPair'}},pair))
            if mode.get('uncached'):
                for filename in ['bin_array_1.bin','bin_array_2.bin']:
                    p=folder/filename;raw=p.read_bytes();bins,_=decode({'defined':{'name':'BinArray'}},raw,8)
                    for b in bins['bins']:b['price']=0
                    p.write_bytes(raw[:8]+encode({'defined':{'name':'BinArray'}},bins))
            overrides=[dict(path=str(p.relative_to(folder)),original_sha256=sha((base_fixture/p.relative_to(folder)).read_bytes()),controlled_sha256=sha(p.read_bytes()))for p in folder.iterdir()if p.is_file()and p.read_bytes()!=(base_fixture/p.relative_to(folder)).read_bytes()]
            zero=mode.get('zero',True);amount=mode.get('amount',1000000);record=native.execute(zero=zero,amount=amount,mm_only=True)
            records.append(dict(mode=mode,preparation_overrides=overrides,native=record))
            cases.append(dict(name=mode['name'],pool=projection.pool(record['before'],record['clock'],record['program_sha256']),input=str(amount),zero_for_one=zero,passed=record['passed'],output=record['recipient_output_delta'],after=projection.pool(record['after'],record['clock'],record['program_sha256'])))
    native.BASE=ORIGINAL
    evidence=ROOT/'integration/artifacts/local/core-meteora-mm-edge-original-20261110.json';evidence.write_text(json.dumps(dict(schema_version=1,generator_sha256=sha(Path(__file__).read_bytes()),authority='controlled Oracle header/timestamps, fee parameters and zero price caches in isolated copies; retained original SBF and actual token CPI',records=records),indent=2)+'\n')
    result=dict(schema_version=1,source_sha256=sha(evidence.read_bytes()),generator_sha256=sha(Path(__file__).read_bytes()),cases=cases)
    for relative in ['integration/test/contracts/meteora-mm-edge.json','dex-router/test/contracts/meteora-mm-edge.json']:(ROOT/relative).write_text(json.dumps(result,separators=(',',':'))+'\n')
    print(json.dumps(dict(passed=True,cases=len(cases),full=sum(c['passed']for c in cases),original_evidence_sha256=sha(evidence.read_bytes()))))
if __name__=='__main__':main()
