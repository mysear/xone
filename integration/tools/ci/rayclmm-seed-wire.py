#!/usr/bin/env python3
"""Independent protoc field62 nonzero u16 seed and bound account identity vectors."""
from pathlib import Path
import json,hashlib,math,subprocess,copy,importlib.util
ROOT=Path(__file__).resolve().parents[3]
ALPHABET='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
def b58(s):
 n=0
 for c in s:n=n*58+ALPHABET.index(c)
 b=b'\0'*(len(s)-len(s.lstrip('1')))+n.to_bytes((n.bit_length()+7)//8,'big');assert len(b)==32;return b
def blob(b):return '"'+''.join('\\x%02x'%v for v in b)+'"'
def encode(obj):
 out=''
 for k,v in obj.items():
  for x in v if isinstance(v,list)else[v]:
   if isinstance(x,dict):out+=k+' { '+encode(x)+' } '
   elif k in ['reserve0','reserve1','sqrt_price_x64','liquidity','fee_growth_global0','fee_growth_global1','liquidity_gross','fee_growth_outside0','fee_growth_outside1']:out+=k+' { uint256_be: '+blob(int(x).to_bytes(32,'big'))+' } '
   elif k=='liquidity_net':out+='liquidity_net_i128_be: '+blob(int(x).to_bytes(16,'big',signed=True))+' '
   elif k in ['hash','parent_hash']:out+=k+': '+blob(b58(x))+' '
   elif k in ['scope_hash','adapter_set_hash','program_code_sha256','token_program_code_sha256']:out+=k+': '+blob(bytes.fromhex(x))+' '
   else:out+=k+': '+(json.dumps(x)if isinstance(x,(str,bool))else str(x))+' '
 return out
spec=importlib.util.spec_from_file_location('state_golden',Path(__file__).with_name('rayclmm-state-golden.py'));module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module)
fixtures=[]
for seed in [1,256,65535]:
 mode=2
 p=json.loads((ROOT/'integration/test/contracts/rayclmm-state.json').read_text());p.update(protocol='raydium-clmm-custom-seed',adapter_version='raydium-clmm/6d7a0f82-spl-custom-seed-1');s=p['raydium_clmm'];s['collect_fee_on']=mode;s['seed_index']=seed
 p['pool_id'],s['pool_bump']=module.pda([b'pool',module.decode(s['amm_config']),module.decode(p['token0']),module.decode(p['token1']),seed.to_bytes(2,'little')]);pool=module.decode(p['pool_id'])
 s['vault0']=module.pda([b'pool_vault',pool,module.decode(p['token0'])])[0];s['vault1']=module.pda([b'pool_vault',pool,module.decode(p['token1'])])[0];s['observation']=module.pda([b'observation',pool])[0]
 for a in s['tick_arrays']:a['address']=module.pda([b'tick_array',pool,a['start_tick_index'].to_bytes(4,'big',signed=True)])[0]
 address=module.pda([b'pool_tick_array_bitmap_extension',module.decode(p['pool_id'])])[0]
 s['bitmap_extension']={'address':address,'positive':[0]*112,'negative':[0]*112}
 s['bitmap_extension']['positive'][111]=1<<63;s['bitmap_extension']['negative'][7]=(1<<63)|1
 snap=json.loads((ROOT/'integration/test/solana/baseline.json').read_text());snap['pools']=[p];snap['reference']['block'].update(number_or_slot=s['bank_slot'],timestamp_ms=s['bank_unix_timestamp']*1000)
 for k,f in [('scope_hash','pool_id'),('adapter_set_hash','adapter_version')]:snap['reference'][k]=hashlib.sha256(json.dumps([p[f]],separators=(',',':')).encode()).hexdigest()
 snap['source']='PINNED_RAYCLMM_CUSTOM_SEED_NUMERIC_FIXTURE'
 raw=subprocess.run(['protoc','--proto_path='+str(ROOT/'common/protoc/proto'),'--encode=dex.v1.JournalUnit',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=('kind:"ONLINE_BASELINE_COMMIT" snapshot { '+encode(snap)+' }').encode(),capture_output=True,check=True).stdout
 r=snap['reference'];base={'schema_version':1,'unit_id':f"solana:localnet/{r['owner_assignment_version']}/{r['writer_epoch']}/{r['journal_seq']}",'kind':'BEGIN','reference':r,'payload_sha256':hashlib.sha256(raw).hexdigest(),'payload_bytes':len(raw),'chunk_count':math.ceil(len(raw)/1024),'chunk_index':0,'data_hex':''};frames=[base.copy()]
 for i in range(base['chunk_count']):f=base.copy();f.update(kind='CHUNK',chunk_index=i,data_hex=raw[i*1024:(i+1)*1024].hex());frames.append(f)
 f=base.copy();f['kind']='COMMIT';frames.append(f);fixtures.append({'seed_index':seed,'frames':frames})
text=json.dumps(fixtures,indent=2)+'\n'
for project in ['integration','chain-parser-go','chain-parser-rust','dex-router']:(ROOT/project/'test/contracts/rayclmm-seed-frames.json').write_text(text)
print(json.dumps({'seeds':[1,256,65535],'sha256':hashlib.sha256(text.encode()).hexdigest(),'protoc_field':62}))
