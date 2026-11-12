#!/usr/bin/env python3
"""Restore fixed public source caches; never substitutes a new program/config version."""
from pathlib import Path
import argparse,base64,hashlib,io,json,tarfile,urllib.request
ROOT=Path(__file__).resolve().parents[3];BASE=ROOT/'integration/tools/reference/pump-bonding'
def require(ok,why):
 if not ok:raise ValueError('PUMP_REFERENCE_FETCH:'+why)
def digest(raw):return hashlib.sha256(raw).hexdigest()
def download(url):
 with urllib.request.urlopen(urllib.request.Request(url,headers={'User-Agent':'xone-fixed-reference'}),timeout=60)as response:raw=response.read(32*1024*1024+1)
 require(len(raw)<=32*1024*1024,'response-budget');return raw
def write(ref,raw):
 require(len(raw)==ref['bytes']and digest(raw)==ref['sha256'],'source-mismatch:'+ref['path']);p=(BASE/ref['path']).resolve();require(p.is_relative_to(BASE.resolve()),'source-path');p.parent.mkdir(parents=True,exist_ok=True)
 if p.exists():require(p.read_bytes()==raw,'changed-cache:'+ref['path'])
 else:
  with p.open('xb')as stream:stream.write(raw)
def main():
 parser=argparse.ArgumentParser();parser.add_argument('--fetch',action='store_true');parser.add_argument('--capture',type=Path);args=parser.parse_args();pins=json.loads((BASE/'source-pins.json').read_bytes());bank=json.loads((BASE/'public-bank-pins.json').read_bytes());missing=[]
 for ref in pins['files']+bank['files']:
  p=BASE/ref['path']
  if p.exists():require(len(p.read_bytes())==ref['bytes']and digest(p.read_bytes())==ref['sha256'],'changed-cache:'+ref['path'])
  else:missing.append(ref)
 if missing:
  require(args.fetch,'missing-input-use--fetch-and-pinned-capture');package=None
  for ref in missing:
   if ref['path'].startswith('packages/'):
    if package is None:
     raw=download(pins['sdk_dist']['tarball']);require('sha512-'+base64.b64encode(hashlib.sha512(raw).digest()).decode()==pins['sdk_dist']['integrity'],'sdk-integrity');package=tarfile.open(fileobj=io.BytesIO(raw),mode='r:gz')
    member='package/'+ref['path'].split('packages/pump-sdk/',1)[1];stream=package.extractfile(member);require(stream is not None,'sdk-member');write(ref,stream.read())
   elif 'url'in ref:write(ref,download(ref['url']))
   else:
    require(args.capture is not None,'public-account-cache-requires-original-capture')
    capture=args.capture.read_bytes();require(digest(capture)==bank['acquisition_sha256'],'acquisition-digest');d=json.loads(capture);require(d['request']['params'][0]==bank['keys'],'account-request-identity');index=bank['keys'].index(ref['address']);write(ref,base64.b64decode(d['response']['result']['value'][index]['data'][0],validate=True))
 for ref in bank['files']:
  if ref['address']not in ['B5MvUwXdiW1NMM6QFFD3ssPKBujD4zMohncbM73Z2BQu','75Uu23mqWBb8LM8vDppqC1mQAnCcBuLXhVaDezVMQLRw']:continue
  raw=(BASE/ref['path']).read_bytes()[45:];name='pump.so'if ref['address'].startswith('B5Mv')else'fee.so';p=BASE/'upstream/artifacts'/name
  expected='7a19affaa2c6187e04a5fe0ea298cee81fa5eb8c24cfcc30d85f615e89f7b02d'if name=='pump.so'else'4f2955d0f6b40cc9ff7daa18ccad7d19119ba1249759f4e7e3bb691beee6b35e';require(digest(raw)==expected,'program-identity')
  if p.exists():require(p.read_bytes()==raw,'program-cache-changed')
  else:p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(raw)
 print(json.dumps(dict(passed=True,source_files=len(pins['files']),public_account_files=len(bank['files']),restored=len(missing),canonical_bank_qualified=False)))
if __name__=='__main__':main()
