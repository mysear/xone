#!/usr/bin/env node
'use strict';
// Restore exact original common-H sources, execute the API's ordered split in
// original Vault/Pool/ERC20 bytecode, then revert and verify the source again.
const fs=require('node:fs'),assert=require('node:assert/strict');
const I=require('../reference/balancer-managed-v2-scheduled-shared/inputs.cjs');
const {ROOT,V,sha,E,A}=I,{ethers}=E.rr('ethers');
const {Harness,clean,eq,num}=require('../reference/balancer-managed-v2-scheduled-shared/harness.cjs');
const C=require('../reference/balancer-managed-v2-scheduled-shared/checker.cjs');
const Native=require('../reference/balancer-managed-v2-scheduled/checker.cjs');
const MAX=(1n<<256n)-1n;
function decimal(value,label,zero=false){assert(typeof value==='string'&&/^(0|[1-9][0-9]*)$/.test(value),'canonical '+label);const n=BigInt(value);assert(n<=MAX&&(zero||n>0n),'uint256 '+label);return n;}
function block(h){return {chain_key:'evm:31337',number_or_slot:num(h.number),hash:h.hash,parent_hash:h.parentHash,timestamp_ms:num(h.timestamp)*1000};}
function validateInput(input,bank,digest){
 assert.equal(input.schema_version,1);assert.equal(input.original_shared_sha256,digest,'qualified native source bytes');
 assert(Array.isArray(input.quotes)&&input.quotes.length>0&&input.quotes.length<=64,'bounded nonempty selected API plans');
 assert.equal(new Set(input.quotes.map(q=>q.id)).size,input.quotes.length,'unique API row IDs');
 return input.quotes.map((row,index)=>{
  const b=bank.branches[row.namespace],source=bank.banks.find(b=>b.id===row.source_bank);assert(b&&source&&source.namespace===row.namespace,'original source namespace/bank');
  const r=row.request,q=row.quote;assert(r&&q,'request and complete quote');
  eq([r.chainKey,r.kind,r.verification,r.caller,r.recipient],['evm:31337','EXACT_IN','FAST',b.roles.trader,b.roles.trader],'original funded caller/recipient');
  assert.equal(r.constraints?.maxHops,1,'finite direct split scope');
  assert(bank.orders.some(o=>o.namespace===row.namespace&&o.source_bank===row.source_bank&&o.token_in===r.sellToken&&o.token_out===r.buyToken&&o.amount_in===r.sellAmount&&o.caller===r.caller&&o.recipient===r.recipient),'original funded order budget');
  const amount=decimal(r.sellAmount,'sell amount'),gross=decimal(q.grossBuyAmount,'gross output');
  assert(Number.isInteger(r.slippageBps)&&r.slippageBps>=0&&r.slippageBps<=10000);
  assert.equal(decimal(q.minBuyAmount,'minimum',true),gross*BigInt(10000-r.slippageBps)/10000n,'exact integer slippage policy');
  eq(q.snapshotRef.block,block(source.header),'actual original common source H');
  assert.equal(q.onlineGeneration,q.snapshotRef.online_generation);assert.equal(q.scopeHash,q.snapshotRef.scope_hash);
  assert.equal(q.verification.level,'INDICATIVE');assert.equal(q.verification.success,false);
  assert.equal(q.consumedIn,r.sellAmount);assert.equal(q.refund,'0');
  assert.equal(q.routes.length,2,'actual two-route split required');
  eq([...q.quality.candidateScope].sort(),b.pools.map(p=>p.pool).sort(),'complete physical candidate scope');eq(q.quality.excludedPools,{});
  let sumIn=0n,sumOut=0n;const poolIDs=[];
  for(const route of q.routes){
   const inputAmount=decimal(route.amount_in,'route input'),outputAmount=decimal(route.amount_out,'route output'),path=route.path;
   eq(path.tokens,[r.sellToken,r.buyToken]);assert.equal(path.amount_in,route.amount_in);assert.equal(path.amount_out,route.amount_out);assert.equal(path.edges.length,1);
   const edge=path.edges[0],p=b.pools.find(p=>p.pool===edge.pool_id);assert(p,'selected original physical pool');
   eq([edge.protocol,edge.from_token,edge.to],[p.requested_protocol,r.sellToken,r.buyToken]);
   poolIDs.push(p.pool);sumIn+=inputAmount;sumOut+=outputAmount;
  }
  assert.equal(new Set(poolIDs).size,2,'both physical pools must execute');assert.equal(sumIn,amount,'integer split input conservation');assert.equal(sumOut,gross,'integer split output sum');
  assert(BigInt(source.global_wallets.find(w=>w.token===r.sellToken).balances[r.caller])>=amount,'original real sender funds');
  return {index,row,source,branch:b,signature:V.canonical({namespace:row.namespace,source:source.id,request:r,routes:q.routes})};
 });
}
function evidence(t){return {signed_transaction:t.signed_transaction,transaction:t.transaction,receipt:t.receipt,pre_header:t.pre_header,execution_header:t.execution_header,receipt_sha256:sha(V.canonical(t.receipt))};}
function sourceEqual(actual,expected){for(const key of ['header','header_anchor_after','pools','global_wallets','global_wallet_getters','accounts','storage'])eq(actual[key],expected[key],'restored exact original source '+key);}
function wallet(bank,token,who){const w=bank.global_wallets.find(w=>w.token===token);assert(w&&Object.hasOwn(w.balances,who));return BigInt(w.balances[who]);}
async function executeMode(items,result){
 const branch=items[0].branch,mode=branch.mode,h=new Harness(mode);
 try{
  await h.replay();result.vm_modes_started.push(mode);result.prefix_transactions+=28;
  h.pools=JSON.parse(JSON.stringify(branch.pools));h.tokenAddresses=[...branch.tokens];h.decimals=[...branch.decimals];h.newIDs=JSON.parse(JSON.stringify(branch.new_identities));
  const ids=[...branch.prefix_identities,...branch.new_identities],abis=new Map();
  for(const id of ids){const artifact=A.locate(h.builds,id.artifact.build,id.artifact.name),abi=new ethers.utils.Interface(artifact.c.abi);abis.set(id.address,abi);h.contracts.set(id.address,new ethers.Contract(id.address,artifact.c.abi,h.wallets[0]));}
  h.tokens=branch.tokens.map(t=>h.contracts.get(t));
  const headers=new Map();const keep=x=>headers.set(num(x.number),x);
  for(const x of branch.prefix_replay.headers)keep(x);for(const t of branch.transactions){keep(t.pre_header);keep(t.execution_header);}for(const x of branch.clock_operations){keep(x.pre_header);keep(x.header);}
  let txIndex=0,height=28;
  async function restore(source){
   const target=num(source.header.number);assert(target>=height,'original source replay is monotonic');
   while(height<target){
    const next=branch.transactions[txIndex],expected=headers.get(height+1);assert(expected,'complete original next header');
    await h.rpc('evm_setTime',[num(expected.timestamp)*1000]);
    if(next&&num(next.execution_header.number)===height+1){
     eq(await h.header(),next.pre_header,'original signed replay preheader');
     assert.equal(await h.rpc('eth_sendRawTransaction',[next.signed_transaction]),next.transaction.hash);
     eq(await h.rpc('eth_getTransactionReceipt',[next.transaction.hash]),next.receipt,'exact original receipt during source restore');eq(await h.rpc('eth_getTransactionByHash',[next.transaction.hash]),next.transaction);
     txIndex++;result.source_transactions_replayed++;
    }else{eq(expected.transactions,[],'only original empty clock block may fill a replay gap');await h.rpc('evm_mine');}
    height++;eq(await h.header(),expected,'exact original replay header');
   }
   eq(await h.header(),source.header,'exact original selected source');
   for(const i of branch.new_identities){const code=await h.rpc('eth_getCode',[i.address,source.header.number]);assert.equal(code,branch.native_runtime[i.runtime_sha256],'original physical runtime at selected source');h.runtime[i.runtime_sha256]=code;}
   const actual=await h.capture('selected-source:'+source.id);sourceEqual(actual,source);return actual;
  }
  const bySource=new Map();for(const item of items){if(!bySource.has(item.source.id))bySource.set(item.source.id,[]);bySource.get(item.source.id).push(item);}
  const groups=[...bySource.values()].sort((a,b)=>num(a[0].source.header.number)-num(b[0].source.header.number));
  for(const group of groups){
   const source=group[0].source;let observedSource=await restore(source);
   const executed=new Map();
   for(const item of group){
    if(executed.has(item.signature)){result.quote_links.push({id:item.row.id,index:item.index,execution_id:executed.get(item.signature)});continue;}
    const snapshot=await h.rpc('evm_snapshot'),execution={id:'E'+result.executions.length,namespace:mode,source_bank:source.id,source:observedSource,request:item.row.request,quote:item.row.quote,routes:[],source_restored:false};
    result.executions.push(execution);let current=observedSource;
    try{
     for(const [routeIndex,route]of item.row.quote.routes.entries()){
      const p=branch.pools.find(p=>p.pool===route.path.edges[0].pool_id),before=current,r=item.row.request;
      const allowance=(await h.call(h.contracts.get(r.sellToken),'allowance',[branch.roles.trader,branch.vault],before.header))[0];assert(BigInt(allowance)>=BigInt(route.amount_in),'original allowance; no extra approval allowed');
      const args=[{poolId:p.pool_id,kind:0,assetIn:r.sellToken,assetOut:r.buyToken,amount:route.amount_in,userData:'0x'},
       {sender:r.caller,recipient:r.recipient,fromInternalBalance:false,toInternalBalance:false},0,ethers.constants.MaxUint256];
      const data=h.vault.interface.encodeFunctionData('swap',args),nonce=num(await h.rpc('eth_getTransactionCount',[branch.roles.trader,'latest']));
      const signed=await h.wallets[3].signTransaction({type:0,chainId:31337,nonce,to:branch.vault,data,gasLimit:3000000,gasPrice:2000000000});
      await h.rpc('evm_setTime',[num(source.header.timestamp)*1000]);const hash=await h.rpc('eth_sendRawTransaction',[signed]);assert.equal(hash,ethers.utils.keccak256(signed));
      const receipt=await h.rpc('eth_getTransactionReceipt',[hash]),tx=await h.rpc('eth_getTransactionByHash',[hash]),header=await h.header();assert.equal(receipt.status,'0x1','actual original Vault selected route succeeded');assert.equal(header.timestamp,source.header.timestamp);
      const record={signed_transaction:signed,transaction:tx,receipt,pre_header:before.header,execution_header:header,method:'swap',args:clean(args)};Native.transaction(record,abis);
      current=await h.capture('selected-execution:'+execution.id+':route-'+routeIndex);C.validateBank(current,branch,ids,abis);
      const actualIn=String(wallet(before,r.sellToken,r.caller)-wallet(current,r.sellToken,r.caller)),actualOut=String(wallet(current,r.buyToken,r.recipient)-wallet(before,r.buyToken,r.recipient));
      assert.equal(actualIn,route.amount_in,'real sender debit equals selected allocation');assert.equal(actualOut,route.amount_out,'real original output equals API selected route');
      const event={pool_id:p.pool_id,token_in:r.sellToken,token_out:r.buyToken,amount_in:actualIn,amount_out:actualOut};
      const nativeCase={id:'selected/'+execution.id+'/'+routeIndex,namespace:mode,operation:'swap',pool_name:p.name,before:before.id,after:current.id,transaction:'selected',index_in:branch.tokens.indexOf(r.sellToken),index_out:branch.tokens.indexOf(r.buyToken),token_in:r.sellToken,token_out:r.buyToken,amount_in:route.amount_in,expected_native:{status:1,actual_amount_in:actualIn,actual_amount_out:actualOut,swap_event:event}};
      C.validateCase(nativeCase,null,branch,new Map([['selected',record]]),new Map([[before.id,before],[current.id,current]]),abis);
      execution.routes.push({index:routeIndex,pool_name:p.name,pool:p.pool,quoted_amount_in:route.amount_in,quoted_amount_out:route.amount_out,actual_amount_in:actualIn,actual_amount_out:actualOut,original_allowance:allowance,before,after:current,transaction:evidence(record)});
      result.selected_transactions_executed++;
     }
     const r=item.row.request;
     execution.actual_amount_in=String(wallet(observedSource,r.sellToken,r.caller)-wallet(current,r.sellToken,r.caller));execution.actual_amount_out=String(wallet(current,r.buyToken,r.recipient)-wallet(observedSource,r.buyToken,r.recipient));
     assert.equal(execution.actual_amount_in,r.sellAmount);assert.equal(execution.actual_amount_out,item.row.quote.grossBuyAmount);assert(BigInt(execution.actual_amount_out)>=BigInt(item.row.quote.minBuyAmount));
     execution.complete_native_after=current;execution.passed=true;
    }finally{
     assert.equal(await h.rpc('evm_revert',[snapshot]),true,'owned exact source snapshot restored');
     const restored=await h.capture('restored-source:'+source.id);sourceEqual(restored,source);execution.source_restored=true;observedSource=restored;
    }
    executed.set(item.signature,execution.id);result.quote_links.push({id:item.row.id,index:item.index,execution_id:execution.id});
   }
  }
 }finally{await h.close();}
}
async function execute(input){
 const file=ROOT+'/integration/test/contracts/managed-v2-scheduled-shared.json',bytes=fs.readFileSync(file),bank=JSON.parse(bytes);C.check(bank);
 const plans=validateInput(input,bank,sha(bytes));
 const result={schema_version:1,passed:false,vm_started:false,vm_modes_started:[],prefix_transactions:0,source_transactions_replayed:0,selected_transactions_executed:0,executions:[],quote_links:[],original_shared_sha256:sha(bytes),verified:false,production_ready:false,independent_gas_reference:false,all_requested_features_complete:false,
  authority:'independent-original-Vault-bytecode-execution-of-actual-selected-two-route-API-plans',
  isolation:'one owned original VM per mode; exact original source restore and checked snapshot/revert per distinct selected plan',
  atomicity:'sequential signed local Vault transactions in separate blocks for reference; no atomic production execution claim',
  limitation:'January 2026 recorded-source local execution; no current-market/TTL/live-chain/trie/public executor or complete gas model proof'};
 for(const mode of ['mixed','active']){const items=plans.filter(p=>p.row.namespace===mode);if(items.length){result.vm_started=true;await executeMode(items,result);}}
 assert.equal(result.quote_links.length,plans.length);assert(result.executions.every(e=>e.passed&&e.source_restored));assert(fs.readFileSync(file).equals(bytes),'original raw unchanged');result.passed=true;return result;
}
async function main(){let result;try{const bytes=fs.readFileSync(0);assert(bytes.length<=8*1024*1024,'bounded selected API input');result=await execute(JSON.parse(bytes));}catch(e){result={schema_version:1,passed:false,verified:false,production_ready:false,all_requested_features_complete:false,error:String(e.stack||e)};process.exitCode=1;}process.stdout.write(JSON.stringify(result)+'\n');}
module.exports={validateInput,execute,main};if(require.main===module)main();
