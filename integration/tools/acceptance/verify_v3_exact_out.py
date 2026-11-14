#!/usr/bin/env python3
"""Authenticate original EVM swaps, callback token flow and pricing projections."""
from pathlib import Path
import argparse,copy,json,subprocess,tempfile
from orca_exact_out_reference import ROOT,require,sha
OUT=ROOT/'integration/artifacts/local/v3-exact-out-original-20261114.json'
CORPUS=ROOT/'integration/test/contracts/v3-exact-out.json'
HELPER=ROOT/'integration/tools/reference/v3-exact-out/Reference.sol'
GENERATOR=ROOT/'integration/tools/acceptance/v3_exact_out_reference.cjs'
def verify(corpus=None,reexecute=False):
 d=json.loads(CORPUS.read_bytes())if corpus is None else corpus;e=json.loads(OUT.read_bytes());require(d['source_sha256']==sha(OUT),'original contract evidence digest');require(e['generator_sha256']==sha(GENERATOR)and e['helper_sha256']==sha(HELPER),'reference source changed');require(e['source_pins_sha256']==sha(ROOT/'integration/tools/reference/v3-exact-out/source-pins.json'),'official source pins changed');require(e['chainId']==31337,'actual local EVM chain identity')
 for item in e['loadedDependencies']:require(sha(item['path'])==item['sha256'],'loaded runtime/reference dependency changed')
 require(len(d['cases'])==len(e['cases'])==96 and len(d['sequences'])==2 and len(d['paths'])==4 and len(d['grids'])==2,'original scope denominator')
 require(d['caller']==e['caller']and d['recipient']==e['recipient'],'declared callback payer/recipient')
 for address,c in e['codes'].items():require(c['sha256']==__import__('hashlib').sha256(bytes.fromhex(c['code'][2:])).hexdigest()and c['bytes']==len(c['code'][2:])//2,'actual deployed code digest')
 full=bad=0;protocols=set()
 def flow(r):
  before=r['before'];after=r['post'];require(len(before)==len(after)==len(r['routePools']),'actual pool observation count')
  for i,(b,a)in enumerate(zip(before,after)):
   require(b['projection']==r['routePools'][i]and a['projection']==r['routeAfter'][i],'actual pricing projection')
   p=b['projection'];require(p['v3']['code_sha256']==e['codes'][p['pool_id']]['sha256']and p['v3']['factory_code_sha256']==e['codes'][p['v3']['factory']]['sha256'],'deployed pool/factory identity')
   require(p['v3']['factory']==a['projection']['v3']['factory']and b['tickWords']==a['tickWords'],'immutable factory and initialized bitmap')
  if r['passed']:
   require(r['receipt']['status']==1 and int(r['requiredInput'])>0 and r['actualOutput']==r['target'],'real original transaction outcome')
   incoming=r['routeTokens'][0];outgoing=r['routeTokens'][-1];paid=int(before[0]['balances'][incoming][e['caller']])-int(after[0]['balances'][incoming][e['caller']]);received=int(after[-1]['balances'][outgoing][e['recipient']])-int(before[-1]['balances'][outgoing][e['recipient']]);require(str(paid)==r['requiredInput']and str(received)==r['target'],'actual callback payer debit and recipient credit')
   for token,balances in before[0]['balances'].items():require(sum(int(after[0]['balances'][token][a])-int(v)for a,v in balances.items())==0,'actual ERC20 conservation')
  else:
   require(r['receipt']['status']==0,'actual failed transaction receipt')
   for b,a in zip(before,after):
    for field in ['projection','slot','liquidity','allTicks','tickWords','protocolFees','feeGrowth','observations','balances']:require(b[field]==a[field],'original observable rollback:'+field)
 for c,r in zip(d['cases'],e['cases']):
  expected={k:copy.deepcopy(v)for k,v in r.items()if k not in ['before','post','receipt','error','control']};expected['receipt']=r['receipt'];expected['control']={'passed':False,'receipt':r['control']['receipt']}if r.get('control')else None;require(c==expected,'case differs from original transaction evidence');flow(r)
  if c['passed']:flow(r['control']);full+=1;protocols.add(c['protocol'])
  else:bad+=1
 require((full,bad,len(protocols))==(80,16,2),'fulfilled/rejected protocol coverage')
 for actual,reference in zip(d['sequences'],e['sequences']):
  require(actual['protocol']==reference['protocol']and len(actual['steps'])==len(reference['steps'])==4,'continuous original swaps')
  for i,(a,b)in enumerate(zip(actual['steps'],reference['steps'])):
   require(a=={k:v for k,v in b.items()if k not in ['before','post']},'sequence source');flow(b)
   if i:require(reference['steps'][i-1]['after']==b['pool'],'evolving pricing state')
 for actual,reference in zip(d['paths'],e['paths']):require(actual=={k:v for k,v in reference.items()if k not in ['before','post']},'real recursive callback path');flow(reference);require(len(reference['routePools'])==2,'two physical swaps')
 for g,r in zip(d['grids'],e['grids']):
  require(g['protocol']==r['protocol']and g['pools']==r['pools']and len(g['grid'])==5,'independent original fee-tier grid');costs=[]
  for a,b in zip(g['grid'],r['grid']):
   require(a=={**b,'rows':[{k:v for k,v in x.items()if k not in ['before','post']}for x in b['rows']]},'actual grid observations')
   for row in b['rows']:flow(row)
   cost=sum(int(row['requiredInput'])for row in b['rows']);require(str(cost)==a['requiredInput']and sum(int(row['target'])for row in b['rows'])==10000,'actual grid conservation');costs.append(cost)
  require(g['bestRequiredInput']==str(min(costs)),'native grid minimum input')
 if reexecute:
  with tempfile.TemporaryDirectory(prefix='v3-original-replay-',dir=ROOT/'integration/artifacts/local')as directory:
   log=Path(directory)/'replay.log'
   with log.open('w')as output:r=subprocess.run(['node',str(GENERATOR),'--output-dir',directory],cwd=ROOT,stdout=output,stderr=subprocess.STDOUT)
   require(r.returncode==0,'original EVM replay process failed: '+log.read_text(errors='replace')[-1500:]);fresh=json.loads((Path(directory)/'original.json').read_bytes())
   # Every observable field and receipt is deterministic in the pinned EVM.
   require(fresh==e,'fresh original contract execution differs')
 return dict(passed=True,cases=96,fulfilled=80,rejected=16,real_max_input_revert_controls=80,sequences=2,two_hop_paths=4,fee_tier_grids=2,reexecuted=reexecute,projection='pricing-state-only; all original observable ledgers retained in evidence',unsignedBuilt=False,publicTransactionsSubmitted=False,productionReady=False)
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('--reexecute',action='store_true');p.add_argument('--output',type=Path);a=p.parse_args();r=verify(reexecute=a.reexecute)
 if a.output:a.output.write_text(json.dumps(r,indent=2)+'\n')
 print(json.dumps(r))
