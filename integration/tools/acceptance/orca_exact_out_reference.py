#!/usr/bin/env python3
"""Fixed original Orca ExactOut manager and complete pool/account post-state.

No Router math import. Native program managers run on controlled states; this
is not SBF/CPI, wallet settlement, historical-bank or deployment qualification.
"""
from pathlib import Path
import base64,copy,hashlib,importlib.util,json,subprocess,sys
ROOT=Path(__file__).resolve().parents[3]
OUT=ROOT/'integration/artifacts/local/orca-exact-out-original-20261114.json'
STATIC=ROOT/'integration/tools/reference/orca-whirlpools/target/release/solana-advanced-orca-reference'
TARGET=Path('/tmp/xone-orca-adaptive-swap-native/debug')
PIN='cc683ec92acb5e7606caa3d8e514d9383e9c7c11'
def require(condition,message):
 if not condition:raise ValueError(message)
def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def module(name,path):
 spec=importlib.util.spec_from_file_location(name,ROOT/path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
class Native:
 def __init__(self,binary):self.binary=binary;self.process=subprocess.Popen([str(binary)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True)
 def ask(self,value):
  self.process.stdin.write(json.dumps(value,separators=(',',':'))+'\n');self.process.stdin.flush();line=self.process.stdout.readline();require(bool(line),'native stopped');result=json.loads(line);require('fatal'not in result,'native invalid request');return result
 def close(self):self.process.stdin.close();require(self.process.wait(timeout=20)==0,'native process failed')
def encode(value):return json.dumps(value,separators=(',',':'),sort_keys=True).encode()
def main():
 records=[];sequences=[];pairs=[];identities={};native_records=[];static=Native(STATIC)
 def static_case(name,p,zero,target):
  p=copy.deepcopy(p);p['chain_key']='solana:mainnet-beta';request=dict(action='quote_exact_out',pool=p,output=str(target),zero_for_one=zero);r=static.ask(request);ok=r.get('reverted')is not True
  native_records.append(dict(index=len(records),kind='static',request=request,response=r))
  row=dict(name=name,profile=p['protocol'],pool=p,zeroForOne=zero,target=str(target),passed=ok,authority='unchanged pinned native swap_manager and post-update, no CPI/SBF')
  if ok:row.update(requiredInput=r['amount_in'],actualOutput=r['amount_out'],after=r['next_pool']);require(r['amount_out']==str(target),'native exact target')
  else:row['error']=r['reason']
  records.append(row);return row
 try:
  for file in ['orca-quotes.json','solana-advanced/orca-rewards.json']:
   source=json.loads((ROOT/'integration/test/contracts'/file).read_bytes());seen=set();selected=[]
   # Include every distinct original named boundary category plus early ordinary states.
   for c in source['cases']:
    p=c['pool'];key=hashlib.sha256(encode(p)).hexdigest()
    if key in seen:continue
    seen.add(key);selected.append((c['name'],p))
   # The full ExactIn corpus remains a separate regression; bound this new target corpus.
   if len(selected)>28:selected=selected[:14]+selected[-14:]
   for name,p in selected:
    for zero in [True,False]:
     for target in [1,1000,15000,(1<<64)-1]:static_case(name,p,zero,target)
   base=next(p for name,p in selected if p['protocol']in ['orca-whirlpools-static','orca-whirlpools-rewards'])
   steps=[];p=copy.deepcopy(base)
   for zero,target in [(True,1000),(True,10000),(False,1000),(True,2000)]:
    row=static_case('sequential',p,zero,target)
    if not row['passed']:break
    steps.append(copy.deepcopy(row));p=row['after']
   if len(steps)==4:sequences.append(dict(profile=base['protocol'],steps=steps))
  identities['static']=dict(binary=str(STATIC),sha256=sha(STATIC),source_sha256=sha(ROOT/'integration/tools/reference/solana-advanced/orca/src/lib.rs'))
 finally:static.close()
 for family in ['orca-adaptive-service','orca-adaptive-rewards-service','orca-adaptive-dynamic-service']:
  native=Native(TARGET/(family+'-reference'));projection=module('projection_'+family,'integration/tools/acceptance/'+family.replace('-service','').replace('-','_')+'_fixture.py')
  try:
   identity=native.ask(dict(action='identity'));require(identity['revision']==PIN and identity['sbf']is False,'native authority');require(identity['executed_binary_sha256']==sha(native.binary),'native identity');identities[family]=identity
   data=json.loads((ROOT/'integration/test/contracts'/(family+'.json')).read_bytes())
   def acquire(config,bank):
    bank=copy.deepcopy(bank);bank['slot']=900000;bank['source_slots']={k:900000 for k in bank['accounts']};clock='SysvarC1ock11111111111111111111111111111111';a=bank['accounts'][clock];raw=bytearray(base64.b64decode(a['data'][0]));raw[:8]=(900000).to_bytes(8,'little');a['data'][0]=base64.b64encode(raw).decode();bank['native_state']=native.ask(dict(action='state',config=config,bank=bank));return bank
   def adaptive_case(config,bank,zero,target,name):
    params=dict(amount=str(target),a_to_b=zero,sqrt_price_limit='0',timestamp=bank['timestamp'],max_steps=8192,amount_specified_is_input=False)
    raw_bank={key:copy.deepcopy(bank[key])for key in ['slot','timestamp','source_slots','accounts']};request=dict(action='trade',config=config,bank=raw_bank,params=params);result=native.ask(request);native_records.append(dict(index=len(records),kind=family,request=request,response=result,before_native_state=copy.deepcopy(bank['native_state'])));p=projection.pool(config,bank['native_state']);p['chain_key']='solana:mainnet-beta';ok=result['status']=='fulfilled'
    row=dict(name=name,profile=p['protocol'],pool=p,zeroForOne=zero,target=str(target),passed=ok,authority='original raw-bank load, native manager, pool/Oracle/tick post-update; vault transfers projected, no CPI/SBF')
    if ok:
     after=projection.pool(config,result['next_state']);after['chain_key']=p['chain_key'];row.update(requiredInput=result['amount_in'],actualOutput=result['amount_out'],after=after);require(result['amount_out']==str(target)and result['amount_remaining']=='0','native exact target')
    else:row['error']=result['error'];require(result['post_accounts']==bank['accounts'],'native helper failure atomicity')
    records.append(row);return row,result
   for scenario in data['scenarios']:
    config=scenario['config']
    # First, already crossed and proof-edge banks: both directions on the same state.
    for index in [0,2,4]:
     bank=acquire(config,scenario['banks'][index])
     for zero in [True,False]:
      for target in [1,1000,15000,(1<<64)-1]:adaptive_case(config,bank,zero,target,scenario['name']+'/'+str(index))
   for scenario in data['scenarios'][::2]:
    config=scenario['config'];bank=acquire(config,scenario['banks'][0]);steps=[]
    for zero,target in [(True,1000),(True,10000),(False,1000),(True,2000)]:
     row,r=adaptive_case(config,bank,zero,target,'sequential')
     require(row['passed'],'planned native sequence rejected');steps.append(copy.deepcopy(row));bank['accounts']=r['post_accounts'];bank['native_state']=r['next_state']
    sequences.append(dict(profile=steps[0]['profile'],steps=steps))
   # Independent two-pool grid: raw banks observed at one explicit local acquisition slot.
   lefts=[s for s in data['scenarios']if 'a-to-b'in s['name']];config0=lefts[0]['config'];config1=lefts[1]['config'];bank0=acquire(config0,lefts[0]['banks'][0]);bank1=acquire(config1,lefts[1]['banks'][0]);grid=[];poolset=None
   for part in range(5):
    rows=[]
    for config,bank,target in [(config0,bank0,part*2500),(config1,bank1,(4-part)*2500)]:
     if target:row,_=adaptive_case(config,bank,True,target,'allocation-grid');require(row['passed'],'native allocation rejected');rows.append(row)
    grid.append(dict(parts=part,requiredInput=str(sum(int(x['requiredInput'])for x in rows)),rows=rows))
   best=min(grid,key=lambda r:int(r['requiredInput']));p0=projection.pool(config0,bank0['native_state']);p1=projection.pool(config1,bank1['native_state'])
   for p in [p0,p1]:p['chain_key']='solana:mainnet-beta'
   pairs.append(dict(profile=p0['protocol'],pools=[p0,p1],target='10000',parts=4,grid=grid,bestRequiredInput=best['requiredInput']))
  finally:native.close()
 profiles=sorted({r['profile']for r in records});require(len(profiles)==5,'five admitted profiles required')
 for profile in profiles:
  require(all(any(r['profile']==profile and r['zeroForOne']==zero and r['passed']for r in records)for zero in [True,False]),'missing native direction')
  require(any(r['profile']==profile and not r['passed']for r in records),'missing native rejection')
 result=dict(schema_version=1,passed=True,revision=PIN,generator_sha256=sha(__file__),identities=identities,records=records,native_records=native_records,sequences=sequences,pairs=pairs,all_requested_features_complete=False,production_ready=False)
 OUT.write_text(json.dumps(result,separators=(',',':'))+'\n')
 corpus=dict(schema_version=1,source_sha256=sha(OUT),revision=PIN,cases=records,sequences=sequences,pairs=pairs)
 encoded=json.dumps(corpus,separators=(',',':'))+'\n'
 for service in ['integration','dex-router']:(ROOT/service/'test/contracts/orca-exact-out.json').write_text(encoded)
 print(json.dumps(dict(passed=True,cases=len(records),fulfilled=sum(r['passed']for r in records),rejected=sum(not r['passed']for r in records),profiles=profiles,sequences=len(sequences),allocation_pairs=len(pairs),sha256=hashlib.sha256(encoded.encode()).hexdigest())))
if __name__=='__main__':main()
