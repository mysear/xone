#!/usr/bin/env python3
"""Authenticate native target evidence, raw bank projections and optional replay."""
from pathlib import Path
import argparse,copy,json
from orca_exact_out_reference import ROOT,OUT,STATIC,TARGET,PIN,Native,module,require,sha
CORPUS=ROOT/'integration/test/contracts/orca-exact-out.json'
def verify(corpus=None,reexecute=False):
 doc=json.loads(CORPUS.read_bytes())if corpus is None else corpus
 evidence=json.loads(OUT.read_bytes())
 require(doc['source_sha256']==sha(OUT),'original evidence digest')
 require(evidence['generator_sha256']==sha(ROOT/'integration/tools/acceptance/orca_exact_out_reference.py'),'original generator changed')
 require(doc['revision']==evidence['revision']==PIN,'original revision')
 cases=doc['cases'];records=evidence['records'];raw=evidence['native_records']
 require(len(cases)==len(records)==len(raw)==600,'complete native case denominator')
 require(doc['sequences']==evidence['sequences']and doc['pairs']==evidence['pairs'],'sequence/grid evidence differs')
 clients={};projections={};profiles={};full=bad=crosses=reward_updates=oracle_updates=0
 try:
  for i,(c,r,n)in enumerate(zip(cases,records,raw)):
   require(c==r and n['index']==i,'case differs from original evidence')
   req=n['request'];result=n['response'];kind=n['kind'];ok=c['passed']
   require(type(ok)is bool,'native outcome type')
   if kind=='static':
    require(req==dict(action='quote_exact_out',pool=c['pool'],output=c['target'],zero_for_one=c['zeroForOne']),'static request binding')
    require(ok==(result.get('reverted')is not True),'static native outcome')
    if ok:require(result['amount_in']==c['requiredInput']and result['amount_out']==c['target']and result['next_pool']==c['after'],'static original post-state')
   else:
    if kind not in projections:projections[kind]=module('verify_projection_'+kind,'integration/tools/acceptance/'+kind.replace('-service','').replace('-','_')+'_fixture.py')
    projection=projections[kind];p=projection.pool(req['config'],n['before_native_state']);p['chain_key']='solana:mainnet-beta'
    require(p==c['pool']and req['params']['amount']==c['target']and req['params']['a_to_b']==c['zeroForOne']and req['params']['amount_specified_is_input']is False,'adaptive raw source binding')
    require(ok==(result['status']=='fulfilled'),'adaptive native outcome')
    if ok:
     after=projection.pool(req['config'],result['next_state']);after['chain_key']=p['chain_key']
     require(after==c['after']and result['amount_in']==c['requiredInput']and result['amount_out']==c['target']and result['amount_remaining']=='0','adaptive complete native post-state')
    else:require(result['post_accounts']==req['bank']['accounts'],'failed native helper state mutation')
   if reexecute:
    if kind not in clients:
     binary=STATIC if kind=='static'else TARGET/(kind+'-reference');clients[kind]=Native(binary)
     expected=evidence['identities'][kind]
     require(sha(binary)==(expected['sha256']if kind=='static'else expected['executed_binary_sha256']),'executed binary changed')
     if kind!='static':require(clients[kind].ask(dict(action='identity'))==expected,'compiled helper/source authority changed')
    if kind!='static':require(clients[kind].ask(dict(action='state',config=req['config'],bank=req['bank']))==n['before_native_state'],'raw account pre-state decode replay')
    require(clients[kind].ask(req)==result,'original program replay differs')
   profile=profiles.setdefault(c['profile'],dict(full=0,rejected=0,directions=set()))
   if ok:
    require(int(c['requiredInput'])>0 and c['actualOutput']==c['target']and int(c['target'])>0,'native exact target conservation')
    before=c['pool']['orca_whirlpool'];after=c['after']['orca_whirlpool'];zero=c['zeroForOne'];side='0'if zero else'1';other='1'if zero else'0'
    require(after['raw_vault'+side]-before['raw_vault'+side]==int(c['requiredInput'])and before['raw_vault'+other]-after['raw_vault'+other]==int(c['target']),'raw vault exact debit/credit')
    crosses+=sum(a['ticks']!=b['ticks']for a,b in zip(before['tick_arrays'],after['tick_arrays']))
    reward_updates+=before.get('reward_infos')!=after.get('reward_infos');oracle_updates+=before.get('adaptive')!=after.get('adaptive')
    profile['full']+=1;profile['directions'].add(zero);full+=1
   else:profile['rejected']+=1;bad+=1
  require(len(profiles)==5 and all(p['full']and p['rejected']and len(p['directions'])==2 for p in profiles.values()),'profile/direction/rejection coverage')
  require(full==431 and bad==169 and crosses>0 and reward_updates>0 and oracle_updates>0,'required numerical/state coverage')
  require(len(doc['sequences'])==8 and all(len(s['steps'])==4 for s in doc['sequences']),'sequence coverage')
  for seq in doc['sequences']:
   for before,after in zip(seq['steps'],seq['steps'][1:]):require(before['after']==after['pool'],'evolving native state not conserved')
  require(len(doc['pairs'])==3,'allocation profile coverage')
  for pair in doc['pairs']:
   require(len(pair['grid'])==5 and pair['parts']==4,'complete two-pool grid')
   costs=[]
   for row in pair['grid']:
    require(sum(int(c['target'])for c in row['rows'])==int(pair['target']),'grid output conservation')
    cost=sum(int(c['requiredInput'])for c in row['rows']);require(str(cost)==row['requiredInput'],'grid input conservation');costs.append(cost)
   require(str(min(costs))==pair['bestRequiredInput'],'independent grid minimum')
  return dict(passed=True,cases=len(cases),fulfilled=full,rejected=bad,profiles={k:{**v,'directions':sorted(v['directions'])}for k,v in profiles.items()},sequences=8,allocation_pairs=3,crossed_array_updates=crosses,reward_updates=reward_updates,oracle_updates=oracle_updates,reexecuted=reexecute,sbf=False,publicTransactionsSubmitted=False,productionReady=False)
 finally:
  for native in clients.values():native.close()
if __name__=='__main__':
 ap=argparse.ArgumentParser();ap.add_argument('--reexecute',action='store_true');ap.add_argument('--output',type=Path);args=ap.parse_args();result=verify(reexecute=args.reexecute)
 if args.output:args.output.write_text(json.dumps(result,indent=2)+'\n')
 print(json.dumps(result))
