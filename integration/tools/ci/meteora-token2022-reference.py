#!/usr/bin/env python3
"""Pinned Token-2022 CPI transfer-fee / epoch inputs for original DLMM SBF."""
from pathlib import Path
import base64,hashlib,importlib.util,json,sys
from functools import cache
ROOT=Path(__file__).resolve().parents[3];sys.path.insert(0,str(ROOT/'integration/tools/reference/meteora-dlmm/harness'))
import native_extended as native
spec=importlib.util.spec_from_file_location('token22_projection',ROOT/'integration/tools/ci/meteora-mm-state.py');projection=importlib.util.module_from_spec(spec);spec.loader.exec_module(projection)
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'));from r01_public_catalog_solana_bank import native_imports
from r01_public_catalog_solana_rebuild import programdata_pda
PROGRAM='TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb';CODE='495e9d7680dd555cb126a6a8e5464af5be9b01f02f2cd70634352722d22e3cad';PROFILE='meteora-dlmm-token2022';ADAPTER='meteora-dlmm/1532e540-token2022-fee-1';TARGET=ROOT/'integration/artifacts/local/core-meteora-token2022-original-20261111.json'
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
@cache
def token_code():
 LiteSVM,Pubkey,*_=native_imports();svm=LiteSVM().with_mainnet_features();p=svm.get_account(Pubkey.from_string(PROGRAM));d=svm.get_account(Pubkey.from_string(programdata_pda(PROGRAM)))
 if hashlib.sha256(d.data[45:]).hexdigest()!=CODE:raise ValueError('TOKEN2022_CODE_CHANGED')
 return p,d
def pool(bank,clock,code):
 p=projection.pool(bank,clock,code);s=p['meteora'];tp,td=token_code();raw=bytes.fromhex(s['pair_data']);vault=__import__('idl_layout').decode('pubkey',raw,152)[0];p.update(protocol=PROFILE,adapter_version=ADAPTER);s['token2022']=dict(programdata=programdata_pda(PROGRAM),deployed_slot=int.from_bytes(td.data[4:12],'little'),program_code_sha256=CODE,bank_epoch=clock['epoch'],mint_data=base64.b64decode(bank[p['token0']]['data_base64']).hex(),vault_data=base64.b64decode(bank[vault]['data_base64']).hex());return p

def main():
 records=[];cases=[]
 for epoch in [1,2]:
  for fees in [(250,1000,500,2000,2),(0,0,10000,100,2),(10000,100,0,0,2)]:
   for zero in [True,False]:
    for exact in [False,True]:
     for amount in [1,100,1000,1000000,1000000000000000]:
      n=native.execute(zero=zero,amount=amount,minimum=10**15 if exact else 1,exact_out=exact,mm_only=True,protocol_share=2000,elapsed=300,dynamic_control=7500,token22_fee=fees,bank_epoch=epoch,followups=(10000,20000)if amount==1000000 else());index=len(records);records.append(dict(index=index,fees=fees,epoch=epoch,zero_for_one=zero,exact_out=exact,native=n));steps=[]
      for step in n['sequence']if n['passed']else[n]:steps.append(dict(passed=step['passed'],amount=step['specified_amount'],actual_input=step['payer_input_delta'],actual_output=step['recipient_output_delta'],pool=pool(step['before'],n['clock'],n['program_sha256']),after=pool(step['after'],n['clock'],n['program_sha256'])))
      cases.append(dict(index=index,zero_for_one=zero,exact_out=exact,steps=steps))
 TARGET.write_text(json.dumps(dict(schema_version=1,passed=True,generator_sha256=sha(__file__),native_sha256=sha(native.__file__),token2022_code_sha256=CODE,records=records,authority='controlled classic-WSOL / Token2022-X TransferFeeConfig banks with exact Token CPI; no hooks or other extensions, no public bank qualification'),indent=2)+'\n')
 base=json.loads((ROOT/'integration/test/contracts/meteora-mm.json').read_bytes());cfg=base['decoder']['config'];cfg['protocol']=PROFILE;tp,td=token_code();cfg['meteora']['token2022_upgrade']=dict(programdata=programdata_pda(PROGRAM),deployed_slot=int.from_bytes(td.data[4:12],'little'));shared=base['decoder']['shared_accounts']
 for id,a in [(PROGRAM,tp),(programdata_pda(PROGRAM),td)]:shared[id]=dict(owner=str(a.owner),executable=a.executable,lamports=a.lamports,data=[base64.b64encode(a.data).decode(),'base64'])
 ids=set(cfg['accounts']);decoder=[]
 for c in records:
  n=c['native'];accounts={id:dict(owner=a['owner'],executable=a['executable'],lamports=a['lamports'],data=[a['data_base64'],'base64'])for id,a in n['before'].items()if id in ids};clock=n['clock'];raw=clock['slot'].to_bytes(8,'little')+bytes(8)+clock['epoch'].to_bytes(8,'little')+bytes(8)+clock['unix_timestamp'].to_bytes(8,'little');accounts['SysvarC1ock11111111111111111111111111111111']=dict(owner='Sysvar1111111111111111111111111111111111111',executable=False,lamports=1,data=[base64.b64encode(raw).decode(),'base64']);decoder.append(dict(accounts=accounts,pool=pool(n['before'],clock,n['program_sha256'])))
 data=dict(schema_version=1,source_sha256=sha(TARGET),cases=cases,decoder=dict(config=cfg,shared_accounts=shared,cases=decoder))
 for service in ['integration','dex-router','chain-parser-rust']:(ROOT/service/'test/contracts/meteora-token2022.json').write_text(json.dumps(data,separators=(',',':'))+'\n')
 print(json.dumps(dict(passed=True,cases=len(cases),success=sum(c['native']['passed']for c in records),steps=sum(len(c['steps'])for c in cases))))
if __name__=='__main__':main()
