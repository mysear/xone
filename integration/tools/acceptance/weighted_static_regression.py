#!/usr/bin/env python3
"""Fresh static + legacy Weighted, stable and baseline process scopes."""
from pathlib import Path
import json,secrets
ROOT=Path(__file__).resolve().parents[3];OUT=ROOT/'integration/artifacts/local'
PRODUCERS=[('smoke.py','three-service-smoke.json'),('stable_smoke.py','stable-three-service-smoke.json'),('weighted_smoke.py','weighted-three-service-smoke.json'),('weighted_static_smoke.py','weighted-static-three-service-smoke.json')]
def run():
 # Imported callers receive the same fail-closed boundary as the CLI.
 OUT.mkdir(parents=True,exist_ok=True)
 for name in ['weighted-static-regression-command.json']+[name for _,name in PRODUCERS]:
  target=OUT/name;temporary=target.with_name(target.name+'.'+secrets.token_hex(6)+'.tmp')
  try:temporary.write_text(json.dumps({'passed':False,'state':'RUNNING'})+'\n');temporary.replace(target)
  finally:temporary.unlink(missing_ok=True)
 import weighted_static_evidence as evidence
 from weighted_regression import run as execute
 return execute(evidence,PRODUCERS,'weighted-static')
if __name__=='__main__':run()
