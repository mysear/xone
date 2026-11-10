from pathlib import Path
import os,json,subprocess,time,hashlib,shutil
root=next(p for p in Path(__file__).resolve().parents if (p/'integration/tools/acceptance').is_dir());old=root/'integration/artifacts/local/r01-public-sbf-runtime';work=root/'integration/artifacts/local/r01-solana-rebuild';tools=work/'platform-tools-v1.44';source=old/'orca-whirlpools-cc683ec92acb5e7606caa3d8e514d9383e9c7c11';label='orca-v144-modern-cargo'
env=os.environ.copy();env.update(CARGO_HOME=str(work/'cargo-home-orca-v144'),RUSTUP_HOME=str(work/'rustup-home'),CARGO_TARGET_DIR=str(work/'target-orca-v144'),RUSTC=str(tools/'rust/bin/rustc'),PATH=str(tools/'rust/bin')+os.pathsep+env['PATH'],RUSTFLAGS='-Zremap-cwd-prefix= --sysroot='+str(work/'sysroot-v144-linux'))
for k,v in [('CC','clang'),('AR','llvm-ar'),('OBJCOPY','llvm-objcopy'),('OBJDUMP','llvm-objdump')]:env[k]=str(tools/'llvm/bin'/v)
cmd=[str(old/'platform-tools-v1.52/rust/bin/cargo'),'build','--release','--target','sbf-solana-solana','--manifest-path',str(source/'programs/whirlpool/Cargo.toml'),'--offline','--locked','--jobs','4','--features','mainnet']
log=work/(label+'.log');start=time.monotonic()
with log.open('w') as f:p=subprocess.run(cmd,cwd=source,env=env,stdout=f,stderr=subprocess.STDOUT,timeout=1800)
report={'command':cmd,'environment':{k:env[k] for k in ['CARGO_HOME','RUSTUP_HOME','CARGO_TARGET_DIR','RUSTC','RUSTFLAGS','CC','AR']},'exit_code':p.returncode,'elapsed_seconds':time.monotonic()-start,'reason_for_cargo_189':'Cargo 1.79 rejects upstream lock dev-dependency edition2024 manifests, including time 0.3.55; parser upgraded, target rustc remains official v1.44','source_lock_sha256':hashlib.sha256((source/'Cargo.lock').read_bytes()).hexdigest(),'log':str(log.relative_to(root))}
if not p.returncode:
 src=work/'target-orca-v144/sbf-solana-solana/release/whirlpool.so';dest=work/'output-orca-v144-modern-cargo/whirlpool.so';dest.parent.mkdir(exist_ok=True)
 command=[str(tools/'llvm/bin/llvm-objcopy'),'--strip-all',str(src),str(dest)];q=subprocess.run(command,capture_output=True,text=True,timeout=60);report.update(strip_command=command,strip_exit_code=q.returncode,strip_stdout=q.stdout,strip_stderr=q.stderr)
 if not q.returncode:report['artifact']={'path':str(dest.relative_to(root)),'sha256':hashlib.sha256(dest.read_bytes()).hexdigest(),'bytes':dest.stat().st_size}
(work/(label+'-build.json')).write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
