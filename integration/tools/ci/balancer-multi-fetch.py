#!/usr/bin/env python3
"""Fetch exact official Solidity import closures. Never writes old sources/fixtures."""
import base64, concurrent.futures, hashlib, json, posixpath, re, subprocess
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
DEST=ROOT/'integration/tools/reference/balancer-multi'
REPO='balancer/balancer-v2-monorepo'
OLD='e281ff40583c39bf1d46c322334b2b13936e0e33'
NEW='e91a2b643a49856f51a648d175667c1b48cf3377'

def acquire(rev, roots):
    manifest=DEST/(rev+'.json')
    if manifest.exists():
        data=json.loads(manifest.read_text())
        for f in data['files']:
            assert hashlib.sha256((DEST/'sources'/rev/f['path']).read_bytes()).hexdigest()==f['sha256']
        return data
    files={}; provenance={}
    if rev==OLD:
        baseline=json.loads((ROOT/'integration/tools/reference/weighted-model/source-metadata.json').read_text())
        for f in baseline['files']:
            b=(ROOT/'integration/.runtime/weighted-reference/sources'/f['path']).read_bytes()
            assert hashlib.sha256(b).hexdigest()==f['sha256']
            files[f['path']]=b;provenance[f['path']]='existing hash-verified pinned closure'
    pending=set(roots);seen=set()
    def fetch(p):
        local=DEST/'sources'/rev/p
        if local.exists():return p,local.read_bytes(),'previous official acquisition'
        b=subprocess.check_output(['curl','--http1.1','-fLsS','--max-time','45','--retry','2',
            f'https://raw.githubusercontent.com/{REPO}/{rev}/{p}'])
        proof='official raw content at immutable commit'
        local.parent.mkdir(parents=True,exist_ok=True)
        local.write_bytes(b)
        return p,b,proof
    while pending:
        batch=sorted(pending-seen);pending=set()
        if not batch:break
        needed=[p for p in batch if p not in files]
        with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
            for p,b,proof in pool.map(fetch,needed):
                files[p]=b;provenance[p]=proof
                target=DEST/'sources'/rev/p;target.parent.mkdir(parents=True,exist_ok=True)
                if not target.exists():target.write_bytes(b)
                print(rev[:7],p,flush=True)
        for p in batch:
            seen.add(p)
            for imp in re.findall(r'import\s+[\s\S]*?["\']([^"\']+)["\']\s*;',files[p].decode()):
                q='pkg/'+imp[len('@balancer-labs/v2-'):] if imp.startswith('@balancer-labs/v2-') else posixpath.join(posixpath.dirname(p),imp) if imp.startswith('.') else imp
                pending.add(posixpath.normpath(q))
    rows=[]
    for p in sorted(seen):
        b=files[p];target=DEST/'sources'/rev/p;target.parent.mkdir(parents=True,exist_ok=True)
        if target.exists():assert target.read_bytes()==b
        else:target.write_bytes(b)
        rows.append({'path':p,'sha256':hashlib.sha256(b).hexdigest(),'acquisition':provenance[p]})
    data={'repo':REPO,'revision':rev,'roots':roots,'files':rows}
    manifest.write_text(json.dumps(data,indent=2)+'\n');print('PINNED',rev,len(rows),flush=True)
    return data
if __name__=='__main__':
    acquire(OLD,['pkg/pool-weighted/contracts/WeightedPool.sol','pkg/pool-weighted/contracts/smart/ManagedPool.sol','pkg/vault/contracts/balances/BalanceAllocation.sol'])
    acquire(NEW,['pkg/pool-stable/contracts/StableMath.sol'])
