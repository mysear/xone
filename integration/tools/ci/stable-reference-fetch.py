#!/usr/bin/env python3
"""Fetch pinned primary sources into isolated reference runtime, retain hashes."""
from pathlib import Path
import urllib.request,json,hashlib,re
ROOT=Path(__file__).resolve().parents[3];OUT=ROOT/'integration/.runtime/stable-reference/sources';OUT.mkdir(parents=True,exist_ok=True)
refs=[('aero/Pool.sol','aerodrome-finance/contracts','1ba30815bba620f7e9faa34769ffd00c214c9b82','contracts/Pool.sol'),('curve/Plain2Basic.vy','curvefi/curve-factory','5272fd2b1199f185d9cd94822be9262c387dc2c2','contracts/implementations/plain-2/Plain2Basic.vy'),('balancer/StableMath.sol','balancer/balancer-v2-monorepo','e281ff40583c39bf1d46c322334b2b13936e0e33','pkg/pool-stable/contracts/StableMath.sol')]
manifest=ROOT/'integration/tools/reference/stable-models/source-metadata.json'
expected={f['local']:f for f in json.loads(manifest.read_text())['files']}
metadata=[];seen=set()
def fetch(local,repo,revision,path):
 if local in seen:return
 seen.add(local);data=urllib.request.urlopen(f'https://raw.githubusercontent.com/{repo}/{revision}/{path}',timeout=30).read();identity={'local':local,'repo':repo,'revision':revision,'path':path,'sha256':hashlib.sha256(data).hexdigest()}
 if expected.get(local)!=identity:raise SystemExit('Pinned source identity mismatch: '+local)
 target=OUT/local;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(data);metadata.append({'local':local,'repo':repo,'revision':revision,'path':path,'sha256':hashlib.sha256(data).hexdigest()})
 if path.endswith('.sol') and local.startswith('balancer'):
  for name in re.findall(r'import ["\']([^"\']+)["\']',data.decode()):
   if name.startswith('@balancer-labs/'):
    relative=name[len('@balancer-labs/'):];package,rest=relative.split('/',1);source='pkg/'+package.removeprefix('v2-')+'/'+rest
   else:source=str(Path(path).parent/name);source=str(Path(__import__('os').path.normpath(source)))
   fetch('balancer-imports/'+name if name.startswith('@') else __import__('os').path.normpath(str(Path(local).parent/name)),repo,revision,source)
for args in refs:fetch(*args)
if seen!=set(expected):raise SystemExit('Pinned source manifest coverage mismatch')
print(json.dumps({'primary_sources':len(metadata),'hashes_verified':True}))
