#!/usr/bin/env python3
"""Fresh typed-state, fixed-block parser, original core math, routing and wire checks."""
from pathlib import Path
import json,hashlib,subprocess,time,os
ROOT=Path(__file__).resolve().parents[3];OUT=ROOT/'integration/artifacts/local';OUT.mkdir(parents=True,exist_ok=True)
file=ROOT/'integration/test/contracts/raydium-quotes.json';golden=json.loads(file.read_text());generation=json.loads((OUT/'raydium-golden-generation.json').read_text())
env=os.environ.copy();env['PATH']=str(Path.home()/'.cargo/bin')+':'+env['PATH']
report={'scope':'raydium-cpmm-creator-classic-spl-finalized','production_ready':False,'case_count':len(golden['cases']),'golden_sha256':hashlib.sha256(file.read_bytes()).hexdigest(),'reference':golden['reference'],'commands':[]}
assert report['golden_sha256']==generation['golden_sha256'] and report['case_count']==generation['counts']['cases'], 'reference fixture/report drift'
report['sequence_count']=generation['counts']['sequences'];report['sequence_steps']=generation['counts']['sequence_steps']
commands=[('chain-parser-go',['go','test','./internal/domain','./internal/config','./internal/chain/evm','./internal/journal','./internal/reducer']),('dex-router',[str(Path.home()/'.cargo/bin/cargo'),'+1.90.0','test','--locked','-p','xone-domain','-p','xone-config','-p','xone-protocol-math','-p','xone-routing','-p','xone-optimizer','-p','xone-execution-plan','-p','xone-kafka-transport']),('chain-parser-rust',[str(Path.home()/'.cargo/bin/cargo'),'+1.90.0','test','--locked','-p','xone-domain','-p','xone-config','-p','xone-kafka-transport','-p','xone-solana-ingest'])]
for project,command in commands:
 log=OUT/f'raydium-differential-{project}.log';started=time.monotonic()
 with log.open('w') as stream:p=subprocess.run(command,cwd=ROOT/project,env=env,stdout=stream,stderr=subprocess.STDOUT)
 report['commands'].append({'project':project,'exit_code':p.returncode,'elapsed_ms':round((time.monotonic()-started)*1000),'log':log.name})
 if p.returncode:print(log.read_text()[-6000:]);break
report.update(passed=len(report['commands'])==len(commands) and all(c['exit_code']==0 for c in report['commands']),measured_at_unix_ms=int(time.time()*1000),tool_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest());(OUT/'raydium-differential.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2));raise SystemExit(0 if report['passed'] else 1)
