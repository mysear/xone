#!/usr/bin/env python3
"""Independent protoc golden containing all supported EVM DEX profiles."""
from pathlib import Path
import hashlib,json,math,subprocess,sys
ROOT=Path(__file__).resolve().parents[3];sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from multidex_smoke import NAMES,profile,BASE
snapshot=json.loads((ROOT/'integration/test/evm/baseline.json').read_text());pools=[]
for name in NAMES:
 cfg,_=profile(name);p={k:cfg[k] for k in ['id','protocol','token0','token1','fee_numerator','fee_denominator']};p['pool_id']=p.pop('id');p.update(chain_key='evm:31337',token_behavior='standard',adapter_version='uniswap-v2/1' if name=='uniswap-v2' else 'uniswap-v3/core-1' if name=='uniswap-v3' else name+'/core-1')
 clmm=name.endswith('v3');p.update(reserve0='0' if clmm else '1000000',reserve1='0' if clmm else '1000000')
 if clmm:
  p['v3']=dict(BASE['v3'],factory=cfg['v3']['expected_factory'],code_sha256=cfg['v3']['expected_code_sha256'],fee_protocol=209718400 if name=='pancake-v3' else 0)
 else:p['v2']={'factory':cfg['v2']['expected_factory'],'code_sha256':cfg['v2']['expected_code_sha256']}
 pools.append(p)
snapshot['pools']=pools;snapshot['reference']['scope_hash']=hashlib.sha256(json.dumps(sorted(p['pool_id'] for p in pools),separators=(',',':')).encode()).hexdigest();snapshot['reference']['adapter_set_hash']=hashlib.sha256(json.dumps(sorted(p['adapter_version'] for p in pools),separators=(',',':')).encode()).hexdigest()
def blob(raw):return '"'+''.join('\\x%02x'%x for x in raw)+'"'
def text(obj):
 out=''
 for key,value in obj.items():
  values=value if isinstance(value,list) else [value]
  for v in values:
   if isinstance(v,dict):out+=key+' { '+text(v)+' } '
   elif key in ['reserve0','reserve1','sqrt_price_x96','liquidity','bitmap','liquidity_gross']:out+=key+' { uint256_be: '+blob(int(v).to_bytes(32,'big'))+' } '
   elif key=='liquidity_net':out+='liquidity_net_i128_be: '+blob(int(v).to_bytes(16,'big',signed=True))+' '
   elif key in ['hash','parent_hash','scope_hash','adapter_set_hash','code_sha256']:out+=key+': '+blob(bytes.fromhex(v.removeprefix('0x')))+' '
   else:out+=key+': '+(json.dumps(v) if isinstance(v,str) else str(v))+' '
 return out
raw=subprocess.run(['protoc','--proto_path='+str(ROOT/'common/protoc/proto'),'--encode=dex.v1.JournalUnit',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=('kind:"ONLINE_BASELINE_COMMIT" snapshot { '+text(snapshot)+' }').encode(),capture_output=True,check=True).stdout
r=snapshot['reference'];base={'schema_version':1,'unit_id':f"evm:31337/{r['owner_assignment_version']}/{r['writer_epoch']}/{r['journal_seq']}",'kind':'BEGIN','reference':r,'payload_sha256':hashlib.sha256(raw).hexdigest(),'payload_bytes':len(raw),'chunk_count':math.ceil(len(raw)/1024),'chunk_index':0,'data_hex':''};frames=[base.copy()]
for i in range(base['chunk_count']):f=base.copy();f.update(kind='CHUNK',chunk_index=i,data_hex=raw[i*1024:(i+1)*1024].hex());frames.append(f)
f=base.copy();f['kind']='COMMIT';frames.append(f);value=json.dumps(frames,indent=2)+'\n'
for project in ['integration','chain-parser-go','chain-parser-rust','dex-router']:(ROOT/project/'test/contracts/dex-version-frames.json').write_text(value)
print(json.dumps({'protocols':NAMES,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}))
