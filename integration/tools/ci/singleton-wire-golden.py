#!/usr/bin/env python3
"""Independent protoc encoding of singleton key/manager and concentrated state."""
from pathlib import Path
import json,hashlib,subprocess,math
ROOT=Path(__file__).resolve().parents[3]
snapshot=json.loads((ROOT/'integration/test/evm/baseline.json').read_text());snapshot['pools']=json.loads((ROOT/'integration/test/contracts/singleton-state.json').read_text())['pools'];snapshot['source']='PINNED_SINGLETON_PROTOCOL_FIXTURE'
for key,field in [('scope_hash','pool_id'),('adapter_set_hash','adapter_version')]:snapshot['reference'][key]=hashlib.sha256(json.dumps(sorted(p[field] for p in snapshot['pools']),separators=(',',':')).encode()).hexdigest()
def blob(b):return '"'+''.join('\\x%02x'%v for v in b)+'"'
def encode(obj,level=''):
 out=''
 for k,v in obj.items():
  for x in v if isinstance(v,list) else [v]:
   if isinstance(x,dict):out+=k+' { '+encode(x,k)+' } '
   elif k in ['reserve0','reserve1','sqrt_price_x96','liquidity','bitmap','liquidity_gross']:out+=k+' { uint256_be: '+blob(int(x).to_bytes(32,'big'))+' } '
   elif k=='liquidity_net':out+='liquidity_net_i128_be: '+blob(int(x).to_bytes(16,'big',signed=True))+' '
   elif k in ['hash','parent_hash','scope_hash','adapter_set_hash','manager_code_sha256','parameters'] or k=='pool_id' and level=='singleton_cl':out+=k+': '+blob(bytes.fromhex(x.removeprefix('0x')))+' '
   else:out+=k+': '+(json.dumps(x) if isinstance(x,str) else str(x))+' '
 return out
raw=subprocess.run(['protoc','--proto_path='+str(ROOT/'common/protoc/proto'),'--encode=dex.v1.JournalUnit',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=('kind:"ONLINE_BASELINE_COMMIT" snapshot { '+encode(snapshot)+' }').encode(),capture_output=True,check=True).stdout
r=snapshot['reference'];base={'schema_version':1,'unit_id':f"evm:31337/{r['owner_assignment_version']}/{r['writer_epoch']}/{r['journal_seq']}",'kind':'BEGIN','reference':r,'payload_sha256':hashlib.sha256(raw).hexdigest(),'payload_bytes':len(raw),'chunk_count':math.ceil(len(raw)/1024),'chunk_index':0,'data_hex':''};frames=[base.copy()]
for i in range(base['chunk_count']):f=base.copy();f.update(kind='CHUNK',chunk_index=i,data_hex=raw[i*1024:(i+1)*1024].hex());frames.append(f)
f=base.copy();f['kind']='COMMIT';frames.append(f);text=json.dumps(frames,indent=2)+'\n'
for p in ['integration','chain-parser-go','chain-parser-rust','dex-router']:(ROOT/p/'test/contracts/singleton-frames.json').write_text(text)
print(json.dumps({'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'protocols':[p['protocol'] for p in snapshot['pools']]}))
