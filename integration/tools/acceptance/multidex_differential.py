#!/usr/bin/env python3
"""Run concrete-version parser/wire and independent mathematical regression tests."""
from pathlib import Path
import hashlib,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[3];OUT=ROOT/'integration/artifacts/local';OUT.mkdir(parents=True,exist_ok=True);file=ROOT/'integration/test/contracts/dex-versions.json';golden=json.loads(file.read_text());env=os.environ.copy();env['PATH']=str(Path.home()/'.cargo/bin')+':'+env['PATH']
report={'scope':'concrete-pancake-v1-v2-v3-sushi-v2','production_ready':False,'reference':golden['reference'],'case_count':len(golden['cases']),'successful_cases':sum('error' not in c['expected'] for c in golden['cases']),'refused_cases':sum('error' in c['expected'] for c in golden['cases']),'golden_sha256':hashlib.sha256(file.read_bytes()).hexdigest(),'commands':[]}
commands=[('chain-parser-go',['go','test','./internal/domain','./internal/config','./internal/chain/evm','./internal/journal','./internal/reducer']),('dex-router',[str(Path.home()/'.cargo/bin/cargo'),'+1.90.0','test','--locked','-p','xone-domain','-p','xone-protocol-math','-p','xone-routing','-p','xone-optimizer','-p','xone-execution-plan','-p','xone-kafka-transport'])]
for project,command in commands:
 log=OUT/f'multidex-differential-{project}.log';started=time.monotonic()
 with log.open('w') as output:process=subprocess.run(command,cwd=ROOT/project,env=env,stdout=output,stderr=subprocess.STDOUT)
 report['commands'].append({'project':project,'exit_code':process.returncode,'elapsed_ms':round((time.monotonic()-started)*1000),'log':log.name})
 if process.returncode:print(log.read_text()[-4000:]);break
report['passed']=len(report['commands'])==len(commands) and all(c['exit_code']==0 for c in report['commands']);report['measured_at_unix_ms']=int(time.time()*1000);report['tool_sha256']=hashlib.sha256(Path(__file__).read_bytes()).hexdigest();(OUT/'multidex-differential.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2));raise SystemExit(0 if report['passed'] else 1)
