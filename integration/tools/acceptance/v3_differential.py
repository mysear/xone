#!/usr/bin/env python3
"""Verify exact amounts/post-state against pinned official-SDK generated data."""
from pathlib import Path
import hashlib,json,os,subprocess,time
ROOT=Path(__file__).resolve().parents[3];OUT=ROOT/'integration/artifacts/local';OUT.mkdir(parents=True,exist_ok=True)
golden=ROOT/'integration/test/contracts/v3-quotes.json';data=json.loads(golden.read_text());env=os.environ.copy();env['PATH']=str(Path.home()/'.cargo/bin')+':'+env['PATH']
report={'scope':'bounded-standard-uniswap-v3-exact-in','production_ready':False,'reference':data['reference'],'case_count':len(data['cases']),'successful_cases':sum('error' not in c['expected'] for c in data['cases']),'refused_cases':sum('error' in c['expected'] for c in data['cases']),'sequential_shared_pool_cases':len(data['allocation_cases']),'candidate_discovery_regressions':len(data['discovery_regressions']),'golden_sha256':hashlib.sha256(golden.read_bytes()).hexdigest(),'commands':[]}
commands=[('chain-parser-go',['go','test','./internal/domain','./internal/config','./internal/chain/evm','./internal/journal','./internal/reducer']),('dex-router',[str(Path.home()/'.cargo/bin/cargo'),'+1.90.0','test','--locked','-p','xone-domain','-p','xone-protocol-math','-p','xone-routing','-p','xone-optimizer','-p','xone-execution-plan','-p','xone-kafka-transport'])]
for index,(project,command) in enumerate(commands):
 started=time.monotonic();log=OUT/f'v3-differential-{project}.log'
 with log.open('w') as output:result=subprocess.run(command,cwd=ROOT/project,env=env,stdout=output,stderr=subprocess.STDOUT)
 report['commands'].append({'project':project,'exit_code':result.returncode,'elapsed_ms':round((time.monotonic()-started)*1000),'log':log.name})
 if result.returncode:print(log.read_text()[-4000:]);break
report['passed']=len(report['commands'])==len(commands) and all(c['exit_code']==0 for c in report['commands']);report['measured_at_unix_ms']=int(time.time()*1000);report['tool_sha256']=hashlib.sha256(Path(__file__).read_bytes()).hexdigest();(OUT/'v3-differential.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2));raise SystemExit(0 if report['passed'] else 1)
