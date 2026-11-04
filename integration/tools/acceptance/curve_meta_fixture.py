"""Versioned service pools projected from original initialized MetaUSD exchanges."""
from pathlib import Path
import copy,hashlib,json
ROOT=Path(__file__).resolve().parents[3]
NAMES=['meta-0','meta-6','meta-8','meta-18']
PROTOCOL='curve-metausd-direct';ADAPTER='curve-metausd-direct/5272fd2b-fixed-base-1'
def digest(raw):return hashlib.sha256(raw).hexdigest()
def corpus():return json.loads((ROOT/'integration/test/contracts/curve-meta-direct.json').read_text())
def original_scenario(name):return next(s for s in corpus()['scenarios']if s['name']==name)
def code_bytes(name):return bytes.fromhex(original_scenario(name)['pool_runtime_hex'].removeprefix('0x'))
def implementation_code():return bytes.fromhex(original_scenario(NAMES[0])['implementation_runtime_hex'].removeprefix('0x'))
def dependency_code(name,token):
 s=original_scenario(name)
 for key,runtime in [('token0','token0_runtime_hex'),('token1','token1_runtime_hex'),('base_pool','base_runtime_hex')]:
  if token==s[key]:return bytes.fromhex(s[runtime].removeprefix('0x'))
 if token==s['implementation']:return implementation_code()
 raise ValueError('unknown Meta dependency '+token)
def block_hash(number):return '0x'+digest(f'META/service/{number}'.encode())
def profile(name,step_index=0,number=100,blockhash=None):
 row=original_scenario(name);step=copy.deepcopy(row['steps'][step_index]);state={'implementation':row['implementation'],'implementation_code_sha256':digest(implementation_code()),'base_pool':row['base_pool'],'base_pool_code_sha256':digest(dependency_code(name,row['base_pool'])),'base_virtual_price':step['base_virtual_price'],'stored_balances':step['stored_balances'],'admin_balances':step['admin_balances'],'timestamp':step['timestamp'],'block_number':number,'block_hash':blockhash or block_hash(number)}
 assets=[{'token':row['token0'],'balance':step['balances'][0],'scaling':step['meta_rate'],'code_sha256':digest(dependency_code(name,row['token0']))},{'token':row['token1'],'balance':step['balances'][1],'scaling':str(10**18),'code_sha256':digest(dependency_code(name,row['token1']))}]
 p={'chain_key':'evm:31337','pool_id':row['pool'],'protocol':PROTOCOL,'token0':assets[0]['token'],'token1':assets[1]['token'],'reserve0':assets[0]['balance'],'reserve1':assets[1]['balance'],'fee_numerator':int(step['fee']),'fee_denominator':10**10,'adapter_version':ADAPTER,'token_behavior':'standard','multi_asset':{'code_sha256':digest(code_bytes(name)),'vault':'','vault_code_sha256':'','pool_id':'','assets':assets,'weights':[],'amplification':step['amp'],'admin_fee':int(step['admin_fee']),'curve_meta':state}}
 cfg={'id':p['pool_id'],'protocol':PROTOCOL,'token0':p['token0'],'token1':p['token1'],'fee_numerator':p['fee_numerator'],'fee_denominator':10**10,'program_id':'','accounts':[],'multi_asset':{'expected_code_sha256':p['multi_asset']['code_sha256'],'expected_implementation':state['implementation'],'expected_implementation_code_sha256':state['implementation_code_sha256'],'expected_base_pool_code_sha256':state['base_pool_code_sha256'],'tokens':[{'token':a['token'],'expected_code_sha256':a['code_sha256']}for a in assets],'token_policy':'standard-base-lp'}}
 expected=copy.deepcopy(p)
 if not step['expected'].get('reverted'):
  e=step['expected'];expected['reserve0'],expected['reserve1']=e['next_balances'];
  for a,b in zip(expected['multi_asset']['assets'],e['next_balances']):a['balance']=b
  expected['multi_asset']['curve_meta'].update(stored_balances=e['next_stored_balances'],admin_balances=e['next_admin_balances'])
 return {'name':name,'step_index':step_index,'pool':p,'config':cfg,'original':step,'expected_pool':expected}
