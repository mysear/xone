#!/usr/bin/env python3
"""Read-only, bounded public receipt vector acquisition. Never sends transactions."""
import concurrent.futures, datetime, hashlib, json, pathlib, urllib.request
ROOT = pathlib.Path(__file__).resolve().parent
CASES = [
 ('ethereum', 'evm:1', 17000000, ['https://ethereum-rpc.publicnode.com', 'https://eth.drpc.org']),
 ('base-regolith', 'evm:8453', 1000000, ['https://mainnet.base.org', 'https://base-rpc.publicnode.com']),
 ('base-canyon', 'evm:8453', 10000000, ['https://mainnet.base.org', 'https://base-rpc.publicnode.com']),
 ('optimism-regolith', 'evm:10', 110000000, ['https://mainnet.optimism.io', 'https://optimism-rpc.publicnode.com']),
 ('optimism-canyon', 'evm:10', 120000000, ['https://optimism.drpc.org', 'https://optimism-rpc.publicnode.com']),
 ('arbitrum', 'evm:42161', 100000000, ['https://arb1.arbitrum.io/rpc', 'https://arbitrum-one-rpc.publicnode.com']),
 ('bsc', 'evm:56', 35000000, ['https://bsc-dataseed.bnbchain.org', 'https://bsc-rpc.publicnode.com']),
 ('polygon', 'evm:137', 50000000, ['https://polygon-bor-rpc.publicnode.com', 'https://polygon.drpc.org']),
 ('avalanche', 'evm:43114', 10000000, ['https://api.avax.network/ext/bc/C/rpc', 'https://avalanche-c-chain-rpc.publicnode.com']),
 ('polygon-synthetic', 'evm:137', 50000144, ['https://polygon-bor-rpc.publicnode.com', 'https://polygon.drpc.org']),
 ('avalanche-current', 'evm:43114', 99642059, ['https://avalanche-c-chain-rpc.publicnode.com']),
]
PER_TX = {'bsc', 'optimism-canyon'}
LIMIT=16<<20

def rpc(url, method, params):
 assert method in ('eth_chainId','eth_getBlockByNumber','eth_getBlockReceipts','eth_getTransactionReceipt','eth_getRawTransactionByHash','eth_getProof')
 payload={'jsonrpc':'2.0','id':1,'method':method,'params':params}
 req=urllib.request.Request(url, json.dumps(payload).encode(), {'Content-Type':'application/json','User-Agent':'xone-public-receipt-vectors'})
 with urllib.request.urlopen(req,timeout=30) as res:
  raw=res.read(LIMIT+1)
 if len(raw)>LIMIT: raise ValueError('response limit')
 result=json.loads(raw)
 if result.get('error'): raise ValueError(result['error'])
 if result.get('result') is None: raise ValueError('null result')
 return result['result'], {'request':payload,'response':result,'response_sha256':hashlib.sha256(raw).hexdigest()}

def capture(case):
 name,key,height,urls=case
 out=ROOT/name
 if (out/'vector.json').exists(): return name+' already captured'
 errors=[]
 for url in urls:
  try:
   cid,identity=rpc(url,'eth_chainId',[])
   if int(cid,16)!=int(key.split(':')[1]): raise ValueError('chain mismatch')
   block,be=rpc(url,'eth_getBlockByNumber',[hex(height),False])
   evidence=[identity,be]
   try:
    if name in PER_TX: raise ValueError('explicit acquisition per-tx')
    receipts,re=rpc(url,'eth_getBlockReceipts',[block['hash']]);evidence.append(re)
   except Exception as e:
    if name not in PER_TX and '-32601' not in str(e): raise
    def one(tx):return rpc(url,'eth_getTransactionReceipt',[tx])
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool: pairs=list(pool.map(one,block['transactions']))
    receipts=[r for r,e in pairs];evidence.extend(e for r,e in pairs)
   if len(receipts) != len(block['transactions']) or not receipts: raise ValueError('incomplete/empty receipt acquisition')
   out.mkdir(exist_ok=True)
   vector={'chain_key':key,'endpoint':url,'captured_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'block':block,'receipts':receipts}
   if name == 'polygon-synthetic': vector['expected_verification_error']='RECEIPT_BOR_SYNTHETIC_UNCOMMITTED'
   if name == 'avalanche-current':
    vector['expected_verification_error']='RECEIPT_ROOT_MISMATCH'
    vector['coverage_note']='Current-network negative vector: header receiptsRoot does not commit these asynchronously executed receipts. Current execution-result commitment verification is not implemented.'
   (out/'vector.json').write_text(json.dumps(vector,indent=2)+'\n')
   (out/'rpc.json').write_text(json.dumps(evidence,indent=2)+'\n')
   return f'{name}: tx={len(block["transactions"])} receipt={len(receipts)} types={sorted(set(r.get("type","0x0") for r in receipts))}'
  except Exception as e:errors.append(url+': '+str(e))
 return name+' FAILED '+repr(errors)
if __name__=='__main__':
 with concurrent.futures.ThreadPoolExecutor(max_workers=9) as pool:
  for line in pool.map(capture,CASES):print(line,flush=True)
