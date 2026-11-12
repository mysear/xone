#!/usr/bin/env python3
"""Controlled MM / processed / open order layers under the actual DLMM SBF."""
from pathlib import Path
import base64,json,sys
ROOT=Path(__file__).resolve().parents[3];sys.path.insert(0,str(ROOT/'integration/tools/reference/meteora-dlmm/harness'))
import native_extended as native
from idl_layout import decode

def execute(zero=True,amount=1000,exact=False,open_amount=3000,processed=2000,mm=1000,followups=()):
 seed=dict(amount_x=mm,amount_y=mm,liquidity_supply=mm<<64,open_order_amount=open_amount,total_processing_order_amount=processed,processed_order_remaining_amount=processed,order_age=1,limit_order_ask_side=0 if zero else 1)
 return native.execute(zero=zero,amount=amount,minimum=10**15 if exact else 1,exact_out=exact,mm_only=True,order_seed=seed,protocol_share=2000,elapsed=300,dynamic_control=7500,followups=followups)
if __name__=='__main__':
 rows=[]
 for zero in [True,False]:
  for amount in [100,1000,3000,10000]:
   n=execute(zero,amount);rows.append(n);print('direction',zero,'amount',amount,'passed',n['passed'],'funds',n['payer_input_delta'],n['recipient_output_delta'])
   for k,a in n['before'].items():
    b=base64.b64decode(a['data_base64']);after=base64.b64decode(n['after'][k]['data_base64'])
    if len(b)==10136 and int.from_bytes(b[8:16],'little',signed=True)==0:
     x,_=decode({'defined':{'name':'BinArray'}},b,8);y,_=decode({'defined':{'name':'BinArray'}},after,8);x=x['bins'][15];y=y['bins'][15];print({k:(x[k],y[k])for k in x if x[k]!=y[k]})
 (ROOT/'integration/artifacts/local/core-meteora-orders-controlled-20261111.json').write_text(json.dumps(dict(records=rows),indent=2)+'\n')
