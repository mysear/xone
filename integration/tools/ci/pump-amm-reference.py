#!/usr/bin/env python3
"""Project original PumpSwap classic/WSOL v1 arithmetic and complete fee accounts."""
from pathlib import Path
import base64,hashlib,importlib.util,json,sys
ROOT=Path(__file__).resolve().parents[3];BASE=ROOT/'integration/tools/reference/pump-amm';spec=importlib.util.spec_from_file_location('pump_amm_native',BASE/'harness/native.py');native=importlib.util.module_from_spec(spec);spec.loader.exec_module(native)
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'));from r01_public_catalog_solana_rebuild import programdata_pda
TARGET=ROOT/'integration/artifacts/local/core-pump-amm-original-20261111.json';PROFILE='pump-amm-classic-wsol';ADAPTER='pump-amm/6a7ad34596d8-classic-wsol-1'
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def pool(bank,ids):
 keys=[ids[k]for k in ['pool','base_mint','quote_mint','pool_base_token_account','pool_quote_token_account','global_config','fee_config','coin_creator_vault_ata','protocol_fee_recipient_token_account','buyback_ata','global_volume_accumulator','user_volume_accumulator']];pins=json.loads((BASE/'public-bank-pins.json').read_bytes());data=(BASE/'upstream/accounts'/f'{programdata_pda(native.PROGRAM)}.bin').read_bytes();fee_pins=json.loads((BASE.parent/'pump-bonding/public-bank-pins.json').read_bytes());fee_data=next(r for r in fee_pins['files']if r['address']==programdata_pda(native.FEE));fee_raw=(BASE.parent/'pump-bonding'/fee_data['path']).read_bytes()
 state=dict(programdata=programdata_pda(native.PROGRAM),deployed_slot=int.from_bytes(data[4:12],'little'),program_code_sha256=sha(BASE/'upstream/pump_amm.so'),fee_programdata=programdata_pda(native.FEE),fee_deployed_slot=int.from_bytes(fee_raw[4:12],'little'),fee_program_code_sha256=sha(BASE.parent/'pump-bonding/upstream/artifacts/fee.so'),token_program_code_sha256='53e587eee80c786bcfba1450397d92179a0f01def776d19fb4ddf690d64431cc',slot=pins['slot'],timestamp=1794375000,user=ids['user'],vault=ids['pool_base_token_account'],creator_vault=ids['coin_creator_vault_ata'],fee_recipient=ids['protocol_fee_recipient'],buyback_recipient=ids['buyback_recipient'],accounts=[dict(address=k,owner=bank[k]['owner'],lamports=bank[k]['lamports'],data=base64.b64decode(bank[k]['data_base64']).hex())for k in sorted(keys)])
 reserves=[str(int.from_bytes(base64.b64decode(bank[ids[k]]['data_base64'])[64:72],'little'))for k in ['pool_base_token_account','pool_quote_token_account']]
 raw_pool=base64.b64decode(bank[ids['pool']]['data_base64']);boost=int.from_bytes(raw_pool[245:261],'little',signed=True)>0
 return dict(chain_key='solana:mainnet-beta',pool_id=ids['pool'],protocol='pump-amm-classic-wsol-boost'if boost else PROFILE,adapter_version='pump-amm/6a7ad34596d8-classic-wsol-boost-1'if boost else ADAPTER,token_behavior='standard',token0=ids['base_mint'],token1=ids['quote_mint'],reserve0=reserves[0],reserve1=reserves[1],fee_numerator=0,fee_denominator=10000,pump=state)
def main():
 records=[];cases=[]
 for kind in ['sell','buy','buy_exact_quote_in']:
  for amount in [1,2,10,100,1000,1000000,1000000000,100000000000,1000000000000000]:
   n=native.execute(kind,amount,10**15 if kind=='buy'else 1,followups=(10000,20000)if amount==1000000 else());index=len(records);records.append(dict(index=index,native=n));steps=[]
   for step in n['sequence']if n['passed']else[n]:steps.append(dict(passed=step['passed'],amount=step['amount'],actual_base=step['user_base_delta'],actual_quote=step['user_quote_delta'],pool=pool(step['before'],n['ids']),after=pool(step['after'],n['ids'])))
   cases.append(dict(index=index,kind=kind,steps=steps))
 TARGET.write_text(json.dumps(dict(schema_version=1,passed=True,generator_sha256=sha(__file__),native_sha256=sha(native.__file__),records=records,authority='constructed canonical PDA classic/WSOL v1 banks, acquired exact PumpSwap/Fee SBF and Token CPI; not migrated account qualification or public execution'),indent=2)+'\n')
 data=dict(schema_version=1,source_sha256=sha(TARGET),cases=cases)
 for service in ['integration','dex-router']:(ROOT/service/'test/contracts/pump-amm.json').write_text(json.dumps(data,separators=(',',':'))+'\n')
 print(json.dumps(dict(passed=True,cases=len(cases),success=sum(n['native']['passed']for n in records))))
if __name__=='__main__':main()
