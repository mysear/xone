#!/usr/bin/env python3
"""Independent protoc encoding of a fixed fixture; no quote/math implementation is used."""
from pathlib import Path
import hashlib,json,math,subprocess
ROOT=Path(__file__).resolve().parents[3]
snapshot=json.loads((ROOT/'integration/test/evm/baseline.json').read_text())
def string(v):return json.dumps(v)
def blob(raw):return '"'+''.join('\\x%02x'%v for v in raw)+'"'
def reference(r):
 b=r['block'];text='block { chain_key: '+string(b['chain_key'])+' number_or_slot: '+str(b['number_or_slot'])+' hash: '+blob(bytes.fromhex(b['hash'][2:]))+' parent_hash: '+blob(bytes.fromhex(b['parent_hash'][2:]))+' timestamp_ms: '+str(b['timestamp_ms'])+' }\n'
 for k,v in r.items():
  if k=='block':continue
  text+=k+': '+(blob(bytes.fromhex(v)) if k in ['adapter_set_hash','scope_hash'] else string(v) if isinstance(v,str) else str(v))+'\n'
 return text
text='kind: "ONLINE_BASELINE_COMMIT"\nsnapshot {\nreference {\n'+reference(snapshot['reference'])+'}\n'
for pool in snapshot['pools']:
 text+='pools {\n'
 for k,v in pool.items():
  if k in ['reserve0','reserve1']:text+=k+' { uint256_be: '+blob(int(v).to_bytes(32,'big'))+' }\n'
  else:text+=k+': '+(string(v) if isinstance(v,str) else str(v))+'\n'
 text+='}\n'
text+='source: '+string(snapshot['source'])+'\n'
for gap in snapshot['history_gaps']:text+='history_gaps { '+' '.join(k+': '+(string(v) if isinstance(v,str) else str(v)) for k,v in gap.items())+' }\n'
text+='}\n'
raw=subprocess.run(['protoc','--proto_path='+str(ROOT/'common/protoc/proto'),'--encode=dex.v1.JournalUnit',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=text.encode(),capture_output=True,check=True).stdout
r=snapshot['reference'];base={'schema_version':1,'unit_id':f"{r['block']['chain_key']}/{r['owner_assignment_version']}/{r['writer_epoch']}/{r['journal_seq']}",'kind':'BEGIN','reference':r,'payload_sha256':hashlib.sha256(raw).hexdigest(),'payload_bytes':len(raw),'chunk_count':math.ceil(len(raw)/1024),'chunk_index':0,'data_hex':''};frames=[base.copy()]
for i in range(math.ceil(len(raw)/1024)):
 f=base.copy();f.update(kind='CHUNK',chunk_index=i,data_hex=raw[i*1024:(i+1)*1024].hex());frames.append(f)
f=base.copy();f['kind']='COMMIT';frames.append(f)
value=json.dumps(frames,indent=2)+'\n'
(ROOT/'integration/test/contracts/journal-frames.json').write_text(value)
for n in ['chain-parser-go','chain-parser-rust','dex-router']:(ROOT/n/'test/contracts/journal-frames.json').write_text(value)
print(json.dumps({'golden_encoding':'protoc JournalUnit protobuf','bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()}))
# Independently encoded populated normalized-facts extension, including uint256 max.
b=snapshot['reference']['block'];p=snapshot['pools'][0]
block_text='chain_key: '+string(b['chain_key'])+' number_or_slot: '+str(b['number_or_slot'])+' hash: '+blob(bytes.fromhex(b['hash'][2:]))+' parent_hash: '+blob(bytes.fromhex(b['parent_hash'][2:]))+' timestamp_ms: '+str(b['timestamp_ms'])
extra='normalized_facts { schema_version: 1 decoder_version: "uniswap-v2/1" blocks { block { '+block_text+' } swaps { '
extra+='event_id: '+string(f"{b['chain_key']}/{b['hash']}/0/1")+' tx_hash: '+blob(bytes([0xaa])*32)+' tx_index: 0 log_index: 1 '
extra+='pool_id: '+string(p['pool_id'])+' token0: '+string(p['token0'])+' token1: '+string(p['token1'])+' '
for key,amount in [('amount0_in',(1<<256)-1),('amount1_in',0),('amount0_out',0),('amount1_out',20)]:extra+=key+' { uint256_be: '+blob(amount.to_bytes(32,'big'))+' } '
extra+='} } }'
fact_raw=subprocess.run(['protoc','--proto_path='+str(ROOT/'common/protoc/proto'),'--encode=dex.v1.JournalUnit',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=(text.replace('ONLINE_BASELINE_COMMIT','BLOCK_COMMIT',1)+extra).encode(),capture_output=True,check=True).stdout
base.update(payload_sha256=hashlib.sha256(fact_raw).hexdigest(),payload_bytes=len(fact_raw),chunk_count=math.ceil(len(fact_raw)/1024))
fact_frames=[base.copy()]
for i in range(base['chunk_count']):
 f=base.copy();f.update(kind='CHUNK',chunk_index=i,data_hex=fact_raw[i*1024:(i+1)*1024].hex());fact_frames.append(f)
f=base.copy();f['kind']='COMMIT';fact_frames.append(f)
value=json.dumps(fact_frames,indent=2)+'\n'
(ROOT/'integration/test/contracts/normalized-facts.json').write_text(value)
for n in ['chain-parser-go','chain-parser-rust','dex-router']:(ROOT/n/'test/contracts/normalized-facts.json').write_text(value)
print(json.dumps({'golden_encoding':'protoc normalized FactBatch','bytes':len(fact_raw),'sha256':hashlib.sha256(fact_raw).hexdigest()}))
