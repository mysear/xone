#!/usr/bin/env python3
"""Independent protoc encoding of synthetic Managed fee-state contract cases."""
from pathlib import Path
import hashlib,json,subprocess
ROOT=Path(__file__).resolve().parents[3]
AMOUNTS={'reserve0','reserve1','balance','scaling','weights','amplification','total_supply','protocol_swap_fee','management_swap_fee','owner_balance','protocol_collector_balance'}
DIGESTS={'hash','parent_hash','adapter_set_hash','scope_hash','code_sha256','vault_code_sha256','block_hash'}
def encode(obj,level=''):
 def blob(raw):return '"'+''.join('\\x%02x'%v for v in raw)+'"'
 out=''
 for k,v in obj.items():
  if v is None:continue
  for item in v if isinstance(v,list)else[v]:
   if isinstance(item,dict):out+=k+' { '+encode(item,k)+' } '
   elif k in AMOUNTS:out+=k+' { uint256_be: '+blob(int(item).to_bytes(32,'big'))+' } '
   elif k in DIGESTS or k=='pool_id'and level=='multi_asset':out+=k+': '+blob(bytes.fromhex(item.removeprefix('0x')))+' '
   else:out+=k+': '+(json.dumps(item)if isinstance(item,(str,bool))else str(item))+' '
 return out

def generate():
 pool=json.loads((ROOT/'integration/test/contracts/multi-asset-state.json').read_text())['pools'][0]
 pool.update(protocol='balancer-v2-managed-fees',adapter_version='balancer-v2-managed-fees/e281ff40-bpt-1')
 pool['multi_asset']['managed_fees']={'total_supply':'1000000000000000000000000','protocol_swap_fee':'300000000000000000','management_swap_fee':'100000000000000000','owner':'0x0000000000000000000000000000000000000090','protocol_collector':'0x0000000000000000000000000000000000000091','owner_balance':'700000000000000000000000','protocol_collector_balance':'200000000000000000000000','timestamp':1767225600,'block_number':100,'block_hash':'0x'+'a'*64}
 s=json.loads((ROOT/'integration/test/evm/baseline.json').read_text());s['pools']=[pool];s['source']='SYNTHETIC_MANAGED_BPT_WIRE_CONTRACT'
 ref=s['reference'];ref['block'].update(number_or_slot=100,hash='0x'+'a'*64,timestamp_ms=1767225600000)
 for key,value in [('adapter_set_hash',[pool['adapter_version']]),('scope_hash',[pool['pool_id']])]:ref[key]=hashlib.sha256(json.dumps(value,separators=(',',':')).encode()).hexdigest()
 raw=subprocess.run(['protoc','-I',str(ROOT/'common/protoc/proto'),'--encode=dex.v1.OnlineSnapshot',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=encode(s).encode(),capture_output=True,check=True).stdout
 text=json.dumps({'snapshot':s,'protobuf_hex':raw.hex(),'protobuf_sha256':hashlib.sha256(raw).hexdigest()},indent=2)+'\n'
 for service in ['integration','chain-parser-go','chain-parser-rust','dex-router']:(ROOT/service/'test/contracts/managed-fees-wire.json').write_text(text)
 print('Managed fees independent wire bytes',len(raw))
if __name__=='__main__':generate()
