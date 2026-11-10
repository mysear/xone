from pathlib import Path
import urllib.request,json,time,hashlib,tarfile,shutil
root=next(p for p in Path(__file__).resolve().parents if (p/'integration/tools/acceptance').is_dir());work=root/'integration/artifacts/local/r01-solana-rebuild';old=root/'integration/artifacts/local/r01-public-sbf-runtime';out=root/'integration/test/source/deployment-catalog/solana-rebuild/public'
url='https://github.com/anza-xyz/platform-tools/releases/download/v1.52/platform-tools-linux-x86_64.tar.bz2';archive=work/'platform-tools-linux-x86_64-v1.52.tar.bz2'
if not archive.exists():
 pending=archive.with_suffix('.pending')
 # Resume only explicit byte ranges; never append an unverified full response.
 start=pending.stat().st_size if pending.exists() else 0
 request=urllib.request.Request(url,headers={'User-Agent':'XONE-public-rebuild/1.0','Range':f'bytes={start}-'})
 with urllib.request.urlopen(request,timeout=60) as r:
  if start and (r.status!=206 or not r.headers.get('Content-Range','').startswith(f'bytes {start}-')):raise RuntimeError('range-refused')
  with pending.open('ab' if start else 'wb') as f:
   total=start;last=time.monotonic()
   while chunk:=r.read(1024*1024):
    total+=len(chunk)
    if total>550*1024*1024:raise RuntimeError('archive-limit')
    f.write(chunk)
    if time.monotonic()-last>20:print('downloaded',total,flush=True);last=time.monotonic()
 pending.rename(archive)
files=[];target=work/'linux-sysroot';target.mkdir(exist_ok=True)
# Keep native macOS host binaries/host std untouched; target SBF libraries are
# public, architecture-independent rlibs built by the Linux release pipeline.
for path in (old/'platform-tools-v1.52/rust').iterdir():
 if path.name!='lib' and not (target/path.name).exists():(target/path.name).symlink_to(path)
(target/'lib').mkdir(exist_ok=True);(target/'lib/rustlib').mkdir(exist_ok=True)
for path in (old/'platform-tools-v1.52/rust/lib').iterdir():
 if path.name!='rustlib' and not (target/'lib'/path.name).exists():(target/'lib'/path.name).symlink_to(path)
for path in (old/'platform-tools-v1.52/rust/lib/rustlib').iterdir():
 if not path.name.startswith(('sbf','sbpf')) and not (target/'lib/rustlib'/path.name).exists():(target/'lib/rustlib'/path.name).symlink_to(path)
with tarfile.open(archive,'r:bz2') as tf:
 for m in tf:
  if not m.isfile():continue
  parts=Path(m.name).parts
  if 'rustlib' not in parts:continue
  idx=parts.index('rustlib')
  if idx+1>=len(parts) or not parts[idx+1].startswith(('sbf','sbpf')):continue
  rel=Path('lib',*parts[idx:]);p=target/rel
  if '..' in rel.parts:raise RuntimeError('archive-traversal')
  data=tf.extractfile(m).read();p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes(data)
  files.append({'path':str(p.relative_to(root)),'sha256':hashlib.sha256(data).hexdigest(),'bytes':len(data),'archive_member':m.name})
(out/'linux-v152-target-libs.json').write_text(json.dumps({'url':url,'archive':str(archive.relative_to(root)),'archive_sha256':hashlib.sha256(archive.read_bytes()).hexdigest(),'files':files,'source_modified':False,'elf_patched':False,'native_host_compiler':'official cached macOS v1.52','target_sysroot':'official Linux v1.52 SBF rlibs','not_full_linux_host_reproduction':True},indent=2)+'\n');print('extracted',len(files),flush=True)
