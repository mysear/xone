#!/usr/bin/env python3
from pathlib import Path
import json,hashlib,subprocess,time,os
ROOT=Path(__file__).resolve().parents[3];OUT=ROOT/'integration/artifacts/local';OUT.mkdir(parents=True,exist_ok=True);file=ROOT/'integration/test/contracts/stable-quotes.json';golden=json.loads(file.read_text());env=os.environ.copy();env['PATH']=str(Path.home()/'.cargo/bin')+':'+env['PATH']
report={'scope':'aero-v2-curve-plain2-balancer-stable2-slipstream-boundaries','production_ready':False,'executed_primary_reference':golden['reference'],'case_count':len(golden['cases']),'allocation_case_count':len(json.loads((ROOT/'integration/test/contracts/stable-allocations.json').read_text())['cases']),'allocation_golden_sha256':hashlib.sha256((ROOT/'integration/test/contracts/stable-allocations.json').read_bytes()).hexdigest(),'golden_sha256':hashlib.sha256(file.read_bytes()).hexdigest(),'commands':[]}
commands=[('chain-parser-go',['go','test','./internal/domain','./internal/config','./internal/chain/evm','./internal/journal','./internal/reducer']),('dex-router',[str(Path.home()/'.cargo/bin/cargo'),'+1.90.0','test','--locked','-p','xone-domain','-p','xone-protocol-math','-p','xone-routing','-p','xone-optimizer','-p','xone-execution-plan','-p','xone-kafka-transport']),('chain-parser-rust',[str(Path.home()/'.cargo/bin/cargo'),'+1.90.0','test','--locked','-p','xone-solana-ingest','--test','cpmm_version'])]
for project,command in commands:
 log=OUT/f'stable-differential-{project}.log';started=time.monotonic()
 with log.open('w') as stream:p=subprocess.run(command,cwd=ROOT/project,env=env,stdout=stream,stderr=subprocess.STDOUT)
 report['commands'].append({'project':project,'exit_code':p.returncode,'elapsed_ms':round((time.monotonic()-started)*1000),'log':log.name})
 if p.returncode:print(log.read_text()[-5000:]);break
report.update(passed=len(report['commands'])==len(commands) and all(c['exit_code']==0 for c in report['commands']),measured_at_unix_ms=int(time.time()*1000),tool_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest());(OUT/'stable-differential.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2));raise SystemExit(0 if report['passed'] else 1)
