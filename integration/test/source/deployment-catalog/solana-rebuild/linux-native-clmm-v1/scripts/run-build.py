import hashlib,json,os,subprocess,time
from pathlib import Path
out=Path('/output')
def hashfile(p):
 h=hashlib.sha256()
 with Path(p).open('rb') as f:
  while chunk:=f.read(1024*1024):h.update(chunk)
 return h.hexdigest()
def command(args):
 p=subprocess.run(args,capture_output=True,text=True)
 return {'command':args,'exit_code':p.returncode,'stdout':p.stdout,'stderr':p.stderr}
commands=[['uname','-a'],['getconf','GNU_LIBC_VERSION'],['cat','/etc/os-release'],['nproc'],['free','-b'],['lsblk','-b','-o','NAME,SIZE,RO,MOUNTPOINTS'],['findmnt','-J'],['/platform-tools/rust/bin/rustc','-Vv'],['/platform-tools/rust/bin/cargo','-Vv'],['/platform-tools/llvm/bin/clang','--version'],['/usr/local/bin/cargo-build-sbf','--version'],['ldd','/platform-tools/rust/bin/rustc'],['ldd','/usr/local/bin/cargo-build-sbf'],['dpkg-query','-W']]
observations=[command(c) for c in commands]
pins=[{'path':p,'sha256':hashfile(p)} for p in ['/platform-tools/rust/bin/rustc','/platform-tools/rust/bin/cargo','/platform-tools/llvm/bin/clang','/platform-tools/llvm/bin/llvm-objcopy','/usr/local/bin/cargo-build-sbf','/build/Cargo.lock','/sdk/sbf/scripts/strip.sh','/sdk/sbf/scripts/install.sh']]
readonly=[]
for path in ['/build','/vendor','/platform-tools','/sdk']:
 try:
  (Path(path)/'r01-write-must-fail').write_text('not-authorized')
  readonly.append({'path':path,'readonly_enforced':False})
 except OSError as e:readonly.append({'path':path,'readonly_enforced':e.errno==30,'errno':e.errno,'error':str(e)})
env_keys=['CARGO_HOME','RUSTUP_HOME','CARGO_TARGET_DIR','RUSTC','RUSTFLAGS','CARGO_BUILD_JOBS','PATH','GIT_CONFIG_NOSYSTEM','GIT_CONFIG_GLOBAL','GIT_TERMINAL_PROMPT']
report={'commands':observations,'toolchain_and_lock_pins':pins,'readonly_probes':readonly,'environment':{k:os.environ[k] for k in env_keys},'guest_login_environment':{k:os.environ.get(k) for k in ['HOME','USER','LOGNAME']}}
(out/'linux-compiler-probe.json').write_text(json.dumps(report,indent=2)+'\n')
if any(not p['readonly_enforced'] for p in readonly):raise SystemExit('readonly input mount proof failed')
if any(p['exit_code'] for p in observations[:13]):raise SystemExit('Linux/glibc/compiler probe failed before build')
expected_lock='9370048a70726a60e281003ff798d425ce81b84ce42343d3ffdeb91def2a7d8a'
if hashfile('/build/Cargo.lock')!=expected_lock:raise SystemExit('fixed CLMM Cargo.lock differs')
source_before={str(p.relative_to('/build')):hashfile(p) for p in Path('/build').rglob('*') if p.is_file()}
args=['/usr/local/bin/cargo-build-sbf','--manifest-path','/build/programs/amm/Cargo.toml','--offline','--skip-tools-install','--no-rustup-override','--sbf-sdk','/sdk/sbf','--sbf-out-dir','/output/deploy','--jobs','2','--verbose','--','--locked']
start=time.monotonic();error=None
with (out/'clmm-linux-native-build.log').open('w') as f:
 try:r=subprocess.run(args,cwd='/build',stdout=f,stderr=subprocess.STDOUT,timeout=1800);exit_code=r.returncode
 except subprocess.TimeoutExpired:exit_code=124;error='single fixed CLMM build exceeded 1800 seconds'
source_after={str(p.relative_to('/build')):hashfile(p) for p in Path('/build').rglob('*') if p.is_file()}
artifacts=[]
for label,directory in [('SDK-standard-output',Path('/output/deploy')),('unstripped-cargo-output',Path('/build-target/sbf-solana-solana/release')),('unstripped-cargo-output-sbpfv0',Path('/build-target/sbpf-solana-solana/release'))]:
 for p in directory.glob('*.so'):
  if label.startswith('unstripped'):
   import shutil
   target=out/(p.stem+'-unstripped.so');shutil.copyfile(p,target);p=target
  artifacts.append({'kind':label,'path':str(p),'bytes':p.stat().st_size,'sha256':hashfile(p)})
record={'command':args,'cwd':'/build','environment':{k:os.environ[k] for k in env_keys},'source_commit':'6d7a0f82d2f293c553c69aaf943377367c6c4cc6','Cargo_lock_sha256_before':expected_lock,'Cargo_lock_sha256_after':hashfile('/build/Cargo.lock'),'source_tree_before':source_before,'source_tree_after':source_after,'source_bytes_unchanged':source_before==source_after,'exit_code':exit_code,'elapsed_seconds':time.monotonic()-start,'error':error,'artifacts':artifacts,'native_Linux_host':True,'postprocessing_authority':'unmodified official SDK standard pipeline only; unstripped file separately retained; no extra byte edits or trimming'}
(out/'clmm-linux-native-build.json').write_text(json.dumps(record,indent=2)+'\n')
# Retain original section/relocation dumps using the same Linux LLVM tools.
for p in Path('/output/deploy').glob('*.so'):
 for option,name in [(['--file-headers','--sections','--relocations'],'elf-layout-relocations'),(['--notes'],'elf-notes')]:
  result=command(['/platform-tools/llvm/bin/llvm-readelf',*option,str(p)])
  (out/(p.stem+'-'+name+'.json')).write_text(json.dumps(result,indent=2)+'\n')
if exit_code:raise SystemExit(exit_code)
if source_before!=source_after:raise SystemExit('source tree changed during build')
