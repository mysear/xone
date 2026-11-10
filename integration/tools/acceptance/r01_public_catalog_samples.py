#!/usr/bin/env python3
"""Capture finite sample pools from deployed factories at the catalog's fixed H.

Token pairs are query parameters, never deployment authority. The actual factory
return, pool code, token getters and clone implementation supply the observations.
"""
import argparse
import concurrent.futures
import json
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parent))
from r01_public_catalog import ROOT, hexbytes, minimal_proxy, require, sha256
from r01_public_catalog_capture import fetch_rpc, write_new

TOKENS={
 'ethereum':('0xC02aaA39b223FE8D0A0e5C4F27eAD9083C756Cc2','0xA0b86991c6218b36c1d19D4a2e9Eb0cE3606eB48'),
 'base':('0x4200000000000000000000000000000000000006','0x833589fCD6eDb6E08f4c7C32D4f71b54bdA02913'),
 'bsc':('0xbb4CdB9CBd36B01bD1cBaEBF2De08d9173bc095c','0x55d398326f99059fF775485246999027B3197955'),
 'arbitrum':('0x82aF49447D8a07e3bd95BD0d56f35241523fBab1','0xaf88d065e77c8cC2239327C5EDb3A432268e5831'),
 'optimism':('0x4200000000000000000000000000000000000006','0x0b2C639c533813f4Aa9D7837CAf62653d097Ff85'),
 'polygon':('0x0d500B1d8E8eF31E21C99d1Db9A6444d3ADf1270','0x3c499c542cEF5E3811e1192ce70d8cC03d5c3359'),
 'avalanche':('0xB31f66AA3C1e785363F0875A1B74E27b85FD66c7','0xB97EF9Ef8734C71904D8002F8b6Bc66Dd9c48a6E'),
}

def capture(row,directory,endpoints=None):
    source=json.loads((ROOT/row['runtime']['observation']['path']).read_text())['endpoint'];source=(endpoints or {}).get(row['chain'],source);tag=hex(row['anchor']['height']);counter=0
    def call(method,params):
        nonlocal counter
        counter+=1;require(counter<=36,'SAMPLE_REQUEST_BUDGET')
        return fetch_rpc(source,method,params,directory/row['id']/f'{counter:02}-{method}.json',budget=2*1024*1024)
    href,header=call('eth_getBlockByNumber',[tag,False]);require(header and header['hash']==row['anchor']['hash'],'SAMPLE_PROVIDER_ANCHOR')
    row['sample_provider_header']=href
    row['sample_pools']=[];row['sample_attempts']=[]
    if row['role']=='factory':
        tokens=TOKENS[row['chain']];selector='0x1698ee82';values=[500,3000]
        protocol=row['protocol']
        if protocol in ['aerodrome-v2','velodrome-v2']:selector='0x79bc57d5';values=[0,1]
        if protocol=='pancake-v2':selector='0xe6a43905';values=[None]
        if protocol=='aerodrome-slipstream':selector='0x28af8d0b';values=[100,200]
        if protocol=='lfj-liquidity-book':selector='0x704037bd';values=[20,25]
        for value in values:
            selected_tokens=tokens
            if value==1 and protocol=='aerodrome-v2':selected_tokens=(tokens[1],'0xd9aAEc86B65D86f6A7B5B1b0c42FFA531710b6CA')
            if value==1 and protocol=='velodrome-v2':selected_tokens=(tokens[1],'0x94b008aA00579c1307B0EF2c499aD98a8ce58e58')
            data=selector+''.join(t.lower()[2:].zfill(64) for t in selected_tokens)+(hex(value)[2:].zfill(64) if value is not None else '')
            ref,result=call('eth_call',[{'to':row['address'].lower(),'data':data},tag])
            row['sample_attempts'].append({'calldata':data,'observation':ref})
            if not result:continue
            words=[result[i:i+64] for i in range(2,len(result),64)]
            offset=1 if protocol=='lfj-liquidity-book' else 0
            if len(words)<=offset or not int(words[offset],16):continue
            address='0x'+words[offset][-40:].lower();cref,code=call('eth_getCode',[address,tag])
            if not code or code=='0x':continue
            sample={'address':address,'query_tokens':[t.lower() for t in selected_tokens],'query_parameter':value,'factory_call':ref,'calldata':data,'result_address_word':offset,'runtime':{'observation':cref,'sha256':sha256(hexbytes(code)),'bytes':len(hexbytes(code))},'token_getters':[]}
            for selector2 in ['0x0dfe1681','0xd21220a7']:
                if protocol=='lfj-liquidity-book':break # LB uses getTokenX/getTokenY; no guessed ERC20 token getters.
                tref,token=call('eth_call',[{'to':address,'data':selector2},tag])
                require(token and len(token)==66,'POOL_TOKEN_GETTER')
                sample['token_getters'].append({'selector':selector2,'address':'0x'+token[-40:].lower(),'observation':tref})
            clone=minimal_proxy(code)
            if clone:
                iref,ic=call('eth_getCode',[clone,tag]);require(ic and ic!='0x','SAMPLE_CLONE_IMPLEMENTATION')
                sample['implementation']={'address':clone,'observation':iref,'sha256':sha256(hexbytes(ic))}
            row['sample_pools'].append(sample)
            if protocol not in ['aerodrome-v2','velodrome-v2']:break
        if protocol=='aerodrome-slipstream':
            selector='0xcefa7799';ref,value=call('eth_call',[{'to':row['address'].lower(),'data':selector},tag])
            if value and len(value)==66 and int(value,16):
                a='0x'+value[-40:].lower();cref,code=call('eth_getCode',[a,tag]);require(code and code!='0x','SLIPSTREAM_IMPLEMENTATION_CODE')
                row['upgrade']['dependencies'].append({'role':'pool-implementation','getter_selector':selector,'getter_observation':ref,'address':a,'observation':cref,'sha256':sha256(hexbytes(code))})
    href,h=call('eth_getBlockByNumber',[tag,False]);require(h and h['hash']==row['anchor']['hash'],'POST_CAPTURE_HASH_CHANGED')
    row['anchor']['recheck']=href
    return row

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--input',type=Path,required=True);p.add_argument('--out',type=Path,required=True);p.add_argument('--endpoints',type=Path);args=p.parse_args()
    endpoints=json.loads(args.endpoints.read_text()) if args.endpoints else {}
    rows=json.loads(args.input.read_text())['deployments']
    def task(row):
        if row['chain']=='solana':return row
        try:return capture(row,args.out,endpoints)
        except (OSError,ValueError,KeyError) as e:row['sample_capture_error']=str(e);return row
    with concurrent.futures.ThreadPoolExecutor(max_workers=3) as ex:result=list(ex.map(task,rows))
    write_new(args.out/'index.json',(json.dumps({'schema_version':1,'deployments':result},indent=2)+'\n').encode())
    print(json.dumps({r['id']:r.get('sample_capture_error',len(r.get('sample_pools',[]))) for r in result},indent=2))
if __name__=='__main__':main()
