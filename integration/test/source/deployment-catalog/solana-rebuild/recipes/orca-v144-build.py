from pathlib import Path
import sys,os,json,shutil,subprocess,time,hashlib
root=next(p for p in Path(__file__).resolve().parents if (p/'integration/tools/acceptance').is_dir());old=root/'integration/artifacts/local/r01-public-sbf-runtime';work=root/'integration/artifacts/local/r01-solana-rebuild';source=old/'orca-whirlpools-cc683ec92acb5e7606caa3d8e514d9383e9c7c11';tools=work/'platform-tools-v1.44';sdk=work/'sdk-v144/sbf';label=sys.argv[1] if len(sys.argv)>1 else 'default'
if not sdk.exists():
 shutil.copytree(work/'sdk/sbf',sdk,symlinks=True)
 link=sdk/'dependencies/platform-tools';link.unlink();link.symlink_to(tools)
 (sdk/'dependencies/platform-tools-v1.44.md').touch()
# Host tools remain unmodified. Select Linux's original target rlibs with a
# separate sysroot tree; this is an explicitly mixed-host experiment.
linux=work/'sysroot-v144-linux'
if not linux.exists():
 shutil.copytree(tools/'rust',linux,symlinks=True)
 for path in (work/'linux-v144-libs/rust/lib/rustlib').glob('*'):
  dest=linux/'lib/rustlib'/path.name
  if dest.exists():shutil.rmtree(dest)
  shutil.copytree(path,dest)
home=work/'cargo-home-orca-v144';env=os.environ.copy();env.update(CARGO_HOME=str(home),RUSTUP_HOME=str(work/'rustup-home'),CARGO_TARGET_DIR=str(work/'target-orca-v144'),RUSTC=str(tools/'rust/bin/rustc'),PATH=str(tools/'rust/bin')+os.pathsep+env['PATH'])
if label=='linux-libs':env['RUSTFLAGS']='--sysroot='+str(linux)
else:env['RUSTFLAGS']=''
output=work/('output-orca-v144-'+label);cmd=[str(old/'3.1.10/solana-release/bin/cargo-build-sbf'),'--manifest-path',str(source/'programs/whirlpool/Cargo.toml'),'--tools-version','v1.44','--offline','--skip-tools-install','--no-rustup-override','--sbf-sdk',str(sdk),'--sbf-out-dir',str(output),'--jobs','4','--','--locked','--features','mainnet']
log=work/('orca-v144-'+label+'.log');start=time.monotonic()
with log.open('w') as stream:p=subprocess.run(cmd,cwd=source,env=env,stdout=stream,stderr=subprocess.STDOUT,timeout=1800)
report={'command':cmd,'environment':{k:env[k] for k in ['CARGO_HOME','RUSTUP_HOME','CARGO_TARGET_DIR','RUSTC','RUSTFLAGS']},'exit_code':p.returncode,'elapsed_seconds':time.monotonic()-start,'log':str(log.relative_to(root)),'source_commit':'cc683ec92acb5e7606caa3d8e514d9383e9c7c11','source_lock_sha256':hashlib.sha256((source/'Cargo.lock').read_bytes()).hexdigest()}
for p in output.glob('*.so'):report['artifact']={'path':str(p.relative_to(root)),'sha256':hashlib.sha256(p.read_bytes()).hexdigest(),'bytes':p.stat().st_size}
(work/('orca-v144-'+label+'-build.json')).write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
