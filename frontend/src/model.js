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
