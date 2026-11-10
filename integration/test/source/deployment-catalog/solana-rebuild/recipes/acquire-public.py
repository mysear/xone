from pathlib import Path
import urllib.request,json,concurrent.futures,hashlib,datetime,tarfile,io
root=next(p for p in Path(__file__).resolve().parents if (p/'integration/tools/acceptance').is_dir());out=root/'integration/test/source/deployment-catalog/solana-rebuild/public';out.mkdir(exist_ok=True)
urls={
 'cpmm-commits.json':'https://api.github.com/repos/raydium-io/raydium-cp-swap/commits?per_page=100',
 'clmm-commits.json':'https://api.github.com/repos/raydium-io/raydium-clmm/commits?per_page=100',
 'orca-program-commits.json':'https://api.github.com/repos/orca-so/whirlpools/commits?path=programs/whirlpool&per_page=100',
 'orca-build-sh.txt':'https://raw.githubusercontent.com/orca-so/whirlpools/cc683ec92acb5e7606caa3d8e514d9383e9c7c11/scripts/setup.sh',
 'solders-pypi.json':'https://pypi.org/pypi/solders/0.29.0/json',
 'platform-v152-release.json':'https://api.github.com/repos/anza-xyz/platform-tools/releases/tags/v1.52',
}
def get(item):
 name,url=item;p=out/name
 if p.exists():return name,'cached'
 started=datetime.datetime.now(datetime.timezone.utc).isoformat()
 try:
  with urllib.request.urlopen(urllib.request.Request(url,headers={'User-Agent':'XONE-public-rebuild/1.0'}),timeout=45) as r:raw=r.read(8*1024*1024);status=r.status;final=r.url
  p.write_bytes(raw);(out/(name+'.receipt.json')).write_text(json.dumps({'url':url,'final_url':final,'observed_at':started,'http_status':status,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest()},indent=2)+'\n');return name,len(raw)
 except Exception as e:(out/(name+'.error.json')).write_text(json.dumps({'url':url,'observed_at':started,'error':str(e)},indent=2)+'\n');return name,str(e)
with concurrent.futures.ThreadPoolExecutor(max_workers=6) as ex:
 for r in ex.map(get,urls.items()):print(r,flush=True)
url='https://codeload.github.com/raydium-io/raydium-cp-swap/tar.gz/3d69122790e97cd6aff24e8ba95940793773f58c'
work=root/'integration/artifacts/local/r01-solana-rebuild';p=work/'cpmm-3d691227.tar.gz'
if not p.exists():p.write_bytes(urllib.request.urlopen(url,timeout=45).read())
dest=work/'sources/cpmm-3d69122790e97cd6aff24e8ba95940793773f58c';dest.mkdir(parents=True,exist_ok=True)
with tarfile.open(p) as tf:
 for m in tf.getmembers():
  rel=Path(*Path(m.name).parts[1:])
  if not rel.parts:continue
  if '..' in rel.parts or rel.is_absolute() or m.issym() or m.islnk():raise ValueError('unsafe archive')
  if m.isfile():target=dest/rel;target.parent.mkdir(parents=True,exist_ok=True);target.write_bytes(tf.extractfile(m).read())
files=[{'path':str(x.relative_to(dest)),'bytes':x.stat().st_size,'sha256':hashlib.sha256(x.read_bytes()).hexdigest()} for x in sorted(dest.rglob('*')) if x.is_file()]
(out/'cpmm-source.json').write_text(json.dumps({'url':url,'commit':'3d69122790e97cd6aff24e8ba95940793773f58c','archive':str(p.relative_to(root)),'archive_sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'source_path':str(dest.relative_to(root)),'files':files},indent=2)+'\n');print('cpmm source',len(files))
