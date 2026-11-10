#!/usr/bin/env node
'use strict';
// Execute the exact original signed swaps selected by the captured API rows.
// This finite single-hop reference does not build or execute arbitrary calldata.
const fs=require('node:fs'),assert=require('node:assert/strict');
const S=require('../reference/balancer-managed-v2-scheduled/inputs.cjs');
const {ROOT,V,sha}=S;
const eq=(a,b,m)=>assert.equal(V.canonical(a),V.canonical(b),m);
function nativeCase(bank,indexed){
 const branch=bank.branches[indexed.namespace],c=branch.cases.find(c=>c.id===indexed.atomic_case);
 assert(c,'original atomic case');
 const before=branch.states.find(s=>s.id===c.before),after=branch.states.find(s=>s.id===c.after);
 const tx=indexed.namespace==='two'?c.execution:branch.transactions.find(t=>t.id===c.transaction);
 assert(before&&after&&tx,'original case observations');
 return {branch,c,before,after,tx};
}
function validateRows(bank,rows){
 assert(Array.isArray(rows)&&rows.length>0&&rows.length<=128,'bounded nonempty selected API rows');
 return rows.map(row=>{
  const indexed=bank.cases.find(c=>c.id===row.case);assert(indexed,'original indexed case');
  const {branch,c,before,tx}=nativeCase(bank,indexed),r=row.request,q=row.quote;
  assert(r&&q,'complete API request and quote');
  const pool=indexed.namespace==='two'?branch.pool:branch.configuration.pool;
  const roles=indexed.namespace==='two'?branch.roles:branch.configuration.roles;
  eq([r.chainKey,r.kind,r.verification,r.sellToken,r.buyToken,r.sellAmount,r.caller,r.recipient],
     ['evm:31337','EXACT_IN','FAST',c.token_in,c.token_out,c.amount_in,tx.transaction.from,roles.trader],
     'exact original request caller/recipient/token/amount');
  assert.equal(q.verification.level,'INDICATIVE','local admission');
  eq(q.snapshotRef.block,{chain_key:'evm:31337',number_or_slot:Number(BigInt(before.header.number)),hash:before.header.hash,
    parent_hash:before.header.parentHash,timestamp_ms:Number(BigInt(before.header.timestamp))*1000},'exact selected source H');
  eq([q.consumedIn,q.grossBuyAmount,q.refund],[c.amount_in,indexed.expected_raw_out,'0'],'selected amount conservation');
  assert.equal(q.routes.length,1,'selected unsplit plan only');
  const route=q.routes[0],p=route.path;
  eq([route.amount_in,route.amount_out,p.amount_in,p.amount_out],
     [c.amount_in,indexed.expected_raw_out,c.amount_in,indexed.expected_raw_out],'selected path amounts');
  eq(p.tokens,[c.token_in,c.token_out],'selected tokens');assert.equal(p.edges.length,1,'single-hop physical plan');
  const e=p.edges[0];eq([e.pool_id,e.protocol,e.from_token,e.to],
     [pool,'balancer-v2-managed-scheduled',c.token_in,c.token_out],'selected physical protocol');
  assert.equal(tx.method,'swap','actual original Vault entry');
  const swap=tx.args[0],funds=tx.args[1];
  const selected=Array.isArray(swap)?[swap[2],swap[3],swap[4]]:[swap.assetIn,swap.assetOut,swap.amount];
  eq(selected,[c.token_in,c.token_out,c.amount_in],'original signed Swap args');
  const identity=Array.isArray(funds)?[funds[0],funds[2],funds[1],funds[3]]:
    [funds.sender,funds.recipient,funds.fromInternalBalance,funds.toInternalBalance];
  eq(identity,[r.caller,r.recipient,false,false],'original signed Vault funds');
  return {case:row.case,actor:row.actor,namespace:indexed.namespace,atomic_case:c.id,transaction_hash:tx.transaction.hash,
    source_hash:before.header.hash,quoted_amount_in:q.consumedIn,quoted_amount_out:q.grossBuyAmount};
 });
}
async function main(){
 const result={schema_version:1,passed:false,vm_started:false,verified:false,production_ready:false,
  all_requested_features_complete:false,authority:'fresh-original-VM-exact-selected-signed-swap-replay',routes:[],
  limitation:'Bounded original one-pool signed Vault swaps at January 2026 clocks; no public executor, independent chain trust, gas model, cross-pool execution or arbitrary calldata support.'};
 try{
  const inputBytes=fs.readFileSync(0),input=JSON.parse(inputBytes),file=ROOT+'/integration/test/contracts/managed-v2-scheduled.json',bytes=fs.readFileSync(file);
  assert.equal(input.schema_version,1);assert.equal(input.original_raw_sha256,sha(bytes),'original raw input identity');
  result.selected_api_input_sha256=sha(inputBytes);result.original_raw_sha256=sha(bytes);
  const bank=JSON.parse(bytes);require('../reference/balancer-managed-v2-scheduled/checker.cjs').check(bank);
  const rows=validateRows(bank,input.quotes);
  // generate executes all actual original branches (including snapshot/revert
  // management), and checks real wallet differences/receipts/complete after
  // state. Bind the selected rows to those exact signed transaction arguments
  // before allowing any target execution.
  S.closure();result.vm_started=true;
  const fresh=await require('../reference/balancer-managed-v2-scheduled/generate.cjs').generate();
  assert(Buffer.from(JSON.stringify(fresh,null,2)+'\n').equals(bytes),'fresh original VM full raw byteexact');
  const freshRows=validateRows(fresh,input.quotes);eq(freshRows,rows,'selected plans survived independent VM replay');
  for(const row of rows){
   const indexed=fresh.cases.find(c=>c.id===row.case),{before,after,tx}=nativeCase(fresh,indexed);
   const tokenRows=s=>s.getters.filter(g=>g.method==='balanceOf');
   const trader=indexed.namespace==='two'?fresh.branches.two.roles.trader:fresh.branches[indexed.namespace].configuration.roles.trader;
   const read=(s,token)=>{const g=tokenRows(s).find(g=>g.to===token&&g.args.length===1&&g.args[0]===trader);assert(g,'real trader balance getter');return BigInt(g.decoded[0]);};
   const c=fresh.branches[indexed.namespace].cases.find(c=>c.id===indexed.atomic_case);
   const actualIn=String(read(before,c.token_in)-read(after,c.token_in)),actualOut=String(read(after,c.token_out)-read(before,c.token_out));
   eq([actualIn,actualOut],[row.quoted_amount_in,row.quoted_amount_out],'actual selected wallet debit/credit');
   result.routes.push({...row,actual_amount_in:actualIn,actual_amount_out:actualOut,
     before_getters_checked:before.getters.length,after_getters_checked:after.getters.length,
     complete_original_poststate_matches:true,actual_receipt_sha256:sha(V.canonical(tx.receipt))});
  }
  assert(fs.readFileSync(file).equals(bytes),'original input changed');
  result.passed=true;result.selected_route_count=rows.length;result.distinct_original_cases=new Set(rows.map(r=>r.case)).size;
 }catch(error){result.error=String(error.stack||error);process.exitCode=1;}
 process.stdout.write(JSON.stringify(result)+'\n');
}
module.exports={validateRows,nativeCase,main};
if(require.main===module)main();
