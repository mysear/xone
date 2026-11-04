#!/usr/bin/env python3
"""Independent protoc encoding of populated V3 bitmap/tick wire proof."""
from pathlib import Path
import hashlib,json,math,subprocess
ROOT=Path(__file__).resolve().parents[3]
pool=json.loads((ROOT/'integration/test/contracts/v3-state.json').read_text())
snapshot=json.loads((ROOT/'integration/test/evm/baseline.json').read_text())
snapshot['pools']=[pool]
snapshot['reference']['scope_hash']=hashlib.sha256(json.dumps([pool['pool_id']],separators=(',',':')).encode()).hexdigest()
snapshot['reference']['adapter_set_hash']=hashlib.sha256(b'["uniswap-v3/core-1"]').hexdigest()
def blob(b):return '"'+''.join('\\x%02x'%v for v in b)+'"'
def string(s):return json.dumps(s)
def amount(name,value):return name+' { uint256_be: '+blob(int(value).to_bytes(32,'big'))+' } '
r=snapshot['reference'];b=r['block']
text='kind: "ONLINE_BASELINE_COMMIT" snapshot { reference { block { chain_key: '+string(b['chain_key'])+' number_or_slot: '+str(b['number_or_slot'])+' hash: '+blob(bytes.fromhex(b['hash'][2:]))+' parent_hash: '+blob(bytes.fromhex(b['parent_hash'][2:]))+' timestamp_ms: '+str(b['timestamp_ms'])+' } '
for k,v in r.items():
 if k=='block':continue
 text+=k+': '+(blob(bytes.fromhex(v)) if k in ['scope_hash','adapter_set_hash'] else string(v) if isinstance(v,str) else str(v))+' '
text+='} pools { '
for k,v in pool.items():
 if k=='v3':continue
 text+=amount(k,v) if k in ['reserve0','reserve1'] else k+': '+(string(v) if isinstance(v,str) else str(v))+' '
v=pool['v3'];text+='v3 { '
for k,value in v.items():
 if k in ['tick_words','ticks']:continue
 if k in ['sqrt_price_x96','liquidity']:text+=amount(k,value)
 else:text+=k+': '+(blob(bytes.fromhex(value)) if k=='code_sha256' else string(value) if isinstance(value,str) else str(value))+' '
for word in v['tick_words']:text+='tick_words { word_pos: '+str(word['word_pos'])+' '+amount('bitmap',word['bitmap'])+'} '
for tick in v['ticks']:text+='ticks { tick: '+str(tick['tick'])+' '+amount('liquidity_gross',tick['liquidity_gross'])+'liquidity_net_i128_be: '+blob(int(tick['liquidity_net']).to_bytes(16,'big',signed=True))+' } '
text+='} } source: "V3_PROTOC_FIXTURE" }'
raw=subprocess.run(['protoc','--proto_path='+str(ROOT/'common/protoc/proto'),'--encode=dex.v1.JournalUnit',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=text.encode(),capture_output=True,check=True).stdout
base={'schema_version':1,'unit_id':f"{b['chain_key']}/{r['owner_assignment_version']}/{r['writer_epoch']}/{r['journal_seq']}",'kind':'BEGIN','reference':r,'payload_sha256':hashlib.sha256(raw).hexdigest(),'payload_bytes':len(raw),'chunk_count':math.ceil(len(raw)/1024),'chunk_index':0,'data_hex':''};frames=[base.copy()]
for i in range(base['chunk_count']):
 f=base.copy();f.update(kind='CHUNK',chunk_index=i,data_hex=raw[i*1024:(i+1)*1024].hex());frames.append(f)
f=base.copy();f['kind']='COMMIT';frames.append(f)
value=json.dumps(frames,indent=2)+'\n'
for project in ['integration','chain-parser-go','chain-parser-rust','dex-router']:(ROOT/project/'test/contracts/v3-journal-frames.json').write_text(value)
print(json.dumps({'encoding':'protoc V3State + JournalUnit','bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}))
