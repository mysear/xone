#!/usr/bin/env python3
"""Independent protoc serialization for Orca static Q64 amount state, field25."""
from pathlib import Path
import json,hashlib,subprocess,math
ROOT=Path(__file__).resolve().parents[3];alphabet='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
def b58(s):
 n=0
 for c in s:n=n*58+alphabet.index(c)
 out=b'\0'*(len(s)-len(s.lstrip('1')))+n.to_bytes((n.bit_length()+7)//8,'big');assert len(out)==32;return out
p=json.loads((ROOT/'integration/test/contracts/orca-state.json').read_text());snap=json.loads((ROOT/'integration/test/solana/baseline.json').read_text());snap['pools']=[p];s=p['orca_whirlpool'];snap['reference']['block'].update(number_or_slot=s['bank_slot'],timestamp_ms=s['bank_unix_timestamp']*1000)
for k,f in [('scope_hash','pool_id'),('adapter_set_hash','adapter_version')]:snap['reference'][k]=hashlib.sha256(json.dumps([p[f]],separators=(',',':')).encode()).hexdigest()
snap['source']='PINNED_ORCA_STATIC_NUMERICAL_FIXTURE'
def blob(b):return '"'+''.join('\\x%02x'%v for v in b)+'"'
def encode(obj):
 out=''
 for k,v in obj.items():
  for x in v if isinstance(v,list) else[v]:
   if isinstance(x,dict):out+=k+' { '+encode(x)+' } '
   elif k in ['reserve0','reserve1','sqrt_price_x64','liquidity','fee_growth_global0','fee_growth_global1','initialized_bitmap','liquidity_gross','fee_growth_outside0','fee_growth_outside1']:out+=k+' { uint256_be: '+blob(int(x).to_bytes(32,'big'))+' } '
   elif k=='liquidity_net':out+='liquidity_net_i128_be: '+blob(int(x).to_bytes(16,'big',signed=True))+' '
   elif k in ['hash','parent_hash']:out+=k+': '+blob(b58(x))+' '
   elif k in ['scope_hash','adapter_set_hash','program_code_sha256','token_program_code_sha256']:out+=k+': '+blob(bytes.fromhex(x))+' '
   else:out+=k+': '+(json.dumps(x)if isinstance(x,(str,bool))else str(x))+' '
 return out
raw=subprocess.run(['protoc','--proto_path='+str(ROOT/'common/protoc/proto'),'--encode=dex.v1.JournalUnit',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=('kind:"ONLINE_BASELINE_COMMIT" snapshot { '+encode(snap)+' }').encode(),capture_output=True,check=True).stdout
ref=snap['reference'];base={'schema_version':1,'unit_id':f"solana:localnet/{ref['owner_assignment_version']}/{ref['writer_epoch']}/{ref['journal_seq']}",'kind':'BEGIN','reference':ref,'payload_sha256':hashlib.sha256(raw).hexdigest(),'payload_bytes':len(raw),'chunk_count':math.ceil(len(raw)/1024),'chunk_index':0,'data_hex':''};frames=[base.copy()]
for i in range(base['chunk_count']):f=base.copy();f.update(kind='CHUNK',chunk_index=i,data_hex=raw[i*1024:(i+1)*1024].hex());frames.append(f)
f=base.copy();f['kind']='COMMIT';frames.append(f);text=json.dumps(frames,indent=2)+'\n'
for service in ['integration','chain-parser-go','chain-parser-rust','dex-router']:(ROOT/service/'test/contracts/orca-frames.json').write_text(text)
print(json.dumps({'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'protocol':p['protocol']}))
