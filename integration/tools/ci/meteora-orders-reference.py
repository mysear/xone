#!/usr/bin/env python3
"""Reward-aware swaps with exact original SBF pair and per-bin reward growth."""
from pathlib import Path
import base64,hashlib,importlib.util,json
ROOT=Path(__file__).resolve().parents[3]
def module(name,path):
 spec=importlib.util.spec_from_file_location(name,ROOT/path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
native=module('order_banks','integration/tools/ci/meteora-orders-preflight.py');projection=module('orders_projection','integration/tools/ci/meteora-mm-state.py')
PROFILE='meteora-dlmm-orders';ADAPTER='meteora-dlmm/1532e540-orders-1';TARGET=ROOT/'integration/artifacts/local/core-meteora-orders-original-20261111.json'
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def pool(bank,clock,code):
 p=projection.pool(bank,clock,code);p.update(protocol=PROFILE,adapter_version=ADAPTER);return p

def main():
 records=[];cases=[]
 for zero in [True,False]:
  for mm in [0,1000]:
   for processed in [0,2000]:
    for open_amount in [0,3000]:
     for amount in [1,100,1000,3000,10000,1000000000000000]:
      n=native.execute(zero,amount,False,open_amount,processed,mm,followups=(100,1000)if amount==1000 else());index=len(records);records.append(dict(index=index,zero_for_one=zero,exact_out=False,mm=mm,processed=processed,open_amount=open_amount,native=n));steps=[]
      for step in n['sequence']if n['passed']else[n]:steps.append(dict(passed=step['passed'],amount=step['specified_amount'],actual_input=step['payer_input_delta'],actual_output=step['recipient_output_delta'],pool=pool(step['before'],n['clock'],n['program_sha256']),after=pool(step['after'],n['clock'],n['program_sha256'])))
      cases.append(dict(index=index,zero_for_one=zero,exact_out=False,steps=steps))
 TARGET.write_text(json.dumps(dict(schema_version=1,passed=True,generator_sha256=sha(__file__),native_sha256=sha(native.__file__),records=records,authority='controlled MM/processed/open order layers; complete fixed SBF swap ledger, no order placement or cancellation claim'),indent=2)+'\n')
 base=json.loads((ROOT/'integration/test/contracts/meteora-mm.json').read_bytes());cfg=base['decoder']['config'];cfg['protocol']=PROFILE;ids=set(cfg['accounts']);decoder=[]
 for c in records:
  n=c['native'];accounts={k:dict(owner=a['owner'],executable=a['executable'],lamports=a['lamports'],data=[a['data_base64'],'base64'])for k,a in n['before'].items()if k in ids};accounts['SysvarC1ock11111111111111111111111111111111']=base['decoder']['cases'][0]['accounts']['SysvarC1ock11111111111111111111111111111111'];decoder.append(dict(accounts=accounts,pool=pool(n['before'],n['clock'],n['program_sha256'])))
 fixture=dict(schema_version=1,source_sha256=sha(TARGET),cases=cases,decoder=dict(config=cfg,shared_accounts=base['decoder']['shared_accounts'],cases=decoder))
 for service in ['integration','dex-router','chain-parser-rust']:(ROOT/service/'test/contracts/meteora-orders.json').write_text(json.dumps(fixture,separators=(',',':'))+'\n')
 print(json.dumps(dict(passed=True,cases=len(cases),success=sum(c['native']['passed']for c in records),steps=sum(len(c['steps'])for c in cases))))
if __name__=='__main__':main()
