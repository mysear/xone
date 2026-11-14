#!/usr/bin/env python3
"""Verify native typed-state Raydium target evidence and optional exact replay."""
from pathlib import Path
import argparse,copy,json
from rayclmm_exact_out_reference import ROOT,OUT,BINARY,PIN
from orca_exact_out_reference import Native,require,sha
CORPUS=ROOT/'integration/test/contracts/rayclmm-exact-out.json'
def verify(corpus=None,reexecute=False):
 d=json.loads(CORPUS.read_bytes())if corpus is None else corpus;e=json.loads(OUT.read_bytes());require(d['source_sha256']==sha(OUT),'original evidence digest');require(e['generator_sha256']==sha(ROOT/'integration/tools/acceptance/rayclmm_exact_out_reference.py'),'generator changed');require(d['revision']==e['revision']==PIN,'fixed revision')
 cases=d['cases'];require(len(cases)==len(e['records'])==len(e['native_records'])==1266,'native case denominator');require(d['sequences']==e['sequences']and d['pairs']==e['pairs']and d['steps']==e['steps']and d['checkpoints']==e['checkpoints'],'sequence/grid/step/serialization proof differs')
 for path,digest in e['sources'].items():require(sha(ROOT/path)==digest,'native basis changed')
 native=Native(BINARY)if reexecute else None;full=bad=outfee=dynamic=rewards=arrays=0;profiles={}
 try:
  if native:require(sha(BINARY)==e['binary']['sha256'],'native binary changed')
  for i,(c,r,n)in enumerate(zip(cases,e['records'],e['native_records'])):
   require(c==r and i==c['index']==n['index'],'case identity/native source differs');request=n['request'];response=n['response'];require(request==dict(action='quote_exact_out',pool=c['pool'],output=c['target'],zero_for_one=c['zeroForOne']),'original request binding');require(type(c['passed'])is bool and c['passed']==(response.get('reverted')is not True),'original outcome differs')
   stats=profiles.setdefault(c['profile'],dict(full=0,rejected=0,directions=set()))
   if c['passed']:
    require(response['amount_in']==c['requiredInput']and response['amount_out']==c['actualOutput']==c['target']and response['next_pool']==c['after'],'complete native output/state');require(int(c['requiredInput'])>0 and int(c['target'])>0,'nonzero target/input')
    before=c['pool']['raydium_clmm'];after=c['after']['raydium_clmm'];left=c['zeroForOne'];side='0'if left else'1';other='1'if left else'0';require(after['raw_vault'+side]-before['raw_vault'+side]==int(c['requiredInput'])and before['raw_vault'+other]-after['raw_vault'+other]==int(c['target']),'checked nominal vault deltas')
    require(all(before[k]==after[k]for k in ['bank_slot','bank_unix_timestamp','bank_epoch']),'swap cannot advance its observed Clock')
    mode=before.get('collect_fee_on',0);fee0=left if mode==0 else mode==1;outfee+=fee0!=left
    dynamic+=before.get('dynamic_fee')!=after.get('dynamic_fee');rewards+=before.get('reward_infos')!=after.get('reward_infos');arrays+=sum(a!=b for a,b in zip(before['tick_arrays'],after['tick_arrays']));stats['full']+=1;stats['directions'].add(left);full+=1
   else:stats['rejected']+=1;bad+=1
   if native:require(native.ask(request)==response,'original native replay differs')
  require(len(profiles)==6 and all(s['full']and s['rejected']and len(s['directions'])==2 for s in profiles.values()),'profile/outcome/direction coverage');require(full==947 and bad==319 and outfee>0 and dynamic>0 and rewards>0 and arrays>0,'native state/fee coverage')
  require(len(d['sequences'])==6,'continuous state coverage')
  for sequence in d['sequences']:
   require(len(sequence['steps'])==4,'four-step sequence')
   for a,b in zip(sequence['steps'],sequence['steps'][1:]):
    p=copy.deepcopy(a['after']);old=p['raydium_clmm'];new=b['pool']['raydium_clmm'];require(new['bank_slot']==old['bank_slot']+1 and new['bank_unix_timestamp']>=old['bank_unix_timestamp'],'observed Clock monotonicity');old['bank_slot']=new['bank_slot'];old['bank_unix_timestamp']=new['bank_unix_timestamp'];require(p==b['pool'],'evolving native state differs')
  require(len(d['pairs'])==12,'native two-pool direction grids')
  for pair in d['pairs']:
   require(len(pair['grid'])==5 and pair['parts']==4 and pair['pools'][0]['pool_id']!=pair['pools'][1]['pool_id'],'four-part physical grid')
   costs=[]
   for row in pair['grid']:
    require(sum(int(r['target'])for r in row['rows'])==int(pair['target']),'grid target conservation');cost=sum(int(r['requiredInput'])for r in row['rows']);require(str(cost)==row['requiredInput'],'grid actual input');costs.append(cost)
   require(pair['bestRequiredInput']==str(min(costs)),'native minimum input')
  require(len(d['steps'])==280,'independent original step denominator');dust=stepfull=0
  for row in d['steps']:
   req=row['request'];response=row['response'];require(req['action']=='step_exact_out','output-mode primitive')
   if native:require(native.ask(req)==response,'original output primitive replay')
   if not response.get('reverted'):
    stepfull+=1
    if req['fee']==0 and not req['fee_on_input']and int(response['fee_amount'])>0:dust+=1
  require(dust>0 and stepfull>100,'zero-rate output dust coverage')
  require(len(d['checkpoints'])==6,'six original struct checkpoints')
  if native:
   for checkpoint in d['checkpoints']:
    c=cases[checkpoint['caseIndex']]
    for phase,field in [('before','pool'),('after','after')]:require(native.ask(dict(action='fixture_target',pool=c[field]))==checkpoint[phase],'native struct serialization replay')
  return dict(passed=True,cases=len(cases),fulfilled=full,rejected=bad,profiles={k:{**v,'directions':sorted(v['directions'])}for k,v in profiles.items()},sequences=6,allocation_grids=12,step_vectors=280,step_fulfilled=stepfull,zero_rate_output_dust_steps=dust,output_fee_cases=outfee,dynamic_updates=dynamic,reward_updates=rewards,array_updates=arrays,reexecuted=reexecute,sbf=False,publicTransactionsSubmitted=False,productionReady=False)
 finally:
  if native:native.close()
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--reexecute',action='store_true');p.add_argument('--output',type=Path);a=p.parse_args();r=verify(reexecute=a.reexecute)
 if a.output:a.output.write_text(json.dumps(r,indent=2)+'\n')
 print(json.dumps(r))
