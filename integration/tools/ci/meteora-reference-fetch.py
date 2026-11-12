#!/usr/bin/env python3
"""Restore only the digest-pinned upstream files declared for the DLMM reference."""
from pathlib import Path
import argparse,hashlib,io,json,urllib.request,zipfile
ROOT=Path(__file__).resolve().parents[3];BASE=ROOT/'integration/tools/reference/meteora-dlmm'
def require(ok,message):
    if not ok:raise ValueError('METEORA_FETCH:'+message)
def main():
    parser=argparse.ArgumentParser();parser.add_argument('--fetch',action='store_true');args=parser.parse_args();metadata=json.loads((BASE/'source-metadata.json').read_bytes());commit=metadata['commit'];require(commit=='1532e540db41ac405dbf4c5bb2ddfceaa1f98ff1','commit')
    missing=[]
    for item in metadata['files']:
        path=BASE/item['path'];require(path.resolve().is_relative_to(BASE.resolve()),'path')
        if not path.exists():missing.append(item);continue
        raw=path.read_bytes();require(len(raw)==item['bytes']and hashlib.sha256(raw).hexdigest()==item['sha256'],'changed-cache:'+item['path'])
    if missing:
        require(args.fetch,'missing-cache-use--fetch')
        with urllib.request.urlopen('https://codeload.github.com/MeteoraAg/dlmm-sdk/zip/'+commit,timeout=90)as response:archive=response.read(128*1024*1024+1)
        require(len(archive)<=128*1024*1024,'archive-budget')
        with zipfile.ZipFile(io.BytesIO(archive))as z:
            for item in missing:
                raw=z.read('dlmm-sdk-'+commit+'/'+item['path']);require(len(raw)==item['bytes']and hashlib.sha256(raw).hexdigest()==item['sha256']and hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest()==item['git_blob_sha1'],'download-identity:'+item['path'])
                path=BASE/item['path'];path.parent.mkdir(exist_ok=True,parents=True)
                with path.open('xb')as out:out.write(raw)
    print(json.dumps(dict(passed=True,files=len(metadata['files']),restored=len(missing),commit=commit)))
if __name__=='__main__':main()
