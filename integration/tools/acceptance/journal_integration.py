#!/usr/bin/env python3
from infrastructure import Infrastructure,ROOT
import json,subprocess,time
out=ROOT/'integration/artifacts/local';out.mkdir(parents=True,exist_ok=True)
report={'profile':'real-local-postgres13-kafka3.9','production_ready':False,'commands':[]}
with Infrastructure() as infra:
 commands=[['go','test','-tags=integration','./internal/control','-count=1','-v','-args','--control-test-dsn-file',str(infra.dsnfile)],['go','test','-tags=integration','./internal/journal','-count=1','-v','-args','--kafka-test-broker',infra.broker],['go','test','-tags=integration','./internal/reducer','-count=1','-v','-args','--reducer-kafka-broker',infra.broker]]
 for index,cmd in enumerate(commands):
  print('Running dedicated integration test '+str(index+1),flush=True);started=time.monotonic();p=subprocess.run(cmd,cwd=ROOT/'chain-parser-go',capture_output=True,text=True,timeout=90);(out/f'journal-integration-{index}.log').write_text(p.stdout+p.stderr);report['commands'].append({'index':index,'exit_code':p.returncode,'elapsed_ms':round((time.monotonic()-started)*1000),'log':f'journal-integration-{index}.log'});print((p.stdout+p.stderr)[-7000:]);
  if p.returncode:break
report['measured_at_unix_ms']=int(time.time()*1000)
report['passed']=len(report['commands'])==3 and all(c['exit_code']==0 for c in report['commands']);(out/'journal-integration.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2));raise SystemExit(0 if report['passed'] else 1)
