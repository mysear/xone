#!/usr/bin/env python3
"""Independent protoc field28/29 and Solana rewards roundtrip fixtures."""
from pathlib import Path
import copy,hashlib,json,subprocess,sys
ROOT=Path(__file__).resolve().parents[3];sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from multi_model_smoke import catalog

ALPHABET='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
def b58(b):
 n=int.from_bytes(b,'big');out=''
 while n:out=ALPHABET[n%58]+out;n//=58
 return '1'*(len(b)-len(b.lstrip(b'\0')))+out
def digest_bytes(k,v):
 if k in ['hash','parent_hash'] and chain=='solana':
  n=0
  for c in v:n=n*58+ALPHABET.index(c)
  return n.to_bytes(32,'big')
 return bytes.fromhex(v.removeprefix('0x'))
def amount(n):return{'uint256_be':bytes(int(n).to_bytes(32,'big'))}
def blob(b):return'"'+''.join('\\x%02x'%v for v in b)+'"'
AMOUNTS={'reserve0','reserve1','balance','scaling','weights','amplification','reward_growths_outside','emissions_per_second_x64','growth_global_x64','reward_growth_global_x64','sqrt_price_x64','sqrt_price_x96','liquidity','liquidity_gross','fee_growth_global0','fee_growth_global1','fee_growth_outside0','fee_growth_outside1','initialized_bitmap','bitmap'}
DIGESTS={'hash','parent_hash','scope_hash','adapter_set_hash','code_sha256','manager_code_sha256','program_code_sha256','token_program_code_sha256','vault_code_sha256','implementation_code_sha256','snapshot_block_hash','parameters'}
def encode(obj,level=''):
 out=''
 for k,v in obj.items():
  if k=='liquidity_net':out+='liquidity_net_i128_be: '+blob(int(v).to_bytes(16,'big',signed=True))+' ';continue
  if k=='extension':out+='extension: '+blob(bytes(v))+' ';continue
  for value in v if isinstance(v,list)else[v]:
   if isinstance(value,dict):out+=k+' { '+encode(value,k)+' } '
   elif k in AMOUNTS and not(k=='bitmap'and level=='raydium_clmm'):out+=k+' { uint256_be: '+blob(int(value).to_bytes(32,'big'))+' } '
   elif k in DIGESTS or k=='pool_id'and level in['snapshot','multi_asset']:out+=k+': '+blob(digest_bytes(k,value))+' '
   else:out+=k+': '+(json.dumps(value)if isinstance(value,str)else str(value).lower())+' '
 return out
# Fix the source string builder above before executing, independently of services.
def snapshot(chain,pools):
 s=json.loads((ROOT/f'integration/test/{chain}/baseline.json').read_text());s['pools']=pools;s['source']='FIVE_FAMILY_SYNTHETIC_SOURCE';r=s['reference'];
 if chain=='solana':r['block']['number_or_slot']=123456;r['block']['timestamp_ms']=pools[0].get('orca_whirlpool',pools[0].get('raydium_clmm',{})).get('bank_unix_timestamp',0)*1000;r['block']['hash']=b58(bytes.fromhex('ab'*32));r['block']['parent_hash']=b58(bytes.fromhex('cd'*32))
 r['scope_hash']=hashlib.sha256(json.dumps(sorted(p['pool_id']for p in pools),separators=(',',':')).encode()).hexdigest();r['adapter_set_hash']=hashlib.sha256(json.dumps(sorted({p['adapter_version']for p in pools}),separators=(',',':')).encode()).hexdigest();return s
pools=[i['pool']for i in catalog()]
for case in json.loads((ROOT/'integration/test/contracts/singleton-advanced-quotes.json').read_text())['cases']:
 s=case['snapshot']
 if any(p['protocol']==s['protocol']+'-advanced'for p in pools):continue
 pools.append({'chain_key':'evm:31337','pool_id':s['snapshot']['manager']+'/'+s['snapshot']['pool_id'],'protocol':s['protocol']+'-advanced','token0':s['currency0'],'token1':s['currency1'],'reserve0':'0','reserve1':'0','fee_numerator':s['snapshot']['lp_fee'],'fee_denominator':1000000,'adapter_version':'uniswap-v4/core-advanced-1'if s['protocol']=='uniswap-v4'else'pancake-infinity-cl/cl-advanced-1','token_behavior':'standard','singleton_advanced':s})
 if len(pools)==8:break
solana=[json.loads((ROOT/'integration/test/contracts/solana-advanced'/name).read_text())['fixture']['pool']for name in['orca-rewards.json','raydium-clmm-rewards.json']]
for label,chain,states in [('evm','evm',pools),('solana','solana',[solana[0]]),('rayclmm','solana',[solana[1]])]:
 s=snapshot(chain,states)
 if chain=='evm':s['reference']['block']['hash']=next(p['singleton_advanced']['snapshot_block_hash']for p in states if 'singleton_advanced'in p)
 text=json.dumps(s,indent=2)+'\n';encoded=subprocess.run(['protoc','-I',str(ROOT/'common/protoc/proto'),'--encode=dex.v1.OnlineSnapshot',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=encode(s).encode(),capture_output=True,check=True).stdout
 fixture={'snapshot':s,'protobuf_hex':encoded.hex(),'snapshot_sha256':hashlib.sha256(json.dumps(s,sort_keys=True,separators=(',',':')).encode()).hexdigest(),'protobuf_sha256':hashlib.sha256(encoded).hexdigest()};text=json.dumps(fixture,indent=2)+'\n'
 for service in['integration','chain-parser-go','chain-parser-rust','dex-router']:(ROOT/service/'test/contracts'/('five-family-'+label+'-wire.json')).write_text(text)
 print(chain,'pools',len(states),'bytes',len(encoded))
