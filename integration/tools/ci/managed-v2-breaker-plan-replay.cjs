#!/usr/bin/env node
'use strict';
// Execute the selected API's finite sequential Vault plan in original bytecode.
const fs=require('node:fs'),assert=require('node:assert/strict');
const I=require('../reference/balancer-managed-v2-breaker/inputs.cjs'),{ROOT,V,sha,E,A}=I,{ethers}=E.rr('ethers');
const {BreakerRuntime:Runtime,eq,num}=require('../reference/balancer-managed-v2-breaker/runtime.cjs');
const C=require('../reference/balancer-managed-v2-breaker/checker.cjs'),Bpt=require('../reference/balancer-managed-v2-bpt/checker.cjs'),Shared=require('../reference/balancer-managed-v2-scheduled-shared/checker.cjs');
const Native=require('../reference/balancer-managed-v2-scheduled/checker.cjs');
const MAX=(1n<<256n)-1n;
function decimal(s,label,zero=false){assert(typeof s==='string'&&/^(0|[1-9][0-9]*)$/.test(s),'canonical '+label);const n=BigInt(s);assert(n<=MAX&&(zero||n>0n),'uint256 '+label);return n;}
function block(h){return {chain_key:'evm:31337',number_or_slot:num(h.number),hash:h.hash,parent_hash:h.parentHash,timestamp_ms:num(h.timestamp)*1000};}
function wallet(b,token,who){const row=b.bpt_wallets.find(w=>w.token===token)||b.global_wallets.find(w=>w.token===token);assert(row&&Object.hasOwn(row.balances,who),'original actual holder');return BigInt(row.balances[who]);}
function validateInput(input,bank,digest){
 assert.equal(input.schema_version,1);assert.equal(input.original_breaker_sha256,digest,'original raw identity');assert(Array.isArray(input.quotes)&&input.quotes.length>0&&input.quotes.length<=1024,'bounded actual API inputs');assert.equal(new Set(input.quotes.map(q=>q.id)).size,input.quotes.length,'unique API rows');
 return input.quotes.map((row,index)=>{
  const branch=bank.branches[row.namespace],source=bank.banks.find(b=>b.id===row.source_bank);assert(branch&&source&&source.namespace===row.namespace,'original source namespace');
  const r=row.request,q=row.quote;assert(r&&q);eq([r.chainKey,r.kind,r.verification],['evm:31337','EXACT_IN','FAST']);assert(Object.values(branch.roles).includes(r.caller),'actual original funded actor');assert.equal(r.recipient,r.caller,'finite original caller/recipient scope');
  const sell=decimal(r.sellAmount,'sell'),gross=decimal(q.grossBuyAmount,'gross');assert(wallet(source,r.sellToken,r.caller)>=sell,'actual original source funds');
  assert(Number.isInteger(r.slippageBps)&&r.slippageBps>=0&&r.slippageBps<=10000);assert([1,2].includes(r.constraints?.maxHops));
  assert.equal(decimal(q.minBuyAmount,'minimum',true),gross*BigInt(10000-r.slippageBps)/10000n,'integer min buy');eq(q.snapshotRef.block,block(source.header),'exact original source H');assert.equal(q.onlineGeneration,q.snapshotRef.online_generation);assert.equal(q.scopeHash,q.snapshotRef.scope_hash);
  assert.equal(q.verification.level,'INDICATIVE');assert.equal(q.verification.success,false);assert.equal(q.consumedIn,r.sellAmount);assert.equal(q.refund,'0');
  eq([...q.quality.candidateScope].sort(),branch.pools.map(p=>p.pool).sort(),'finite complete physical scope');eq(q.quality.excludedPools,{});assert(Array.isArray(q.routes)&&q.routes.length>=1&&q.routes.length<=2);
  let sumIn=0n,sumOut=0n;
  for(const route of q.routes){
   const amount=decimal(route.amount_in,'route input'),out=decimal(route.amount_out,'route output'),p=route.path;assert(p&&Array.isArray(p.edges)&&p.edges.length>=1&&p.edges.length<=r.constraints.maxHops);assert.equal(new Set(p.edges.map(e=>e.pool_id)).size,p.edges.length,'unique physical pool per searched path');
   assert.equal(p.amount_in,route.amount_in);assert.equal(p.amount_out,route.amount_out);assert.equal(p.tokens.length,p.edges.length+1);assert.equal(p.tokens[0],r.sellToken);assert.equal(p.tokens.at(-1),r.buyToken);
   for(const [hop,e]of p.edges.entries()){
    const pool=branch.pools.find(p=>p.pool===e.pool_id);assert(pool,'selected original physical pool');assert.equal(e.protocol,'balancer-v2-managed-breaker');eq([e.from_token,e.to],[p.tokens[hop],p.tokens[hop+1]]);
    const tokens=new Set([...branch.tokens,pool.pool]);assert(tokens.has(e.from_token)&&tokens.has(e.to)&&e.from_token!==e.to,'registered token units');
   }
   sumIn+=amount;sumOut+=out;
  }
  assert.equal(sumIn,sell);assert.equal(sumOut,gross);
  return {row,index,branch,source,signature:V.canonical({namespace:row.namespace,source:source.id,request:r,routes:q.routes})};
 });
}
function sourceEqual(a,b){for(const k of ['header','header_anchor_after','pools','global_wallets','global_wallet_getters','accounts','storage','bpt_wallets','actor_allowlists','control_storage'])eq(a[k],b[k],'exact source restore '+k);}
async function executeMode(items,result){
 const branch=items[0].branch,namespace=branch.mode,r=new Runtime(namespace),h=r.h;
 try{
  await h.replay();h.mode=namespace;result.vm_namespaces_started.push(namespace);result.prefix_transactions+=28;
  h.pools=structuredClone(branch.pools);h.tokenAddresses=[...branch.tokens];h.decimals=[...branch.decimals];h.newIDs=structuredClone(branch.new_identities);r.slots={...branch.control_mapping_slots};
  const ids=[...branch.prefix_identities,...branch.new_identities],abis=new Map();
  for(const id of ids){const artifact=A.locate(h.builds,id.artifact.build,id.artifact.name);abis.set(id.address,new ethers.utils.Interface(artifact.c.abi));h.contracts.set(id.address,new ethers.Contract(id.address,artifact.c.abi,h.wallets[0]));}
  h.tokens=branch.tokens.map(t=>h.contracts.get(t));
  const headers=new Map(),keep=x=>headers.set(num(x.number),x);for(const x of branch.prefix_replay.headers)keep(x);for(const t of branch.transactions){keep(t.pre_header);keep(t.execution_header);}for(const x of branch.clock_operations){keep(x.pre_header);keep(x.header);}
  let txIndex=0,height=28;
  async function restore(source){
   const target=num(source.header.number);assert(target>=height,'source replay monotonic');
   while(height<target){const next=branch.transactions[txIndex],expected=headers.get(height+1);assert(expected,'original next header');await h.rpc('evm_setTime',[num(expected.timestamp)*1000]);
    if(next&&num(next.execution_header.number)===height+1){eq(await h.header(),next.pre_header);assert.equal(await h.rpc('eth_sendRawTransaction',[next.signed_transaction]),next.transaction.hash);eq(await h.rpc('eth_getTransactionReceipt',[next.transaction.hash]),next.receipt);eq(await h.rpc('eth_getTransactionByHash',[next.transaction.hash]),next.transaction);txIndex++;result.source_transactions_replayed++;}
    else{eq(expected.transactions,[]);await h.rpc('evm_mine');}height++;eq(await h.header(),expected,'exact original header');
   }
   eq(await h.header(),source.header);for(const id of branch.new_identities){const code=await h.rpc('eth_getCode',[id.address,source.header.number]);assert.equal(code,branch.native_runtime[id.runtime_sha256]);h.runtime[id.runtime_sha256]=code;}
   const actual=await r.capture('selected-source:'+source.id);sourceEqual(actual,source);return actual;
  }
  const groups=new Map();for(const item of items){if(!groups.has(item.source.id))groups.set(item.source.id,[]);groups.get(item.source.id).push(item);}
  for(const group of [...groups.values()].sort((a,b)=>num(a[0].source.header.number)-num(b[0].source.header.number))){
   const source=group[0].source;let observed=await restore(source);const executed=new Map();
   for(const item of group){
    if(executed.has(item.signature)){result.quote_links.push({id:item.row.id,index:item.index,execution_id:executed.get(item.signature)});continue;}
    const snap=await h.rpc('evm_snapshot'),e={id:'E'+result.executions.length,namespace,source_bank:source.id,source:observed,request:item.row.request,quote:item.row.quote,routes:[],source_restored:false};result.executions.push(e);
    try{
     const req=item.row.request,signer=Object.values(branch.roles).indexOf(req.caller);assert(signer>=0);let current=observed;
     for(const [routeIndex,route]of item.row.quote.routes.entries()){
      let amount=route.amount_in;const rr={index:routeIndex,steps:[],amount_in:amount};
      for(const [hop,edge]of route.path.edges.entries()){
       const pool=branch.pools.find(p=>p.pool===edge.pool_id),before=current;
       const c=await r.swap('actual-API:'+e.id+':'+routeIndex+':'+hop,pool.name,edge.from_token,edge.to,amount,signer,1);current=r.current;
       const tx=h.transactions.find(t=>t.id===c.transaction);assert(tx);Native.transaction(tx,abis);Bpt.validateBoundBank(current,branch,source,abis);C.validateCircuitMath(current,branch);Bpt.validateSwap(c,branch,new Map([[before.id,before],[current.id,current]]),new Map([[tx.id,tx]]),abis);
       rr.steps.push({index:hop,pool_name:pool.name,token_in:edge.from_token,token_out:edge.to,amount_in:amount,amount_out:c.expected_native.actual_amount_out,before,after:current,transaction:tx});amount=c.expected_native.actual_amount_out;result.selected_transactions_executed++;
      }
      assert.equal(amount,route.amount_out,'actual route wallet credit matches selected API amount');rr.amount_out=amount;e.routes.push(rr);
     }
     e.actual_amount_in=String(wallet(observed,req.sellToken,req.caller)-wallet(current,req.sellToken,req.caller));e.actual_amount_out=String(wallet(current,req.buyToken,req.recipient)-wallet(observed,req.buyToken,req.recipient));assert.equal(e.actual_amount_in,req.sellAmount);assert.equal(e.actual_amount_out,e.quote.grossBuyAmount);assert(BigInt(e.actual_amount_out)>=BigInt(e.quote.minBuyAmount));e.complete_native_after=current;e.passed=true;
    }finally{assert.equal(await h.rpc('evm_revert',[snap]),true);const restored=await r.capture('restored-source:'+source.id);sourceEqual(restored,source);observed=restored;e.source_restored=true;}
    executed.set(item.signature,e.id);result.quote_links.push({id:item.row.id,index:item.index,execution_id:e.id});
   }
  }
 }finally{await r.close();}
}
function validateReport(report,bank,digest,input){
 const plans=validateInput(input,bank,digest);
 assert.equal(report.selected_input_sha256,sha(V.canonical(input)),'selected actual input commitment');
 assert.equal(report.schema_version,1);assert.equal(report.passed,true);assert.equal(report.original_breaker_sha256,digest);assert.equal(report.verified,false);assert.equal(report.production_ready,false);assert.equal(report.all_requested_features_complete,false);
 assert(Array.isArray(report.executions)&&report.executions.length>0&&report.executions.length<=1024);assert.equal(new Set(report.executions.map(e=>e.id)).size,report.executions.length);
 let transactions=0;
 for(const execution of report.executions){
  const seenTransactions=new Set();
  const source=bank.banks.find(b=>b.id===execution.source_bank),branch=bank.branches[execution.namespace];assert(source&&branch&&source.namespace===execution.namespace);
  validateInput({schema_version:1,original_breaker_sha256:digest,quotes:[{id:execution.id,namespace:execution.namespace,source_bank:execution.source_bank,request:execution.request,quote:execution.quote}]},bank,digest);
  assert.equal(execution.passed,true);assert.equal(execution.source_restored,true);sourceEqual(execution.source,source);
  const ids=[...branch.prefix_identities,...branch.new_identities],abis=new Map();for(const id of ids){const a=A.locate(A.load().builds,id.artifact.build,id.artifact.name);abis.set(id.address,new ethers.utils.Interface(a.c.abi));}
  let current=execution.source;assert.equal(execution.routes.length,execution.quote.routes.length);
  for(const [ri,route]of execution.routes.entries()){
   const selected=execution.quote.routes[ri];assert.equal(route.index,ri);assert.equal(route.amount_in,selected.amount_in);assert.equal(route.steps.length,selected.path.edges.length);let amount=selected.amount_in;
   for(const [hop,step]of route.steps.entries()){
    const edge=selected.path.edges[hop],pool=branch.pools.find(p=>p.pool===edge.pool_id),tx=step.transaction;assert(pool);eq(step.before,current,'complete native sequence');
    assert.equal(step.index,hop);assert.equal(step.pool_name,pool.name);eq([step.token_in,step.token_out],[edge.from_token,edge.to]);assert.equal(step.amount_in,amount);decimal(step.amount_out,'actual step output');
    Native.transaction(tx,abis);assert(!seenTransactions.has(tx.transaction.hash),'distinct selected signed transactions');seenTransactions.add(tx.transaction.hash);eq(tx.pre_header,step.before.header);eq(tx.execution_header,step.after.header);assert.equal(tx.execution_header.timestamp,source.header.timestamp);
    Bpt.validateBoundBank(step.after,branch,source,abis);C.validateCircuitMath(step.after,branch);
    const operation=edge.to===pool.pool?'bpt_join':edge.from_token===pool.pool?'bpt_exit':'token_swap';
    const c={before:step.before.id,after:step.after.id,pool_name:pool.name,transaction:tx.id,actor:execution.request.caller,operation,token_in:edge.from_token,token_out:edge.to,amount_in:amount,expected_native:{status:1,actual_amount_in:amount,actual_amount_out:step.amount_out}};
    const old=step.before.pools.find(p=>p.name===pool.name).state.aum,next=step.after.pools.find(p=>p.name===pool.name).state.aum;
    Object.assign(c.expected_native,{aum_minted_bpt:String(BigInt(next.total_supply)-BigInt(old.total_supply)),manager_bpt_delta:String(BigInt(next.manager_bpt)-BigInt(old.manager_bpt)),collector_bpt_delta:String(BigInt(next.collector_bpt)-BigInt(old.collector_bpt)),swap_event:{pool_id:pool.pool_id,token_in:edge.from_token,token_out:edge.to,amount_in:amount,amount_out:step.amount_out}});
    Bpt.validateSwap(c,branch,new Map([[step.before.id,step.before],[step.after.id,step.after]]),new Map([[tx.id,tx]]),abis);
    amount=step.amount_out;current=step.after;transactions++;
   }
   assert.equal(amount,route.amount_out);assert.equal(amount,selected.amount_out);
  }
  eq(execution.complete_native_after,current);
  const r=execution.request,actualIn=String(wallet(source,r.sellToken,r.caller)-wallet(current,r.sellToken,r.caller)),actualOut=String(wallet(current,r.buyToken,r.recipient)-wallet(source,r.buyToken,r.recipient));
  assert.equal(actualIn,r.sellAmount);assert.equal(actualIn,execution.actual_amount_in);assert.equal(actualOut,execution.quote.grossBuyAmount);assert.equal(actualOut,execution.actual_amount_out);
 }
 assert.equal(report.selected_transactions_executed,transactions);assert(Array.isArray(report.quote_links));assert.equal(report.quote_links.length,plans.length,'every actual API input consumed exactly once');
 const linked=new Set(),executions=new Map(report.executions.map(e=>[e.id,e]));
 for(const link of report.quote_links){
  assert(Number.isInteger(link.index)&&link.index>=0&&link.index<plans.length&&!linked.has(link.index),'exact unique input index');linked.add(link.index);
  const plan=plans[link.index],e=executions.get(link.execution_id);assert(e,'linked native execution exists');assert.equal(link.id,plan.row.id,'actual input ID');
  eq([e.namespace,e.source_bank,e.request,e.quote.routes],[plan.row.namespace,plan.row.source_bank,plan.row.request,plan.row.quote.routes],'exact input execution signature');
  // Same source/request/routes may deduplicate across replicas or generations.
  // Each actual response remains in the independently supplied input commitment;
  // the execution must correspond to one complete real response in that group.
  assert(plans.some(p=>p.row.namespace===e.namespace&&p.row.source_bank===e.source_bank&&V.canonical(p.row.request)===V.canonical(e.request)&&V.canonical(p.row.quote)===V.canonical(e.quote)),'execution quote is a complete actual input response');
 }
 const links=new Set(report.quote_links.map(q=>q.execution_id));assert(report.executions.every(e=>links.has(e.id)));
 return {passed:true,executions:report.executions.length,selected_transactions:transactions};
}
async function execute(input){
 const path=ROOT+'/integration/test/contracts/managed-v2-breaker.json',bytes=fs.readFileSync(path),bank=JSON.parse(bytes);C.check(bank);const plans=validateInput(input,bank,sha(bytes));
 const result={schema_version:1,passed:false,authority:'independent-original-Vault-bytecode-execution-of-actual-selected-breaker-API-plans',original_breaker_sha256:sha(bytes),selected_input_sha256:sha(V.canonical(input)),vm_namespaces_started:[],prefix_transactions:0,source_transactions_replayed:0,selected_transactions_executed:0,executions:[],quote_links:[],verified:false,production_ready:false,all_requested_features_complete:false,atomicity:'sequential signed local Vault transactions; no public atomic executor proof'};
 for(const name of Object.keys(bank.branches)){const items=plans.filter(p=>p.row.namespace===name);if(items.length)await executeMode(items,result);}
 assert.equal(result.quote_links.length,plans.length);assert(result.executions.every(e=>e.passed&&e.source_restored));assert(fs.readFileSync(path).equals(bytes));result.passed=true;validateReport(result,bank,sha(bytes),input);return result;
}
async function main(){let result;try{const bytes=fs.readFileSync(0);assert(bytes.length<=8*1024*1024);result=await execute(JSON.parse(bytes));}catch(e){result={schema_version:1,passed:false,verified:false,production_ready:false,all_requested_features_complete:false,error:String(e.stack||e)};process.exitCode=1;}process.stdout.write(JSON.stringify(result)+'\n');}
module.exports={validateInput,validateReport,execute,main};if(require.main===module)main();
