#!/usr/bin/env python3
"""Restore exact public Cargo.lock crates into an owned directory source.

Avoids Cargo's legacy giant Git index and global installation. Every crate is
checked against its upstream lock checksum before extraction; no source is edited.
"""
import argparse
import concurrent.futures
import hashlib
import io
import json
from pathlib import Path
import re
import sys
import tarfile
import tomllib
sys.path.insert(0,str(Path(__file__).resolve().parent))
from r01_public_catalog import ROOT,require,sha256
from r01_public_catalog_capture import request,write_new,reference
OWNED=ROOT/'integration/artifacts/local/r01-public-sbf-runtime'
VENDOR_ROOT=OWNED/'vendor'

def restore(package):
    name,version,checksum=package['name'],package['version'],package['checksum'];require(re.fullmatch(r'[a-zA-Z0-9_-]+',name),'CRATE_NAME')
    cache=OWNED/'crate-archives'/(name+'-'+version+'.crate');dest=VENDOR_ROOT/(name+'-'+version)
    if (dest/'.cargo-checksum.json').exists():
        require(json.loads((dest/'.cargo-checksum.json').read_text())['package']==checksum,'CACHED_CRATE_CHECKSUM')
        return {'name':name,'version':version,'checksum':checksum,'restored':True,'cached':True}
    raw=None;origin=None
    for candidate in Path('/Users/mac/.cargo/registry/cache').glob('*/'+cache.name):
        data=candidate.read_bytes()
        if sha256(data)==checksum:raw=data;origin='existing-public-crate-cache-checked-against-upstream-lock';break
    if raw is None:
        url=f'https://static.crates.io/crates/{name}/{name}-{version}.crate'
        for attempt in range(2):
            try:_,raw,_=request(url,None,16*1024*1024);origin=url;break
            except (OSError,ValueError):
                if attempt:raise
    require(sha256(raw)==checksum,'UPSTREAM_CRATE_CHECKSUM:'+name)
    if not cache.exists():write_new(cache,raw)
    files={}
    with tarfile.open(fileobj=io.BytesIO(raw),mode='r:gz') as tar:
        for member in tar:
            if not member.isfile():continue
            parts=Path(member.name).parts;require(parts and parts[0]==name+'-'+version,'CRATE_ROOT')
            relative=Path(*parts[1:]);require(not relative.is_absolute() and '..' not in relative.parts,'CRATE_PATH')
            require(member.size<=64*1024*1024,'CRATE_MEMBER_LIMIT');data=tar.extractfile(member).read();target=dest/relative
            if not target.exists():write_new(target,data)
            require(target.read_bytes()==data,'CRATE_SOURCE_CHANGED');files[relative.as_posix()]=sha256(data)
    write_new(dest/'.cargo-checksum.json',(json.dumps({'files':files,'package':checksum},sort_keys=True)+'\n').encode())
    return {'name':name,'version':version,'checksum':checksum,'archive':reference(cache),'origin':origin,'files':len(files),'restored':True}

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--lock',type=Path,required=True);p.add_argument('--vendor-root',type=Path,default=VENDOR_ROOT);p.add_argument('--out',type=Path,default=ROOT/'integration/test/source/deployment-catalog/solana-verifiable/cpmm-public-crates.json');a=p.parse_args()
    globals()['VENDOR_ROOT']=a.vendor_root.resolve();require(VENDOR_ROOT.is_relative_to(OWNED.resolve()),'OWNED_VENDOR_ROOT')
    packages=[p for p in tomllib.loads(a.lock.read_text())['package'] if p.get('source','').startswith('registry+')]
    def safe(package):
        try:return restore(package)
        except Exception as e:return {'name':package['name'],'version':package['version'],'restored':False,'error':str(e)}
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as executor:results=list(executor.map(safe,packages))
    report={'schema_version':1,'lock':reference(a.lock),'source_modified':False,'directory_source':str(VENDOR_ROOT.relative_to(ROOT)),'crates':results,'complete':all(p['restored'] for p in results)}
    path=a.out;path.write_text(json.dumps(report,indent=2)+'\n');print(json.dumps({'complete':report['complete'],'crates':len(results),'failed':[r for r in results if not r['restored']]},indent=2));sys.exit(0 if report['complete'] else 1)
if __name__=='__main__':main()
