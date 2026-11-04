#!/usr/bin/env python3
"""Independent protoc of nested NG accounting/oracle fields and source anchors."""
from pathlib import Path
import hashlib,json,subprocess,sys
ROOT=Path(__file__).resolve().parents[3];sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from curve_ng_fixture import profile,NAMES
AMOUNTS={'reserve0','reserve1','balance','scaling','weights','amplification','stored_balances','admin_balances','last_prices_packed','last_d_packed','ma_last_time','offpeg_fee_multiplier'}
DIGESTS={'hash','parent_hash','adapter_set_hash','scope_hash','code_sha256','vault_code_sha256','block_hash'}
def blob(b):return'"'+''.join('\\x%02x'%v for v in b)+'"'
def encode(obj,level=''):
 out=''
 for k,v in obj.items():
  for a in v if isinstance(v,list)else[v]:
   if isinstance(a,dict):out+=k+' { '+encode(a,k)+' } '
   elif k in AMOUNTS:out+=k+' { uint256_be: '+blob(int(a).to_bytes(32,'big'))+' } '
   elif k in DIGESTS or k=='pool_id'and level=='multi_asset':out+=k+': '+blob(bytes.fromhex(a.removeprefix('0x')))+' '
   else:out+=k+': '+(json.dumps(a)if isinstance(a,str)else str(a))+' '
 return out
states=[]
for name in NAMES:
 for i,step in enumerate(__import__('curve_ng_fixture').original_scenario(name)['steps']):
  if not step['expected'].get('reverted'):states.append(profile(name,i))
text=json.dumps({'schema_version':1,'cases':states},indent=2)+'\n'
for service in ['integration','chain-parser-go','chain-parser-rust','dex-router']:(ROOT/service/'test/contracts/curve-ng-service-state.json').write_text(text)
snap=json.loads((ROOT/'integration/test/evm/baseline.json').read_text());snap['pools']=[profile(NAMES[0])['pool']];g=snap['pools'][0]['multi_asset']['curve_ng'];r=snap['reference'];r['block'].update(number_or_slot=g['block_number'],hash=g['block_hash'],timestamp_ms=g['timestamp']*1000);r['adapter_set_hash']=hashlib.sha256(json.dumps([snap['pools'][0]['adapter_version']],separators=(',',':')).encode()).hexdigest();r['scope_hash']=hashlib.sha256(json.dumps([snap['pools'][0]['pool_id']],separators=(',',':')).encode()).hexdigest();snap['source']='PINNED_ORIGINAL_NG_SYNTHETIC_RPC_REPLAY'
raw=subprocess.run(['protoc','-I',str(ROOT/'common/protoc/proto'),'--encode=dex.v1.OnlineSnapshot',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=encode(snap).encode(),capture_output=True,check=True).stdout
text=json.dumps({'snapshot':snap,'protobuf_hex':raw.hex(),'protobuf_sha256':hashlib.sha256(raw).hexdigest()},indent=2)+'\n'
for service in ['integration','chain-parser-go','chain-parser-rust','dex-router']:(ROOT/service/'test/contracts/curve-ng-service-wire.json').write_text(text)
print('NG live-state cases',len(states),'independent protobuf bytes',len(raw))
