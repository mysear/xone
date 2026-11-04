"""Translate pinned native struct dumps to the independent public service state.

Addresses/RPC banks are synthetic, packed by the original program; no live
program deployment or SPL CPI/transaction verification is implied.
"""
from pathlib import Path
import copy,json,hashlib
ROOT=Path(__file__).resolve().parents[3]
PROFILE='orca-whirlpools-adaptive-rewards';ADAPTER='orca-whirlpools/cc683ec9-spl-adaptive-rewards-1'
CLOCK='SysvarC1ock11111111111111111111111111111111'
def corpus():return json.loads((ROOT/'integration/test/contracts/orca-adaptive-rewards-service.json').read_text())
def clean_config(config):return {k:copy.deepcopy(v)for k,v in config.items()if k not in ['fixture','adapter_version']}
def pool(config,native):
 proof=config['orca_whirlpool'];o=native['oracle'];constants=o['adaptive_fee_info']['constants'];variables=o['adaptive_fee_info']['variables'];adaptive={k:v for k,v in constants.items()if k!='reserved'};adaptive['constants_reserved']=constants['reserved'];adaptive.update({k:v for k,v in variables.items()if k!='reserved'});adaptive.update(variables_reserved=variables['reserved'],reserved=o['reserved'],whirlpool=o['whirlpool'],trade_enable_timestamp=o['trade_enable_timestamp'])
 s={'adaptive':adaptive,'reward_infos':[{k:v for k,v in r.items()if k!='initialized'}for r in native['reward_infos']],
 'program_id':config['program_id'],'programdata':proof['expected_programdata'],'program_code_sha256':proof['expected_program_code_sha256'],'deployed_slot':proof['expected_deployed_slot'],'token_program_code_sha256':proof['expected_token_program_code_sha256'],'whirlpools_config':native['whirlpools_config'],'whirlpool_bump':native['whirlpool_bump'][0],
 'oracle':o['address'],'vault0':native['token_vault_a'],'vault1':native['token_vault_b'],'raw_vault0':int(native['raw_vault_a']),'raw_vault1':int(native['raw_vault_b']),'mint0_decimals':native['mints'][0]['decimals'],'mint1_decimals':native['mints'][1]['decimals'],'bank_slot':native['bank_slot'],'bank_unix_timestamp':native['bank_unix_timestamp'],'bank_epoch':native['clock']['epoch'],'fee_tier_index':native['fee_tier_index'],'fee_rate':native['fee_rate'],'protocol_fee_rate':native['protocol_fee_rate'],'sqrt_price_x64':native['sqrt_price_x64'],'tick_current_index':native['tick_current_index'],'liquidity':native['liquidity'],'tick_spacing':native['tick_spacing'],'protocol_fee_owed0':int(native['protocol_fee_owed_a']),'protocol_fee_owed1':int(native['protocol_fee_owed_b']),'fee_growth_global0':native['fee_growth_global_a'],'fee_growth_global1':native['fee_growth_global_b'],'reward_last_updated_timestamp':native['reward_last_updated_timestamp'],'max_steps':proof['max_steps'],'tick_arrays':[]}
 for a in native['tick_arrays']:
  ticks=[]
  for t in a['ticks']:
   if t['initialized']:ticks.append({'tick':t['tick'],'liquidity_gross':t['liquidity_gross'],'liquidity_net':t['liquidity_net'],'fee_growth_outside0':t['fee_growth_outside_a'],'fee_growth_outside1':t['fee_growth_outside_b'],'reward_growths_outside':t['reward_growths_outside']})
  s['tick_arrays'].append({'address':a['address'],'start_tick_index':a['start_tick_index'],'layout':a['layout'],'initialized_bitmap':a['initialized_bitmap'],'ticks':ticks})
 return {'chain_key':'solana:localnet','pool_id':config['id'],'protocol':PROFILE,'token0':config['token0'],'token1':config['token1'],'reserve0':'0','reserve1':'0','fee_numerator':s['fee_rate'],'fee_denominator':1000000,'adapter_version':ADAPTER,'token_behavior':'standard','orca_whirlpool':s}
def generate():
 source=ROOT/'integration/test/contracts/orca-adaptive-rewards-service.json';d=corpus();cases=[]
 for scenario in d['scenarios']:
  for index,bank in enumerate(scenario['banks']):
   t=bank['next_trade'];e=t['expected'];cases.append({'name':scenario['name']+'/'+str(index),'config':clean_config(scenario['config']),'pool':pool(scenario['config'],bank['native_state']),'params':t['params'],'expected':{'status':e['status'],**({'amount_in':e['amount_in'],'amount_out':e['amount_out'],'amount_remaining':e['amount_remaining'],'post_pool':pool(scenario['config'],e['next_state'])}if e['status']=='fulfilled'else{'error':e['error']})}})
 data=json.dumps({'source':'PINNED_ORIGINAL_PACK_AND_NATIVE_SWAP_REPLAY','source_corpus_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'cases':cases},indent=2)+'\n'
 for service in ['integration','chain-parser-rust','dex-router']:(ROOT/service/'test/contracts/orca-adaptive-rewards-pools.json').write_text(data)
 print('Projected complete adaptive pool cases',len(cases))
if __name__=='__main__':generate()
