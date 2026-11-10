#!/usr/bin/env python3
"""Owned bounded public GET/JSON-RPC probe; no accounts, API keys, or transactions.

Capture artifacts are raw response envelopes. No observation upgrades itself to a
state proof or consensus claim. Existing files are never overwritten.
"""
import argparse
import concurrent.futures
import datetime
import json
from pathlib import Path
import sys
import urllib.error
import urllib.request

sys.path.insert(0,str(Path(__file__).resolve().parent))
from r01_public_catalog import (CatalogError, ROOT, public_url, public_rpc_endpoint, require, sha256,
    IMPLEMENTATION_SLOT, BEACON_SLOT, minimal_proxy, hexbytes, decode_program,
    decode_programdata)

READ_ONLY = frozenset(('eth_chainId','eth_getBlockByNumber','eth_getCode','eth_getStorageAt',
    'eth_getProof','eth_call','eth_getBalance','eth_getTransactionCount','eth_getBlockByHash','eth_getTransactionByHash','eth_getTransactionReceipt','eth_getBlockReceipts','net_version','web3_clientVersion','getGenesisHash','getSlot','getBlock','getAccountInfo',
    'getMultipleAccounts','getSignaturesForAddress','getTransaction'))
MAX_BYTES=16*1024*1024
UA='XONE-source-evidence/1.0'

def write_new(path,data):
    path=Path(path);path.parent.mkdir(parents=True,exist_ok=True)
    with path.open('xb') as f:f.write(data)

def request(url,data,budget):
    public_url(url);require(type(budget) is int and 0<budget<=MAX_BYTES,'RESPONSE_BUDGET')
    req=urllib.request.Request(url,data=data,headers={'User-Agent':UA,'Content-Type':'application/json' if data else 'text/plain'})
    # Standard HTTPS transport, bounded redirects; recorded final URL remains public.
    with urllib.request.urlopen(req,timeout=18) as response:
        public_url(response.geturl());body=response.read(budget+1)
        require(len(body)<=budget,'RPC_RESPONSE_LIMIT')
        return response.status,body,response.geturl()

def reference(path):
    return {'path':str(Path(path).resolve().relative_to(ROOT)),'sha256':sha256(Path(path).read_bytes())}

def fetch_rpc(url,method,params,path,budget=MAX_BYTES):
    require(method in READ_ONLY,'READ_ONLY_METHOD');require(0<budget<=MAX_BYTES,'RESPONSE_BUDGET')
    body=json.dumps({'jsonrpc':'2.0','id':1,'method':method,'params':params}).encode()
    started=datetime.datetime.now(datetime.timezone.utc).isoformat()
    envelope={'endpoint':public_rpc_endpoint(url),'method':method,'params':params,'observed_at':started,'response_budget_bytes':budget,'timeout_seconds':18,'user_agent':UA,'consensus_verified':False}
    try:
        status,raw,final=request(url,body,budget)
        envelope.update(http_status=status,response=json.loads(raw),response_sha256=sha256(raw),response_bytes=len(raw),final_endpoint=final)
    except (OSError,ValueError) as e:
        envelope.update(http_status=getattr(e,'code',None),error=type(e).__name__+':'+str(e))
    write_new(path,(json.dumps(envelope,indent=2)+'\n').encode())
    return reference(path),envelope.get('response',{}).get('result')

def fetch_document(url,path):
    status,raw,final=request(url,None,2*1024*1024)
    write_new(path,raw)
    return {'url':url,'final_url':final,'http_status':status,'blob':reference(path),'authority':'official-deployment-directory'}

DOCUMENTS={
 'uni-ethereum':'https://docs.uniswap.org/contracts/v3/reference/deployments/ethereum-deployments',
 'uni-base':'https://docs.uniswap.org/contracts/v3/reference/deployments/base-deployments',
 'uni-bsc':'https://docs.uniswap.org/contracts/v3/reference/deployments/bnb-deployments',
 'uni-arbitrum':'https://docs.uniswap.org/contracts/v3/reference/deployments/arbitrum-deployments',
 'uni-optimism':'https://docs.uniswap.org/contracts/v3/reference/deployments/optimism-deployments',
 'uni-polygon':'https://docs.uniswap.org/contracts/v3/reference/deployments/polygon-deployments',
 'uni-avalanche':'https://developers.uniswap.org/docs/protocols/v3/deployments/v3-avalanche-deployments',
 'uni-v2':'https://raw.githubusercontent.com/Uniswap/v2-sdk/main/src/constants.ts',
 'pancake-v2':'https://developer.pancakeswap.finance/contracts/v2/addresses',
 'pancake-v3':'https://developer.pancakeswap.finance/contracts/v3/addresses',
 'aerodrome':'https://raw.githubusercontent.com/aerodrome-finance/contracts/main/README.md',
 'slipstream':'https://raw.githubusercontent.com/aerodrome-finance/slipstream/main/README.md',
 'velodrome':'https://raw.githubusercontent.com/velodrome-finance/contracts/main/README.md',
 'raydium':'https://docs.raydium.io/reference/program-addresses',
 'orca':'https://raw.githubusercontent.com/orca-so/whirlpools/main/README.md',
 'lfj':'https://raw.githubusercontent.com/lfj-gg/joe-sdk-v2/main/src/constants/v2Addrs.ts',
}

def capture_docs(directory):
    def task(item):
        key,url=item
        try:return key,fetch_document(url,directory/(key+'.txt'))
        except (OSError,ValueError) as e:return key,{'url':url,'error':str(e)}
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as ex:results=dict(ex.map(task,DOCUMENTS.items()))
    write_new(directory/'index.json',(json.dumps(results,indent=2)+'\n').encode())
    return results

def capture_evm(chain,target,rows,directory):
    """One chain at a time, fixed H, at most two endpoints and finite address scope."""
    endpoints=target['rpc_urls'][:2]
    extra={'bsc':'https://bsc-rpc.publicnode.com','avalanche':'https://avalanche-c-chain-rpc.publicnode.com'}
    if len(endpoints)<2 and chain in extra:endpoints.append(extra[chain])
    calls=0
    def call(endpoint,method,params):
        nonlocal calls
        calls+=1;require(calls<=90,'CHAIN_REQUEST_BUDGET')
        return fetch_rpc(endpoint,method,params,directory/f'{calls:03}-{method}.json',budget=2*1024*1024)
    selected=None;ident=None;head=None
    for endpoint in endpoints:
        cref,cid=call(endpoint,'eth_chainId',[])
        if cid!=hex(target['chain_id']):continue
        gref,g=call(endpoint,'eth_getBlockByNumber',['0x0',False])
        if not g or g['hash']!=target['genesis_hash']:continue
        # Finality is provider-declared. Only an actual returned header is retained.
        requested=hex(rows[0]['capture_height']) if rows[0].get('capture_height') else 'finalized'
        href,h=call(endpoint,'eth_getBlockByNumber',[requested,False])
        if not h:href,h=call(endpoint,'eth_getBlockByNumber',['latest',False])
        if not h:continue
        selected=endpoint;ident={'chain_id':cref,'genesis':gref};head=h;break
    require(selected,'CHAIN_PUBLIC_RPC_UNAVAILABLE:'+chain)
    tag=head['number'];href,h=call(selected,'eth_getBlockByNumber',[tag,False])
    require(h and h['hash']==head['hash'],'CHAIN_HEAD_CHANGED')
    witnesses=[]
    for endpoint in endpoints:
        if endpoint==selected:continue
        wref,w=call(endpoint,'eth_getBlockByNumber',[tag,False])
        if w and w['hash']==h['hash']:witnesses.append(wref)
    cache={}
    for row in rows:
        row['identity']=ident;row['anchor']={'height':int(tag,16),'hash':h['hash'],'header':href,'witnesses':witnesses,'authority':'public-rpc-observation-not-consensus'}
        address=row['address'].lower()
        if row.get('factory_binding'):
            binding=row['factory_binding'];ref,value=call(selected,'eth_call',[{'to':binding['address'].lower(),'data':binding['calldata']},tag])
            require(value and '0x'+value[-40:].lower()==address,'FACTORY_POOL_BINDING')
            binding['observation']=ref
        if address in cache:
            row.update(cache[address]);continue
        rref,code=call(selected,'eth_getCode',[address,tag]);require(code and code!='0x','RUNTIME_EMPTY:'+address)
        runtime={'observation':rref,'sha256':sha256(hexbytes(code)),'bytes':len(hexbytes(code))}
        slots={};values={};deps=[]
        for slot in (IMPLEMENTATION_SLOT,BEACON_SLOT):
            ref,value=call(selected,'eth_getStorageAt',[address,slot,tag]);require(value is not None,'UPGRADE_SLOT_UNAVAILABLE')
            slots[slot]=ref;values[slot]=value
        kind='slots-observed-custom-unresolved';clone=minimal_proxy(code)
        impl=clone or ('0x'+values[IMPLEMENTATION_SLOT][-40:] if int(values[IMPLEMENTATION_SLOT],16) else None)
        beacon='0x'+values[BEACON_SLOT][-40:] if int(values[BEACON_SLOT],16) else None
        upgrade={'kind':'eip1167' if clone else 'eip1967' if impl or beacon else kind,'slots':slots,'dependencies':deps}
        if beacon:
            ref,bc=call(selected,'eth_getCode',[beacon,tag]);require(bc and bc!='0x','BEACON_RUNTIME_EMPTY')
            deps.append({'role':'beacon','address':beacon,'observation':ref,'sha256':sha256(hexbytes(bc))})
            ref,value=call(selected,'eth_call',[{'to':beacon,'data':'0x5c60da1b'},tag]);require(value,'BEACON_IMPLEMENTATION_READ')
            upgrade['beacon_call']=ref;impl='0x'+value[-40:]
        if impl:
            ref,ic=call(selected,'eth_getCode',[impl,tag]);require(ic and ic!='0x','IMPLEMENTATION_RUNTIME_EMPTY')
            deps.append({'role':'implementation','address':impl,'observation':ref,'sha256':sha256(hexbytes(ic))})
        for dependency in row.get('public_dependency_calls',[]):
            # Original factories with custom implementation getters are observed
            # separately; generic EIP-1967 slots do not establish their semantics.
            ref,value=call(selected,'eth_call',[{'to':address,'data':dependency['selector']},tag])
            if value and len(value)==66 and int(value,16):
                addr='0x'+value[-40:];cref,c=call(selected,'eth_getCode',[addr,tag])
                if c and c!='0x':deps.append({'role':dependency['role'],'getter_observation':ref,'getter_selector':dependency['selector'],'address':addr,'observation':cref,'sha256':sha256(hexbytes(c))})
        item={'runtime':runtime,'upgrade':upgrade};cache[address]=item;row.update(item)
    return rows

def capture_solana(target,rows,directory):
    import base64
    endpoints=target['rpc_urls'][:2];calls=0
    def call(endpoint,method,params,budget=MAX_BYTES):
        nonlocal calls
        calls+=1;require(calls<=35,'CHAIN_REQUEST_BUDGET')
        return fetch_rpc(endpoint,method,params,directory/f'{calls:03}-{method}.json',budget)
    for row in rows:
        success=False
        for endpoint in endpoints:
            gr,g=call(endpoint,'getGenesisHash',[])
            if g!=target['genesis_hash']:continue
            _,p=call(endpoint,'getAccountInfo',[row['address'],{'encoding':'base64','commitment':'finalized'}])
            if not p or not p.get('value'):continue
            pdata=decode_program(base64.b64decode(p['value']['data'][0]))
            ref,both=call(endpoint,'getMultipleAccounts',[[row['address'],pdata],{'encoding':'base64','commitment':'finalized'}])
            if not both or not all(both['value']):continue
            program,data=both['value'];pb,db=[base64.b64decode(x['data'][0]) for x in (program,data)]
            details=decode_programdata(db);slot=both['context']['slot']
            bref,block=call(endpoint,'getBlock',[slot,{'commitment':'finalized','transactionDetails':'none','rewards':False,'maxSupportedTransactionVersion':0}])
            if not block:continue
            row['identity']={'genesis':gr};row['anchor']={'height':slot,'hash':block['blockhash'],'header':bref,'authority':'finalized-rpc-bank-context-not-historical-bank-proof'}
            row['runtime']={'observation':ref,'sha256':details['payload_sha256'],'program_sha256':sha256(pb),'bytes':details['payload_bytes'],'includes_allocation_padding':True}
            row['upgrade']={'kind':'solana-upgradeable-loader','programdata':pdata,'slot':details['upgrade_slot'],'authority':details['upgrade_authority']}
            success=True;break
        require(success,'SOLANA_PUBLIC_RPC_UNAVAILABLE:'+row['id'])
    return rows

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('command',choices=['docs','capture'])
    p.add_argument('--out',type=Path,required=True);p.add_argument('--seed',type=Path)
    args=p.parse_args()
    if args.command=='docs':result=capture_docs(args.out)
    else:
        require(args.seed,'SEED_REQUIRED');seed=json.loads(args.seed.read_text());targets=json.loads((ROOT/'integration/config/chain-targets.json').read_text())['targets']
        groups={}
        for row in seed['deployments']:groups.setdefault(row['chain'],[]).append(row)
        def task(item):
            chain,rows=item;target=targets[chain]
            for row in rows:row.update(chain_key=target['chain_key'],genesis_hash=target['genesis_hash'])
            try:
                return chain,(capture_solana(target,rows,args.out/chain) if target['family']=='solana' else capture_evm(chain,target,rows,args.out/chain))
            except (OSError,ValueError,KeyError) as e:return chain,{'error':str(e),'partial_rows':rows}
        with concurrent.futures.ThreadPoolExecutor(max_workers=3) as ex:result=dict(ex.map(task,groups.items()))
    write_new(args.out/'capture-index.json',(json.dumps(result,indent=2)+'\n').encode());print(json.dumps({k:len(v) if isinstance(v,list) else v.get('error','ok') for k,v in result.items()},indent=2))

if __name__=='__main__':main()
