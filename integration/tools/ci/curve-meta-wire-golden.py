#!/usr/bin/env python3
"""Independent protoc contract for the new versioned Meta direct state."""
from pathlib import Path
import hashlib,json,subprocess
ROOT=Path(__file__).resolve().parents[3]
AMOUNTS={'reserve0','reserve1','balance','scaling','weights','amplification','stored_balances','admin_balances','base_virtual_price'}
DIGESTS={'hash','parent_hash','adapter_set_hash','scope_hash','code_sha256','vault_code_sha256','block_hash','base_pool_code_sha256','implementation_code_sha256'}
def encode(obj,level=''):
 def blob(raw):return '"'+''.join('\\x%02x'%v for v in raw)+'"'
 out=''
 for key,value in obj.items():
  if value is None:continue
  for item in value if isinstance(value,list)else[value]:
   if isinstance(item,dict):out+=key+' { '+encode(item,key)+' } '
   elif key in AMOUNTS:out+=key+' { uint256_be: '+blob(int(item).to_bytes(32,'big'))+' } '
   elif key in DIGESTS or key=='pool_id'and level=='multi_asset':out+=key+': '+blob(bytes.fromhex(item.removeprefix('0x')))+' '
   else:out+=key+': '+(json.dumps(item)if isinstance(item,(str,bool))else str(item))+' '
 return out

def pool():
 p=json.loads((ROOT/'integration/test/contracts/multi-asset-state.json').read_text())['pools'][0];s=p['multi_asset'];assets=s['assets'][:2]
 assets[1].update(token='0x6c3f90f043a72fa612cbac8115ee7e52bde6e490',scaling='1000000000000000000')
 impl='0x0000000000000000000000000000000000000099';code=bytes.fromhex('363d3d373d3d3d363d73'+impl[2:]+'5af43d82803e903d91602b57fd5bf3')
 s.update(assets=assets,weights=[],amplification='10000',admin_fee=5000000000,vault='',vault_code_sha256='',pool_id='',code_sha256=hashlib.sha256(code).hexdigest())
 s['curve_meta']={'implementation':impl,'implementation_code_sha256':'c5351fd332fdf0f04486a838b2bceba15d7a45965e3df61e9a0ccedc674d450d','base_pool':'0xbebc44782c7db0a1a60cb6fe97d0b483032ff1c7','base_pool_code_sha256':'b'*64,'base_virtual_price':'1234567890123456789','stored_balances':[a['balance']for a in assets],'admin_balances':['0','0'],'timestamp':1767225600,'block_number':100,'block_hash':'0x'+'a'*64}
 p.update(protocol='curve-metausd-direct',adapter_version='curve-metausd-direct/5272fd2b-fixed-base-1',token0=assets[0]['token'],token1=assets[1]['token'],reserve0=assets[0]['balance'],reserve1=assets[1]['balance'],fee_numerator=4000000,fee_denominator=10000000000)
 return p

def generate():
 s=json.loads((ROOT/'integration/test/evm/baseline.json').read_text());p=pool();s['pools']=[p];s['source']='SYNTHETIC_META_DIRECT_WIRE_CONTRACT';ref=s['reference'];ref['block'].update(number_or_slot=100,hash='0x'+'a'*64,timestamp_ms=1767225600000)
 for key,val in [('adapter_set_hash',[p['adapter_version']]),('scope_hash',[p['pool_id']])]:ref[key]=hashlib.sha256(json.dumps(val,separators=(',',':')).encode()).hexdigest()
 raw=subprocess.run(['protoc','-I',str(ROOT/'common/protoc/proto'),'--encode=dex.v1.OnlineSnapshot',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=encode(s).encode(),capture_output=True,check=True).stdout
 data=json.dumps({'snapshot':s,'protobuf_hex':raw.hex(),'protobuf_sha256':hashlib.sha256(raw).hexdigest()},indent=2)+'\n'
 for project in ['integration','chain-parser-go','chain-parser-rust','dex-router']:(ROOT/project/'test/contracts/curve-meta-wire.json').write_text(data)
 print('Independent Meta wire bytes',len(raw))
if __name__=='__main__':generate()
