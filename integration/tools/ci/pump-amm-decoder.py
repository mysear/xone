#!/usr/bin/env python3
"""Exact PumpSwap dependency closure, separately copied into the parser tests."""
from pathlib import Path
import base64,hashlib,json
ROOT=Path(__file__).resolve().parents[3];BASE=ROOT/'integration/tools/reference/pump-amm'
def main():
 f=json.loads((ROOT/'integration/test/contracts/pump-amm.json').read_bytes());e=json.loads((ROOT/'integration/artifacts/local/core-pump-amm-original-20261111.json').read_bytes());shared={};old=json.loads((ROOT/'integration/test/contracts/pump-bonding-decoder.json').read_bytes());s=f['cases'][0]['steps'][0]['pool']['pump'];keep=['TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA',s['fee_programdata'],'pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ',old['config']['pump']['token_program_upgrade']['programdata']]
 for id in keep:shared[id]=old['shared_accounts'][id]
 pins=json.loads((BASE/'public-bank-pins.json').read_bytes())
 for item in pins['files']:
  if item.get('address')not in [pins['program'],s['programdata']]:continue
  raw=(BASE/item['path']).read_bytes()
  if hashlib.sha256(raw).hexdigest()!=item['sha256']:raise ValueError('FIXED_AMM_PROGRAM_CHANGED')
  shared[item['address']]=dict(owner=item['owner'],lamports=item['lamports'],executable=item['executable'],data=[base64.b64encode(raw).decode(),'base64'])
 p=f['cases'][0]['steps'][0]['pool'];ids=[a['address']for a in s['accounts']];cfg=dict(id=p['pool_id'],protocol=p['protocol'],program_id=pins['program'],token0=p['token0'],token1=p['token1'],fee_numerator=0,fee_denominator=10000,accounts=ids,pump={k:s[k]for k in []});cfg['pump']=dict(expected_programdata=s['programdata'],expected_deployed_slot=s['deployed_slot'],expected_program_code_sha256=s['program_code_sha256'],expected_fee_programdata=s['fee_programdata'],expected_fee_deployed_slot=s['fee_deployed_slot'],expected_fee_program_code_sha256=s['fee_program_code_sha256'],expected_token_program_code_sha256=s['token_program_code_sha256'],user=s['user'],token_program_upgrade=old['config']['pump']['token_program_upgrade'])
 cases=[]
 for row,c in zip(f['cases'],e['records']):
  n=c['native'];accounts={id:dict(owner=a['owner'],lamports=a['lamports'],executable=a['executable'],data=[a['data_base64'],'base64'])for id,a in n['before'].items()if id in ids};raw=s['slot'].to_bytes(8,'little')+bytes(24)+s['timestamp'].to_bytes(8,'little');accounts['SysvarC1ock11111111111111111111111111111111']=dict(owner='Sysvar1111111111111111111111111111111111111',executable=False,lamports=1,data=[base64.b64encode(raw).decode(),'base64']);cases.append(dict(index=c['index'],pool=row['steps'][0]['pool'],accounts=accounts))
 migration=json.loads((ROOT/'integration/test/contracts/pump-migration.json').read_bytes());boost=[]
 for row in migration['cases']:
  pool=json.loads(json.dumps(row['pool']));pool.update(protocol='pump-amm-classic-wsol',adapter_version='pump-amm/6a7ad34596d8-classic-wsol-1');state=pool['pump'];config=json.loads(json.dumps(cfg));config.update(id=pool['pool_id'],token0=pool['token0'],token1=pool['token1'],accounts=[a['address']for a in state['accounts']]);config['pump']['user']=state['user'];accounts={a['address']:dict(owner=a['owner'],lamports=a['lamports'],executable=False,data=[base64.b64encode(bytes.fromhex(a['data'])).decode(),'base64'])for a in state['accounts']};clock=state['slot'].to_bytes(8,'little')+bytes(24)+state['timestamp'].to_bytes(8,'little');accounts['SysvarC1ock11111111111111111111111111111111']=dict(owner='Sysvar1111111111111111111111111111111111111',executable=False,lamports=1,data=[base64.b64encode(clock).decode(),'base64']);boost.append(dict(config=config,pool=pool,accounts=accounts))
 result=dict(schema_version=1,config=cfg,shared_accounts=shared,cases=cases,boost_cases=boost,source_sha256=hashlib.sha256((ROOT/'integration/artifacts/local/core-pump-amm-original-20261111.json').read_bytes()).hexdigest())
 for service in ['integration','chain-parser-rust']:(ROOT/service/'test/contracts/pump-amm-decoder.json').write_text(json.dumps(result,separators=(',',':'))+'\n')
 print(json.dumps(dict(passed=True,cases=len(cases),dependencies=len(ids)+len(shared)+1)))
if __name__=='__main__':main()
