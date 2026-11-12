#!/usr/bin/env python3
"""Acquire public PumpSwap code/config once, then require exact pinned bytes."""
from pathlib import Path
import base64,hashlib,json,sys,urllib.request
ROOT=Path(__file__).resolve().parents[3];BASE=ROOT/'integration/tools/reference/pump-amm';PIN=BASE/'public-bank-pins.json';PROGRAM='pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA'
def sha(raw):return hashlib.sha256(raw).hexdigest()
def rpc(keys):
 body=dict(jsonrpc='2.0',id=1,method='getMultipleAccounts',params=[keys,dict(encoding='base64',commitment='finalized')]);req=urllib.request.Request('https://api.mainnet-beta.solana.com',data=json.dumps(body).encode(),headers={'Content-Type':'application/json','User-Agent':'xone-fixed-reference'})
 with urllib.request.urlopen(req,timeout=60)as response:raw=response.read(32*1024*1024+1)
 if len(raw)>32*1024*1024:raise ValueError('RPC_BUDGET')
 value=json.loads(raw)
 if 'error'in value:raise ValueError('PUBLIC_RPC_ERROR:'+json.dumps(value['error']))
 return dict(request=body,response=value)
def main():
 if PIN.exists():
  pins=json.loads(PIN.read_bytes())
  capture=None
  if '--fetch'in sys.argv:
   if '--capture'not in sys.argv:raise ValueError('RESTORE_REQUIRES_ORIGINAL_CAPTURE')
   path=Path(sys.argv[sys.argv.index('--capture')+1]);raw=path.read_bytes()
   if sha(raw)!=pins['capture_sha256']:raise ValueError('RESTORE_CAPTURE_DIGEST')
   capture=json.loads(raw);saved=dict(zip(capture['request']['params'][0],capture['response']['result']['value']))
  for ref in pins['files']:
   path=BASE/ref['path']
   if not path.exists()and capture is not None:
    if 'address'in ref:raw=base64.b64decode(saved[ref['address']]['data'][0],validate=True)
    else:
     data=next(item for item in pins['files']if item.get('address')and item['bytes']>1000000);raw=base64.b64decode(saved[data['address']]['data'][0],validate=True)[45:]
    if sha(raw)!=ref['sha256']or len(raw)!=ref['bytes']:raise ValueError('RESTORE_CONTENT_DIGEST')
    path.parent.mkdir(parents=True,exist_ok=True);path.write_bytes(raw)
   raw=path.read_bytes()
   if sha(raw)!=ref['sha256']or len(raw)!=ref['bytes']:raise ValueError('PIN_CHANGED:'+ref['path'])
  print(json.dumps(dict(passed=True,verified=len(pins['files']))));return
 if '--acquire'not in sys.argv:raise ValueError('MISSING_PIN_USE_EXPLICIT_ACQUIRE')
 sys.path.insert(0,str(ROOT/'integration/tools/acceptance'));from r01_public_catalog_solana_bank import native_imports
 from r01_public_catalog_solana_rebuild import programdata_pda
 _,Pubkey,*_=native_imports();pid=Pubkey.from_string(PROGRAM);global_id=str(Pubkey.find_program_address([b'global_config'],pid)[0]);fee_id=str(Pubkey.find_program_address([b'fee_config',bytes(pid)],Pubkey.from_string('pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ'))[0]);keys=[PROGRAM,programdata_pda(PROGRAM),global_id,fee_id]
 capture=rpc(keys);files=[]
 for key,a in zip(keys,capture['response']['result']['value']):
  if a is None:raise ValueError('PUBLIC_ACCOUNT_MISSING:'+key)
  raw=base64.b64decode(a['data'][0],validate=True);rel='upstream/accounts/'+key+'.bin';p=BASE/rel;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(raw);files.append(dict(address=key,path=rel,bytes=len(raw),sha256=sha(raw),owner=a['owner'],executable=a['executable'],lamports=a['lamports']))
 code=base64.b64decode(capture['response']['result']['value'][1]['data'][0],validate=True)[45:];p=BASE/'upstream/pump_amm.so';p.write_bytes(code);files.append(dict(path='upstream/pump_amm.so',bytes=len(code),sha256=sha(code)))
 path=ROOT/'integration/artifacts/local/pump-amm-public-acquisition-20261111.json';path.write_text(json.dumps(capture)+'\n');PIN.write_text(json.dumps(dict(schema_version=1,program=PROGRAM,global_id=global_id,fee_id=fee_id,slot=capture['response']['result']['context']['slot'],files=files,capture_sha256=sha(path.read_bytes()),authority='single public finalized acquisition only; no trusted historical bank'),indent=2)+'\n');print(json.dumps(dict(passed=True,code_sha256=sha(code),files=len(files))))
if __name__=='__main__':main()
