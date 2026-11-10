#!/usr/bin/env python3
"""Request normal ACPI shutdown of the live task-owned vfkit process only."""
import json,subprocess,urllib.request
from pathlib import Path
stage=Path(__file__).resolve().parent;root=stage.parent;pid=(stage/'ubuntu-vfkit.pid').read_text().strip()
if not pid.isdigit():raise SystemExit('invalid task pidfile')
p=subprocess.run(['/bin/ps','-p',pid,'-o','comm='],capture_output=True,text=True)
if p.returncode:raise SystemExit('task-owned VM process has already exited')
if str(root/'tools/vfkit-0.6.4') not in p.stdout:raise SystemExit('PID no longer identifies the task vfkit executable; no action sent')
record=json.loads((stage/'ubuntu-launch.json').read_text());url=record['task_own_rest_endpoint']+'/vm/state'
request=urllib.request.Request(url,data=b'{"state":"Stop"}',headers={'Content-Type':'application/json'},method='POST')
with urllib.request.urlopen(request,timeout=5) as response:
 receipt={'pid':int(pid),'verified_command':p.stdout.strip(),'request':{'endpoint':url,'state':'Stop'},'http_status':response.status,'body':response.read().decode(),'host_admin_required':False,'HardStop_requested':False}
(stage/'owned-shutdown-receipt.json').write_text(json.dumps(receipt,indent=2)+'\n');print(json.dumps(receipt,indent=2))
