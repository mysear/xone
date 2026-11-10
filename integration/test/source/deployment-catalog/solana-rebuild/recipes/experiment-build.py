from pathlib import Path
import os,sys,json,shutil,subprocess,time,hashlib
root=next(p for p in Path(__file__).resolve().parents if (p/'integration/tools/acceptance').is_dir())
old=root/'integration/artifacts/local/r01-public-sbf-runtime'
work=root/'integration/artifacts/local/r01-solana-rebuild'
sdk=work/'sdk/sbf'
if not sdk.exists():
 shutil.copytree(old/'3.1.10/solana-release/bin/platform-tools-sdk/sbf',sdk,symlinks=True)
 # The upstream SDK dependency links are absolute and read-only cached tool inputs.
 for marker in ['platform-tools-v1.52.md','criterion-v2.3.2.md']:
  if not (sdk/'dependencies'/marker).exists():raise RuntimeError(marker)
name=sys.argv[1]
source={'clmm':old/'raydium-clmm-6d7a0f82d2f293c553c69aaf943377367c6c4cc6','cpmm':work/'sources/cpmm-3d69122790e97cd6aff24e8ba95940793773f58c','orca':old/'orca-whirlpools-cc683ec92acb5e7606caa3d8e514d9383e9c7c11'}[name]
manifest={'clmm':'programs/amm/Cargo.toml','cpmm':'programs/cp-swap/Cargo.toml','orca':'programs/whirlpool/Cargo.toml'}[name]
label=sys.argv[2] if len(sys.argv)>2 else 'remap'
cargo_home=work/'cargo-home';cargo_home.mkdir(exist_ok=True)
(cargo_home/'config.toml').write_text('[source.crates-io]\nreplace-with = "r01-vendor"\n[source.r01-vendor]\ndirectory = '+json.dumps(str(old/'vendor-modern'))+'\n[source.parity-git]\ngit = "https://github.com/raydium-io/parity-common"\nreplace-with = "r01-parity"\n[source.r01-parity]\ndirectory = '+json.dumps(str(old/'clmm-parity-git'))+'\n')
env=os.environ.copy();env.update(CARGO_HOME=str(cargo_home),RUSTUP_HOME=str(work/'rustup-home'),CARGO_TARGET_DIR=str(work/('target-'+name+'-'+('remap' if label=='linux-libs' else label))),RUSTC=str(old/'platform-tools-v1.52/rust/bin/rustc'),PATH=str(old/'platform-tools-v1.52/rust/bin')+os.pathsep+env['PATH'])
flags={'remap':'--remap-path-prefix=/Users/runner=/home/runner','default':'','linux-libs':'--remap-path-prefix=/Users/runner=/home/runner --sysroot='+str(work/'linux-sysroot')}
env['RUSTFLAGS']=flags[label]
output=work/('output-'+name+'-'+label)
cmd=[str(old/'3.1.10/solana-release/bin/cargo-build-sbf'),'--manifest-path',str(source/manifest),'--offline','--skip-tools-install','--no-rustup-override','--sbf-sdk',str(sdk),'--sbf-out-dir',str(output),'--jobs','4','--','--locked']
if label=='linux-libs':env['RUSTFLAGS']=flags[label]
log=work/(name+'-'+label+'.log');started=time.monotonic()
with log.open('w') as f:p=subprocess.run(cmd,cwd=source,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=1800)
record={'command':cmd,'env':{k:env[k] for k in ['CARGO_HOME','RUSTUP_HOME','CARGO_TARGET_DIR','RUSTC','RUSTFLAGS']},'exit_code':p.returncode,'elapsed_seconds':time.monotonic()-started,'source_path':str(source.relative_to(root)),'source_lock_sha256':hashlib.sha256((source/'Cargo.lock').read_bytes()).hexdigest(),'log':str(log.relative_to(root))}
for artifact in output.glob('*.so'):record['artifact']={'path':str(artifact.relative_to(root)),'sha256':hashlib.sha256(artifact.read_bytes()).hexdigest(),'bytes':artifact.stat().st_size}
(work/(name+'-'+label+'-build.json')).write_text(json.dumps(record,indent=2)+'\n')
print(json.dumps(record,indent=2))
