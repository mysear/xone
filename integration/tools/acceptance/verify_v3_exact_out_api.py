#!/usr/bin/env python3
"""Actual HTTP quote/plan capture to original callback token-flow evidence."""
from pathlib import Path
import argparse,json
from orca_exact_out_reference import require
from verify_v3_exact_out import CORPUS

def verify(directory):
 f=json.loads(CORPUS.read_bytes());paths=list(Path(directory).glob('v3-*-api.json'));require(len(paths)==len(f['cases'])==96,'actual HTTP denominator');full=bad=0;seen=set()
 for path in paths:
  d=json.loads(path.read_bytes());i=d['index'];require(i not in seen and 0<=i<96,'capture identity');seen.add(i);c=f['cases'][i];require(d['passed']==c['passed'],'actual API/original outcome');q=d['response'];request=d['request']
  require(request['kind']=='EXACT_OUT'and request['caller']==f['caller']and request['recipient']==f['recipient']and request['buyAmount']==c['target']and request['sellToken']==c['routeTokens'][0]and request['buyToken']==c['routeTokens'][-1],'actual declared original funding context')
  if not c['passed']:require(q.get('quoteId')is None,'failed original full target produced quote');bad+=1;continue
  require(q['requiredSellAmount']==c['requiredInput']and q['grossBuyAmount']==q['minBuyAmount']==c['target']and q['outputSurplus']=='0','actual HTTP amount conservation');e=d['evidence'];plan=e['execution_plan'];require(q['planHash']==plan['plan_hash']and q['quoteId']==e['quote']['quoteId']and e['snapshot_valid']is True,'actual plan/view binding');a=plan['allocation'];require(a['required_input']==c['requiredInput']and a['target_output']==c['target'],'actual output allocation');require(len(a['routes'])==1 and a['routes'][0]['factory']==c['pool']['v3']['factory'],'original single-factory path');require(q['verification']['level']=='INDICATIVE'and e['simulation_evidence']is None,'reference execution cannot mark live quote verified');full+=1
 require((full,bad)==(80,16),'actual HTTP case outcomes');return dict(passed=True,httpTargetQuotes=96,fulfilled=80,rejected=16,unsignedBuilt=False,publicTransactionsSubmitted=False)
if __name__=='__main__':
 p=argparse.ArgumentParser();p.add_argument('directory',type=Path);p.add_argument('--output',type=Path);a=p.parse_args();r=verify(a.directory)
 if a.output:a.output.write_text(json.dumps(r,indent=2)+'\n')
 print(json.dumps(r))
