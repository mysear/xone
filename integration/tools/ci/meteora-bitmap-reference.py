#!/usr/bin/env python3
"""Rebind valid low-step pools, execute pinned original SBF beyond default bitmap."""
from pathlib import Path
import base64,copy,hashlib,importlib.util,json,shutil,struct,sys,tempfile
ROOT=Path(__file__).resolve().parents[3];BASE=ROOT/'integration/tools/reference/meteora-dlmm';sys.path.insert(0,str(BASE/'harness'))
import native_extended as native
from idl_layout import account,decode,encode,IDL
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from r01_public_catalog_solana_bank import native_imports
spec=importlib.util.spec_from_file_location('bitmap_projection',ROOT/'integration/tools/ci/meteora-mm-state.py');projection=importlib.util.module_from_spec(spec);spec.loader.exec_module(projection)
spec=importlib.util.spec_from_file_location('bitmap_price',ROOT/'integration/tools/ci/meteora-exact-out-reference.py');prices=importlib.util.module_from_spec(spec);spec.loader.exec_module(prices)
TARGET=ROOT/'integration/artifacts/local/core-meteora-bitmap-original-20261111.json';PROFILE='meteora-dlmm-mm-bitmap';ADAPTER='meteora-dlmm/1532e540-mm-bitmap-1'
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def execute(negative,zero,amount,exact=False,missing=False,followups=()):
    _,Pubkey,*_=native_imports();pid=Pubkey.from_string(IDL['address']);source=BASE/'commons/tests/fixtures'/projection.PAIR;pair,_=account('LbPair',source/'lb_pair.bin');pair=copy.deepcopy(pair)
    pair['parameters']['min_bin_id']=-443636;pair['parameters']['max_bin_id']=443636;pair['bin_step']=1;pair['bin_step_seed']=[1,0];pair['parameters']['base_factor']=1000;pair['base_factor_seed']=[232,3]
    low,high=sorted([bytes(Pubkey.from_string(pair[k]))for k in ['token_x_mint','token_y_mint']]);pk,bump=Pubkey.find_program_address([low,high,bytes(pair['bin_step_seed']),bytes(pair['base_factor_seed'])],pid);pair_id=str(pk);pair['bump_seed']=[bump]
    for name,seeds in [('reserve_x',[bytes(pk),bytes(Pubkey.from_string(pair['token_x_mint']))]),('reserve_y',[bytes(pk),bytes(Pubkey.from_string(pair['token_y_mint']))]),('oracle',[b'oracle',bytes(pk)])]:pair[name]=str(Pubkey.find_program_address(seeds,pid)[0])
    indices=[-513,-512]if negative else[511,512];active=(-513*70+69 if not zero else -512*70)if negative else(511*70+69 if not zero else 512*70)
    pair['active_id']=active;pair['v_parameters']['index_reference']=active;pair['bin_array_bitmap']=[0]*16
    for index in indices:
        if -512<=index<=511:pair['bin_array_bitmap'][(index+512)//64]|=1<<((index+512)%64)
    extension=dict(lb_pair=pair_id,positive_bin_array_bitmap=[[0]*8 for _ in range(12)],negative_bin_array_bitmap=[[0]*8 for _ in range(12)])
    for index in indices:
        if not -512<=index<=511:
            bit=index-512 if index>=512 else-index-513
            if not missing:extension['positive_bin_array_bitmap'if index>=512 else'negative_bin_array_bitmap'][bit//512][bit%512//64]|=1<<(bit%64)
    binary=BASE/'commons/tests/artifacts/lb_clmm_prod.so'
    if sha(binary)!='942f609d03440911f15c40e8df1733d808670768fd9042af1de956964fb2f0e6':raise ValueError('FIXED_SBF_CHANGED')
    with tempfile.TemporaryDirectory(prefix='xone-bitmap-bank-')as temp:
        base=Path(temp);folder=base/'commons/tests/fixtures'/pair_id;shutil.copytree(source,folder);artifact=base/'commons/tests/artifacts/lb_clmm_prod.so';artifact.parent.mkdir(parents=True);artifact.write_bytes(binary.read_bytes())
        raw=(source/'lb_pair.bin').read_bytes();(folder/'lb_pair.bin').write_bytes(raw[:8]+encode({'defined':{'name':'LbPair'}},pair))
        for number,index in enumerate(indices,1):
            p=folder/f'bin_array_{number}.bin';array,_=account('BinArray',p);array['index']=index;array['lb_pair']=pair_id
            template=copy.deepcopy(array['bins'][0])
            for b in array['bins']:
                for k in b:
                    if isinstance(b[k],int):b[k]=0
                b['_padding_1']=[0,0,0]
            # Liquidity on either side of the default/extension boundary.
            id=index*70+(69 if index==indices[0]else 0);b=array['bins'][id%70];b['amount_x']=100000;b['amount_y']=100000;b['liquidity_supply']=100000<<64;b['price']=prices.price(id,1)
            p.write_bytes(p.read_bytes()[:8]+encode({'defined':{'name':'BinArray'}},array))
        for name in ['reserve_x.bin','reserve_y.bin']:
            p=folder/name;raw=bytearray(p.read_bytes());raw[32:64]=bytes(pk);p.write_bytes(raw)
        raw=hashlib.sha256(b'account:BinArrayBitmapExtension').digest()[:8]+encode({'defined':{'name':'BinArrayBitmapExtension'}},extension);(folder/'bitmap.bin').write_bytes(raw)
        prior=native.BASE;native.BASE=base
        try:return native.execute(zero=zero,amount=amount,minimum=10**15 if exact else 1,exact_out=exact,mm_only=True,protocol_share=2000,elapsed=300,dynamic_control=7500,fixture_name=pair_id,bitmap=True,followups=followups)
        finally:native.BASE=prior

def pool(bank,clock,code,pair_id):
    prior=projection.PAIR;projection.PAIR=pair_id
    try:p=projection.pool(bank,clock,code)
    finally:projection.PAIR=prior
    bitmap=[(k,a)for k,a in bank.items()if len(base64.b64decode(a['data_base64']))==1576]
    if len(bitmap)!=1:raise ValueError('EXTENSION_BANK')
    p.update(protocol=PROFILE,adapter_version=ADAPTER);p['meteora']['bitmap_extension']=dict(address=bitmap[0][0],data=base64.b64decode(bitmap[0][1]['data_base64']).hex());return p

def main():
    records=[];cases=[]
    for neg in [False,True]:
        for zero in [False,True]:
            for exact in [False,True]:
                for amount in [1,1000,5000,100000,150000,1000000,10**15]:
                    n=execute(neg,zero,amount,exact);index=len(records);records.append(dict(index=index,negative=neg,zero_for_one=zero,exact_out=exact,native=n));steps=[]
                    for step in n['sequence']if n['passed']else[n]:steps.append(dict(passed=step['passed'],amount=str(amount),actual_input=step['payer_input_delta'],actual_output=step['recipient_output_delta'],pool=pool(step['before'],n['clock'],n['program_sha256'],n['pair_id']),after=pool(step['after'],n['clock'],n['program_sha256'],n['pair_id'])))
                    cases.append(dict(index=index,zero_for_one=zero,exact_out=exact,steps=steps))
    TARGET.write_text(json.dumps(dict(schema_version=1,passed=True,generator_sha256=sha(__file__),native_sha256=sha(native.__file__),records=records,authority='controlled PDA-rebound low-step banks; fixed retained original SBF, no canonical deployment qualification'),indent=2)+'\n')
    shared=json.loads((ROOT/'integration/test/contracts/meteora-mm.json').read_bytes())['decoder']['shared_accounts']
    decoder=[]
    for row,record in zip(cases,records):
        p=row['steps'][0]['pool'];m=p['meteora'];raw=bytes.fromhex(m['pair_data']);pair,_=decode({'defined':{'name':'LbPair'}},raw,8)
        ids=[p['pool_id'],pair['reserve_x'],pair['reserve_y'],p['token0'],p['token1'],pair['oracle']]+[a['address']for a in m['bin_arrays']]+[m['bitmap_extension']['address']]
        cfg=dict(id=p['pool_id'],protocol=PROFILE,program_id=IDL['address'],token0=p['token0'],token1=p['token1'],fee_numerator=0,fee_denominator=1000000000,accounts=ids,meteora=dict(bitmap_extension=ids[-1],expected_programdata=m['programdata'],expected_program_code_sha256=m['program_code_sha256'],expected_deployed_slot=1,expected_token_program_code_sha256=m['token_program_code_sha256'],token_program_upgrade=json.loads((ROOT/'integration/test/contracts/meteora-mm.json').read_bytes())['decoder']['config']['meteora']['token_program_upgrade'],bin_array_indices=[a['index']for a in m['bin_arrays']],max_steps=m['max_steps']))
        accounts={k:dict(owner=a['owner'],lamports=a['lamports'],executable=a['executable'],data=[a['data_base64'],'base64'])for k,a in record['native']['before'].items()if k in ids}
        clock=record['native']['clock'];accounts['SysvarC1ock11111111111111111111111111111111']=dict(owner='Sysvar1111111111111111111111111111111111111',lamports=1,executable=False,data=[base64.b64encode(clock['slot'].to_bytes(8,'little')+bytes(24)+clock['unix_timestamp'].to_bytes(8,'little')).decode(),'base64'])
        decoder.append(dict(config=cfg,accounts=accounts,pool=p))
    fixture=dict(schema_version=1,source_sha256=sha(TARGET),cases=cases,decoder=dict(shared_accounts=shared,cases=decoder))
    for service in ['integration','dex-router','chain-parser-rust']:
        (ROOT/service/'test/contracts/meteora-bitmap.json').write_text(json.dumps(fixture,separators=(',',':'))+'\n')
    print(json.dumps(dict(passed=True,cases=len(records),success=sum(r['native']['passed']for r in records))))
if __name__=='__main__':main()
