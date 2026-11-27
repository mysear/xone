#!/usr/bin/env python3
"""Normal per-candidate local worker ranking with independent original execution."""
import copy
from fractions import Fraction
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
import urllib.error
import urllib.request
from evm_dynamic_fee_http import ROOT, require, sha
from native_valuation_http import prepare
from native_valuation_vectors import sign


def assert_objective_bound(quote, request, pools, exact):
    sell, buy = request['sellToken'], request['buyToken']

    def rates(token, visited, rate):
        if token == buy:
            yield rate
            return
        if len(visited) == 4:
            return
        for pool in pools:
            if pool['token0'] == token:
                target, debit, credit = pool['token1'], pool['reserve0'], pool['reserve1']
            elif pool['token1'] == token:
                target, debit, credit = pool['token0'], pool['reserve1'], pool['reserve0']
            else:
                continue
            if target not in visited:
                yield from rates(target, visited | {target},
                                 rate * Fraction(997 * int(credit), 1000 * int(debit)))

    path_rates = list(rates(sell, {sell}, Fraction(1)))
    require(len(path_rates) == 2, 'Bound test graph changed')
    best = max(path_rates)
    if exact:
        target = int(quote['buyAmount']) / best
        limit = -(-target.numerator // target.denominator)
        gap = int(quote['requiredSellAmount']) - limit
    else:
        limit = int(int(quote['sellAmount']) * best)
        gap = limit - int(quote['grossBuyAmount'])
    require(gap >= 0 and quote['quality']['objectiveBound'] == dict(
        scope='RETAINED_STANDARD_V2_PATHS_AT_SNAPSHOT',
        objective='REQUIRED_INPUT_LOWER_BOUND' if exact else 'GROSS_OUTPUT_UPPER_BOUND',
        candidatePaths=len(path_rates), limitAtoms=str(limit), gapAtoms=str(gap),
    ), 'Quote bound differs from independent rational enumeration')


def run(binary, output, corpus, *, exact=False, mode='priced', limit=16, legacy=False):
    output.mkdir(parents=True,exist_ok=False)
    source=next(c for c in corpus['cases'] if c['id']==('EXACT_OUT-1-false' if exact else 'EXACT_IN-1-false'))
    pricing_source=dict(state_anchor=source['fixture']['state_anchor'],token_in=source['path'][0],token_out=source['path'][-1])
    config,book,policy=prepare(output/'oracle', mode if mode in ['stale','future','missing-buy','underflow','overflow'] else 'priced',pricing_source)
    if mode=='priced':
        for i,p in enumerate(policy['policies']):book['prices'][i]=sign(p,book['prices'][i]['observation']['anchor'],10000000,1)
        Path(policy['observations_file']).write_text(json.dumps(book)+'\n')
    info_path=output/'info.json';env=dict(os.environ)
    for key in ['XONE_OP_API_SOURCE','XONE_EVM_PREPARED_HTTP','XONE_EVM_COINBASE_ALIAS','XONE_EVM_LEGACY_FEE']:env.pop(key,None)
    env.update(XONE_NATIVE_VALUATION_CONFIG=str(config),XONE_CANDIDATE_COST_MAX_PLANS=str(limit),XONE_CANDIDATE_COST_CORPUS=str(corpus['_path']),XONE_EVM_PATH_CASE=source['id'])
    if legacy:env['XONE_EVM_LEGACY_FEE']='1'
    with (output/'process.log').open('wb') as log:
        process=subprocess.Popen([str(binary),str(info_path)],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,start_new_session=True)
        try:
            until=time.monotonic()+30
            while not info_path.exists():
                require(process.poll() is None,'Candidate HTTP startup failed: '+str(output));require(time.monotonic()<until,'Candidate startup timeout');time.sleep(.05)
            info=json.loads(info_path.read_bytes());opener=urllib.request.build_opener(urllib.request.ProxyHandler({}))
            def call(path,body=None):
                data=None if body is None else json.dumps(body).encode()
                request=urllib.request.Request(info['api']+path,data=data,headers={'Authorization':'Bearer '+info['token'],'Content-Type':'application/json'})
                try:
                    with opener.open(request,timeout=20) as response:return response.status,json.load(response)
                except urllib.error.HTTPError as e:return e.code,json.load(e)
            request=info['exactOutRequest'] if exact else info['request'];endpoint='/v1/quote-exact-out' if exact else '/v1/quote'
            start=time.monotonic();status,q=call(endpoint,request);elapsed=time.monotonic()-start
            require(status==200,'Candidate quote failed: '+json.dumps(q));ranking=q['quality']['costRanking'];optimizer=ranking['optimizer']
            assert_objective_bound(q, request, source['pools'], exact)
            require(ranking['selectedPlanHash']==q['planHash'] and ranking['scope']=='LOCAL_EVM_TRANSACTION_EXECUTION_FEES_ONLY' and ranking['productionVerified'] is False and ranking['completeQuoteCostRanking'] is False,'Candidate scope overclaimed')
            require(q['verification']['level']=='INDICATIVE' and q['verification']['success'] is False and q['netSellAmountEstimate' if exact else 'netBuyAmountEstimate'] is None,'Local cost promoted production/full net')
            require(optimizer['evaluated_candidates']==2 and ranking['simulationAttempts']==min(limit,2) and len(ranking['candidateExecutions'])==min(limit,2),'Candidate set/plan budget changed')
            applied=limit>=2 and mode in ['priced','underflow']
            if exact and mode in ['missing-buy','overflow']:applied=True
            require(ranking['costAdjustmentApplied']==applied,'Incomplete coverage did not uniformly fall back')
            require(ranking['simulationBudgetExhausted']==(limit<2),'Simulation truncation not disclosed')
            if limit<2:require(q['quality']['budgetExhausted'] is True and ranking['unavailableReasons']=={'MAX_SIMULATION_PLANS':1},'Plan budget was hidden or changed')
            selected=ranking['selectedExecution'];costs=q['costs']
            if selected is None:
                require(costs['gasUnits'] is None and costs['gasNative'] is None and costs['chainFeeCompleteness']=='UNAVAILABLE','Unexecuted selected plan borrowed candidate costs')
            else:
                require(costs['scope']=='SELECTED_LOCAL_CANDIDATE_TRANSACTION' and costs['selectedPlanHash']==q['planHash'] and costs['completeQuoteCosts'] is False,'Selected partial cost scope changed')
                for field in ['gasUnits','gasNative','nativeKnownSubtotal','payerNetNativeDebit','transactionEconomics','feeBreakdown','bindingHash','authority','chainFeeCompleteness','missingComponents']:
                    require(costs[field]==selected['costs'][field],'Selected quote cost detached: '+field)
                source=costs['sourceEvidence']
                require(source['planHash']==q['planHash'] and source['bindingHash']==selected['bindingHash'] and source['bundleHash']==selected['bundleHash'] and source['workerHash']==selected['workerHash'] and source['quoteViewId']==q['quoteViewId'] and source['snapshotRef']==q['snapshotRef'] and source['productionVerified'] is False,'Selected quote provenance detached')
                require(costs['quoteRankingApplied']==applied and costs['totalNativeEstimate'] is None and costs['netBuyAmountEstimate'] is None and costs['netSellAmountEstimate'] is None and costs['deductedFromQuoteOutput'] is False,'Partial fee estimate promoted complete or deducted twice')
                valued=any(isinstance(costs['transactionEconomics'].get(side+'TokenFee'),str) for side in ['buy','sell'])
                require(costs['tokenValuationStatus']==('CONFIGURED_ORACLE_AUTHORIZED_WITH_PER_SIDE_AVAILABILITY' if valued else 'UNAVAILABLE'),'Per-side price availability changed')
            records=[];scored=[]
            for h,execution in ranking['candidateExecutions'].items():
                require(execution['verification']['success'] is True,'Candidate execution mismatch')
                economics=execution['costs']['transactionEconomics'];outcome=execution['outcome'];side='sell' if exact else 'buy'
                price=next((p for p in book['prices'] if p['observation']['token']==(request['sellToken'] if exact else request['buyToken'])),None)
                if price is not None and mode not in ['stale','future']:
                    o=price['observation'];fee=(int(execution['costs']['payerNetNativeDebit'])*int(o['token_atoms'])+int(o['native_atoms'])-1)//int(o['native_atoms'])
                    if fee<2**256:
                        require(economics[side+'TokenFee']==str(fee),'Native conversion differs from independent integer arithmetic')
                        score=int(outcome['actual_input_consumed'])+fee if exact else int(outcome['actual_buy_amount'])-fee
                        scored.append((score,h,fee))
                # ABI tail deadline must be the final quote deadline, never renewed.
                calldata=execution['transaction']['data'];words=[calldata[10+i:10+i+64] for i in range(0,len(calldata)-10,64)]
                require(int(words[4],16)==q['validity']['expiresAtMs']//1000,'Candidate/final deadline detached')
                records.append(dict(id=output.name+'-'+h,plan_hash=h,fixture=json.loads(info_path.with_suffix('.state.json').read_bytes())['fixture'],execution=execution,sell_token=request['sellToken'],buy_token=request['buyToken'],recipient=request['recipient']))
            if applied:
                chosen=(min if exact else max)(scored)[1]
                score, _, fee = next(row for row in scored if row[1] == chosen)
                require(chosen==q['planHash'] and optimizer['signed_objective_token_units']==str(score),'Normal API did not optimize measured candidate economics')
                require(costs['rankingObjective'] == {
                    'token': request['sellToken' if exact else 'buyToken'],
                    'signedAmountAtoms': str(score),
                    'feeAtoms': str(fee),
                    'chainFeeCompleteness': 'LOCAL_EVM_ONLY',
                }, 'Published objective differs from independently valued settlement')
                # Costly two-hop route has better raw amounts; priced costs prefer direct.
                if mode=='priced':require(len(q['routes'][0]['edges'] if exact else q['routes'][0]['path']['edges'])==1,'Fees did not reverse raw route preference')
                if mode=='underflow' and not exact:require(int(optimizer['signed_objective_token_units'])<0,'Negative objectives were clamped')
            else:
                require('rankingObjective' not in costs, 'Gross fallback disclosed an adjusted objective')
                require(len(q['routes'][0]['edges'] if exact else q['routes'][0]['path']['edges'])==2,'Uniform raw fallback dropped an unpriced candidate')
            status,cached=call(endpoint,request);require(status==200 and cached==q,'Cached candidate quote changed')
            status,evidence=call('/internal/quotes/'+q['quoteId']+'/evidence');require(status==200 and evidence['quote']==q and evidence['execution_plan']['plan_hash']==q['planHash'],'Selected quote cache/evidence binding lost')
            if ranking['selectedExecution'] is not None:
                status,simulation=call('/v1/quotes/'+q['quoteId']+'/simulate',{})
                require(status==200 and simulation['bindingHash']==ranking['selectedExecution']['bindingHash'] and simulation['transaction']==ranking['selectedExecution']['transaction'] and simulation['outcome']==ranking['selectedExecution']['outcome'],'Final selected simulation differs from scored calldata/outcome')
                (output/'simulation.json').write_text(json.dumps(simulation,indent=2)+'\n')
            bad=copy.deepcopy(request);bad['candidateCosts']={'fee':'0'};status,rejected=call(endpoint,bad);require(status>=400,'Client cost override accepted')
            for name,value in [('quote',q),('evidence',evidence),('rejected',rejected),('replay-input',dict(records=records))]:(output/(name+'.json')).write_text(json.dumps(value,indent=2)+'\n')
            return dict(mode=mode,exact_out=exact,legacy=legacy,limit=limit,seconds=elapsed,records=records,cost_adjustment_applied=applied)
        finally:
            if process.poll() is None:
                os.killpg(process.pid,signal.SIGTERM)
                try:process.wait(timeout=5)
                except subprocess.TimeoutExpired:os.killpg(process.pid,signal.SIGKILL);process.wait()


def main(binary,corpus_path,output):
    binary=Path(binary).resolve();output=Path(output).resolve();output.mkdir(parents=True,exist_ok=False)
    corpus_path=Path(corpus_path).resolve();corpus=json.loads(corpus_path.read_bytes());corpus['_path']=str(corpus_path);rows=[];records=[]
    for exact in [False,True]:
        for mode,limit,legacy in [('priced',16,False),('priced',16,True),('priced',1,False),('stale',16,False),('future',16,False),('missing-buy',16,False),('underflow',16,False),('overflow',16,False)]:
            row=run(binary,output/(f'{int(exact)}-{mode}-{limit}-{int(legacy)}'),corpus,exact=exact,mode=mode,limit=limit,legacy=legacy)
            records.extend(row.pop('records'));rows.append(row)
    replay=output/'original-replay-input.json';replay.write_text(json.dumps(dict(records=records),indent=2)+'\n')
    result=subprocess.run(['node','integration/tools/ci/candidate-cost-original-replay.cjs',str(replay),str(output/'original-replay.json')],cwd=ROOT,capture_output=True,text=True,timeout=300)
    (output/'original-replay.log').write_text(result.stdout+result.stderr);require(result.returncode==0,'Original candidate execution replay failed: '+result.stdout+result.stderr)
    (output/'report.json').write_text(json.dumps(dict(passed=True,cases=rows,candidate_executions=len(records),independent_original_replays=len(records),oracle_scope='PINNED_TEST_SIGNERS_SYNTHETIC_RATIO_NOT_LIVE_MARKET',harness_sha256=sha(binary),corpus_sha256=sha(corpus_path),artifacts={str(p.relative_to(output)):sha(p) for p in sorted(output.rglob('*')) if p.is_file()},production_ready=False),indent=2)+'\n')

if __name__=='__main__':main(*sys.argv[1:])
