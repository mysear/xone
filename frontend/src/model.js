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
