"""Original Vyper state fixtures projected onto the versioned service contract."""
from pathlib import Path
import copy,hashlib,json
ROOT=Path(__file__).resolve().parents[3]
NAMES=['two-sequential','three-mixed-decimals','four','five','six','seven','eight']
PROTOCOL='curve-stableswap-ng';ADAPTER='curve-stableswap-ng/3332cd44-standard-1'
def digest(data):return hashlib.sha256(data).hexdigest()
def corpus():return json.loads((ROOT/'integration/test/contracts/curve-ng-state.json').read_text())
def original_scenario(name):return next(s for s in corpus()['scenarios']if s['name']==name)
def code_bytes(name):return bytes.fromhex((ROOT/'integration/tools/reference/curve-ng-state'/('runtime-'+name+'.hex')).read_text().strip().removeprefix('0x'))
def token_code(index):return bytes([0x60,index+1,0x60,0,0x55])
def block_hash(name,number):return'0x'+digest(f'NG/{name}/{number}'.encode())
def profile(name,step_index=0,number=100,blockhash=None):
 row=original_scenario(name);step=copy.deepcopy(row['steps'][step_index]);n=len(step['balances']);code=code_bytes(name)
 # The original immutable coins and multipliers are carried unchanged.
 address='0x'+digest(('pool/'+name).encode())[:40]
 tokens=[a.lower()for a in row['pool']['coins']]
 state=copy.deepcopy(step['state']);state['ma_exp_time']=int(state['ma_exp_time']);state['d_ma_time']=int(state['d_ma_time']);state.update(block_number=number,block_hash=blockhash or block_hash(name,number),offpeg_fee_multiplier=step['offpeg_fee_multiplier'])
 assets=[{'token':tokens[i],'balance':step['balances'][i],'scaling':step['rates'][i],'code_sha256':digest(token_code(i))}for i in range(n)]
 p={'chain_key':'evm:31337','pool_id':address,'protocol':PROTOCOL,'token0':tokens[0],'token1':tokens[1],'reserve0':assets[0]['balance'],'reserve1':assets[1]['balance'],'fee_numerator':int(step['fee']),'fee_denominator':10**10,'adapter_version':ADAPTER,'token_behavior':'standard','multi_asset':{'code_sha256':digest(code),'vault':'','vault_code_sha256':'','pool_id':'','assets':assets,'weights':[],'amplification':step['amp'],'admin_fee':int(step['admin_fee']),'curve_ng':state}}
 cfg={'id':address,'protocol':PROTOCOL,'token0':tokens[0],'token1':tokens[1],'fee_numerator':p['fee_numerator'],'fee_denominator':p['fee_denominator'],'program_id':'','accounts':[],'multi_asset':{'expected_code_sha256':digest(code),'tokens':[{'token':a['token'],'expected_code_sha256':a['code_sha256']}for a in assets],'token_policy':'standard-no-bpt'}}
 expected=copy.deepcopy(p)
 if not step['expected'].get('reverted'):
  for a,b in zip(expected['multi_asset']['assets'],step['expected']['next_balances']):a['balance']=b
  expected['reserve0']=expected['multi_asset']['assets'][0]['balance'];expected['reserve1']=expected['multi_asset']['assets'][1]['balance'];ng=copy.deepcopy(step['expected']['next_state']);ng['ma_exp_time']=int(ng['ma_exp_time']);ng['d_ma_time']=int(ng['d_ma_time']);ng.update(block_number=number,block_hash=p['multi_asset']['curve_ng']['block_hash'],offpeg_fee_multiplier=step['offpeg_fee_multiplier']);expected['multi_asset']['curve_ng']=ng
 return {'name':name,'step_index':step_index,'pool':p,'config':cfg,'original':step,'expected_pool':expected}
