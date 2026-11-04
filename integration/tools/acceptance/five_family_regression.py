#!/usr/bin/env python3
from pathlib import Path
import json,secrets
ROOT=Path(__file__).resolve().parents[3];OUT=ROOT/'integration/artifacts/local'
SCOPES=['three-service-smoke.json','weighted-static-three-service-smoke.json','multi-model-three-service-smoke.json','rewards-orca-three-service-smoke.json','rewards-raydium-clmm-three-service-smoke.json','singleton-three-service-smoke.json']
def run():
 OUT.mkdir(parents=True,exist_ok=True)
 for n in ['five-family-regression-command.json']+SCOPES:
  p=OUT/n;t=p.with_name(p.name+'.'+secrets.token_hex(6)+'.tmp')
  try:t.write_text(json.dumps({'passed':False,'state':'RUNNING'})+'\n');t.replace(p)
  finally:t.unlink(missing_ok=True)
 import five_family_evidence as evidence
 from weighted_regression import run as execute
 return execute(evidence,evidence.PRODUCERS,'five-family')
if __name__=='__main__':run()
