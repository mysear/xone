#!/usr/bin/env python3
"""Project recorded original SBF banks with the independent pinned IDL reader."""
from pathlib import Path
import base64,hashlib,json,sys
ROOT=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(ROOT/'integration/tools/reference/meteora-dlmm/harness'))
from idl_layout import decode,IDL,BASE
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from r01_public_catalog_solana_rebuild import programdata_pda,b58decode
from r01_public_catalog_solana_bank import native_imports,TOKEN,LOADER
TOKEN_CODE='53e587eee80c786bcfba1450397d92179a0f01def776d19fb4ddf690d64431cc'
SOURCE=ROOT/'integration/artifacts/local/core-meteora-dlmm-mm-original-20261110.json'
PAIR='EtAdVRLFH22rjWh3mcUasKFF27WtHhsaCvK27tPFFWig'

def pool(bank,clock,code):
    raw=base64.b64decode(bank[PAIR]['data_base64']);p,end=decode({'defined':{'name':'LbPair'}},raw,8)
    if end!=len(raw):raise ValueError('PAIR_LAYOUT')
    arrays=[]
    for addr,account in bank.items():
        data=base64.b64decode(account['data_base64'])
        if len(data)==10136:
            value,end=decode({'defined':{'name':'BinArray'}},data,8)
            if end!=len(data) or value['lb_pair']!=PAIR:raise ValueError('ARRAY_LAYOUT')
            arrays.append(dict(address=addr,index=value['index'],data=data.hex()))
    vaults=[int.from_bytes(base64.b64decode(bank[p[name]]['data_base64'])[64:72],'little')for name in ['reserve_x','reserve_y']]
    state=dict(programdata=programdata_pda(IDL['address']),program_code_sha256=code,deployed_slot=1,token_program_code_sha256=TOKEN_CODE,slot=clock['slot'],timestamp=clock['unix_timestamp'],pair_data=raw.hex(),oracle_data=base64.b64decode(bank[p['oracle']]['data_base64']).hex(),bin_arrays=sorted(arrays,key=lambda a:a['index']),raw_vault0=vaults[0],raw_vault1=vaults[1],max_steps=256)
    return dict(chain_key='solana:mainnet-beta',pool_id=PAIR,protocol='meteora-dlmm-mm',adapter_version='meteora-dlmm/1532e540-mm-1',token_behavior='standard',token0=p['token_x_mint'],token1=p['token_y_mint'],reserve0=str(vaults[0]),reserve1=str(vaults[1]),fee_numerator=0,fee_denominator=1000000000,meteora=state)

def main():
    source=json.loads(SOURCE.read_bytes());cases=[]
    LiteSVM,Pubkey,*_=native_imports();svm=LiteSVM().with_mainnet_features()
    token_program=svm.get_account(Pubkey.from_string(TOKEN));token_data_id=str(Pubkey.from_bytes(token_program.data[4:36]));token_data=svm.get_account(Pubkey.from_string(token_data_id))
    if hashlib.sha256(token_data.data[45:]).hexdigest()!=TOKEN_CODE:raise ValueError('TOKEN_CODE_CHANGED')
    def acc(raw,owner,executable=False):return dict(lamports=10000000000,owner=owner,executable=executable,data=[base64.b64encode(raw).decode(),'base64'])
    token_original_slot=int.from_bytes(token_data.data[4:12],'little');patched=bytearray(token_data.data);patched[4:12]=(1).to_bytes(8,'little')
    programdata=programdata_pda(IDL['address']);binary=(BASE/'commons/tests/artifacts/lb_clmm_prod.so').read_bytes()
    shared={TOKEN:acc(token_program.data,str(token_program.owner),True),token_data_id:acc(bytes(patched),str(token_data.owner)),IDL['address']:acc((2).to_bytes(4,'little')+b58decode(programdata),LOADER,True),programdata:acc((3).to_bytes(4,'little')+(1).to_bytes(8,'little')+bytes(33)+binary,LOADER)}
    if source['upstream_commit']!='1532e540db41ac405dbf4c5bb2ddfceaa1f98ff1':raise ValueError('COMMIT')
    for c in source['cases']:
        steps=c['sequence'] if c['passed']else[c]
        rows=[]
        for step in steps:
            rows.append(dict(pool=pool(step['before'],c['clock'],c['program_sha256']),input=step['specified_amount'],zero_for_one=c['zero_for_one'],passed=step['passed'],output=step['recipient_output_delta'],after=pool(step['after'],c['clock'],c['program_sha256'])))
        cases.append(dict(index=c['index'],mode=c['mode'],steps=rows))
    first=cases[0]['steps'][0]['pool'];state=first['meteora'];pair,_=decode({'defined':{'name':'LbPair'}},bytes.fromhex(state['pair_data']),8)
    config=dict(id=PAIR,protocol=first['protocol'],program_id=IDL['address'],token0=first['token0'],token1=first['token1'],fee_numerator=0,fee_denominator=1000000000,accounts=[PAIR,pair['reserve_x'],pair['reserve_y'],pair['token_x_mint'],pair['token_y_mint'],pair['oracle']]+[a['address']for a in state['bin_arrays']],meteora=dict(expected_programdata=state['programdata'],expected_program_code_sha256=state['program_code_sha256'],expected_deployed_slot=1,expected_token_program_code_sha256=TOKEN_CODE,token_program_upgrade=dict(programdata=token_data_id,deployed_slot=1),bin_array_indices=[a['index']for a in state['bin_arrays']],max_steps=256))
    bank_cases=[]
    for c in source['cases']:
        accounts={key:dict(lamports=a['lamports'],owner=a['owner'],executable=a['executable'],data=[a['data_base64'],'base64'])for key,a in c['before'].items()if key in config['accounts']}
        clock=c['clock'];raw=clock['slot'].to_bytes(8,'little')+bytes(24)+clock['unix_timestamp'].to_bytes(8,'little')
        accounts['SysvarC1ock11111111111111111111111111111111']=acc(raw,'Sysvar1111111111111111111111111111111111111')
        bank_cases.append(dict(accounts=accounts,pool=pool(c['before'],clock,c['program_sha256'])))
    result=dict(schema_version=1,decoder=dict(config=config,shared_accounts=shared,cases=bank_cases,controlled_token_programdata_original_slot=token_original_slot,controlled_programdata_slot=1),source_sha256=hashlib.sha256(SOURCE.read_bytes()).hexdigest(),projector_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),source_metadata_sha256=hashlib.sha256((BASE/'source-metadata.json').read_bytes()).hexdigest(),authority=source['authority'],cases=cases)
    for rel in ['integration/test/contracts/meteora-mm.json','dex-router/test/contracts/meteora-mm.json','chain-parser-rust/test/contracts/meteora-mm.json']:
        target=ROOT/rel;target.parent.mkdir(exist_ok=True,parents=True);target.write_text(json.dumps(result,separators=(',',':'))+'\n')
    print(json.dumps(dict(passed=True,cases=len(cases),steps=sum(len(c['steps'])for c in cases),files=3)))
if __name__=='__main__':main()
