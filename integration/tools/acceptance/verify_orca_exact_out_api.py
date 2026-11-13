#!/usr/bin/env python3
"""Check real Router HTTP captures against the authenticated native corpus."""
from pathlib import Path
import argparse,json
from orca_exact_out_reference import require
from verify_orca_exact_out import CORPUS

def verify(directory):
 corpus=json.loads(CORPUS.read_bytes());paths=list(Path(directory).glob('orca-*-api.json'));require(len(paths)==len(corpus['cases'])==600,'actual API capture denominator')
 full=bad=0;seen=set()
 for path in paths:
  d=json.loads(path.read_bytes());i=d['caseIndex'];require(i not in seen and 0<=i<600,'API capture identity');seen.add(i);c=corpus['cases'][i];require(d['passed']==c['passed'],'API/native outcome mismatch')
  if not d['passed']:
   require(d['phase']in ['BASELINE_ADMISSION','QUOTE'],'API rejection phase');require(d.get('response',{}).get('quoteId')is None,'rejected API quote retained');bad+=1;continue
  req=d['request'];q=d['response'];e=d['evidence'];p=c['pool'];plan=e['execution_plan']
  require(req['kind']=='EXACT_OUT'and req['buyAmount']==c['target']and req['sellToken']==p['token0'if c['zeroForOne']else'token1']and req['buyToken']==p['token1'if c['zeroForOne']else'token0'],'actual API request direction/target')
  require(q['requiredSellAmount']==c['requiredInput']and q['grossBuyAmount']==q['buyAmount']==q['minBuyAmount']==c['target']and q['outputSurplus']=='0','actual API exact amount conservation')
  require(q['quoteId']==e['quote']['quoteId']and q['planHash']==plan['plan_hash']and e['snapshot_valid']is True and e['simulation_evidence']is None,'actual quote/plan/view binding')
  allocation=plan['allocation'];require(allocation['required_input']==c['requiredInput']and allocation['target_output']==c['target']and len(allocation['routes'])==1,'actual ordered allocation')
  route=allocation['routes'][0];require(route['edges'][0]['pool_id']==p['pool_id']and route['factory']=='whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc'and route['amount_in']==c['requiredInput']and route['amount_out']==c['target'],'actual route amount/factory')
  require(q['verification']['level']=='INDICATIVE'and q['verification']['success']is False,'native math cannot grant transaction verification');full+=1
 require(full==431 and bad==169,'API fulfilled/rejected scope');return dict(passed=True,captures=600,fulfilled=full,rejected=bad,transactionsBuilt=False,sbf=False,publicTransactionsSubmitted=False)
if __name__=='__main__':
 parser=argparse.ArgumentParser();parser.add_argument('directory',type=Path);parser.add_argument('--output',type=Path);args=parser.parse_args();result=verify(args.directory)
 if args.output:args.output.write_text(json.dumps(result,indent=2)+'\n')
 print(json.dumps(result))
