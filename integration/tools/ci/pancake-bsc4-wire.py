#!/usr/bin/env python3
"""Independent complete contextual C++ wire vectors and observed zero/false presence."""
from pathlib import Path
import json,hashlib,subprocess,copy
R=Path(__file__).resolve().parents[3];S=json.loads((R/'integration/tools/reference/pancake-v3-lm/context-schema.json').read_text());F=json.loads((R/'integration/test/contracts/pancake-bsc4-state.json').read_text())
S.update({'PancakeBscLmFlag':[['tick','i32'],['migrated','bool']], 'PancakeBscLmPredecessor':[['address','String'],['code_sha256','digest'],['template_sha256','digest'],['ticks','Vec<PancakeLmTick>'],['flags','Vec<PancakeBscLmFlag>']], 'PancakeBscLmState':[['ledger','PancakeLmState'],['initialization','bool'],['flags','Vec<PancakeBscLmFlag>'],['predecessors','Vec<PancakeBscLmPredecessor>']]})
def amount(v):return {'uint256_be':int(v).to_bytes(32,'big')}
def digest(v):return bytes.fromhex(v.removeprefix('0x'))
def core(v):
 out=copy.deepcopy(v)
 for key in ['sqrt_price_x96','liquidity']:out[key]=amount(out[key])
 for key in ['code_sha256','factory_code_sha256','implementation_code_sha256']:
  if key in out:out[key]=digest(out[key])
 for w in out['tick_words']:w['bitmap']=amount(w['bitmap'])
 for t in out['ticks']:t['liquidity_gross']=amount(t['liquidity_gross']);t['liquidity_net_i128_be']=int(t.pop('liquidity_net')).to_bytes(16,'big',signed=True)
 return out
def message(name,v):
 out={}
 for key,kind in S[name]:
  if key not in v:
   if kind.startswith('Option<'):continue
   raise ValueError('MISSING_REQUIRED_'+name+'.'+key)
  value=v[key]
  if kind=='Amount':value=amount(value)
  elif kind in ['digest','blockhash']:value=digest(value)
  elif kind.startswith('signed'):value=int(value).to_bytes(int(kind[6:])//8,'big',signed=True)
  elif kind=='V3State':value=core(value)
  elif kind.startswith('Vec<'):value=[message(kind[4:-1],item)for item in value]
  elif kind.startswith('Option<'):value=message(kind[7:-1],value)
  elif kind in S:value=message(kind,value)
  out[key]=value
 return out
def text_value(value):
 if isinstance(value,bytes):return '"'+''.join('\\%03o'%b for b in value)+'"'
 if isinstance(value,str):return json.dumps(value)
 if isinstance(value,bool):return 'true'if value else'false'
 return str(value)
def encode(value):
 lines=[]
 for key,value in value.items():
  for item in value if isinstance(value,list)else[value]:
   lines.append(key+' { '+encode(item)+' }'if isinstance(item,dict)else key+': '+text_value(item))
 return '\n'.join(lines)
selected=[next(c for c in F['cases']if c['mode']==mode and c['input']=='1000000000')for mode in ['migrated','first','mixed']]
zero=copy.deepcopy(selected[-1]);zero['index']=-1;zero['pool']['pancake_bsc_lm']['ledger']['block_number']=0;zero['pool']['pancake_bsc_lm']['ledger']['timestamp']=0;selected.append(zero)
vectors=[]
for case in selected:
 pool=case['pool'];wrapper=pool['pancake_bsc_lm'];state=wrapper['ledger'];scope=hashlib.sha256(json.dumps([pool['pool_id']],separators=(',',':')).encode()).hexdigest();adapters=hashlib.sha256(json.dumps([pool['adapter_version']],separators=(',',':')).encode()).hexdigest()
 snapshot={'reference':{'block':{'chain_key':pool['chain_key'],'number_or_slot':state['block_number'],'hash':state['block_hash'],'parent_hash':'0x'+('c6'*32 if state['block_number']else'00'*32),'timestamp_ms':state['timestamp']*1000},'canonical_epoch':1,'journal_seq':1,'schema_version':1,'adapter_set_hash':adapters,'online_generation':1,'scope_hash':scope,'writer_epoch':1,'owner_service':'chain-parser-go','owner_assignment_version':1},'pools':[pool],'source':'PROVIDED_LOCAL_STATE','history_gaps':[]}
 pb=copy.deepcopy(snapshot)
 for key in ['adapter_set_hash','scope_hash']:pb['reference'][key]=digest(pb['reference'][key])
 for key in ['hash','parent_hash']:pb['reference']['block'][key]=digest(pb['reference']['block'][key])
 for key in ['reserve0','reserve1']:pb['pools'][0][key]=amount(pb['pools'][0][key])
 pb['pools'][0]['pancake_bsc_lm']=message('PancakeBscLmState',wrapper)
 raw=subprocess.run(['protoc','-I',str(R/'common/protoc/proto'),'--encode=dex.v1.OnlineSnapshot',str(R/'common/protoc/proto/dex/v1/state.proto')],input=encode(pb).encode(),capture_output=True,check=True).stdout
 vectors.append({'case_index':case['index'],'snapshot':snapshot,'protobuf_hex':raw.hex(),'protobuf_sha256':hashlib.sha256(raw).hexdigest()})
value={'schema_version':1,'authority':'independent C++ protobuf encoder of complete contextual field34; explicit optional zero/false fields retained','vectors':vectors}
for service in ['integration','chain-parser-go','chain-parser-rust','dex-router']:(R/service/'test/contracts/pancake-bsc4-wire.json').write_text(json.dumps(value,indent=2)+'\n')
print('Independent complete contextual C++ vectors',len(vectors))
