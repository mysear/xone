#!/usr/bin/env python3
"""Actual original Router HTTP worker fee disclosures for both quote targets."""
import json
from pathlib import Path
import sys
from evm_dynamic_fee_http import run,require,sha

def main(binary,output,balance_audit=False,payer_cost=False):
    output=Path(output).resolve();output.mkdir(parents=True,exist_ok=False);rows=[]
    for exact_out,native_alias in [(False,False),(True,False)]+([(False,True),(True,True)] if balance_audit else []):
        directory=output/(('alias-' if native_alias else '')+('exact-out' if exact_out else 'exact-in'));report=run(Path(binary),directory,exact_out=exact_out,native_alias=native_alias)
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
        if balance_audit:
            audit=cost['balanceReconciliation'];require(audit['status']=='MATCHED' and audit['sameAccount']==native_alias and audit['scope']=='ZERO_VALUE_BOUND_ERC20_TRANSACTION','Native balance reconciliation was not observed')
            require(audit['caller']==result['transaction']['from'] and audit['coinbase']==result['executionCoinbase'],'Measured fee account identity changed')
            require(cost['nativeUnit']=='EVM_NATIVE_ATOMS','Native atomic units not declared')
        if payer_cost:
            require(cost['payerCostModelVersion']=='source-bound-native-payer/1' and cost['nativeKnownSubtotalKind']=='GROSS_EXECUTION_CHARGE','Gross/net payer semantics missing')
            require(cost['maximumFundingAdmission']=='SOURCE_BALANCE_COVERS_BOUND_GAS_RESERVE' and int(native['caller_before'])>=reserve,'Maximum funding not admitted from source balance')
            debit=int(native['caller_before'])-int(native['caller_after'])
            returned=int(parts['priorityFeeNative']) if native_alias else 0
            require(cost['payerNetNativeDebit']==str(debit) and cost['priorityReturnedToPayerNative']==str(returned),'Payer debit/returned priority differ from original execution')
            require(debit+returned==int(cost['gasNative']),'Payer net and gross cost fail conservation')
        rows.append({'quote_kind':report['quote_kind'],'simulation_sha256':sha(directory/'simulation.json'),'native_execution_fee':cost['gasNative'],'cost_model_version':cost['modelVersion'],'all_disclosure_checks_passed':True,'coinbase_alias':native_alias,'balance_audit_required':balance_audit,'payer_cost_required':payer_cost})
    (output/'report.json').write_text(json.dumps({'passed':True,'actual_worker_executions':len(rows),'cases':rows,'complete_chain_cost_claimed':False,'publicTransactionsSubmitted':False},indent=2)+'\n')
if __name__=='__main__':
    require(len(sys.argv) in [3,4] and (len(sys.argv)==3 or sys.argv[3] in ['--balance-audit','--payer-cost']),'Expected BINARY OUTPUT_DIRECTORY [--balance-audit|--payer-cost]');main(sys.argv[1],sys.argv[2],len(sys.argv)==4,len(sys.argv)==4 and sys.argv[3]=='--payer-cost')
