#!/usr/bin/env python3
"""Project original-exchange observations into fixed-block service fixtures.

The RPC envelopes/addresses are local synthetic replay; expected amounts and all
pricing/LP/raw poststate come from unchanged original pool and LP contracts.
"""
from pathlib import Path
import copy,hashlib,json
ROOT=Path(__file__).resolve().parents[3]
LP='d3f43d24527d6a8ed0fa9f24d110ad7cc9325f40da335ed327e8e0e002a8694d'
PROTOCOL='curve-crypto2-full';ADAPTER=PROTOCOL+'/d7d04cd9-standard-1'
def address(i):return '0x'+f'{i:040x}'
def digest(b):return hashlib.sha256(b).hexdigest()
def make_pool(c,template):
 tokens=[address(1),address(2)];lp=address(99);pool_id=address(50);receiver=address(98)if c['admin_receiver_enabled']else address(0)
 tail=b''.join(int(x,16).to_bytes(32,'big')for x in [lp,*tokens])+b''.join(int(x).to_bytes(32,'big')for x in c['precisions'])
 code=digest(bytes.fromhex(template['runtime_template_hex'])+tail);params=c['params'];state=c['state']
 crypto={k:params[k]for k in ['gamma','out_fee','fee_gamma','allowed_extra_profit','adjustment_step']};crypto.update({k:state[k]for k in ['price_scale','last_prices','xcp_profit','xcp_profit_a','virtual_price']});crypto.update(invariant=state['D'],price_oracle=state['_price_oracle'],ma_half_time=int(params['ma_half_time']),last_prices_timestamp=int(state['last_prices_timestamp']),not_adjusted=state['not_adjusted']=='1',future_a_gamma_time=int(state['future_A_gamma_time']),lp_token=lp,lp_token_code_sha256=LP,total_supply=c['total_supply'],admin_receiver=receiver,admin_receiver_balance=c['receiver_balance'],raw_balances=c['raw_balances'],timestamp=c['timestamp'],block_number=100,block_hash='0x'+'a'*64)
 assets=[{'token':t,'balance':c['balances'][i],'scaling':str(int(c['precisions'][i])*10**18),'code_sha256':digest(bytes.fromhex('600255'))}for i,t in enumerate(tokens)]
 s={'code_sha256':code,'vault':'','vault_code_sha256':'','pool_id':'','assets':assets,'weights':[],'amplification':params['amp'],'admin_fee':int(params['admin_fee']),'curve_crypto':crypto}
 return {'chain_key':'evm:31337','pool_id':pool_id,'protocol':PROTOCOL,'token0':tokens[0],'token1':tokens[1],'reserve0':c['balances'][0],'reserve1':c['balances'][1],'fee_numerator':int(params['mid_fee']),'fee_denominator':10**10,'adapter_version':ADAPTER,'token_behavior':'standard','multi_asset':s}
def generate():
 corpus=json.loads((ROOT/'integration/test/contracts/curve-crypto-full.json').read_text());template=json.loads((ROOT/'integration/test/contracts/curve-crypto-runtime-template.json').read_text());cases=[]
 for c in corpus['cases']:
  if c['operation']!='exchange' or int(c['state']['future_A_gamma_time'])>1 or int(c['state']['last_prices_timestamp'])>c['timestamp']:continue
  p=make_pool(c,template);s=p['multi_asset'];config={'program_id':'','accounts':[],'id':p['pool_id'],'protocol':p['protocol'],'token0':p['token0'],'token1':p['token1'],'fee_numerator':p['fee_numerator'],'fee_denominator':p['fee_denominator'],'multi_asset':{'expected_code_sha256':s['code_sha256'],'expected_lp_token':s['curve_crypto']['lp_token'],'expected_lp_token_code_sha256':LP,'token_policy':'standard-no-bpt','tokens':[{'token':a['token'],'expected_code_sha256':a['code_sha256']}for a in s['assets']]}}
  cases.append({'name':c['name'],'pool':p,'config':config,'input':c['input'],'index_in':c['index_in'],'index_out':c['index_out'],'expected':copy.deepcopy(c['expected'])})
 doc={'source':'LOCAL_RPC_REPLAY_OF_PINNED_ORIGINAL_EXCHANGE_AND_LP','source_corpus_sha256':digest((ROOT/'integration/test/contracts/curve-crypto-full.json').read_bytes()),'profile':ADAPTER,'scope':'standard nominal transfers; settled A/gamma; INDICATIVE only','cases':cases}
 data=json.dumps(doc,indent=2)+'\n'
 for service in ['integration','chain-parser-go','chain-parser-rust','dex-router']:(ROOT/service/'test/contracts/curve-crypto-service.json').write_text(data)
 print('Crypto service replay cases',len(cases))
if __name__=='__main__':generate()
