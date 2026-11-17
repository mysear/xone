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
