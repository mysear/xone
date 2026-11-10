#!/usr/bin/env python3
"""Read exact runtime CBOR pointers and acquire public source metadata unchanged."""
import argparse
import concurrent.futures
import hashlib
import json
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parent))
from r01_public_catalog import ROOT, require, sha256, b58encode, load_json, read_blob
from r01_public_catalog_capture import request, write_new, reference

BASE=ROOT/'integration/test/source/deployment-catalog'

def cbor_item(data,offset=0,depth=0):
    require(depth<10 and offset<len(data),'CBOR_BOUNDS')
    head=data[offset];offset+=1;major=head>>5;extra=head&31
    require(extra<28,'CBOR_INDEFINITE_UNSUPPORTED')
    if extra<24:n=extra
    else:
        width={24:1,25:2,26:4,27:8}[extra];require(offset+width<=len(data),'CBOR_LENGTH')
        n=int.from_bytes(data[offset:offset+width],'big');offset+=width
    if major==0:return n,offset
    if major in (2,3):
        require(offset+n<=len(data),'CBOR_STRING')
        value=data[offset:offset+n];offset+=n
        return (value if major==2 else value.decode('utf8')),offset
    if major==5:
        require(n<=16,'CBOR_MAP_LIMIT');value={}
        for _ in range(n):
            key,offset=cbor_item(data,offset,depth+1);require(isinstance(key,str) and key not in value,'CBOR_KEY')
            value[key],offset=cbor_item(data,offset,depth+1)
        return value,offset
    require(False,'CBOR_TYPE_UNSUPPORTED')

def runtime_metadata(runtime):
    require(len(runtime)>=4,'CBOR_RUNTIME_SHORT');length=int.from_bytes(runtime[-2:],'big')
    require(0<length<=len(runtime)-2,'CBOR_RUNTIME_LENGTH');offset=len(runtime)-2-length
    decoded,end=cbor_item(runtime[offset:-2]);require(end==length and isinstance(decoded,dict),'CBOR_TRAILING')
    require(any(k in decoded for k in ('ipfs','bzzr0','bzzr1','solc')),'CBOR_EMPTY_METADATA')
    result={'metadata_offset':offset,'metadata_bytes':length,'runtime_sha256':sha256(runtime),'cbor_hex':runtime[offset:-2].hex()}
    if 'solc' in decoded:
        value=decoded['solc'];require(isinstance(value,bytes) and len(value)==3,'CBOR_SOLC_VERSION')
        result['compiler_version']='.'.join(str(v) for v in value)
    if 'ipfs' in decoded:
        value=decoded['ipfs'];require(isinstance(value,bytes) and len(value)==34 and value[:2]==b'\x12\x20','CBOR_IPFS_MULTIHASH')
        result['ipfs_cid']=b58encode(value)
    for key in ('bzzr0','bzzr1'):
        if key in decoded:require(isinstance(decoded[key],bytes) and len(decoded[key])==32,'CBOR_SWARM_HASH');result[key]=decoded[key].hex()
    return result

def acquire(row,directory):
    raw=load_json(ROOT/row['runtime']['observation']['path'])['response']['result']
    metadata=runtime_metadata(bytes.fromhex(raw[2:]));result={'id':row['id'],'runtime':metadata,'attempts':[]}
    cid=metadata.get('ipfs_cid');urls=[]
    if cid:urls=[f'https://ipfs.io/ipfs/{cid}',f'https://dweb.link/ipfs/{cid}',f'https://gateway.pinata.cloud/ipfs/{cid}']
    if metadata.get('bzzr1'):urls=[f"https://api.gateway.ethswarm.org/bzz/{metadata['bzzr1']}/",f"https://gateway.ethswarm.org/bzz/{metadata['bzzr1']}/"]
    for index,url in enumerate(urls):
        try:
            status,body,final=request(url,None,4*1024*1024)
            content=json.loads(body);require('compiler' in content and 'sources' in content and 'settings' in content,'METADATA_CONTENT')
            path=directory/row['id']/'metadata.json';write_new(path,body)
            result['metadata']={'url':url,'final_url':final,'blob':reference(path),'http_status':status};break
        except (OSError,ValueError) as e:result['attempts'].append({'url':url,'error':str(e)})
    return result

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--catalog',type=Path,default=BASE/'catalog.json');p.add_argument('--out',type=Path,required=True);p.add_argument('--ids',nargs='+',required=True);a=p.parse_args()
    rows=[r for r in load_json(a.catalog)['deployments'] if r['id'] in a.ids]
    with concurrent.futures.ThreadPoolExecutor(max_workers=3) as executor:results=list(executor.map(lambda r:acquire(r,a.out),rows))
    write_new(a.out/'index.json',(json.dumps({'schema_version':1,'entries':results},indent=2)+'\n').encode())
    print(json.dumps([{**{'id':r['id']},'metadata_acquired':'metadata' in r,'attempts':r['attempts']} for r in results],indent=2))
if __name__=='__main__':main()
