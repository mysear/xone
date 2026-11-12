#!/usr/bin/env python3
"""Reward-aware swaps with exact original SBF pair and per-bin reward growth."""
from pathlib import Path
import base64,hashlib,importlib.util,json
ROOT=Path(__file__).resolve().parents[3]
def module(name,path):
 spec=importlib.util.spec_from_file_location(name,ROOT/path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
native=module('reward_banks','integration/tools/ci/meteora-rewards-preflight.py');projection=module('reward_projection','integration/tools/ci/meteora-mm-state.py')
PROFILE='meteora-dlmm-mm-rewards';ADAPTER='meteora-dlmm/1532e540-mm-rewards-1';TARGET=ROOT/'integration/artifacts/local/core-meteora-rewards-original-20261111.json'
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def pool(bank,clock,code):
 p=projection.pool(bank,clock,code);p.update(protocol=PROFILE,adapter_version=ADAPTER);return p

def main():
 records=[];cases=[]
 for zero in [True,False]:
  for exact in [False,True]:
   for empty in [False,True]:
    for end,last in [(1734392500,1734392400),(1734392420,1734392400),(1734392400,1734392400),(1734392500,1734392448)]:
     for amount in [1,100,1000000,100000000]:
      n=native.execute(zero,exact,amount,end,last,empty=empty,followups=(10000,20000)if amount==1000000 else());index=len(records);records.append(dict(index=index,zero_for_one=zero,exact_out=exact,empty=empty,end=end,last=last,native=n));steps=[]
      for step in n['sequence']if n['passed']else[n]:steps.append(dict(passed=step['passed'],amount=step['specified_amount'],actual_input=step['payer_input_delta'],actual_output=step['recipient_output_delta'],pool=pool(step['before'],n['clock'],n['program_sha256']),after=pool(step['after'],n['clock'],n['program_sha256'])))
      cases.append(dict(index=index,zero_for_one=zero,exact_out=exact,steps=steps))
 for zero in [True,False]:
  for exact in [False,True]:
   for end,last in [(1734392500,1734392400),(1734392420,1734392400)]:
    n=native.execute(zero,exact,1000000,end,last,second=True);index=len(records);records.append(dict(index=index,zero_for_one=zero,exact_out=exact,empty=False,end=end,last=last,second=True,native=n));steps=[]
    for step in n['sequence']if n['passed']else[n]:steps.append(dict(passed=step['passed'],amount=step['specified_amount'],actual_input=step['payer_input_delta'],actual_output=step['recipient_output_delta'],pool=pool(step['before'],n['clock'],n['program_sha256']),after=pool(step['after'],n['clock'],n['program_sha256'])))
    cases.append(dict(index=index,zero_for_one=zero,exact_out=exact,steps=steps))
 TARGET.write_text(json.dumps(dict(schema_version=1,passed=True,generator_sha256=sha(__file__),native_sha256=sha(native.__file__),records=records,authority='controlled initialized rewards; fixed original SBF swaps, no reward collection or production qualification'),indent=2)+'\n')
 base=json.loads((ROOT/'integration/test/contracts/meteora-mm.json').read_bytes());cfg=base['decoder']['config'];cfg['protocol']=PROFILE;ids=set(cfg['accounts']);decoder=[]
 for c in records:
  n=c['native'];accounts={k:dict(owner=a['owner'],executable=a['executable'],lamports=a['lamports'],data=[a['data_base64'],'base64'])for k,a in n['before'].items()if k in ids};accounts['SysvarC1ock11111111111111111111111111111111']=base['decoder']['cases'][0]['accounts']['SysvarC1ock11111111111111111111111111111111'];decoder.append(dict(accounts=accounts,pool=pool(n['before'],n['clock'],n['program_sha256'])))
 fixture=dict(schema_version=1,source_sha256=sha(TARGET),cases=cases,decoder=dict(config=cfg,shared_accounts=base['decoder']['shared_accounts'],cases=decoder))
 for service in ['integration','dex-router','chain-parser-rust']:(ROOT/service/'test/contracts/meteora-rewards.json').write_text(json.dumps(fixture,separators=(',',':'))+'\n')
 print(json.dumps(dict(passed=True,cases=len(cases),success=sum(c['native']['passed']for c in records),steps=sum(len(c['steps'])for c in cases))))
if __name__=='__main__':main()
