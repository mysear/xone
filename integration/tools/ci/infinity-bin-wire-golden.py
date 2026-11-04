#!/usr/bin/env python3
"""Independent ABI key and protoc bytes for complete initialized Bin tree state."""
from pathlib import Path
import hashlib,json,subprocess
ROOT=Path(__file__).resolve().parents[3]
AMOUNTS={'reserve0','reserve1','total_shares','tree_level0','value','app_reserve0','app_reserve1','protocol_fees0','protocol_fees1'}
DIGESTS={'hash','parent_hash','adapter_set_hash','scope_hash','manager_code_sha256','vault_code_sha256','token0_code_sha256','token1_code_sha256','parameters','block_hash'}
def address(i):return '0x'+f'{i:040x}'
def key_id(token0,token1,manager,fee,parameters):
 script="const {ethers}=require('./integration/.runtime/singleton-reference/node_modules/ethers');const v=JSON.parse(process.argv[1]);console.log(ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['address','address','address','address','uint24','bytes32'],[v[0],v[1],'0x0000000000000000000000000000000000000000',v[2],v[3],v[4]])));"
 return subprocess.run(['node','-e',script,json.dumps([token0,token1,manager,fee,parameters])],cwd=ROOT,capture_output=True,text=True,check=True).stdout.strip()
def encode(obj,level=''):
 def blob(raw):return '"'+''.join('\\x%02x'%v for v in raw)+'"'
 out=''
 for key,v in obj.items():
  if v is None:continue
  for item in v if isinstance(v,list)else[v]:
   if isinstance(item,dict):out+=key+' { '+encode(item,key)+' } '
   elif key in AMOUNTS:out+=key+' { uint256_be: '+blob(int(item).to_bytes(32,'big'))+' } '
   elif key in DIGESTS or key=='pool_id'and level=='infinity_bin':out+=key+': '+blob(bytes.fromhex(item.removeprefix('0x')))+' '
   else:out+=key+': '+(json.dumps(item)if isinstance(item,(str,bool))else str(item))+' '
 return out

def pool():
 manager,vault=address(50),address(60);tokens=[address(1),address(2)];params='0x'+f'{1<<16:064x}';raw=key_id(*tokens,manager,3000,params);ids=[8388352,8388608,8388864];l1={};l2={};root=0
 for i in ids:
  root|=1<<(i>>16);l1[i>>16]=l1.get(i>>16,0)|(1<<((i>>8)&255));l2[i>>8]=l2.get(i>>8,0)|(1<<(i&255))
 s={'manager':manager,'manager_code_sha256':'a'*64,'vault':vault,'vault_code_sha256':'b'*64,'pool_id':raw,'hooks':address(0),'parameters':params,'token0_code_sha256':'c'*64,'token1_code_sha256':'d'*64,'bin_step':1,'active_id':8388608,'lp_fee':3000,'protocol_fee0':4000,'protocol_fee1':17,'max_bins':64,'max_tree_words':64,'max_steps':64,'tree_level0':str(root),'tree_level1':[{'index':i,'value':str(v)}for i,v in sorted(l1.items())],'tree_level2':[{'index':i,'value':str(v)}for i,v in sorted(l2.items())],'bins':[{'id':i,'reserve0':'1000000','reserve1':'1000000','total_shares':'1000000000000000000'}for i in ids],'app_reserve0':'1000000000','app_reserve1':'2000000000','protocol_fees0':'3','protocol_fees1':'4','block_number':100,'timestamp':1793577600,'block_hash':'0x'+'a'*64}
 return {'chain_key':'evm:31337','pool_id':manager+'/'+raw,'protocol':'pancake-infinity-bin','token0':tokens[0],'token1':tokens[1],'reserve0':'0','reserve1':'0','fee_numerator':3000,'fee_denominator':1000000,'adapter_version':'pancake-infinity-bin/3c5ef1be-static-1','token_behavior':'standard','infinity_bin':s}
def generate():
 p=pool();snap=json.loads((ROOT/'integration/test/evm/baseline.json').read_text());snap['source']='SYNTHETIC_COMPLETE_BIN_STATE_WIRE';snap['pools']=[p];ref=snap['reference'];s=p['infinity_bin'];ref['block'].update(number_or_slot=s['block_number'],hash=s['block_hash'],timestamp_ms=s['timestamp']*1000)
 for k,f in [('scope_hash','pool_id'),('adapter_set_hash','adapter_version')]:ref[k]=hashlib.sha256(json.dumps([p[f]],separators=(',',':')).encode()).hexdigest()
 raw=subprocess.run(['protoc','-I',str(ROOT/'common/protoc/proto'),'--encode=dex.v1.OnlineSnapshot',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=encode(snap).encode(),capture_output=True,check=True).stdout;data=json.dumps({'snapshot':snap,'protobuf_hex':raw.hex(),'protobuf_sha256':hashlib.sha256(raw).hexdigest()},indent=2)+'\n'
 for service in ['integration','chain-parser-go','chain-parser-rust','dex-router']:(ROOT/service/'test/contracts/infinity-bin-wire.json').write_text(data)
 print('Independent complete Bin wire bytes',len(raw))
if __name__=='__main__':generate()
