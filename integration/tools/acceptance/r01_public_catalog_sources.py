#!/usr/bin/env python3
"""Restore publicly pinned solc builds, then exactly rebuild captured runtimes.

The compiler cache lives only in integration/artifacts/local/r01-public-compilers.
No private provider or account is involved. Compilation never executes a contract.
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import urllib.request
import tempfile
sys.path.insert(0,str(Path(__file__).resolve().parent))
from r01_public_catalog import ROOT, require, sha256, read_blob
from r01_public_catalog_capture import request, write_new, reference, fetch_document

BASE=ROOT/'integration/test/source/deployment-catalog'
CACHE=ROOT/'integration/artifacts/local/r01-public-compilers'

def compiler_bytes(url):
    # A compiler is a public distribution artifact, not a node/RPC response.
    # Keep the RPC budget at 16 MiB; independently cap a compiler file at 32 MiB.
    require(url.startswith('https://binaries.soliditylang.org/bin/soljson-v'),'COMPILER_ORIGIN')
    req=urllib.request.Request(url,headers={'User-Agent':'XONE-source-evidence/1.0'})
    with urllib.request.urlopen(req,timeout=25) as response:
        raw=response.read(32*1024*1024+1)
        require(len(raw)<=32*1024*1024,'COMPILER_FILE_LIMIT')
        return raw

def prepare(rows):
    CACHE.mkdir(parents=True,exist_ok=True)
    listing=CACHE/'list.json'
    if not listing.exists():fetch_document('https://binaries.soliditylang.org/bin/list.json',listing)
    snapshot=BASE/'compiler-build-list.json'
    if not snapshot.exists():write_new(snapshot,listing.read_bytes())
    builds=json.loads(snapshot.read_text())['builds'];compilers={};entries=[]
    sources={}
    for name in ['verified-source-v2','verified-source-inputs']:
        sources.update(json.loads((BASE/name/'index.json').read_text()))
    for row in rows:
        source=sources.get(row['id'])
        if not source or 'blob' not in source:continue
        data=json.loads(read_blob(ROOT,source['blob']));version=data['compilation']['compilerVersion']
        if version not in compilers:
            build=next(b for b in builds if b.get('longVersion')==version)
            file=CACHE/build['path'];url='https://binaries.soliditylang.org/bin/'+build['path']
            expected=build['sha256'].removeprefix('0x')
            if not file.exists():
                raw=compiler_bytes(url);require(sha256(raw)==expected,'PUBLIC_COMPILER_HASH');write_new(file,raw)
            require(sha256(file.read_bytes())==expected,'CACHED_COMPILER_HASH')
            compilers[version]={'version':version,'url':url,'blob':reference(file),'official_list':reference(snapshot)}
        entries.append({'id':row['id'],'source':source,'compiler':compilers[version],'runtime_sha256':row['runtime']['sha256'],'runtime_observation':row['runtime']['observation']})
    return {'schema_version':1,'entries':entries,'compilers':list(compilers.values())}

def restore(manifest):
    for compiler in manifest['compilers']:
        path=ROOT/compiler['blob']['path']
        require(path.resolve().is_relative_to(CACHE.resolve()),'COMPILER_CACHE_PATH')
        if not path.exists():
            raw=compiler_bytes(compiler['url']);require(sha256(raw)==compiler['blob']['sha256'],'PUBLIC_COMPILER_HASH');write_new(path,raw)
        read_blob(ROOT,compiler['blob'])

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('command',choices=['prepare','restore','replay']);p.add_argument('--input',type=Path,default=BASE/'samples/index.json');p.add_argument('--manifest',type=Path,default=BASE/'public-source-build-inputs.json');p.add_argument('--out',type=Path,default=BASE/'public-source-build-report.json');a=p.parse_args()
    if a.command=='prepare':
        manifest=prepare(json.loads(a.input.read_text())['deployments']);write_new(a.manifest,(json.dumps(manifest,indent=2)+'\n').encode());print('public compiler versions:',[c['version'] for c in manifest['compilers']])
    elif a.command=='restore':restore(json.loads(a.manifest.read_text()))
    else:
        if a.out.exists():
            with tempfile.TemporaryDirectory(prefix='r01-public-source-check-',dir=ROOT/'integration/artifacts/local') as directory:
                output=Path(directory)/'report.json'
                result=subprocess.run(['node',str(Path(__file__).with_suffix('.cjs')),str(ROOT),str(a.manifest),str(output)],timeout=180)
                if result.returncode:sys.exit(result.returncode)
                require(json.loads(output.read_text())==json.loads(a.out.read_text()),'PUBLIC_SOURCE_REPORT_REPLAY_MISMATCH')
                print('All retained public compiler results reproduced; individual failed builds remain unqualified.')
        else:
            result=subprocess.run(['node',str(Path(__file__).with_suffix('.cjs')),str(ROOT),str(a.manifest),str(a.out)],timeout=180)
            sys.exit(result.returncode)
if __name__=='__main__':main()
