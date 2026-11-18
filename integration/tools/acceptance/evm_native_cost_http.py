#!/usr/bin/env python3
"""Actual original Router HTTP worker fee disclosures for both quote targets."""
import json
from pathlib import Path
import sys
from evm_dynamic_fee_http import run,require,sha

def main(binary,output):
    output=Path(output).resolve();output.mkdir(parents=True,exist_ok=False);rows=[]
    for exact_out in [False,True]:
        directory=output/('exact-out' if exact_out else 'exact-in');report=run(Path(binary),directory,exact_out=exact_out)
        result=json.loads((directory/'simulation.json').read_text());quote=json.loads((directory/'quote.json').read_text());cost=result['costs'];native=result['outcome']['native_fees']
        require(cost['modelVersion']=='xone-evm-native-cost/1' and cost['scope']=='DECLARED_LOCAL_EVM_TRANSACTION','Native cost version/scope changed')
        require(cost['chainKey']==quote['chainKey'] and cost['bindingHash']==result['bindingHash'],'Native cost binding changed')
        require(cost['gasNative']==result['fees']['evmFeeWei']==native['execution_fee'],'Native subtotal differs from executed fee')
        require(cost['nativeKnownSubtotal']==cost['gasNative']==cost['feeBreakdown'][0]['amountNative'],'Native charge double counted')
        parts=cost['feeBreakdown'][0]['parts'];require(int(parts['baseFeeBurnedNative'])+int(parts['priorityFeeNative'])==int(cost['gasNative']),'Fee component conservation failed')
        reserve=int(result['transactionContext']['gas_limit'])*int(result['transactionContext']['gas_price'])
        require(cost['maxExecutionReserveNative']==str(reserve) and int(cost['reserveHeadroomNative'])+int(cost['gasNative'])==reserve,'Maximum reserve was treated as actual fee')
        require(cost['gasUnits']==str(report['gas_used']) and cost['gasPriceNativePerUnit']==report['effective_gas_price'],'Actual gas/price missing')
        require(all(cost[key] is None for key in ['l1DataNative','approvalNative','totalNativeEstimate','netBuyAmountEstimate','netSellAmountEstimate']),'Missing chain cost/valuation treated as zero')
        require(cost['deductedFromQuoteOutput'] is False and len(cost['missingComponents'])==3 and cost['chainFeeCompleteness']=='LOCAL_EVM_ONLY' and result['verified'] is False,'Native cost promoted beyond source scope')
        rows.append({'quote_kind':report['quote_kind'],'simulation_sha256':sha(directory/'simulation.json'),'native_execution_fee':cost['gasNative'],'cost_model_version':cost['modelVersion'],'all_disclosure_checks_passed':True})
    (output/'report.json').write_text(json.dumps({'passed':True,'actual_worker_executions':2,'cases':rows,'complete_chain_cost_claimed':False,'publicTransactionsSubmitted':False},indent=2)+'\n')
if __name__=='__main__':
    require(len(sys.argv)==3,'Expected BINARY OUTPUT_DIRECTORY');main(sys.argv[1],sys.argv[2])
