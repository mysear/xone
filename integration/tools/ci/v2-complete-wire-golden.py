#!/usr/bin/env python3
"""Independent C++ protoc encoding of complete V2 snapshots, including known zero."""
from pathlib import Path
import copy,hashlib,json,shutil,subprocess
ROOT=Path(__file__).resolve().parents[3]
def require(ok,message):
 if not ok:raise ValueError(message)
def canonical(value):return json.dumps(value,sort_keys=True,separators=(',',':')).encode()
def digest(value):return hashlib.sha256(canonical(value)).hexdigest()
def text(value):return json.dumps(value)
def data(value):return '"'+''.join('\\%03o'%b for b in value)+'"'
def raw(value):return bytes.fromhex(value.removeprefix('0x'))
def amount(value):return '{ uint256_be: '+data(int(value).to_bytes(32,'big'))+' }'
def encode(snapshot,protoc):
 r=snapshot['reference'];b=r['block'];p=snapshot['pools'][0];s=p['v2']['swap']
 head='reference { block { '+f'chain_key: {text(b["chain_key"])} number_or_slot: {b["number_or_slot"]} hash: {data(raw(b["hash"]))} parent_hash: {data(raw(b["parent_hash"]))} timestamp_ms: {b["timestamp_ms"]}'+' } '
 for key in ['canonical_epoch','journal_seq','schema_version','online_generation','writer_epoch','owner_assignment_version']:head+=f'{key}: {r[key]} '
 for key in ['adapter_set_hash','scope_hash']:head+=f'{key}: {data(raw(r[key]))} '
 head+=f'owner_service: {text(r["owner_service"])}'+' } pools { '
 for key in ['chain_key','pool_id','protocol','token0','token1','adapter_version','token_behavior']:head+=f'{key}: {text(p[key])} '
 for key in ['reserve0','reserve1']:head+=f'{key}: {amount(p[key])} '
 head+=f'fee_numerator: {p["fee_numerator"]} fee_denominator: {p["fee_denominator"]} '+'v2 { '+f'factory: {text(p["v2"]["factory"])} code_sha256: {data(raw(p["v2"]["code_sha256"]))}'+' swap { '
 for key in ['raw_balance0','raw_balance1','price0_cumulative_last','price1_cumulative_last']:head+=f'{key}: {amount(s[key])} '
 for key in ['block_timestamp_last','bank_block_number','bank_unix_timestamp']:head+=f'{key}: {s[key]} '
 head+='unlocked: true '+f'factory_code_sha256: {data(raw(s["factory_code_sha256"]))} bank_block_hash: {data(raw(s["bank_block_hash"]))}'+' } } } '+f'source: {text(snapshot["source"])}'
 result=subprocess.run([protoc,'-I',str(ROOT/'common/protoc/proto'),'--encode=dex.v1.OnlineSnapshot',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=head.encode(),capture_output=True,check=True)
 return result.stdout.hex()
def main():
 native=json.loads((ROOT/'integration/test/contracts/v2-complete.json').read_text());protoc=shutil.which('protoc');require(protoc is not None,'protoc missing');cases=[]
 for index,name in [(0,'original-synchronized'),(2,'original-unsynced-donation'),(0,'synthetic-known-zero-schema-only')]:
  pool=copy.deepcopy(native['cases'][index]['pool']);swap=pool['v2']['swap'];anchor=native['cases'][index]['state_anchor'];block=dict(chain_key=pool['chain_key'],number_or_slot=anchor['block_number'],hash=anchor['block_hash'],parent_hash=anchor['parent_hash'],timestamp_ms=anchor['timestamp']*1000)
  if name.startswith('synthetic'):
   swap.update(block_timestamp_last=0,bank_block_number=0,bank_unix_timestamp=0,bank_block_hash='0x'+'aa'*32);block.update(number_or_slot=0,timestamp_ms=0,hash=swap['bank_block_hash'],parent_hash='0x'+'00'*32)
  reference=dict(block=block,canonical_epoch=1,journal_seq=1,schema_version=1,adapter_set_hash=digest([pool['adapter_version']]),online_generation=1,scope_hash=digest([pool['pool_id']]),writer_epoch=1,owner_service='chain-parser-go',owner_assignment_version=1)
  snapshot=dict(reference=reference,pools=[pool],source='SCHEMA_WIRE_ONLY_NOT_CANONICAL',history_gaps=[]);cases.append(dict(name=name,snapshot=snapshot,protobuf_hex=encode(snapshot,protoc)))
 document=dict(schema_version=1,authority='independent-cpp-protoc-schema-encoding-not-execution',protoc={'executable':protoc,'version':subprocess.run([protoc,'--version'],capture_output=True,text=True,check=True).stdout.strip(),'sha256':hashlib.sha256(Path(protoc).read_bytes()).hexdigest()},cases=cases)
 encoded=json.dumps(document,indent=2)+'\n'
 for service in ['integration','chain-parser-go','chain-parser-rust','dex-router']:(ROOT/service/'test/contracts/v2-complete-wire.json').write_text(encoded)
 print('3 independent complete-state snapshots encoded by C++ protoc; 4 copies written')
if __name__=='__main__':main()
