#!/usr/bin/env python3
"""Reexecute the fixed CLMM build with a fresh guest Cargo target directory."""
import datetime,json,subprocess,sys
from pathlib import Path
stage=Path(__file__).resolve().parent
pidfile=stage/'ubuntu-vfkit.pid'
if pidfile.exists() and pidfile.read_text().strip().isdigit():
 r=subprocess.run(['/bin/ps','-p',pidfile.read_text().strip(),'-o','pid='],capture_output=True,text=True)
 if r.returncode==0:raise SystemExit('recorded task VM PID is still present; do not launch or stop any other VM')
stamp=datetime.datetime.now(datetime.timezone.utc).strftime('%Y%m%dT%H%M%SZ')
archive=stage/'replay-history'/stamp;archive.mkdir(parents=True,exist_ok=False)
for name in ['ubuntu-launch.json','ubuntu-launch-exit.json','ubuntu-console.typescript','vm-watch.json']:
 p=stage/name
 if p.exists():p.rename(archive/name)
if (stage/'output').exists():(stage/'output').rename(archive/'output')
(stage/'output').mkdir()
meta=stage/'cloud-init/meta-data';obj=json.loads(meta.read_text());obj['instance-id']='r01-clmm-linux-v152-'+stamp;meta.write_text(json.dumps(obj,indent=2)+'\n')
user=stage/'cloud-init/user-data';obj=json.loads(user.read_text().split('\n',1)[1]);command=obj['runcmd'][0][2]
command=command.replace("--command 'exec bash /inputs/bootstrap.sh'","--command 'rm -rf /build-target; exec bash /inputs/bootstrap.sh'")
obj['runcmd'][0][2]=command;user.write_text('#cloud-config\n'+json.dumps(obj,indent=2)+'\n')
(stage/'replay-invocation.json').write_text(json.dumps({'archived_previous_run':str(archive),'fresh_guest_target_only':'/build-target','original_host_inputs_or_sealed_evidence_deleted':False,'fixed_input_iso':str(stage/'clmm-inputs-readonly.iso'),'command':[sys.executable,str(stage/'launch-ubuntu.py')]},indent=2)+'\n')
raise SystemExit(subprocess.call([sys.executable,str(stage/'launch-ubuntu.py')]))
