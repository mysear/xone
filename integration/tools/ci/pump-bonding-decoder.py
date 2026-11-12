#!/usr/bin/env python3
"""Prepare independent normal-decoder input bytes and exact dependency closure."""
from pathlib import Path
import base64,hashlib,importlib.util,json,sys
ROOT=Path(__file__).resolve().parents[3];BASE=ROOT/'integration/tools/reference/pump-bonding';sys.path.insert(0,str(BASE/'harness'));import native
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'));from r01_public_catalog_solana_bank import native_imports,TOKEN
from r01_public_catalog_solana_rebuild import b58encode

def main():
    reference=json.loads((ROOT/'integration/test/contracts/pump-bonding.json').read_bytes());evidence=json.loads((ROOT/'integration/artifacts/local/core-pump-bonding-original-20261111.json').read_bytes());pins=json.loads((BASE/'public-bank-pins.json').read_bytes());shared={}
    for item in pins['files']:
        raw=(BASE/item['path']).read_bytes()
        if hashlib.sha256(raw).hexdigest()!=item['sha256']:raise ValueError('PUMP_ACQUISITION_CHANGED')
        if item['bytes']in [1088,4097]:continue
        shared[item['address']]=dict(owner=item['owner'],executable=item['executable'],lamports=item['lamports'],data=[base64.b64encode(raw).decode(),'base64'])
    LiteSVM,Pubkey,*_=native_imports();svm=LiteSVM().with_mainnet_features();program=svm.get_account(Pubkey.from_string(TOKEN));data_id=str(Pubkey.from_bytes(program.data[4:36]));data=svm.get_account(Pubkey.from_string(data_id));
    for key,a in [(TOKEN,program),(data_id,data)]:shared[key]=dict(owner=str(a.owner),executable=a.executable,lamports=a.lamports,data=[base64.b64encode(a.data).decode(),'base64'])
    p=reference['cases'][46]['steps'][0]['pool'];s=p['pump'];accounts=[a['address']for a in s['accounts']]
    cfg=dict(id=p['pool_id'],protocol=p['protocol'],program_id=native.PUMP,token0=p['token0'],token1=p['token1'],fee_numerator=0,fee_denominator=10000,accounts=accounts,pump=dict(expected_programdata=s['programdata'],expected_deployed_slot=s['deployed_slot'],expected_program_code_sha256=s['program_code_sha256'],expected_fee_programdata=s['fee_programdata'],expected_fee_deployed_slot=s['fee_deployed_slot'],expected_fee_program_code_sha256=s['fee_program_code_sha256'],expected_token_program_code_sha256=s['token_program_code_sha256'],user=s['user'],token_program_upgrade=dict(programdata=data_id,deployed_slot=int.from_bytes(data.data[4:12],'little'))))
    rows=[]
    for index in [26,46,10,30,50]:
        c=evidence['records'][index];pool=reference['cases'][index]['steps'][0]['pool'];values={k:dict(owner=a['owner'],executable=a['executable'],lamports=a['lamports'],data=[a['data_base64'],'base64'])for k,a in c['before'].items()if k in accounts}
        raw=(s['slot']).to_bytes(8,'little')+bytes(24)+s['timestamp'].to_bytes(8,'little');values['SysvarC1ock11111111111111111111111111111111']=dict(owner='Sysvar1111111111111111111111111111111111111',executable=False,lamports=1,data=[base64.b64encode(raw).decode(),'base64']);rows.append(dict(index=index,pool=pool,accounts=values))
    result=dict(schema_version=1,config=cfg,shared_accounts=shared,cases=rows,authority='copied original programdata and actual controlled original-account bytes; not canonical historical bank')
    raw=json.dumps(result,separators=(',',':'))+'\n'
    for path in ['integration/test/contracts/pump-bonding-decoder.json','chain-parser-rust/test/contracts/pump-bonding-decoder.json']:(ROOT/path).write_text(raw)
    print(json.dumps(dict(passed=True,cases=len(rows),dependencies=len(accounts)+len(shared)+1,bytes=len(raw))))
if __name__=='__main__':main()
