#!/usr/bin/env python3
"""Execute each selected process scope within the checked build/input binding."""
from pathlib import Path
import json,os,secrets,subprocess,sys,time
ROOT=Path(__file__).resolve().parents[3];OUT=ROOT/'integration/artifacts/local'
def startup_result(path,value):
 path=Path(path);path.parent.mkdir(parents=True,exist_ok=True);temporary=path.with_name(path.name+'.'+secrets.token_hex(6)+'.tmp')
 try:temporary.write_text(json.dumps(value,indent=2)+'\n');temporary.replace(path)
 finally:temporary.unlink(missing_ok=True)
def invalidate(prefix,producers):
 for name in [prefix+'-regression-command.json']+[entry[1] for entry in producers]:startup_result(OUT/name,{'passed':False,'state':'RUNNING'})
def run(evidence=None,producers=None,prefix='weighted'):
 if producers is None:producers=[('smoke.py','three-service-smoke.json'),('stable_smoke.py','stable-three-service-smoke.json'),('weighted_smoke.py','weighted-three-service-smoke.json')]
 # This boundary is callable as well as a CLI; reset before imports or reads.
 invalidate(prefix,producers)
 try:
  return execute(evidence,producers,prefix)
 except BaseException as e:
  startup_result(OUT/(prefix+'-regression-command.json'),{'passed':False,'error':str(e),'error_type':type(e).__name__})
  raise
def execute(evidence,producers,prefix):
 if evidence is None:
  import weighted_evidence as evidence
 load=evidence.load;require=evidence.require;runtime_fingerprint=evidence.runtime_fingerprint;snapshot_inputs=evidence.snapshot_inputs;sha=evidence.sha;write_result=evidence.write_result
 binding_name=prefix+'-runtime-binding.json';command_name=prefix+'-regression-command.json'
 binding=load(binding_name);require(binding['passed'] is True,'binding not ready');binding_hash=sha(OUT/binding_name);commands=[]
 for entry in producers:
  script,report=entry[:2];arguments=list(entry[2]) if len(entry)>2 else []
  target=OUT/report;write_result(target,{'passed':False,'state':'RUNNING'})
  try:
   before=runtime_fingerprint();inputs=snapshot_inputs();started=int(time.time()*1000)
   require(before==binding['runtime'] and inputs==binding['input_hashes'] and started>=binding['measured_at_unix_ms'],'inputs/build changed before execution')
   suffix='-'+ '-'.join(v.lstrip('-').replace('/','_')for v in arguments) if arguments else ''
   log=OUT/(prefix+'-regression-'+script.removesuffix('.py')+suffix+'.log')
   env=os.environ.copy();env.pop('PYTHONOPTIMIZE',None)
   with log.open('w') as stream:p=subprocess.run([sys.executable,str(Path(__file__).parent/script)]+arguments,cwd=ROOT,stdout=stream,stderr=subprocess.STDOUT,env=env)
   ended=int(time.time()*1000);commands.append({'producer':script,'report':report,'started_at_unix_ms':started,'measured_at_unix_ms':ended,'exit_code':p.returncode,'log':log.name})
   require(p.returncode==0,'process failed; see '+log.name);value=load(report);require(value.get('passed') is True,'producer failed')
   require(started<=value['measured_at_unix_ms']<=ended,'producer timestamp outside execution')
   require(before==runtime_fingerprint() and inputs==snapshot_inputs() and sha(OUT/binding_name)==binding_hash,'inputs/build/binding changed during execution')
   if script in ['weighted_smoke.py','weighted_static_smoke.py']:require(value['runtime']==before and value['input_hashes']==inputs,'weighted inner binding changed')
   value.update(runtime=before,input_hashes=inputs,execution_binding_sha256=binding_hash,started_at_unix_ms=started,measured_at_unix_ms=ended);write_result(target,value)
   write_result(OUT/command_name,{'passed':len(commands)==len(producers),'commands':commands,'runtime_binding_sha256':binding_hash})
  except BaseException as e:write_result(target,{'passed':False,'error':str(e)});write_result(OUT/command_name,{'passed':False,'commands':commands,'error':str(e)});raise
 return commands
if __name__=='__main__':run()
