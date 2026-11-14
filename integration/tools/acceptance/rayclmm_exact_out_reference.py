#!/usr/bin/env python3
"""Fixed Raydium native output-specified step/manager evidence, no Router math.

Pool/vault transfers are explicit checked projections. No SBF/CPI, account
instruction constraints, historical bank or deployed program qualification.
"""
from pathlib import Path
import copy,hashlib,json
from orca_exact_out_reference import ROOT,Native,require,sha,encode
PIN='6d7a0f82d2f293c553c69aaf943377367c6c4cc6'
BINARY=Path('/tmp/xone-solana-advanced-ray/debug/solana-advanced-raydium-clmm-reference')
OUT=ROOT/'integration/artifacts/local/rayclmm-exact-out-original-20261114.json'
FILES=['rayclmm-quotes.json','solana-advanced/raydium-clmm-rewards.json','rayclmm-token-fee-quotes.json','rayclmm-bitmap-quotes.json','rayclmm-seed-quotes.json','rayclmm-dynamic-quotes.json']
def main():
 native=Native(BINARY);records=[];raw=[];sequences=[];pairs=[];steps=[];checkpoints=[];sources={}
 def case(pool,zero,target,name):
  p=copy.deepcopy(pool);p['chain_key']='solana:mainnet-beta';request=dict(action='quote_exact_out',pool=p,output=str(target),zero_for_one=zero);result=native.ask(request);ok=result.get('reverted')is not True
  row=dict(index=len(records),name=name,profile=p['protocol'],pool=p,zeroForOne=zero,target=str(target),passed=ok)
  if ok:
   require(result['amount_out']==str(target)and int(result['amount_in'])>0,'native exact-output conservation');row.update(requiredInput=result['amount_in'],actualOutput=result['amount_out'],after=result['next_pool'])
  else:row['error']=result['reason']
  raw.append(dict(index=len(records),request=request,response=result));records.append(row);return row
 try:
  for file in FILES:
   path=ROOT/'integration/test/contracts'/file;data=json.loads(path.read_bytes());sources[str(path.relative_to(ROOT))]=sha(path);base=data['base_pool'];groups={}
   for c in data['cases']:
    if c['expected'].get('reverted'):continue
    p=c['pool'];mode=p['raydium_clmm'].get('collect_fee_on',0);key=hashlib.sha256(encode(p)).hexdigest();groups.setdefault(mode,{})[key]=(c['name'],p)
   for mode,values in sorted(groups.items()):
    selected=list(values.values());selected=selected if len(selected)<=8 else selected[:4]+selected[-4:]
    for name,p in selected:
     for zero in [True,False]:
      for target in [1,17,1000,15000,(1<<64)-1]:case(p,zero,target,name)
   for liquidity in [1<<70,(1<<128)-1]:
    p=copy.deepcopy(base);p['raydium_clmm']['liquidity']=str(liquidity)
    for zero in [True,False]:
     for target in [1,1000]:case(p,zero,target,'wide-liquidity')
   if base['protocol']not in ['raydium-clmm-dynamic']:
    modes=[0]if base['protocol']in ['raydium-clmm-static','raydium-clmm-rewards']else[1,2]if base['protocol']=='raydium-clmm-token-fee'else[0,1,2]
    for mode in modes:
     for rate in [0,1,999999]:
      p=copy.deepcopy(base);p['raydium_clmm']['collect_fee_on']=mode;p['raydium_clmm']['trade_fee_rate']=rate;p['fee_numerator']=rate
      for zero in [True,False]:case(p,zero,1,'fee-cap-rounding')
   for zero in [True,False]:
    p=copy.deepcopy(base);p['raydium_clmm']['raw_vault1'if zero else'raw_vault0']=0;case(p,zero,1,'empty-output-vault')
    p=copy.deepcopy(base);p['raydium_clmm']['open_time']=p['raydium_clmm']['bank_unix_timestamp'];case(p,zero,1,'not-open')
    case(base,zero,0,'zero-target');case(base,zero,1<<64,'u64-width')
   # Evolving original state, including a new Clock acquisition between steps.
   p=copy.deepcopy(base);seq=[]
   for n,(zero,target,elapsed)in enumerate([(True,1000,0),(True,2000,0),(False,1000,15),(True,2000,0)]):
    if n:p['raydium_clmm']['bank_slot']+=1;p['raydium_clmm']['bank_unix_timestamp']+=elapsed
    row=case(p,zero,target,'sequential');require(row['passed'],'planned native sequence rejected');seq.append(copy.deepcopy(row));p=copy.deepcopy(row['after'])
   sequences.append(dict(profile=base['protocol'],steps=seq))
   # Distinct program-derived peer, same config/price/liquidity; reward state in
   # the constructed peer is uninitialized. No historical/deployment claim.
   peer=native.ask(dict(action='target_peer',pool=base,seed=255));require(peer['pool_id']!=base['pool_id'],'distinct physical peer')
   for p in [base,peer]:p['chain_key']='solana:mainnet-beta'
   for zero in [True,False]:
    grid=[]
    for part in range(5):
     rows=[]
     for p,target in [(base,part*2500),(peer,(4-part)*2500)]:
      if target:
       row=case(p,zero,target,'allocation-grid');require(row['passed'],f"native grid allocation rejected:{row['profile']}:{zero}:{target}:{row.get('error')}");rows.append(copy.deepcopy(row))
     grid.append(dict(parts=part,requiredInput=str(sum(int(r['requiredInput'])for r in rows)),rows=rows))
    pairs.append(dict(profile=base['protocol'],pools=[copy.deepcopy(base),peer],zeroForOne=zero,target='10000',parts=4,grid=grid,bestRequiredInput=str(min(int(g['requiredInput'])for g in grid))))
   # Original struct serialization for parser checkpoints, not instruction/SBF execution.
   row=next(c for c in records if c['profile']==base['protocol']and c['passed']and c['target']=='1000'and c['zeroForOne'])
   checkpoints.append(dict(profile=base['protocol'],caseIndex=row['index'],before=native.ask(dict(action='fixture_target',pool=row['pool'])),after=native.ask(dict(action='fixture_target',pool=row['after']))))
  q=1<<64
  for zero in [True,False]:
   for on_input in [True,False]:
    for liquidity in [0,1296,1<<70,(1<<128)-1]:
     for fee in [0,1,20000,600000,999999]:
      for output in [1,1000,(1<<64)-1]:
       req=dict(action='step_exact_out',price=str(9*q),target_price=str(4*q if zero else 16*q),liquidity=str(liquidity),output=str(output),fee=fee,zero_for_one=zero,fee_on_input=on_input);steps.append(dict(request=req,response=native.ask(req)))
  for zero in [True,False]:
   for on_input in [True,False]:
    for fee in [0,1,2500]:
     for output in [1,17,1000]:
      req=dict(action='step_exact_out',price=str(q),target_price=str(q//2 if zero else 2*q),liquidity=str(1<<70),output=str(output),fee=fee,zero_for_one=zero,fee_on_input=on_input);steps.append(dict(request=req,response=native.ask(req)))
  for tick,other,zero in [(-443635,-443636,True),(443635,443636,False)]:
   price=native.ask(dict(action='price',tick=tick))['sqrt_price_x64'];target=native.ask(dict(action='price',tick=other))['sqrt_price_x64']
   for on_input in [True,False]:
    req=dict(action='step_exact_out',price=price,target_price=target,liquidity=str(1<<70),output='1',fee=3000,zero_for_one=zero,fee_on_input=on_input);steps.append(dict(request=req,response=native.ask(req)))
 finally:native.close()
 profiles=sorted({c['profile']for c in records});require(len(profiles)==6,'six profiles')
 for profile in profiles:
  require(all(any(c['profile']==profile and c['passed']and c['zeroForOne']==zero for c in records)for zero in [True,False]),'native direction coverage')
  require(any(c['profile']==profile and not c['passed']for c in records),'native rejection coverage')
 result=dict(schema_version=1,passed=True,revision=PIN,generator_sha256=sha(__file__),binary=dict(path=str(BINARY),sha256=sha(BINARY)),sources=sources,records=records,native_records=raw,sequences=sequences,pairs=pairs,steps=steps,checkpoints=checkpoints,sbf=False,all_requested_features_complete=False,production_ready=False)
 OUT.write_text(json.dumps(result,separators=(',',':'))+'\n')
 data=dict(schema_version=1,source_sha256=sha(OUT),revision=PIN,cases=records,sequences=sequences,pairs=pairs,steps=steps,checkpoints=checkpoints)
 encoded=json.dumps(data,separators=(',',':'))+'\n'
 for service in ['integration','dex-router']:(ROOT/service/'test/contracts/rayclmm-exact-out.json').write_text(encoded)
 frames=json.dumps(dict(schema_version=1,source_sha256=sha(OUT),revision=PIN,checkpoints=checkpoints),separators=(',',':'))+'\n'
 for service in ['integration','chain-parser-rust']:(ROOT/service/'test/contracts/rayclmm-exact-out-frames.json').write_text(frames)
 print(json.dumps(dict(passed=True,cases=len(records),fulfilled=sum(c['passed']for c in records),rejected=sum(not c['passed']for c in records),profiles=profiles,sequences=len(sequences),pairs=len(pairs),step_vectors=len(steps),checkpoints=len(checkpoints))))
if __name__=='__main__':main()
