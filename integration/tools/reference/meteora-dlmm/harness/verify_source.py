"""Verify the exact retained upstream file subset against its pinned Git tree."""
from pathlib import Path
import hashlib,json
BASE=Path(__file__).resolve().parents[1]
def verify():
    m=json.loads((BASE/'source-metadata.json').read_bytes());tree_raw=(BASE/'upstream-tree.json').read_bytes()
    if hashlib.sha256(tree_raw).hexdigest()!=m['tree_sha256']:raise ValueError('DLMM_TREE_CHANGED')
    t=json.loads(tree_raw)
    if t.get('truncated'):raise ValueError('DLMM_TREE_INCOMPLETE')
    tree={x['path']:x for x in t['tree']if x['type']=='blob'};seen=set()
    for row in m['files']:
        if row['path']in seen:raise ValueError('DLMM_DUPLICATE_SOURCE')
        seen.add(row['path']);path=(BASE/row['path']).resolve()
        if not path.is_relative_to(BASE.resolve()):raise ValueError('DLMM_NONLOCAL_SOURCE')
        raw=path.read_bytes();blob=hashlib.sha1(b'blob '+str(len(raw)).encode()+b'\0'+raw).hexdigest()
        if len(raw)!=row['bytes']or hashlib.sha256(raw).hexdigest()!=row['sha256']or blob!=row['git_blob_sha1']or tree[row['path']]['sha']!=blob:raise ValueError('DLMM_PINNED_SOURCE_CHANGED:'+row['path'])
    return dict(passed=True,commit=m['commit'],files=len(seen),bytes=sum(r['bytes']for r in m['files']),authority=m['authority'])
if __name__=='__main__':print(json.dumps(verify()))
