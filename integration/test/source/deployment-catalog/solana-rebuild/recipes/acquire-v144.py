from pathlib import Path
import urllib.request,json,tarfile,hashlib,concurrent.futures,time
root=next(p for p in Path(__file__).resolve().parents if (p/'integration/tools/acceptance').is_dir());work=root/'integration/artifacts/local/r01-solana-rebuild';out=root/'integration/test/source/deployment-catalog/solana-rebuild/public'
def acquire(platform):
 url=f'https://github.com/anza-xyz/platform-tools/releases/download/v1.44/platform-tools-{platform}.tar.bz2';archive=work/f'platform-tools-{platform}-v1.44.tar.bz2';target=work/('platform-tools-v1.44' if platform=='osx-x86_64' else 'linux-v144-libs')
 if not archive.exists():
  pending=archive.with_suffix('.pending');start=pending.stat().st_size if pending.exists() else 0
  with urllib.request.urlopen(urllib.request.Request(url,headers={'User-Agent':'XONE-public-rebuild','Range':f'bytes={start}-'}),timeout=60) as r:
   if start and (r.status!=206 or not r.headers.get('Content-Range','').startswith(f'bytes {start}-')):raise ValueError('range-refused')
   with pending.open('ab' if start else 'wb') as f:
    total=start;last=time.monotonic()
    while chunk:=r.read(1024*1024):
     total+=len(chunk)
     if total>550*1024*1024:raise ValueError('size-limit')
     f.write(chunk)
     if time.monotonic()-last>25:print(platform,total,flush=True);last=time.monotonic()
  pending.rename(archive)
 files=[];target.mkdir(exist_ok=True)
 with tarfile.open(archive) as tf:
  for m in tf:
   path=Path(m.name)
   if path.is_absolute() or '..' in path.parts:raise ValueError('unsafe-path')
   if platform.startswith('linux') and not ('rustlib/sbf-solana-solana/' in m.name or 'rustlib/sbpf-solana-solana/' in m.name):continue
   p=target/path
   if m.isfile():
    data=tf.extractfile(m).read();p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(data);p.chmod(m.mode&0o777)
    if platform.startswith('linux') or '/bin/' in m.name:files.append({'path':str(p.relative_to(root)),'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest(),'archive_member':m.name})
   elif m.issym():
    require=(p.parent/m.linkname).resolve().is_relative_to(target.resolve())
    if not require:raise ValueError('unsafe-link')
    p.parent.mkdir(parents=True,exist_ok=True)
    if not p.exists() and not p.is_symlink():p.symlink_to(m.linkname)
 h=hashlib.sha256()
 with archive.open('rb') as f:
  while chunk:=f.read(1024*1024):h.update(chunk)
 (out/f'platform-v144-{platform}.json').write_text(json.dumps({'url':url,'archive':str(archive.relative_to(root)),'sha256':h.hexdigest(),'files':files},indent=2)+'\n');print(platform,'complete',len(files),flush=True)
with concurrent.futures.ThreadPoolExecutor(max_workers=2) as ex:
 for result in ex.map(acquire,['osx-x86_64','linux-x86_64']):pass
