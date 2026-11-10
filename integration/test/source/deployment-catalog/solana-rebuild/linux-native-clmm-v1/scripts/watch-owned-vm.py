#!/usr/bin/env python3
"""Bound startup and one fixed build; ACPI-stop only this task's vfkit VM."""
import json,time,urllib.request,urllib.error
from pathlib import Path
stage=Path(__file__).resolve().parent;record=json.loads((stage/'ubuntu-launch.json').read_text());url=record['task_own_rest_endpoint']+'/vm/state';launch_time=(stage/'ubuntu-launch.json').stat().st_mtime;events=[]
def state():
 with urllib.request.urlopen(url,timeout=3) as r:return json.loads(r.read())
def save():
 (stage/'vm-watch.json').write_text(json.dumps({'scope':'only REST endpoint from task-owned ubuntu-launch.json','pre_compiler_budget_seconds':900,'single_build_budget_seconds':1860,'events':events},indent=2)+'\n')
last=None
while True:
 if (stage/'ubuntu-launch-exit.json').exists() and (stage/'ubuntu-launch-exit.json').stat().st_mtime>=launch_time:
  events.append({'event':'owned-launch-exited','timestamp':time.time(),'exit':json.loads((stage/'ubuntu-launch-exit.json').read_text())});save();break
 probe=stage/'output/linux-compiler-probe.json';build=stage/'output/clmm-linux-native-build.json';exitfile=stage/'output/guest-exit.json'
 phase='guest-finished' if exitfile.exists() else 'build-finished' if build.exists() else 'compiler-probe-written' if probe.exists() else 'boot-or-prerequisites'
 if phase!=last:events.append({'event':'phase-observed','phase':phase,'timestamp':time.time()});last=phase;save()
 budget=60 if exitfile.exists() else 1860 if probe.exists() else 900
 start=exitfile.stat().st_mtime if exitfile.exists() else probe.stat().st_mtime if probe.exists() else launch_time
 if time.time()-start>budget:
  try:
   before=state();request=urllib.request.Request(url,data=b'{"state":"Stop"}',headers={'Content-Type':'application/json'},method='POST')
   with urllib.request.urlopen(request,timeout=5) as r:response={'http_status':r.status,'body':r.read().decode()}
   events.append({'event':'owned-ACPI-stop-requested','phase':phase,'elapsed_seconds':time.time()-start,'budget_seconds':budget,'state_before':before,'response':response});save()
  except Exception as e:events.append({'event':'owned-stop-error','error':str(e)});save()
  break
 time.sleep(3)
