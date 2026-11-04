"""Project original manager getters/tree banks into the versioned service model."""
from pathlib import Path
import hashlib,json
ROOT=Path(__file__).resolve().parents[3]
PROFILE='pancake-infinity-bin';ADAPTER='pancake-infinity-bin/3c5ef1be-static-1'
def digest(raw):return hashlib.sha256(raw).hexdigest()
def corpus():return json.loads((ROOT/'integration/test/contracts/infinity-bin-service.json').read_text())
def value(x):return int(x,16)if isinstance(x,str)and x.startswith('0x')else int(x)
def project(d,bank,index):
 definition=d['pools'][index];key=definition['key'];p=bank['pools'][index];manager,vault=d['manager'],d['vault'];t0,t1=key['currency0'],key['currency1'];codehash=lambda token:digest(bytes.fromhex(bank['code'][token][2:]));params=key['parameters'];s={'manager':manager,'manager_code_sha256':codehash(manager),'vault':vault,'vault_code_sha256':codehash(vault),'pool_id':p['pool_id'],'hooks':key['hooks'],'parameters':params,'token0_code_sha256':codehash(t0),'token1_code_sha256':codehash(t1),'bin_step':definition['bin_step'],'active_id':p['active_id'],'lp_fee':p['lp_fee'],'protocol_fee0':p['protocol_fee']&4095,'protocol_fee1':p['protocol_fee']>>12,'max_bins':64,'max_tree_words':64,'max_steps':64,'tree_level0':str(value(p['tree']['level0'])),'tree_level1':[{'index':value(i),'value':str(value(v))}for i,v in sorted(p['tree']['level1'].items(),key=lambda e:value(e[0]))],'tree_level2':[{'index':value(i),'value':str(value(v))}for i,v in sorted(p['tree']['level2'].items(),key=lambda e:value(e[0]))],'bins':[{'id':value(i),'reserve0':b['reserve0'],'reserve1':b['reserve1'],'total_shares':b['total_shares']}for i,b in sorted(p['bins'].items(),key=lambda e:value(e[0]))],'app_reserve0':bank['app_reserves'][t0],'app_reserve1':bank['app_reserves'][t1],'protocol_fees0':bank['protocol_fees'][t0],'protocol_fees1':bank['protocol_fees'][t1],'block_number':bank['block']['num'],'timestamp':bank['block']['timestamp'],'block_hash':bank['block']['hash']}
 pool={'chain_key':'evm:31337','pool_id':manager+'/'+p['pool_id'],'protocol':PROFILE,'token0':t0,'token1':t1,'reserve0':'0','reserve1':'0','fee_numerator':s['lp_fee'],'fee_denominator':1000000,'adapter_version':ADAPTER,'token_behavior':'standard','infinity_bin':s}
 config={'id':pool['pool_id'],'protocol':PROFILE,'token0':t0,'token1':t1,'fee_numerator':s['lp_fee'],'fee_denominator':1000000,'program_id':'','accounts':[],'infinity_bin':{'manager':manager,'expected_manager_code_sha256':s['manager_code_sha256'],'vault':vault,'expected_vault_code_sha256':s['vault_code_sha256'],'pool_id':p['pool_id'],'bin_step':s['bin_step'],'lp_fee':s['lp_fee'],'expected_token0_code_sha256':s['token0_code_sha256'],'expected_token1_code_sha256':s['token1_code_sha256'],'token_policy':'standard','max_bins':64,'max_tree_words':64,'max_steps':64}}
 return {'pool':pool,'config':config}
def generate():
 d=corpus();banks=[]
 for bank in d['RPCbanks']:banks.append({'id':bank['id'],'block':bank['block'],'items':[project(d,bank,i)for i in range(len(d['pools']))]})
 data=json.dumps({'source':'SYNTHETIC_ORIGINAL_BIN_MANAGER_GETTERS_AND_BINPOOL_SWAPS','source_corpus_sha256':digest((ROOT/'integration/test/contracts/infinity-bin-service.json').read_bytes()),'banks':banks,'trades':d['trades']},indent=2)+'\n'
 for service in ['integration','chain-parser-go','chain-parser-rust','dex-router']:(ROOT/service/'test/contracts/infinity-bin-pools.json').write_text(data)
 print('Projected Bin source banks',len(banks))
if __name__=='__main__':generate()
