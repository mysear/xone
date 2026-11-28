#!/usr/bin/env python3
"""Independent protoc field27 encoding of immutable Weighted state."""
from pathlib import Path
import sys,json,hashlib,subprocess,math
ROOT=Path(__file__).resolve().parents[3];sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from weighted_smoke import NAMES,profile
profiles = [profile(name) for name in NAMES]
state = {'pools': [pool for pool, _ in profiles], 'configs': [config for _, config in profiles]}
snapshot = json.loads((ROOT / 'integration/test/evm/baseline.json').read_text())
snapshot['pools'] = state['pools']
pool_ids = sorted(pool['pool_id'] for pool in state['pools'])
adapters = sorted({pool['adapter_version'] for pool in state['pools']})
snapshot['reference']['scope_hash'] = hashlib.sha256(json.dumps(pool_ids, separators=(',', ':')).encode()).hexdigest()
snapshot['reference']['adapter_set_hash'] = hashlib.sha256(json.dumps(adapters, separators=(',', ':')).encode()).hexdigest()
snapshot['source'] = 'PINNED_WEIGHTED_PROTOCOL_FIXTURE'
def blob(b):return '"'+''.join('\\x%02x'%v for v in b)+'"'
def encode(obj,level=''):
 out=''
 for k,v in obj.items():
  for value in v if isinstance(v,list) else [v]:
   if isinstance(value,dict):out+=k+' { '+encode(value,k)+' } '
   elif k in ['reserve0','reserve1','rate0','rate1','amplification','sqrt_price_x96','liquidity','bitmap','liquidity_gross','scaling0','scaling1','weight0','weight1']:out+=k+' { uint256_be: '+blob(int(value).to_bytes(32,'big'))+' } '
   elif k=='liquidity_net':out+='liquidity_net_i128_be: '+blob(int(value).to_bytes(16,'big',signed=True))+' '
   elif k in ['hash','parent_hash','scope_hash','adapter_set_hash','code_sha256','factory_code_sha256','vault_code_sha256','implementation_code_sha256','token0_code_sha256','token1_code_sha256'] or k=='pool_id' and level=='weighted':out+=k+': '+blob(bytes.fromhex(value.removeprefix('0x')))+' '
   else:out+=k+': '+(json.dumps(value) if isinstance(value,str) else str(value))+' '
 return out
raw=subprocess.run(['protoc','--proto_path='+str(ROOT/'common/protoc/proto'),'--encode=dex.v1.JournalUnit',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=('kind:"ONLINE_BASELINE_COMMIT" snapshot { '+encode(snapshot)+' }').encode(),capture_output=True,check=True).stdout
r=snapshot['reference'];base={'schema_version':1,'unit_id':f"evm:31337/{r['owner_assignment_version']}/{r['writer_epoch']}/{r['journal_seq']}",'kind':'BEGIN','reference':r,'payload_sha256':hashlib.sha256(raw).hexdigest(),'payload_bytes':len(raw),'chunk_count':math.ceil(len(raw)/1024),'chunk_index':0,'data_hex':''};frames=[base.copy()]
for i in range(base['chunk_count']):f=base.copy();f.update(kind='CHUNK',chunk_index=i,data_hex=raw[i*1024:(i+1)*1024].hex());frames.append(f)
f=base.copy();f['kind']='COMMIT';frames.append(f);text=json.dumps(frames,indent=2)+'\n'
for project in ['integration', 'chain-parser-go', 'chain-parser-rust', 'dex-router']:
    contracts = ROOT / project / 'test/contracts'
    (contracts / 'weighted-frames.json').write_text(text)
    (contracts / 'weighted-state.json').write_text(json.dumps(state, indent=2) + '\n')
print(json.dumps({'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'protocols':NAMES}))
