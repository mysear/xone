#!/usr/bin/env python3
"""Normal executed Router transactions with independent signed oracle fixtures."""
import copy
import json
from pathlib import Path
import subprocess
import sys
from evm_dynamic_fee_http import run,require,sha,ROOT
from native_valuation_vectors import policy,sign,digest


def prepare(directory, mode='priced', source=None):
    directory.mkdir(parents=True,exist_ok=False)
    if source is None:
        corpus=json.loads((ROOT/'dex-router/test/contracts/v2-complete.json').read_bytes())
        source=next(c for c in corpus['cases'] if c['service_eligible'] and c['pool']['protocol']=='uniswap-v2-complete')
    s=source['state_anchor']
    anchor=dict(chain_key=s['chain_key'],genesis_hash=s['genesis_hash'],block_number=s['block_number'],block_hash=s['block_hash'],parent_hash=s['parent_hash'],timestamp_ms=s['timestamp']*1000)
    policies=[policy(s['chain_key'],s['genesis_hash'],source[t]) for t in ['token_in','token_out']]
    ratios=[(1,3),(3,2)]
    if mode=='underflow':ratios[1]=(10**18,1)
    if mode=='overflow':ratios[1]=(2**256-1,1)
    prices=[sign(p,anchor,n,d) for p,(n,d) in zip(policies,ratios)]
    if mode=='missing-buy':prices.pop()
    if mode in ['stale','future']:
        for i,(p,(n,d)) in enumerate(zip(policies,ratios)):
            prices[i]=sign(p,anchor,n,d,anchor['timestamp_ms']-60001 if mode=='stale' else anchor['timestamp_ms']+1,anchor['timestamp_ms']-1 if mode=='stale' else anchor['timestamp_ms']+60001)
    book=dict(schema_version=1,prices=prices)
    book_path=directory/'prices.json';book_path.write_text(json.dumps(book,indent=2)+'\n')
    config=dict(observations_file=str(book_path),policies=policies)
    path=directory/'config.json';path.write_text(json.dumps(config,indent=2)+'\n')
    return path,book,config


def audit_result(directory,book,config,mode):
    simulation=json.loads((directory/'simulation.json').read_bytes());quote=json.loads((directory/'quote.json').read_bytes())
    costs=simulation['costs'];e=costs['transactionEconomics'];out=simulation['outcome']
    require(e['modelVersion']=='xone-authenticated-native-valuation/1' and e['scope']=='EVALUATED_TRANSACTION_NETWORK_FEES_ONLY','Price scope changed')
    require(e['priceBookHash']==digest(book) and e['bindingHash']==simulation['bindingHash'] and e['feePayer']==simulation['transaction']['from'],'Price book/transaction binding changed')
    require(e['actualInput']==out['actual_input_consumed'] and e['actualOutput']==out['actual_buy_amount'],'Price amounts were not actually measured')
    require(e['payerNetNativeDebit']==costs['payerNetNativeDebit'] and e['grossChainFeeNative']==costs['nativeKnownSubtotal'],'Price cost did not use parent-audited payer debit')
    require(e['bankAuthority']=='PROVIDED_LOCAL_STATE' and e['productionVerified'] is False and e['quoteRankingApplied'] is False and e['additionalOperationsIncluded'] is False and e['deductedFromQuoteOutput'] is False,'Price incorrectly upgraded scope')
    require(costs['totalNativeEstimate'] is None and costs['netBuyAmountEstimate'] is None and costs['netSellAmountEstimate'] is None,'Single transaction priced as whole quote')
    require(quote['quality']['costRanking']['costAdjustmentApplied'] is False,'Single execution changed all-candidate ranking')
    for side,token,amount in [('buy',quote['buyToken'] if 'buyToken' in quote else config['policies'][1]['token'],int(out['actual_buy_amount'])),('sell',config['policies'][0]['token'],int(out['actual_input_consumed']))]:
        key='feeAdjustedBuyAmount' if side=='buy' else 'feeAdjustedSellAmount'
        price=next((p for p in book['prices'] if p['observation']['token']==token),None)
        if price is None or mode in ['stale','future']:
            require(e[key] is None and e[side+'UnavailableReason']==('AUTHORIZED_TOKEN_PRICE_UNAVAILABLE' if price is None else 'NATIVE_VALUATION_TIME'),'Missing/stale price fabricated')
            continue
        observation=price['observation'];n=int(observation['token_atoms']);d=int(observation['native_atoms']);debit=int(costs['payerNetNativeDebit']);fee=(debit*n+d-1)//d
        if fee>=2**256:
            require(e[key] is None and e[side+'UnavailableReason']=='AMOUNT_OVERFLOW','Price overflow truncated')
            continue
        require(e[side+'TokenFee']==str(fee),'Integer native conversion differs')
        reference=e[side+'Price'];require(reference['policyHash']==observation['policy_hash'] and reference['observationHash']==digest(observation) and reference['anchor']==observation['anchor'],'Price provenance changed')
        require(reference['signers']==['1','2'] and reference['threshold']==2 and reference['authority']=='CONFIGURED_ORACLE_SIGNER_QUORUM' and reference['sourceIndependenceAttested'] is False and reference['bankConsensusAttested'] is False,'Oracle authority changed')
        expected=amount-fee if side=='buy' else amount+fee
        if expected<0:require(e[key] is None and e[side+'UnavailableReason']=='NETWORK_FEE_EXCEEDS_GROSS_OUTPUT','Negative net clamped')
        elif expected>=2**256:require(e[key] is None and e[side+'UnavailableReason']=='AMOUNT_OVERFLOW','Input fee overflow truncated')
        else:require(e[key]==str(expected) and e[side+'UnavailableReason'] is None,'Fee-adjusted amount differs')


def rejected_startups(binary,output):
    rows=[]
    for mode in ['signature','duplicate-signature','wrong-anchor','unknown-policy','duplicate-price','unknown-field']:
        directory=output/mode;path,book,config=prepare(directory)
        if mode=='signature':book['prices'][0]['signatures'][0]['signature']='00'*64
        elif mode=='duplicate-signature':book['prices'][0]['signatures'][1]=copy.deepcopy(book['prices'][0]['signatures'][0])
        elif mode=='wrong-anchor':book['prices'][0]['observation']['anchor']['block_hash']='0x'+'44'*32
        elif mode=='unknown-policy':book['prices'][0]['observation']['policy_hash']='44'*32
        elif mode=='duplicate-price':book['prices'].append(copy.deepcopy(book['prices'][0]))
        else:book['prices'][0]['trustCallerPrice']=True
        Path(config['observations_file']).write_text(json.dumps(book)+'\n')
        info=directory/'info.json'
        import os
        env=dict(os.environ,XONE_NATIVE_VALUATION_CONFIG=str(path.resolve()))
        for key in ['XONE_OP_API_SOURCE','XONE_EVM_PATH_CASE','XONE_EVM_PREPARED_HTTP','XONE_EVM_COINBASE_ALIAS','XONE_EVM_LEGACY_FEE']:env.pop(key,None)
        with (directory/'process.log').open('wb') as log:
            result=subprocess.run([str(binary.resolve()),str(info.resolve())],cwd=ROOT,env=env,stdout=log,stderr=subprocess.STDOUT,timeout=30)
        raw=(directory/'process.log').read_text()
        require(result.returncode!=0 and not info.exists() and 'NATIVE_VALUATION_' in raw,'Forged oracle started HTTP: '+mode)
        rows.append(dict(mode=mode,log_sha256=sha(directory/'process.log')))
    return rows


def main(binary,output):
    binary=Path(binary);output=Path(output).resolve();output.mkdir(parents=True,exist_ok=False);rows=[]
    for legacy in [False,True]:
        for alias in [False,True]:
            for exact in [False,True]:
                name=f'priced-{int(legacy)}-{int(alias)}-{int(exact)}';directory=output/name;config,book,policy_doc=prepare(output/(name+'-oracle'))
                run(binary,directory,legacy=legacy,native_alias=alias,exact_out=exact,valuation_config=config)
                audit_result(directory,book,policy_doc,'priced');rows.append(dict(id=name,simulation_sha256=sha(directory/'simulation.json')))
    for mode in ['stale','future','missing-buy','underflow','overflow']:
        config,book,policy_doc=prepare(output/(mode+'-oracle'),mode);directory=output/mode
        run(binary,directory,valuation_config=config);audit_result(directory,book,policy_doc,mode);rows.append(dict(id=mode,simulation_sha256=sha(directory/'simulation.json')))
    rejected=rejected_startups(binary,output/'rejections')
    (output/'report.json').write_text(json.dumps(dict(passed=True,cases=rows,actual_worker_executions=len(rows),startup_rejections=rejected,harness_sha256=sha(binary),artifacts={str(p.relative_to(output)):sha(p) for p in sorted(output.rglob('*')) if p.is_file()},oracle_scope='PINNED_TEST_SIGNER_QUORUM_NOT_LIVE_MARKET',quote_ranking_applied=False,production_ready=False,publicTransactionsSubmitted=False),indent=2)+'\n')


if __name__=='__main__':main(sys.argv[1],sys.argv[2])
