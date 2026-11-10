#!/usr/bin/env python3
"""Bounded public official platform-tools range restore in the owned runtime."""
import concurrent.futures
import hashlib
import json
from pathlib import Path
import tarfile
import time
import urllib.request
from r01_public_catalog import ROOT, require

RUNTIME=ROOT/'integration/artifacts/local/r01-public-sbf-runtime'
URL='https://github.com/anza-xyz/platform-tools/releases/download/v1.52/platform-tools-osx-x86_64.tar.bz2'
SIZE=432740907
CHUNK=4*1024*1024
RESOLVED_URL=None

def fetch(index):
    directory=RUNTIME/'v152-ranges';directory.mkdir(parents=True,exist_ok=True)
    start=index*CHUNK;end=min(SIZE,start+CHUNK)-1;path=directory/f'{index:04d}.bin'
    if path.exists():
        require(path.stat().st_size==end-start+1,'RANGE_RETAINED_LENGTH')
        return {'index':index,'bytes':path.stat().st_size,'sha256':hashlib.sha256(path.read_bytes()).hexdigest(),'retained':True}
    errors=[]
    for attempt in range(3):
        try:
            # A distinct public query avoids a stale/throttled CDN redirect cache.
            req=urllib.request.Request(RESOLVED_URL or URL+f'?r01-chunk={index}&attempt={attempt}',headers={'User-Agent':'XONE-source-evidence/1.0','Range':f'bytes={start}-{end}'})
            with urllib.request.urlopen(req,timeout=45) as response:
                require(response.status==206 and response.headers.get('Content-Range')==f'bytes {start}-{end}/{SIZE}','OFFICIAL_RANGE_BINDING')
                data=response.read(CHUNK+1);require(len(data)==end-start+1,'OFFICIAL_RANGE_LENGTH')
            with path.open('xb') as output:output.write(data)
            return {'index':index,'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest(),'attempt_errors':errors}
        except Exception as error:
            errors.append(str(error));time.sleep(min(10,2**attempt))
    raise RuntimeError(f'RANGE_FAILED:{index}:{errors}')

def main():
    global RESOLVED_URL
    start=time.monotonic();count=(SIZE+CHUNK-1)//CHUNK
    with urllib.request.urlopen(urllib.request.Request(URL+'?download=1',headers={'User-Agent':'XONE-source-evidence/1.0','Range':'bytes=0-0'}),timeout=45) as response:
        require(response.status==206,'OFFICIAL_REDIRECT_RANGE');RESOLVED_URL=response.geturl()
    with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:parts=list(pool.map(fetch,range(count)))
    archive=RUNTIME/'platform-tools-osx-x86_64-v1.52.tar.bz2';assembly=RUNTIME/'platform-v152-assembled.pending'
    digest=hashlib.sha256()
    with assembly.open('wb') as output:
        for part in parts:
            data=(RUNTIME/'v152-ranges'/f"{part['index']:04d}.bin").read_bytes()
            require(hashlib.sha256(data).hexdigest()==part['sha256'],'ASSEMBLY_RANGE_HASH');digest.update(data);output.write(data)
    require(assembly.stat().st_size==SIZE,'ASSEMBLY_TOTAL')
    with tarfile.open(assembly,'r:bz2') as tar:
        for member in tar:
            require(not Path(member.name).is_absolute() and '..' not in Path(member.name).parts,'OFFICIAL_ARCHIVE_PATH')
    require(not archive.exists(),'OFFICIAL_ARCHIVE_EXISTS');assembly.rename(archive)
    result={'schema_version':1,'url':URL,'size':SIZE,'sha256':digest.hexdigest(),'hash_authority':'observed official HTTPS release bytes, not an independent publisher signature','range_parts':parts,'workers':3,'elapsed_seconds':round(time.monotonic()-start,3),'global_install':False}
    (ROOT/'integration/test/source/deployment-catalog/solana-verifiable/platform-v152-range-acquisition.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({k:v for k,v in result.items() if k!='range_parts'},indent=2))
if __name__=='__main__':main()
