export function positiveInteger(value, label) {
  if (value.length > 78 || !/^[0-9]+$/.test(value) || BigInt(value) === 0n || BigInt(value) >= (1n << 256n)) {
    throw new Error(`${label}必须是大于 0 的最小单位整数`);
  }
  return BigInt(value).toString();
}
export function boundedInteger(value, min, max, label) {
  if (!/^[0-9]+$/.test(value) || !Number.isSafeInteger(Number(value)) || Number(value) < min || Number(value) > max) {
    throw new Error(`${label}应在 ${min} 至 ${max} 之间`);
  }
  return Number(value);
}
export function routeView(route) {
  const path = route.path || route;
  return {
    tokens: path.tokens || [],
    pools: (path.edges || []).map(e => e.pool_id || e.poolId),
    protocols: [...new Set((path.edges || []).map(e => e.protocol))],
    input: route.amount_in ?? route.amountIn ?? path.amount_in ?? '—',
    output: route.amount_out ?? route.amountOut ?? path.amount_out ?? '—',
  };
}
export function snapshotView(q) {
  const b = q.snapshotRef?.block || {};
  return `${b.chain_key || b.chainKey || q.chainKey || '—'} / ${b.number_or_slot ?? b.numberOrSlot ?? '—'}`;
}
export function escapeHtml(value) {
  return String(value).replace(/[&<>"']/g, c => ({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;',"'":'&#39;'}[c]));
}

export function protocolBindingsMatch(quote,chain) {
  return Object.entries(quote?.protocolViewIds||{}).every(([key,id])=>{
    const view=(chain?.protocolViews||[]).find(p=>`${p.protocol}@${p.adapterVersion}`===key);
    return view?.ready===true && view.viewId===id;
  });
}

export function singlePreparationPool(quote,chain) {
  const routes=quote?.routes||[];
  if(routes.length!==1)return null;
  const edges=routes[0].path?.edges||routes[0].edges||[];
  if(edges.length!==1)return null;
  return (chain?.pools||[]).find(pool=>pool.available&&pool.poolId===edges[0].pool_id&&pool.protocol===edges[0].protocol)||null;
}


export function evmPreparationPools(quote, chain) {
  if (!quote?.chainKey?.startsWith('evm:') || quote.chainKey !== chain?.chainKey || quote.routes?.length !== 1) return [];
  const path = quote.routes[0].path || quote.routes[0];
  const edges = path.edges || [];
  if (!edges.length || edges.length > 8 || new Set(edges.map(e => e.pool_id)).size !== edges.length) return [];
  const pools = edges.map(e => (chain.pools || []).find(p => p.available && p.poolId === e.pool_id && p.protocol === e.protocol && p.execution?.evmPreparation));
  return pools.every(Boolean) ? pools : [];
}
export function unsignedInteger(value, label) {
  if (value.length > 78 || !/^[0-9]+$/.test(value) || BigInt(value) >= (1n << 256n)) throw new Error(`${label}必须是非负整数`);
  return BigInt(value).toString();
}
export function evmFeeRequest(gas, maximum, priority, budget) {
  const gas_limit = String(boundedInteger(gas, 21000, 15000000, 'Gas上限'));
  const max_fee_per_gas = unsignedInteger(maximum, '最高Gas单价');
  const max_priority_fee_per_gas = unsignedInteger(priority, '优先费');
  const max_network_fee = unsignedInteger(budget, '网络费预算');
  if (BigInt(max_priority_fee_per_gas) > BigInt(max_fee_per_gas)) throw new Error('优先费不能大于最高Gas单价');
  if (BigInt(gas_limit) * BigInt(max_fee_per_gas) > BigInt(max_network_fee)) throw new Error('网络费预算小于Gas上限乘以最高单价');
  return {gas_limit, max_fee_per_gas, max_priority_fee_per_gas, max_network_fee};
}

export function evmPreparedSimulationAvailable(quote,chain) {
  const pools=evmPreparationPools(quote,chain);
  return pools.length>0 && pools.every(p=>p.execution?.evmIndependentLocalSimulation===true);
}
export function evmPreparedEvidenceMatches(quote,preparation,evidence,unsignedHash) {
  const kind=quote?.kind||'EXACT_IN';
  const component=kind==='EXACT_OUT'?preparation?.component?.preparation:preparation?.component;
  const result=evidence?.preparedExecution;
  return Boolean(quote && preparation?.ready && result && /^[0-9a-f]{64}$/.test(unsignedHash)
    && evidence.quoteKind===kind && evidence.quoteId===quote.quoteId && evidence.planHash===quote.planHash
    && evidence.quoteViewId===quote.quoteViewId && preparation.quoteViewId===quote.quoteViewId
    && evidence.apiPreparationHash===preparation.preparationHash
    && evidence.rpcPreparationHash===preparation.rpcPreparation?.rpc_preparation_hash
    && result.preparation_hash===component?.preparation_hash && result.unsigned_transaction_hash===unsignedHash
    && (kind!=='EXACT_OUT'||result.exact_out_preparation_hash===preparation.component?.exact_out_preparation_hash)
    && evidence.verification?.success===true && evidence.verified===false
    && evidence.authority==='PROVIDED_LOCAL_STATE' && evidence.fees?.chainFeeCompleteness==='LOCAL_EVM_ONLY'
    && result.simulation?.verified===false && result.simulation?.matches_plan===true
    && result.simulation?.actual_input_consumed===preparation.requiredInput
    && result.simulation?.actual_buy_amount===quote.grossBuyAmount);
}

// Display the evaluated transaction's economics without promoting the FAST quote.
export function nativeEconomicsText(observed, identity, receipt=false) {
  const c=observed?.costs;
  if (!c) return ''; // Older durable receipts remain displayable.
  const amount=v=>typeof v==='string'&&/^(0|[1-9][0-9]*)$/.test(v)&&v.length<=20&&BigInt(v)<(1n<<64n);
  const signed=v=>typeof v==='string'&&/^(0|-?[1-9][0-9]*)$/.test(v)&&v.length<=21&&BigInt(v)>-(1n<<64n)&&BigInt(v)<(1n<<64n);
  const allowed=receipt?['RPC_TRANSACTION_RECEIPT']:['PINNED_LOCAL_SBF_EXECUTION','RPC_CAPTURE_SBF_EXECUTION'];
  const valid=(receipt?observed?.verified===true:observed?.success===true)&&c.modelVersion==='xone-solana-native-economics/1'&&allowed.includes(c.sourceAuthority)
    &&c.scope==='EVALUATED_PREPARED_TRANSACTION_ONLY'&&c.amountBasis==='OBSERVED_ACTUAL_SETTLEMENT'
    &&c.nativeUnit==='LAMPORTS'&&c.chainKey===identity?.chainKey&&c.preparationHash===identity?.preparationHash
    &&c.transactionSha256===observed.transactionSha256&&(!receipt||c.transactionSha256===identity?.transactionSha256)&&c.historicalBankVerified===false
    &&c.independentProductionVerification===false&&c.quoteRankingApplied===false&&c.deductedFromQuoteOutput===false
    &&c.networkFeeIncludedInSwapAmounts===false&&c.depositTreatment==='CAPITAL_DEPOSIT_SEPARATE_FROM_NETWORK_FEE'
    &&amount(c.networkFeeNative)&&c.networkFeeNative===observed.feeLamports
    &&amount(c.retainedRentDepositNative)&&signed(c.actualPayerNativeDelta)&&c.actualPayerNativeDelta===observed.payerLamportDelta;
  if(!valid)return ' · 费用后金额不可用：费用证据与当前交易不一致';
  const native=c.tokenValuationStatus==='NATIVE_UNIT_IDENTITY';
  const input=c.netSellAmountEstimate,output=c.netBuyAmountEstimate;
  if ((input!==null&&!amount(input))||(output!==null&&!amount(output))
    ||(input!==null&&output!==null)||(!native&&(input!==null||output!==null)))return ' · 费用后金额不可用：金额或估值状态无效';
  if(native&&(c.nativeCashReconciliation!=='MATCHED'||c.valuationRatio?.numerator!=='1'||c.valuationRatio?.denominator!=='1'||c.valuationRatio?.method!=='NATIVE_UNIT_IDENTITY'))return ' · 费用后金额不可用：同单位估值证据无效';
  const basis=receipt?'RPC回执':'本地执行';
  const parts=[`${basis}网络费 ${c.networkFeeNative} lamports`];
  if(input!==null)parts.push(`费用后输入 ${input} lamports（SOL同单位）`);
  else if(output!==null)parts.push(`费用后收款 ${output} lamports（SOL同单位）`);
  else parts.push(c.netBuyUnavailableReason==='NETWORK_FEE_EXCEEDS_GROSS_OUTPUT'?'费用后收款不可用：网络费超过实际收款':'费用后金额不可用：缺少适用的同单位估值');
  parts.push(`留存租金押金 ${c.retainedRentDepositNative} lamports（资本另列）`);
  return ' · '+parts.join(' · ');
}

// Failure fees describe spent native funds; they never create swap settlement.
export function failedFeeText(receipt) {
  const f=receipt?.failureFee;
  if(!f) {
    if(receipt?.error?.code==='ONCHAIN_TRANSACTION_FAILED'&&receipt.state==='RECEIPT_PENDING')return ' · 失败交易网络费待核验';
    if(receipt?.error?.code==='ONCHAIN_TRANSACTION_FAILED'&&receipt.state==='SETTLEMENT_MISMATCH')return ' · 失败交易网络费不可用：费用或余额核验未通过';
    if(receipt?.state==='FAILED')return ' · 失败交易网络费不可用：尚无已核验费用记录';
    return '';
  }
  const amount=v=>typeof v==='string'&&/^(0|[1-9][0-9]*)$/.test(v)&&v.length<=20&&BigInt(v)<(1n<<64n);
  const hash=v=>typeof v==='string'&&/^[a-f0-9]{64}$/.test(v);
  const failure=f.onchainError?.InstructionError;
  const valid=['FAILED','RECEIPT_PENDING'].includes(receipt.state)&&receipt.confirmationStatus===(receipt.state==='FAILED'?'finalized':'confirmed')
    &&receipt.settlement===null&&receipt.error?.code==='ONCHAIN_TRANSACTION_FAILED'
    &&f.modelVersion==='xone-solana-failed-fee/1'&&f.scope==='FAILED_PREPARED_RECENT_BLOCKHASH_INSTRUCTION'&&f.sourceAuthority==='RPC_TRANSACTION_RECEIPT'
    &&typeof receipt.chainKey==='string'&&receipt.chainKey.startsWith('solana:')&&f.chainKey===receipt.chainKey
    &&hash(f.preparationHash)&&f.preparationHash===receipt.preparationHash&&hash(f.transactionSha256)&&f.transactionSha256===receipt.transactionSha256
    &&typeof f.signature==='string'&&f.signature.length>0&&f.signature===receipt.signature&&Number.isSafeInteger(f.slot)&&f.slot>=0&&f.slot===receipt.slot
    &&hash(f.transactionResultHash)&&hash(f.metaProjectionHash)&&f.nativeUnit==='LAMPORTS'
    &&Array.isArray(failure)&&failure.length===2&&Number.isInteger(failure[0])&&failure[0]>=0&&failure[0]<=255
    &&JSON.stringify(f.onchainError)===JSON.stringify(receipt.error.detail)
    &&f.observedNativeRollbackMatched===true&&f.observedTokenRollbackMatched===true&&f.retainedRentDepositNative==='0'
    &&f.settlementApplied===false&&f.netBuyAmountEstimate===null&&f.netSellAmountEstimate===null
    &&f.quoteRankingApplied===false&&f.historicalBankVerified===false&&f.independentProductionVerification===false
    &&amount(f.networkFeeNative)&&amount(f.payerNativeBefore)&&amount(f.payerNativeAfter)
    &&BigInt(f.payerNativeBefore)-BigInt(f.payerNativeAfter)===BigInt(f.networkFeeNative)&&f.payerLamportDelta===String(-BigInt(f.networkFeeNative));
  if(!valid)return ' · 失败交易网络费不可用：费用证据与当前失败回执不一致';
  return ` · RPC回执失败交易网络费 ${f.networkFeeNative} lamports（${receipt.state==='FAILED'?'已最终确认':'待最终确认'}） · 付款人SOL净变动 ${f.payerLamportDelta} lamports · 已报告余额回滚一致 · 留存租金押金 0 lamports · 失败收付不计为成交净额`;
}

// Amounts stay in native atoms throughout. Only the selected local execution has
// an opening balance requirement. Another candidate's balance is irrelevant.
export function quoteFundingRows(quote) {
  const costs = quote.costs;
  const funding = costs?.nativeFunding;
  if (!funding) return [];
  const ranking = quote.quality?.costRanking;
  const execution = ranking?.selectedExecution;
  const inconsistent = [['资金估算', '所选路线的费用证据不一致']];
  const binding = costs.bindingHash;
  if (!execution || !/^[0-9a-f]{64}$/.test(binding) || !quote.planHash ||
      quote.planHash !== ranking.selectedPlanHash || quote.planHash !== costs.selectedPlanHash ||
      execution.bindingHash !== binding || funding.bindingHash !== binding ||
      execution.verification?.success !== true || execution.authority !== 'PROVIDED_LOCAL_STATE' ||
      costs.chainKey !== quote.chainKey || costs.nativeUnit !== 'EVM_NATIVE_ATOMS' ||
      costs.chainFeeCompleteness !== 'LOCAL_EVM_ONLY' || execution.transaction?.from !== funding.payer) return inconsistent;

  const isAmount = value => typeof value === 'string' && /^(0|[1-9][0-9]{0,77})$/.test(value) && BigInt(value) < (1n << 256n);
  const principal = funding.wrappedPrincipalNative ?? '0';
  const fees = costs.payerNetNativeDebit;
  if (![principal, fees, funding.totalNetDebitNative, funding.minimumOpeningBalanceNative, funding.sourceBalanceNative].every(isAmount) ||
      BigInt(principal) + BigInt(fees) !== BigInt(funding.totalNetDebitNative) ||
      BigInt(funding.minimumOpeningBalanceNative) < BigInt(funding.totalNetDebitNative) ||
      BigInt(funding.sourceBalanceNative) < BigInt(funding.minimumOpeningBalanceNative)) return inconsistent;

  const rows = [['付款人网络费（本地仿真）', `${fees} wei`]];
  if (funding.wrappedPrincipalNative !== undefined) rows.push(['包装本金', `${principal} wei`]);
  rows.push(['执行所需最低起始余额', `${funding.minimumOpeningBalanceNative} wei`],
    ['付款人预计原币支出', `${funding.totalNetDebitNative} wei`],
    ['报价排序依据', ranking.costAdjustmentApplied ? '计入已折算的网络费' : '按毛额比较']);
  return rows;
}
