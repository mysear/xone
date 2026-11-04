'use strict';
const assert=require('node:assert/strict');
const ROOT=require('node:path').resolve(__dirname,'../../../..');
const V=require(ROOT+'/integration/tools/reference/balancer-managed-v2-aum/validate.cjs');
const same=(a,b,m)=>assert.equal(V.canonical(a),V.canonical(b),m);
const state=s=>s.pool.multi_asset.managed_v2;
function sharedWallet(b){
 assert.equal(b.pools.length,2,'two physical pools per common H');
 const [a,z]=b.pools,va=a.pool.multi_asset,vz=z.pool.multi_asset;
 assert.notEqual(a.pool.pool_id,z.pool.pool_id,'distinct physical BPTs');assert.equal(va.vault,vz.vault,'same Vault');
 same(va.assets.map(x=>x.token),vz.assets.map(x=>x.token),'same underlying tokens');
 return va.assets.map((asset,i)=>{
  const x=state(a).token_info[i],y=state(z).token_info[i];
  assert.equal(x.vault_actual_balance,y.vault_actual_balance,'shared Vault wallet disagreement');
  const total=[x,y].reduce((n,t)=>n+BigInt(t.cash)+BigInt(t.managed),0n);
  assert(BigInt(x.vault_actual_balance)>=total,'global wallet covers combined registered cash and managed');
  return {token:asset.token,vault:va.vault,actual_balance:x.vault_actual_balance,registered_total:total.toString(),
   pools:b.pools.map((s,j)=>({pool:s.pool.pool_id,cash:[x,y][j].cash,managed:[x,y][j].managed}))};
 });
}
function deltaEvidence(before,after,selected){
 return sharedWallet(before).map((x,i)=>{const y=sharedWallet(after)[i];return {token:x.token,vault:x.vault,
  before:x.actual_balance,after:y.actual_balance,delta:(BigInt(y.actual_balance)-BigInt(x.actual_balance)).toString(),
  registered_deltas:before.pools.map((p,j)=>{const a=state(p).token_info[i],b=state(after.pools[j]).token_info[i];return {
   pool:p.pool.pool_id,selected:p.scenario===selected,cash_delta:(BigInt(b.cash)-BigInt(a.cash)).toString(),managed_delta:(BigInt(b.managed)-BigInt(a.managed)).toString(),
   observed_global_wallet_delta:(BigInt(b.vault_actual_balance)-BigInt(a.vault_actual_balance)).toString()};})};});
}
function stableAccounting(sn){
 const p=structuredClone(sn.pool),m=p.multi_asset.managed_v2;
 for(const k of ['block_number','block_hash','timestamp'])delete m[k];
 for(const t of m.token_info)delete t.vault_actual_balance;
 return p;
}
function validateShared(c,abis){
 assert.equal(c.profile,'balancer-v2-managed-aum-shared-vault-native');
 same(c.scenarios.map(s=>s.name),['PoolA','PoolB']);
 const [sa,sb]=c.scenarios;assert.notEqual(sa.pool,sb.pool);assert.notEqual(sa.pool_id,sb.pool_id);
 same(sa.tokens,sb.tokens,'original shared tokens');
 same(sb.configuration.settings.normalizedWeights,['500000000000000000','500000000000000000']);
 assert.equal(sb.configuration.owner,sa.configuration.owner,'original manager key2');
 const swaps=c.cases.filter(x=>x.operation==='swap');same(swaps.map(x=>x.scenario),['PoolA','PoolB','PoolA','PoolB']);
 assert.equal(c.cases.filter(x=>x.operation==='collect').length,c.generator.collect_30d?1:0);
 for(const b of Object.values(c.banks)){
  same(b.shared_vault_wallet,sharedWallet(b),'global Vault wallet summary from both physical pools');
  same(b.bpt_ledgers,b.pools.map(s=>{const m=state(s);return {pool:s.pool.pool_id,pool_id:s.pool.multi_asset.pool_id,manager:m.manager,
   manager_bpt_balance:m.manager_bpt_balance,protocol_collector:m.protocol_collector,protocol_collector_bpt_balance:m.protocol_collector_bpt_balance,
   vault_bpt_actual_balance:m.vault_bpt_actual_balance,total_supply:m.total_supply,virtual_supply:m.virtual_supply,actual_supply:m.actual_supply,pending_aum:m.pending_aum};}),'BPT ledgers keyed by physical pool');
 }
 for(const x of c.cases){
  const src=c.banks[x.source_bank2pools],dst=c.banks[x.expected_after_bank2pools];
  same(x.shared_vault_balance_deltas,deltaEvidence(src,dst,x.scenario),'actual full Vault wallet delta evidence');
  const selected=sa.name===x.scenario?0:1,other=1-selected;
  same(stableAccounting(src.pools[other]),stableAccounting(dst.pools[other]),'nonselected pool registered balances/weights/fees/BPT unchanged');
  const a=state(src.pools[selected]),b=state(dst.pools[selected]);
  if(x.operation==='swap'){
   const args=abis.get(x.execution.transaction.to).decodeFunctionData('swap',x.execution.transaction.input),single=args[0];
   assert.equal(single.amount.toString(),'100000000000000000000');assert.equal(single.kind,0);
   assert.equal(single.poolId.toLowerCase(),c.scenarios[selected].pool_id);
   const input=single.assetIn.toLowerCase(),output=single.assetOut.toLowerCase();
   for(const d of x.shared_vault_balance_deltas){
    const expected=d.token===input?BigInt(x.expected_native.amount_in):-BigInt(x.expected_native.amount_out);
    assert(d.token===input||d.token===output);assert.equal(BigInt(d.delta),expected,'full Vault raw token delta = native swap');
    for(const r of d.registered_deltas){assert.equal(r.managed_delta,'0');assert.equal(BigInt(r.cash_delta),r.selected?expected:0n);assert.equal(r.observed_global_wallet_delta,d.delta,'both pools observe the actual shared wallet change');}
   }
   for(const k of ['total_supply','virtual_supply','actual_supply','pending_aum','manager_bpt_balance','protocol_collector_bpt_balance','vault_bpt_cash','vault_bpt_managed','vault_bpt_actual_balance','last_collection_timestamp'])assert.equal(a[k],b[k],'swap stable BPT/AUM '+k);
   same(src.pools[selected].pool.multi_asset.weights,dst.pools[selected].pool.multi_asset.weights);
  } else {
   assert.equal(x.scenario,'PoolA');for(const d of x.shared_vault_balance_deltas){assert.equal(d.delta,'0');for(const r of d.registered_deltas){assert.equal(r.cash_delta,'0');assert.equal(r.managed_delta,'0');}}
   const events=x.execution.decoded_events.filter(e=>e.address===sa.pool&&e.name==='Transfer'&&e.args.from==='0x0000000000000000000000000000000000000000');
   assert.equal(events.reduce((n,e)=>n+BigInt(e.args.value),0n).toString(),x.expected_native.minted_bpt,'actual BPT mint transfer events');
  }
 }
 return true;
}
module.exports={sharedWallet,deltaEvidence,validateShared};
