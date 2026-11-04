#!/usr/bin/env python3
"""Independent protoc bytes for the versioned complete adaptive Oracle state."""
from pathlib import Path
import hashlib,json,subprocess
ROOT=Path(__file__).resolve().parents[3]
AMOUNTS={'reserve0','reserve1','sqrt_price_x64','liquidity','fee_growth_global0','fee_growth_global1','initialized_bitmap','liquidity_gross','fee_growth_outside0','fee_growth_outside1','emissions_per_second_x64','growth_global_x64','reward_growths_outside'}
BYTES={'extension','constants_reserved','variables_reserved','reserved'}
DIGESTS={'scope_hash','adapter_set_hash','program_code_sha256','token_program_code_sha256'}
ALPHABET='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
def pubkey(text):
 n=0
 for c in text:n=n*58+ALPHABET.index(c)
 raw=b'\0'*(len(text)-len(text.lstrip('1')))+n.to_bytes((n.bit_length()+7)//8,'big')
 if len(raw)!=32:raise ValueError('public key width')
 return raw
def encode(obj):
 def blob(b):return '"'+''.join('\\x%02x'%v for v in b)+'"'
 out=''
 for key,value in obj.items():
  if value is None:continue
  if key in BYTES:out+=key+': '+blob(bytes(value))+' ';continue
  for item in value if isinstance(value,list)else[value]:
   if isinstance(item,dict):out+=key+' { '+encode(item)+' } '
   elif key in AMOUNTS:out+=key+' { uint256_be: '+blob(int(item).to_bytes(32,'big'))+' } '
   elif key=='liquidity_net':out+='liquidity_net_i128_be: '+blob(int(item).to_bytes(16,'big',signed=True))+' '
   elif key in ['hash','parent_hash']:out+=key+': '+blob(pubkey(item))+' '
   elif key in DIGESTS:out+=key+': '+blob(bytes.fromhex(item))+' '
   else:out+=key+': '+(json.dumps(item)if isinstance(item,(str,bool))else str(item))+' '
 return out

def generate():
 p=json.loads((ROOT/'integration/test/contracts/orca-state.json').read_text());p.update(protocol='orca-whirlpools-adaptive',adapter_version='orca-whirlpools/cc683ec9-spl-adaptive-1');s=p['orca_whirlpool'];s['fee_tier_index']=65;s['tick_arrays']=s['tick_arrays'][:3]
 s['reward_infos']=[{'mint':'1'*32,'vault':'1'*32,'extension':[3 if i==0 else 0]*32,'emissions_per_second_x64':'0','growth_global_x64':'0'}for i in range(3)]
 for a in s['tick_arrays']:
  a['layout']='fixed'
  for t in a['ticks']:t['reward_growths_outside']=['0']*3
 s['adaptive']={'whirlpool':p['pool_id'],'trade_enable_timestamp':0,'filter_period':10,'decay_period':100,'reduction_factor':5000,'adaptive_fee_control_factor':1000,'max_volatility_accumulator':100000,'tick_group_size':8,'major_swap_threshold_ticks':64,'constants_reserved':[3]*16,'last_reference_update_timestamp':s['bank_unix_timestamp'],'last_major_swap_timestamp':s['bank_unix_timestamp'],'volatility_reference':20000,'tick_group_index_reference':0,'volatility_accumulator':40000,'variables_reserved':[5]*16,'reserved':[7]*128}
 snap=json.loads((ROOT/'integration/test/solana/baseline.json').read_text());snap['pools']=[p];snap['source']='SYNTHETIC_ORCA_ADAPTIVE_COMPLETE_WIRE';ref=snap['reference'];ref['block'].update(number_or_slot=s['bank_slot'],timestamp_ms=s['bank_unix_timestamp']*1000)
 for k,f in [('scope_hash','pool_id'),('adapter_set_hash','adapter_version')]:ref[k]=hashlib.sha256(json.dumps([p[f]],separators=(',',':')).encode()).hexdigest()
 raw=subprocess.run(['protoc','--proto_path='+str(ROOT/'common/protoc/proto'),'--encode=dex.v1.OnlineSnapshot',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=encode(snap).encode(),capture_output=True,check=True).stdout
 text=json.dumps({'snapshot':snap,'protobuf_hex':raw.hex(),'protobuf_sha256':hashlib.sha256(raw).hexdigest()},indent=2)+'\n'
 for service in ['integration','chain-parser-go','chain-parser-rust','dex-router']:(ROOT/service/'test/contracts/orca-adaptive-wire.json').write_text(text)
 print('Independent adaptive Oracle wire bytes',len(raw))
if __name__=='__main__':generate()
