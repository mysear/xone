#!/usr/bin/env python3
"""Normal complete V2 parser/replicas/isolated worker over actual local banks."""
from pathlib import Path
import json,secrets,time
ROOT=Path(__file__).resolve().parents[3]
OUT=ROOT/'integration/artifacts/local'
TARGET=OUT/'v2-complete-three-service-smoke.json'
AUTHORITY='ORIGINAL_LOCAL_V2_COMPLETE_BANK_PROVIDED_STATE_REVM_NOT_CANONICAL'
def initial():return dict(passed=False,state='RUNNING',verified=False,production_ready=False,all_requested_features_complete=False,authority=AUTHORITY)
def write(value):
 OUT.mkdir(parents=True,exist_ok=True);temporary=TARGET.with_name(TARGET.name+'.'+secrets.token_hex(6)+'.tmp')
 try:temporary.write_text(json.dumps(value,indent=2)+'\n');temporary.replace(TARGET)
 finally:temporary.unlink(missing_ok=True)
if __name__=='__main__':write(initial())
from evm_execution_smoke import run as run_common,require
from v2_complete_bank import NativeCompleteBank,FIXTURE,hashlib

def words(value,count):
 require(type(value)is str and value.startswith('0x')and value==value.lower()and len(value)==2+64*count,'complete observation ABI size/case differs')
 try:raw=bytes.fromhex(value[2:])
 except ValueError as error:raise ValueError('complete observation ABI hex differs')from error
 require(len(raw)==32*count,'complete observation byte width differs')
 return [int.from_bytes(raw[32*i:32*(i+1)],'big') for i in range(count)]
def check_complete_poststate(bank,result):
 expected=bank.case['expected']['pool_after'];state=expected['v2']['swap'];p=bank.case['pool'];rows=result['outcome']['observations'];records={}
 specifications={
  'pair_reserves_after':(p['pool_id'],'0x0902f1ac',3,[int(expected['reserve0']),int(expected['reserve1']),state['block_timestamp_last']]),
  'pair_balance0_after':(p['token0'],'0x70a08231'+'0'*24+p['pool_id'][2:],1,[int(state['raw_balance0'])]),
  'pair_balance1_after':(p['token1'],'0x70a08231'+'0'*24+p['pool_id'][2:],1,[int(state['raw_balance1'])]),
  'pair_price0_cumulative_after':(p['pool_id'],'0x5909c0d5',1,[int(state['price0_cumulative_last'])]),
  'pair_price1_cumulative_after':(p['pool_id'],'0x5a3d5493',1,[int(state['price1_cumulative_last'])]),
  'pair_lock_after':(p['pool_id'],'',1,[1]),
 }
 for phase,(target,data,count,native) in specifications.items():
  matches=[r for r in rows if r.get('phase')==phase];require(len(matches)==1,'complete poststate phase missing or duplicated: '+phase);row=matches[0]
  require(row.get('target')==target and row.get('calldata')==data,'complete poststate call target/data changed: '+phase)
  actual=words(row.get('output'),count);require(actual==native,'complete execution differs from original native poststate: '+phase)
  usage=row.get('usage');require(type(usage)is dict and type(usage.get('reads'))is int and usage['reads']>0 and type(usage.get('read_bytes'))is int and usage['read_bytes']>0,'complete state evidence has no actual reads')
  if phase=='pair_lock_after':
   require(row.get('type')=='STORAGE'and row.get('slot')=='0x'+'0'*62+'0c'and row.get('calldata')=='','lock observation is not explicit storage slot12')
   require(usage['reads']==1 and usage['read_bytes']==84 and all(usage.get(k)==0 for k in ['instructions','input_bytes','gas_used','executions']),'lock storage read is unmetered or disguised as an EVM call')
  else:require(row.get('type')is None and row.get('slot')is None,'getter observation disguised as storage')
  records[phase]=row
 require(state['bank_block_hash']==bank.header['hash']and state['bank_block_number']==bank.number and state['bank_unix_timestamp']==bank.timestamp,'derived poststate claims a new canonical source bank')
 return dict(case=bank.case['name'],source_block_hash=bank.header['hash'],receipt_hash=result['outcome']['receipt_hash'],observations=records)

def run():
 observations=[]
 def inspect(bank,response):observations.append(check_complete_poststate(bank,response))
 result=run_common(bank_factory=NativeCompleteBank,on_simulation=inspect)
 require(len(observations)==7 and len(observations)==result['simulation_checks'],'complete actual execution observation count differs')
 require([row['case']for row in observations]==['sync-0-to-1']*2+['sync-1-to-0']*2+['donate0-0-to-1']*3,'complete execution bank timeline differs')
 result.update(authority=AUTHORITY,profile='uniswap-v2-complete',adapter_version='uniswap-v2/core-1.0.1-full-swap-1',native_fixture_sha256=hashlib.sha256(FIXTURE.read_bytes()).hexdigest(),complete_poststate_checks=7,complete_poststate_evidence=observations)
 result['checks']+=['complete-profile-wire-retained-through-normal-parser-and-two-replicas','actual-complete-pair-poststate-and-metered-slot12-match-original-node']
 return result

def main():
 write(initial())
 try:
  result=run();write(result);print(json.dumps({'passed':True,'checks':len(result['checks']),'simulations':result['simulation_checks'],'complete_poststate_checks':result['complete_poststate_checks']}))
 except BaseException as error:
  result=initial();result.update(state='FAILED',error=str(error),error_type=type(error).__name__);write(result);raise
if __name__=='__main__':main()
