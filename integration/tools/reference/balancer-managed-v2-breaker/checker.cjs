'use strict';
// Source, ABI and integer-layout qualification. Swap expectations are checked
// against actual original wallets/events; local quote math is never the oracle.
const assert=require('node:assert/strict'),I=require('./inputs.cjs'),{eq}=require('./runtime.cjs');
const {ethers}=I.E.rr('ethers'),Bpt=require('../balancer-managed-v2-bpt/checker.cjs');
const Shared=require('../balancer-managed-v2-scheduled-shared/checker.cjs'),Native=require('../balancer-managed-v2-scheduled/checker.cjs');
const ONE=10n**18n,MAX=(1n<<256n)-1n;
function get(s,to,method,args=[]){const g=s.getters.filter(g=>g.to===to&&g.method===method&&I.V.canonical(g.args)===I.V.canonical(args));assert(g.length,'required original getter '+method);assert(g.every(x=>x.result===g[0].result));return g[0].decoded;}
function mul(a,b){const n=a*b;assert(n<=MAX,'original uint256 intermediate');return n;}
function fixed(a,b,up){const n=mul(a,b);return up&&n!==0n?(n-1n)/ONE+1n:n/ONE;}
function validateCircuitMath(bank,b){
 const library=b.prefix_identities.find(i=>i.label==='CircuitBreakerLib').address,used=[];
 for(const p of b.pools){const s=bank.pools.find(row=>row.name===p.name).state;
  for(let index=0;index<b.tokens.length;index++){
   const token=b.tokens[index],row=bank.control_storage.find(r=>r.address===p.pool&&r.kind==='circuit'&&r.token===token);assert(row);
   const word=BigInt(row.value==='0x'?'0x0':row.value),price=(word>>96n)&((1n<<96n)-1n),reference=(word>>32n)&((1n<<64n)-1n),weight=BigInt(s.weights[index]),scaling=BigInt(s.scales[index]);
   const getter=get(s,p.pool,'getCircuitBreakerState',[token]);
   for(const lower of [true,false]){
    const bound=((word>>(lower?0n:16n))&65535n)<<48n;let expected=0n;
    if(bound!==0n){let ratio;
     if(reference===weight){const compressed=(word>>(lower?192n:224n))&0xffffffffn;ratio=mul(compressed,10n*ONE)/0xffffffffn;}
     else{const rows=bank.circuit_math_calls.filter(r=>r.pool_name===p.name&&r.token===token&&r.is_lower===lower);assert.equal(rows.length,1,'exact original cache-miss pure-call occurrence');const r=rows[0],g=s.getters[r.getter_ordinal];assert(g&&g.to===library&&g.method==='calcAdjustedBound');eq(g.args,[String(bound),String(weight),lower]);assert.equal(r.ratio,g.decoded[0]);ratio=BigInt(r.ratio);used.push(r);}
     expected=fixed(fixed(price,ratio,lower),scaling,false);
    }
    assert.equal(getter[lower?4:5],String(expected),'original complete bound getter follows stored cache or original pure library');
   }
  }
 }
 eq(used,bank.circuit_math_calls,'no unowned pure-call evidence');
}
function nativeControlAccounts(before,after,t){
 const from=t.transaction.from,miner=t.execution_header.miner,fee=BigInt(t.receipt.gasUsed)*BigInt(t.receipt.effectiveGasPrice),tip=BigInt(t.receipt.gasUsed)*(BigInt(t.receipt.effectiveGasPrice)-BigInt(t.execution_header.baseFeePerGas));
 const sender=before.accounts.find(a=>a.address===from);assert(sender);assert.equal(BigInt(sender.nonce),BigInt(t.transaction.nonce),'signed control nonce equals original before account');
 for(const a of before.accounts){const z=after.accounts.find(x=>x.address===a.address);assert(z);assert.equal(z.code_sha256,a.code_sha256);assert.equal(BigInt(z.nonce),BigInt(a.nonce)+(a.address===from?1n:0n),'every control account nonce transition');const delta=(a.address===from?-fee:0n)+(a.address===miner?tip:0n);assert.equal(BigInt(z.native_balance)-BigInt(a.native_balance),delta,'every control native gas/coinbase ledger');}
}
function checkObservations(raw){
assert.equal(raw.schema_version,1);assert.equal(raw.authority,'independent-local-original-managed-v2-breaker');assert.equal(raw.production_ready,false);assert.equal(raw.all_requested_features_complete,false);
 eq(Object.keys(raw.branches),['cached','dynamic','boundaries','shared']);I.originalBpt();
 const namespaces=new Set(Object.keys(raw.branches));for(const name of ['banks','cases','controls','checkpoints','orders']){assert(Array.isArray(raw[name]),'complete '+name+' array');for(const row of raw[name])assert(namespaces.has(row.namespace),'no orphan '+name+' namespace');}
 assert.equal(new Set(raw.cases.map(c=>c.id)).size,raw.cases.length,'unique case IDs');assert.equal(new Set(raw.checkpoints.map(c=>c.id)).size,raw.checkpoints.length,'unique checkpoint IDs');assert.equal(new Set(raw.orders.map(o=>o.id)).size,raw.orders.length,'unique order IDs');
 const original=I.originalBank(),builds=I.A.load().builds;eq(raw.original_reference.source_manifest,I.sourceManifest());eq(raw.original_reference.compiler_jobs,original.reference.compiler_jobs);eq(raw.original_reference.source_pins,I.A.load().pins);assert.equal(raw.original_reference.source_pins_sha256,original.reference.source_pins_sha256);eq(raw.original_reference.provenance,require('../balancer-managed-v2-scheduled/generate.cjs').provenance());
 const banks=new Map(raw.banks.map(b=>[b.id,b]));assert.equal(banks.size,raw.banks.length);
 for(const [namespace,b]of Object.entries(raw.branches)){
  require('./scenarios.cjs').validate(raw,namespace,eq);
  assert.equal(b.mode,namespace);assert.equal(b.namespace,namespace);eq(b.chain_options,original.reference.chain_options);assert.equal(b.chain_options.chain.allowUnlimitedContractSize,false);assert.equal(b.chain_options.chain.allowUnlimitedInitCodeSize,false);
  const rows=raw.banks.filter(b=>b.namespace===namespace);eq(b.bank_ids,rows.map(b=>b.id));assert(rows.length>1);
  const prefix=I.originalBpt().branches.settled;eq(b.prefix_identities,prefix.prefix_identities,'exact fixed prefix identity set');eq(b.prefix_replay,prefix.prefix_replay,'exact original signed prefix');eq(b.roles,prefix.roles,'fixed original funded roles');eq(b.decimals,[18,6,8]);eq(b.pools.map(p=>p.name),['A','B']);eq(b.new_identities.map(i=>i.address).sort(),[...b.tokens,...b.pools.map(p=>p.pool)].sort(),'no unowned new identities');
  const ids=[...b.prefix_identities,...b.new_identities],txs=new Map(b.transactions.map(t=>[t.id,t]));assert.equal(ids.length,new Set(ids.map(i=>i.address)).size);assert.equal(txs.size,b.transactions.length);
  const abis=new Map(ids.map(i=>[i.address,new ethers.utils.Interface(I.A.locate(builds,i.artifact.build,i.artifact.name).c.abi)]));
  for(const t of b.transactions)Native.transaction(t,abis);
  const authority={...raw,inputs:{original:raw.inputs.compiler}};
  for(const p of b.pools){assert.equal(p.requested_protocol,'balancer-v2-managed-breaker');const context=Shared.poolContext(authority,b,p,original);context.states=context.states.slice(0,1);Native.verifyBranch(context,namespace+'/'+p.name,original,builds);
   const init=txs.get(p.initialize);assert(init);const lock=init.receipt.logs.filter(l=>l.address===p.pool).flatMap(l=>{const e=abis.get(p.pool).parseLog(l);return e.name==='Transfer'&&e.args[0]===ethers.constants.AddressZero&&e.args[1]===ethers.constants.AddressZero?[BigInt(e.args[2].toString())]:[];}).reduce((s,n)=>s+n,0n);assert.equal(lock,1000000n);
  }
  for(const row of rows){Bpt.validateBoundBank(row,b,rows[0],abis);validateCircuitMath(row,b);}
  const cases=raw.cases.filter(c=>c.namespace===namespace);assert.equal(cases.length,new Set(cases.map(c=>c.transaction)).size);eq(cases.map(c=>c.transaction).sort(),b.transactions.filter(t=>t.method==='swap').map(t=>t.id).sort());
  for(const c of cases){const t=txs.get(c.transaction),before=banks.get(c.before);assert.equal(BigInt(before.accounts.find(a=>a.address===c.actor).nonce),BigInt(t.transaction.nonce),'signed swap nonce equals actual pre-account');Bpt.validateSwap(c,b,banks,txs,abis);if(c.expected_native.rejection_reason){assert.equal(require('./generate.cjs').rejection(txs.get(c.transaction)),c.expected_native.rejection_reason,'original breaker failure reason');const before=banks.get(c.before),row=before.bpt_wallets.find(w=>w.token===c.token_in)||before.global_wallets.find(w=>w.token===c.token_in);assert(row&&BigInt(row.balances[c.actor])>=BigInt(c.amount_in),'breaker rejection must have original input funds');}}
  const controls=raw.controls.filter(c=>c.namespace===namespace);for(const c of controls){const t=txs.get(c.transaction),before=banks.get(c.before),after=banks.get(c.after),p=b.pools.find(p=>p.name===c.pool_name);assert(t&&before&&after&&p);assert.equal(t.transaction.to,p.pool);assert.equal(t.method,c.method);assert.equal(t.transaction.from,c.actor||b.roles.manager);assert.equal(Number(BigInt(t.receipt.status)),c.expected_native?.status??1);eq(t.args,c.args);eq(t.pre_header,before.header);eq(t.execution_header,after.header);nativeControlAccounts(before,after,t);eq(before.global_wallets,after.global_wallets,'manager control preserves underlying wallets');eq(before.bpt_wallets,after.bpt_wallets,'weight/fee/circuit controls preserve BPT ledgers');if(c.expected_native?.status===0){assert.equal(t.receipt.status,'0x0');eq(before.storage,after.storage);eq(before.global_wallets,after.global_wallets);eq(before.bpt_wallets,after.bpt_wallets);}}
  const initialHeight=BigInt(rows[0].header.number),postInitialization=b.transactions.filter(t=>BigInt(t.execution_header.number)>initialHeight);eq([...cases,...controls].map(c=>c.transaction).sort(),postInitialization.map(t=>t.id).sort(),'every post-initialization signed transaction owns exactly one bank transition');
  const changes=new Map([...cases,...controls].map(c=>[c.after,c]));assert.equal(changes.size,cases.length+controls.length);const clocks=new Map(b.clock_operations.map(c=>[c.header.hash,c]));assert.equal(clocks.size,b.clock_operations.length,'unique original clock headers');assert.equal(new Set(b.clock_operations.map(c=>BigInt(c.header.number).toString())).size,b.clock_operations.length,'unique original clock heights');let changesSeen=0,clockSeen=0;
  for(let i=1;i<rows.length;i++){const before=rows[i-1],after=rows[i];assert.equal(after.header.parentHash,before.header.hash);assert.equal(BigInt(after.header.number),BigInt(before.header.number)+1n);if(changes.has(after.id)){assert.equal(changes.get(after.id).before,before.id);changesSeen++;}else{const c=clocks.get(after.header.hash);assert(c,'every bank transition owns an original transaction or clock');eq(c.pre_header,before.header);eq(c.header,after.header);eq(before.storage,after.storage);eq(before.accounts,after.accounts,'clock-only preserves original account/nonce/native balance');eq(before.global_wallets,after.global_wallets);eq(before.bpt_wallets,after.bpt_wallets);clockSeen++;}}
  assert.equal(changesSeen,changes.size);assert.equal(clockSeen,clocks.size);
 }
 const casesById=new Map(raw.cases.map(c=>[c.id,c]));
 for(const [namespace,b]of Object.entries(raw.branches)){
  const rows=raw.banks.filter(x=>x.namespace===namespace),clocks=new Map(b.clock_operations.map(c=>[c.header.hash,c]));
  const points=raw.checkpoints.filter(c=>c.namespace===namespace);assert.equal(points.length,clocks.size,'every original clock is a named checkpoint');assert.equal(new Set(points.map(c=>c.bank)).size,points.length);
  for(const point of points){const bank=banks.get(point.bank);assert(bank&&bank.namespace===namespace);const clock=clocks.get(bank.header.hash);assert(clock);assert.equal(point.timestamp,clock.timestamp);assert.equal(point.label,clock.label);}
 }
 assert.equal(raw.orders.length,1,'explicit complete shared input witness inventory');
 for(const o of raw.orders){
  assert.equal(o.namespace,'shared');const b=raw.branches[o.namespace],source=banks.get(o.source_bank),after=banks.get(o.after);assert(source&&after&&source.namespace===o.namespace&&after.namespace===o.namespace);
  assert.equal(o.caller,b.roles.lp);assert.equal(o.recipient,o.caller);assert.equal(o.token_in,b.pools[0].pool);assert.equal(o.token_out,b.pools[1].pool);
  assert.equal(o.reference_routes.length,2);let sumIn=0n,sumOut=0n,current=source.id;const used=new Set();
  for(const [index,route]of o.reference_routes.entries()){
   assert.equal(route.index,index);assert.equal(route.cases.length,2);const first=casesById.get(route.cases[0]),last=casesById.get(route.cases[1]);assert(first&&last&&!used.has(first.id)&&!used.has(last.id));used.add(first.id);used.add(last.id);
   eq([first.namespace,last.namespace],[o.namespace,o.namespace]);eq([first.pool_name,last.pool_name],['A','B']);assert.equal(first.before,current);assert.equal(last.before,first.after);assert.equal(first.actor,o.caller);assert.equal(last.actor,o.caller);
   eq([first.token_in,first.token_out,last.token_in,last.token_out],[o.token_in,route.token,route.token,o.token_out]);assert.equal(first.amount_in,route.amount_in);assert.equal(first.expected_native.status,1);assert.equal(last.expected_native.status,1);assert.equal(first.expected_native.actual_amount_out,last.amount_in);assert.equal(last.expected_native.actual_amount_out,route.actual_amount_out);
   assert(/^[1-9][0-9]*$/.test(route.amount_in));sumIn+=BigInt(route.amount_in);sumOut+=BigInt(route.actual_amount_out);current=last.after;
  }
  assert.equal(current,o.after);assert.equal(String(sumIn),o.amount_in);assert.equal(o.expected_native.actual_amount_in,o.amount_in);assert.equal(String(sumOut),o.expected_native.actual_amount_out);
  const wallet=(bank,token,who)=>BigInt(bank.bpt_wallets.find(w=>w.token===token).balances[who]);
  assert.equal(String(wallet(source,o.token_in,o.caller)-wallet(after,o.token_in,o.caller)),o.amount_in);assert.equal(String(wallet(after,o.token_out,o.recipient)-wallet(source,o.token_out,o.recipient)),o.expected_native.actual_amount_out);
 }
 return {passed:true,banks:raw.banks.length,cases:raw.cases.length};
}
function check(raw){eq(raw.inputs,I.closure(),'complete new source and inherited original closure');return checkObservations(raw);}
module.exports={check,checkObservations,validateCircuitMath};
