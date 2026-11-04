#!/usr/bin/env python3
"""Run fresh implemented-scope verification and retain exact command results."""
from pathlib import Path
import hashlib,json,os,platform,secrets,subprocess,time
root=Path(__file__).resolve().parents[3];out=root/'integration/artifacts/local';out.mkdir(parents=True,exist_ok=True)
def write_result(value):
 target=out/'verification.json';temporary=target.with_name(target.name+'.'+secrets.token_hex(6)+'.tmp')
 try:temporary.write_text(json.dumps(value,indent=2)+'\n');temporary.replace(target)
 finally:temporary.unlink(missing_ok=True)
write_result({'passed':False,'state':'RUNNING','started_at_unix_ms':int(time.time()*1000)})

def source_hash():
 h=hashlib.sha256()
 for project in ['chain-parser-go','chain-parser-rust','dex-router','common/protoc']:
  files=[]
  for directory,dirs,names in os.walk(root/project):
   dirs[:]=[d for d in dirs if d not in {'target','bin','.runtime','secrets','.git','__pycache__'}]
   for name in names:
    p=Path(directory)/name
    if p.suffix in ['.go','.rs','.toml','.mod','.sum','.lock','.proto','.yaml','.json','.py'] or name=='Makefile':files.append(p)
  for p in sorted(files):h.update(str(p.relative_to(root)).encode());h.update(p.read_bytes())
 return h.hexdigest()

result={'schema_version':1,'scope':'implemented-local-scope','production_ready':False,'source_tree_sha256':source_hash(),'platform':platform.platform(),'cpu_count':os.cpu_count(),'commands':[],'checked_binary_sha256':{},'tool_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
for project in ['chain-parser-go','chain-parser-rust','dex-router','common/protoc']:
 command=['make','check'];print(f'Checking {project} ...',flush=True);started=time.monotonic();log=out/('verification-'+project.replace('/','-')+'.log')
 with log.open('w') as output:process=subprocess.run(command,cwd=root/project,stdout=output,stderr=subprocess.STDOUT)
 result['commands'].append({'project':project,'command':command,'exit_code':process.returncode,'elapsed_ms':round((time.monotonic()-started)*1000),'log':log.name})
 print(f'{project}: exit {process.returncode}',flush=True)
 if process.returncode==0 and project in ['chain-parser-go','chain-parser-rust','dex-router']:
  binary=root/project/('bin/chain-parser-go' if project=='chain-parser-go' else 'target/debug/'+project)
  result['checked_binary_sha256'][str(binary.relative_to(root))]=hashlib.sha256(binary.read_bytes()).hexdigest()
 if process.returncode:print(log.read_text()[-6000:])
for project in ['chain-parser-go','chain-parser-rust']:
 command=['make','debug-build'];log=out/('verification-debug-'+project+'.log');started=time.monotonic()
 with log.open('w') as stream:p=subprocess.run(command,cwd=root/project,stdout=stream,stderr=subprocess.STDOUT)
 result['commands'].append({'project':project,'command':command,'exit_code':p.returncode,'elapsed_ms':round((time.monotonic()-started)*1000),'log':log.name})
 if p.returncode==0:
  binary=root/project/('bin/fixture-parser-go' if project=='chain-parser-go' else 'target/debug/fixture-parser-rust')
  result['checked_binary_sha256'][str(binary.relative_to(root))]=hashlib.sha256(binary.read_bytes()).hexdigest()
 else:print(log.read_text()[-6000:])
for tool in ['contracts.py','boundary.py']:
 command=['python3',str(root/'integration/tools/ci'/tool)];p=subprocess.run(command,capture_output=True,text=True);result['commands'].append({'project':'integration','command':['python3','tools/ci/'+tool],'exit_code':p.returncode,'output':p.stdout+p.stderr});print(p.stdout+p.stderr,end='',flush=True)
result['toolchains']={}
for name,cmd in [('go',['go','version']),('rustc',[str(Path.home()/'.cargo/bin/rustc'),'+1.90.0','--version']),('protoc',['protoc','--version'])]:
 try:result['toolchains'][name]=subprocess.run(cmd,capture_output=True,text=True,check=True).stdout.strip()
 except (OSError,subprocess.CalledProcessError):result['toolchains'][name]='unavailable'
result['source_unchanged_during_verification']=result['source_tree_sha256']==source_hash();result['verification_tool_unchanged']=result['tool_sha256']==hashlib.sha256(Path(__file__).read_bytes()).hexdigest();result['passed']=result['verification_tool_unchanged'] and all(c['exit_code']==0 for c in result['commands']) and result['source_unchanged_during_verification'];result['measured_at_unix_ms']=int(time.time()*1000);write_result(result);print(json.dumps({'passed':result['passed'],'report':str(out/'verification.json'),'production_ready':False}));raise SystemExit(0 if result['passed'] else 1)
