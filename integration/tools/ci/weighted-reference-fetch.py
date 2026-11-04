#!/usr/bin/env python3
"""Fetch and hash-check the complete, pinned WeightedPool/Vault reference closure."""
from pathlib import Path
import hashlib,json,urllib.request
ROOT=Path(__file__).resolve().parents[3]
def fetch():
 manifest=json.loads((ROOT/'integration/tools/reference/weighted-model/source-metadata.json').read_text())
 dest=ROOT/'integration/.runtime/weighted-reference/sources'
 for item in manifest['files']:
  p=dest/item['path']
  if not p.exists() or hashlib.sha256(p.read_bytes()).hexdigest()!=item['sha256']:
   data=urllib.request.urlopen(f"https://raw.githubusercontent.com/{manifest['repo']}/{manifest['revision']}/{item['path']}",timeout=30).read()
   if hashlib.sha256(data).hexdigest()!=item['sha256']:raise RuntimeError('Pinned source mismatch: '+item['path'])
   p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(data)
 return manifest
if __name__=='__main__':print(json.dumps({'pinned_files':len(fetch()['files'])}))
