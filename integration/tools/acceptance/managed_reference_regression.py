#!/usr/bin/env python3
"""Fresh existing service flows with Managed reference capability kept closed."""
from pathlib import Path
import json
import secrets
ROOT=Path(__file__).resolve().parents[3]
OUT=ROOT/'integration/artifacts/local'
def invalidate():
 for name in ['managed-reference-regression-command.json','managed-reference-acceptance.json','managed-reference-scope-execution.json','three-service-smoke.json','multi-model-three-service-smoke.json','curve-ng-three-service-smoke.json']:
  path=OUT/name;path.parent.mkdir(parents=True,exist_ok=True);temp=path.with_name(path.name+'.'+secrets.token_hex(6)+'.tmp')
  try:temp.write_text(json.dumps({'passed':False,'state':'RUNNING','service_admission':False,'all_requested_features_complete':False,'production_ready':False})+'\n');temp.replace(path)
  finally:temp.unlink(missing_ok=True)
def run():
 invalidate()
 try:
  import managed_reference_evidence as evidence
  from weighted_regression import run as execute
  return execute(evidence,evidence.PRODUCERS,'managed-reference')
 except BaseException as error:
  path=OUT/'managed-reference-regression-command.json'
  temp=path.with_name(path.name+'.'+secrets.token_hex(6)+'.tmp')
  try:temp.write_text(json.dumps({'passed':False,'error':str(error)})+'\n');temp.replace(path)
  finally:temp.unlink(missing_ok=True)
  raise
if __name__=='__main__':run()
