#!/usr/bin/env python3
"""Original sell minimum-output instructions, nonmonotone ceil fees and surplus."""
from pathlib import Path
import hashlib,importlib.util,json,sys
ROOT=Path(__file__).resolve().parents[3];sys.path.insert(0,str(ROOT/'integration/tools/reference/pump-bonding/harness'))
import curve_extended as native
from layout import account
spec=importlib.util.spec_from_file_location('sell_projection',ROOT/'integration/tools/ci/pump-bonding-reference.py');projection=importlib.util.module_from_spec(spec);spec.loader.exec_module(projection)
TARGET=ROOT/'integration/artifacts/local/core-pump-sell-exact-out-original-20261111.json'
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def required(p,target):
 s=p['pump'];raw=bytes.fromhex(next(a['data']for a in s['accounts']if a['address']==p['pool_id']));vt=int.from_bytes(raw[8:16],'little');vq=int.from_bytes(raw[16:24],'little');real=int.from_bytes(raw[32:40],'little');cfg=next(bytes.fromhex(a['data'])for a in s['accounts']if a['owner']==native.FEES);fees,_=account('FeeConfig',cfg,'pump_fees');market=10**15*vq//vt;tier=fees['fee_tiers'][0]
 for t in fees['fee_tiers']:
  if t['market_cap_lamports_threshold']<=market:tier=t
 rates=tier['fees'];protocol=rates['protocol_fee_bps'];creator=rates['creator_fee_bps'];D=10000-protocol-creator
 def fee(n,bps):return (n*bps+9999)//10000
 lower=1
 for _ in range(32):
  values=[]
  for r in range(10000):
   base=r-fee(r,protocol)-fee(r,creator);periods=max(0,(target-base+D-1)//D,(lower-r+9999)//10000);g=periods*10000+r
   if lower<=g<=real and g<vq:values.append(g)
  if not values:return None
  g=min(values);used=(g*vt+vq-g-1)//(vq-g);actual=used*vq//(vt+used);net=actual-fee(actual,protocol)-fee(actual,creator)
  if net>=target:return used
  lower=actual+1
 raise ValueError('REFERENCE_INVERSION_BUDGET')
def main():
 records=[];cases=[]
 for reserve in [None,10**19]:
  bank=native.execute(kind='sell',amount=1,limit=1,quote_reserve=reserve);p=projection.pool(bank['before'],bank['ids'])
  for target in [1,2,100,1000,9899,9900,9901,1000000,1000000000,int(p['reserve1'])+1]:
   amount=required(p,target);n=None;control=None
   if amount is not None:
    n=native.execute(kind='sell',amount=amount,limit=target,quote_reserve=reserve)
    if not n['passed']or n['user_sol_delta']<target:raise ValueError('ORIGINAL_SELL_TARGET')
    control=native.execute(kind='sell',amount=amount-1,limit=target,quote_reserve=reserve)
    if control['passed']or control['before']!=n['before']or control['after']!=control['before']:raise ValueError('ORIGINAL_MINIMUM_INPUT_CONTROL')
   index=len(records);records.append(dict(index=index,target=str(target),quote_reserve=reserve,required_input=str(amount)if amount is not None else None,native=n,input_minus_one=control));cases.append(dict(index=index,target=str(target),passed=n is not None,pool=p,required_input=str(amount)if amount is not None else None,actual_output=str(n['user_sol_delta'])if n else None,after=projection.pool(n['after'],n['ids'])if n else None))
 TARGET.write_text(json.dumps(dict(schema_version=1,passed=True,generator_sha256=sha(__file__),native_sha256=sha(native.__file__),records=records,authority='original sell with explicit minimum SOL target and actual surplus, all feasible targets have original input-minus-one failure controls; finite controlled banks'),indent=2)+'\n')
 f=dict(schema_version=1,source_sha256=sha(TARGET),cases=cases)
 for service in ['integration','dex-router']:(ROOT/service/'test/contracts/pump-sell-exact-out.json').write_text(json.dumps(f,separators=(',',':'))+'\n')
 print(json.dumps(dict(passed=True,cases=len(cases),success=sum(c['passed']for c in cases),surplus=sum(c['passed']and int(c['actual_output'])>int(c['target'])for c in cases))))
if __name__=='__main__':main()
