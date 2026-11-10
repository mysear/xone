#!/usr/bin/env python3
"""Independent C++ protobuf encoder for complete LFJ field31 and observed zeros."""
from pathlib import Path
import json,hashlib,subprocess
R=Path(__file__).resolve().parents[3]
def digest(value):return bytes.fromhex(value.removeprefix('0x'))
def amount(value):return {'uint256_be':int(value).to_bytes(32,'big')}
def text_value(value):
 if isinstance(value,bytes):return '"'+''.join('\\%03o'%b for b in value)+'"'
 if isinstance(value,str):return json.dumps(value)
 if isinstance(value,bool):return 'true'if value else'false'
 return str(value)
def text_message(value):
 rows=[]
 for key,v in value.items():
  items=v if isinstance(v,list)else[v]
  for item in items:
   if isinstance(item,dict):rows.append(key+' { '+text_message(item)+' }')
   else:rows.append(key+': '+text_value(item))
 return '\n'.join(rows)
normalized=json.loads((R/'integration/artifacts/local/core-lfj-v21-normalized.json').read_text())
selected=[normalized['cases'][0],next(c for c in normalized['cases']if ((int(c['pool']['lfj']['parameters'])>>216)&65535)>0 and ((int(c['pool']['lfj']['parameters'])>>54)&0xffffff)>0 and c['expected'].get('amount_out')),next(c for c in normalized['cases']if not c['zero_for_one'] and ((int(c['pool']['lfj']['parameters'])>>216)&65535)>0 and ((int(c['pool']['lfj']['parameters'])>>54)&0xffffff)>0 and c['input']=='2500000')]
vectors=[]
zero_case=json.loads(json.dumps(selected[0]))
zero_case['index']=-1
zero_case['pool']['lfj']['block_number']=0
zero_case['pool']['lfj']['timestamp']=0
zero_case['pool']['lfj']['parameters']=str(int(zero_case['pool']['lfj']['parameters']) & ~(((1<<40)-1)<<176))
selected.append(zero_case)
for case in selected:
 pool=case['pool'];state=pool['lfj'];scope=hashlib.sha256(json.dumps([pool['pool_id']],separators=(',',':')).encode()).hexdigest();adapters=hashlib.sha256(json.dumps([pool['adapter_version']],separators=(',',':')).encode()).hexdigest()
 snap={'reference':{'block':{'chain_key':'evm:31337','number_or_slot':state['block_number'],'hash':state['block_hash'],'parent_hash':normalized['parent_hash']if state['block_number']else'0x'+'00'*32,'timestamp_ms':state['timestamp']*1000},'canonical_epoch':1,'journal_seq':1,'schema_version':1,'adapter_set_hash':adapters,'online_generation':1,'scope_hash':scope,'writer_epoch':1,'owner_service':'chain-parser-go','owner_assignment_version':1},'pools':[pool],'source':'PROVIDED_LOCAL_STATE','history_gaps':[]}
 pb=json.loads(json.dumps(snap));ref=pb['reference']
 for k in ['adapter_set_hash','scope_hash']:ref[k]=digest(ref[k])
 for k in ['hash','parent_hash']:ref['block'][k]=digest(ref['block'][k])
 p=pb['pools'][0]
 for k in ['reserve0','reserve1']:p[k]=amount(p[k])
 s=p['lfj']
 for k in ['code_sha256','implementation_code_sha256','factory_code_sha256','token0_code_sha256','token1_code_sha256','block_hash']:s[k]=digest(s[k])
 for k in ['parameters','raw_reserve0','raw_reserve1','protocol_fees0','protocol_fees1']:s[k]=amount(s[k])
 for b in s['bins']:
  for k in ['reserve0','reserve1','total_shares']:b[k]=amount(b[k])
 for o in s['oracle_samples']:o['data']=amount(o['data'])
 wire=subprocess.run(['protoc','-I',str(R/'common/protoc/proto'),'--encode=dex.v1.OnlineSnapshot',str(R/'common/protoc/proto/dex/v1/state.proto')],input=text_message(pb).encode(),capture_output=True,check=True).stdout
 vectors.append({'case_index':case['index'],'snapshot':snap,'protobuf_hex':wire.hex(),'protobuf_sha256':hashlib.sha256(wire).hexdigest()})
result={'schema_version':1,'authority':'independent protoc C++ encoder; complete LFJ typed field31 snapshots','vectors':vectors}
for service in ['integration','chain-parser-go','chain-parser-rust','dex-router']:
 (R/service/'test/contracts/lfj-v21-wire.json').write_text(json.dumps(result,indent=2)+'\n')
print('LFJ C++ wire vectors',len(vectors))
