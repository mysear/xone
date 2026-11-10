'use strict';
// Structural qualification is separate from fresh original-VM byte equality.
const assert=require('node:assert/strict');
const I=require('./inputs.cjs'),{A,E,V,sha}=I;
const C=require('../balancer-managed-v2-scheduled/checker.cjs');
const {ethers}=E.rr('ethers'),{eq,clean}=require('./harness.cjs');
const PROFILE='balancer-v2-managed-scheduled-shared-native';
function ref(original,r,k){return V.resolveObject(original.objects,r.ref,k);}
function lookup(tx,table){const t=table.get(tx);assert(t,'original signed transaction '+tx);return t;}
function read(state,to,method,args=[]){
 const key=a=>V.canonical(a.map(x=>typeof x==='number'?String(x):x));
 const rows=state.getters.filter(g=>g.to===to&&g.method===method&&key(g.args)===key(args));
 assert(rows.length,'required original getter '+method);return rows[0].decoded;
}
function validateIdentity(i,b,txs,builds){
 const a=A.locate(builds,i.artifact.build,i.artifact.name);assert.equal(a.source,i.artifact.source);
 assert(['TestToken','ManagedPool'].includes(a.name),'only original shared tokens and pools');
 assert.equal(a.build,a.name==='TestToken'?'Authorizer':'ManagedPoolFactory');
 const code=b.native_runtime[i.runtime_sha256];assert(typeof code==='string'&&/^0x(?:[0-9a-f]{2})*$/.test(code));assert.equal(sha(Buffer.from(code.slice(2),'hex')),i.runtime_sha256);
 assert.equal((code.length-2)/2,i.runtime_bytes);assert(i.runtime_bytes<=24576,'normal EIP170');
 eq(i.runtime_template,a.c.evm.deployedBytecode,'exact original compiled template');
 const vars=new Map(Object.values(builds[a.build].output.sources).flatMap(s=>s.ast.nodes.filter(n=>n.nodeType==='ContractDefinition').flatMap(n=>n.nodes)).filter(n=>n.nodeType==='VariableDeclaration').map(v=>[String(v.id),v]));
 const wantedImmutables=Object.entries(a.c.evm.deployedBytecode.immutableReferences||{}).map(([id,spans])=>({ast_id:id,name:vars.get(id).name,type:vars.get(id).typeDescriptions.typeString,spans:spans.map(s=>({...s,value:'0x'+code.slice(2+s.start*2,2+(s.start+s.length)*2)}))}));
 eq(i.immutables,wantedImmutables,'immutable interpretation from original AST and bytes');
 let masked=code.slice(2),expected=a.c.evm.deployedBytecode.object;const links=[];
 for(const libs of Object.values(a.c.evm.deployedBytecode.linkReferences||{}))for(const [name,spans]of Object.entries(libs)){
  const address=b.prefix_identities.find(x=>x.label===name)?.address;assert(address,'pinned original library');links.push({name,address,spans});
  for(const s of spans){assert.equal(code.slice(2+s.start*2,2+(s.start+s.length)*2),address.slice(2));expected=expected.slice(0,s.start*2)+address.slice(2)+expected.slice((s.start+s.length)*2);}
 }
 eq(i.library_links,links,'complete original library links');
 for(const row of wantedImmutables)for(const s of row.spans){masked=masked.slice(0,s.start*2)+'0'.repeat(s.length*2)+masked.slice((s.start+s.length)*2);expected=expected.slice(0,s.start*2)+'0'.repeat(s.length*2)+expected.slice((s.start+s.length)*2);}
 assert.equal(masked,expected,'original runtime outside declared immutable contexts');
 const tx=lookup(i.creation_transaction,txs);assert.equal(tx.receipt.status,'0x1');
 if(a.name==='TestToken'){
  assert.equal(tx.transaction.to,null);assert.equal(tx.receipt.contractAddress,i.address);
  assert.equal(ethers.utils.getContractAddress({from:tx.transaction.from,nonce:Number(BigInt(tx.transaction.nonce))}).toLowerCase(),i.address);
  eq(i.constructor_arguments,tx.args,'actual token constructor args');
  const addresses=Object.fromEntries(b.prefix_identities.map(x=>[x.label,x.address]));
  const deploy=new ethers.ContractFactory(a.c.abi,A.link(a,addresses)).getDeployTransaction(...tx.args).data.toLowerCase();
  assert.equal(tx.transaction.input,deploy,'original token initcode plus constructor');
 }else assert.equal(i.runtime_bytes,24561,'published ManagedPool runtime');
}
function poolContext(raw,b,p,original){
 const bankRows=raw.banks.filter(x=>x.namespace===b.mode);
 const ids=b.new_identities.filter(i=>i.address===p.pool||b.tokens.includes(i.address));
 const allowed=new Set([...b.prefix_identities.map(i=>i.address),...ids.map(i=>i.address),...Object.values(b.roles),...original.reference.factory_code_storage.contracts.map(x=>x.address)]);
 const targetSet=new Set([...b.prefix_identities.map(i=>i.address),...ids.map(i=>i.address)]);
 // This is an exact observation view, not a generated Pool or pricing model.
 const states=bankRows.map(bank=>{
  const state=bank.pools.find(x=>x.name===p.name).state;
  return {...state,id:bank.id,label:bank.label,getters:state.getters.filter(g=>targetSet.has(g.to)),accounts:bank.accounts.filter(a=>allowed.has(a.address)),storage:bank.storage.filter(s=>allowed.has(s.address))};
 });
 return {configuration:{...p,factory:b.factory,vault:b.vault,tokens:b.tokens,decimals:b.decimals,roles:b.roles},chain_options:b.chain_options,prefix_identities:b.prefix_identities,new_identities:ids,native_runtime:b.native_runtime,storage_layout:b.storage_layout,encoding:b.encoding,
  inputs:{source_manifest:raw.original_reference.source_manifest,input_pins:raw.inputs.original.original,node_pins:raw.inputs.original.node,original_compiler_jobs:raw.original_reference.compiler_jobs,source_pins:raw.original_reference.source_pins},
  prefix_replay:b.prefix_replay,states,transactions:b.transactions.filter(t=>t.transaction.to===null||targetSet.has(t.transaction.to))};
}
function validateGetter(g,bank,abis,observed){
 const abi=abis.get(g.to);assert(abi,'known raw getter original ABI');
 const fn=abi.getFunction(g.method);assert(['view','pure'].includes(fn.stateMutability),'retained getter must be read-only');
 assert.equal(abi.encodeFunctionData(g.method,g.args).toLowerCase(),g.data,'canonical raw getter args');
 const decoded=clean(abi.decodeFunctionResult(g.method,g.result));eq(g.decoded,decoded,'raw ABI getter decode');
 assert.equal(abi.encodeFunctionResult(g.method,decoded).toLowerCase(),g.result,'canonical raw getter result');
 assert.equal(g.block_number,bank.header.number,'every retained getter source block');assert.equal(g.block_hash,bank.header.hash,'every retained getter source hash');
 const key=g.to+g.data,value={result:g.result,decoded};
 if(observed.has(key))eq(value,observed.get(key),'one original getter result across the whole common-H bank');
 else observed.set(key,value);
}
function poolGetterView(state,p,excluded=[]){
 const values=new Map();for(const g of state.getters)if(g.to===p.pool&&!excluded.includes(g.method))values.set(g.data,g.result);
 return [...values].sort(([a],[b])=>a.localeCompare(b));
}
function validateBank(bank,b,ids,abis){
 C.header(bank.header);eq(bank.header_anchor_after,bank.header,'common H complete reanchor');
 eq(bank.pools.map(p=>p.name),['A','B'],'two physical pools at common H');
 const accounts=[...new Set([...ids.map(i=>i.address),...Object.values(b.roles),...I.originalBank().reference.factory_code_storage.contracts.map(x=>x.address)])].sort();
 eq(bank.accounts.map(a=>a.address),accounts,'complete shared account set');
 for(const a of bank.accounts){const code=b.native_runtime[a.code_sha256];assert(typeof code==='string');assert.equal((code.length-2)/2,a.runtime_bytes);}
 const observed=new Map();
 for(const row of bank.pools){
  const state=row.state,p=b.pools.find(p=>p.name===row.name);assert(p);eq(state.header,bank.header,'per-pool common header');eq(state.header_anchor_after,bank.header,'per-pool source reanchor');
  assert.equal(state.source_h,bank.header.number);assert.equal(state.source_clock,Number(BigInt(bank.header.timestamp)));
  const allowed=new Set([...b.prefix_identities.map(i=>i.address),...b.tokens,p.pool]);
  for(const g of state.getters){validateGetter(g,bank,abis,observed);assert(allowed.has(g.to),'per-pool getter target outside declared observation scope');}
  const get=(to,m,args=[])=>read(state,to,m,args);
  eq(state.weights,get(p.pool,'getNormalizedWeights')[0],'actual current weight view');
  assert.equal(state.sum_weights,String(state.weights.reduce((s,w)=>s+BigInt(w),0n)));
  assert.equal(state.fee,get(p.pool,'getSwapFeePercentage')[0]);
  eq(state.weight_schedule,get(p.pool,'getGradualWeightUpdateParams'));eq(state.fee_schedule,get(p.pool,'getGradualSwapFeeUpdateParams'));
  eq(state.registration,get(b.vault,'getPoolTokens',[p.pool_id]));eq(state.registration[0],[p.pool,...b.tokens]);
  eq(state.scales,get(p.pool,'getScalingFactors')[0]);assert.equal(state.owner,b.roles.manager);
  const aum=state.aum,bpt=get(b.vault,'getPoolTokenInfo',[p.pool_id,p.pool]);
  const changes=[BigInt(bpt[2]),...state.token_states.map(t=>BigInt(t.info[2]))];
  assert(changes.every(n=>n<=BigInt(bank.header.number)&&n<=0xffffffffn),'source uint32 registration clocks');
  assert.equal(BigInt(state.registration[2]),changes.reduce((a,z)=>a>z?a:z),'Vault max underlying/BPT last change');
  eq(aum.params,get(p.pool,'getManagementAumFeeParams'));eq(aum.bpt_info,bpt);
  assert.equal(aum.total_supply,get(p.pool,'totalSupply')[0]);assert.equal(aum.actual_supply,get(p.pool,'getActualSupply')[0]);
  assert.equal(aum.virtual_supply,String(BigInt(aum.total_supply)-BigInt(bpt[0])-BigInt(bpt[1])));
  assert.equal(aum.pending_aum,String(BigInt(aum.actual_supply)-BigInt(aum.virtual_supply)));
  assert.equal(aum.protocol_share,get(p.pool,'getProtocolFeePercentageCache',[3])[0]);assert.equal(aum.provider_id,get(p.pool,'getProviderFeeId',[3])[0]);
  assert.equal(aum.provider_percentage,get(b.fee_provider,'getFeeTypePercentage',[aum.provider_id])[0]);
  assert.equal(aum.manager_bpt,get(p.pool,'balanceOf',[b.roles.manager])[0]);
  const collector=get(b.vault,'getProtocolFeesCollector')[0];assert.equal(aum.collector_bpt,get(p.pool,'balanceOf',[collector])[0]);
  const aumStorage=bank.storage.find(r=>r.address===p.pool&&r.key==='0xa');assert(aumStorage);assert.equal(aum.raw_word,aumStorage.value);
  eq(state.packed.tokens.map(t=>t.token),b.tokens);assert.equal(state.token_states.length,3);
  for(let i=0;i<3;i++){
   const t=state.token_states[i],token=b.tokens[i],packed=state.packed.tokens[i];assert.equal(t.token,token);assert.equal(t.index,i);assert.equal(t.mapping_slot,11);assert.equal(t.decimals,b.decimals[i]);
   eq(t.info,get(b.vault,'getPoolTokenInfo',[p.pool_id,token]));assert.equal(t.decimals,get(token,'decimals')[0]);
   assert.equal(t.storage_key,packed.key);assert.equal(t.raw_word,packed.word);
   for(const who of [b.roles.lp,b.roles.trader,b.vault])assert.equal(t.wallet_balances[who],get(token,'balanceOf',[who])[0]);
   assert.equal(t.trader_allowance,get(token,'allowance',[b.roles.trader,b.vault])[0]);
   assert.equal(BigInt(state.registration[1][i+1]),BigInt(t.info[0])+BigInt(t.info[1]));
  }
  if(p.requested_protocol==='balancer-v2-managed-aum'){
   assert(BigInt(state.weight_schedule[1])<=BigInt(bank.header.timestamp)&&BigInt(state.fee_schedule[1])<=BigInt(bank.header.timestamp),'legacy profile cannot silently accept active schedule');
   eq(state.weights,state.weight_schedule[3],'legacy terminal weight view');assert.equal(state.fee,state.fee_schedule[3]);
  }
 }
 eq(bank.global_wallets.map(w=>w.token),b.tokens,'complete shared underlying wallet set');
 const walletKeys=new Set();
 for(const g of bank.global_wallet_getters){assert(b.tokens.includes(g.to)&&['balanceOf','totalSupply'].includes(g.method),'only declared global underlying wallet reads');validateGetter(g,bank,abis,observed);const key=g.to+g.data;assert(!walletKeys.has(key),'duplicate global wallet observation');walletKeys.add(key);}
 assert.equal(bank.global_wallet_getters.length,3*8,'complete unique global holder and supply reads');
 for(const w of bank.global_wallets){
  const get=(m,args=[])=>{const g=bank.global_wallet_getters.find(g=>g.to===w.token&&g.method===m&&V.canonical(g.args)===V.canonical(args));assert(g,'actual global wallet getter');return g.decoded[0];};
  assert.equal(w.total_supply,get('totalSupply'));
  const holders=[...new Set([...Object.values(b.roles),b.vault,...b.pools.map(p=>p.pool)])];eq(Object.keys(w.balances),holders);
  for(const who of holders)assert.equal(w.balances[who],get('balanceOf',[who]));
  let cash=0n;for(const p of bank.pools){const t=p.state.token_states.find(t=>t.token===w.token);assert.equal(t.wallet_balances[b.vault],w.balances[b.vault],'one actual Vault wallet independently seen by both pools');cash+=BigInt(t.info[0]);}
  assert(BigInt(w.balances[b.vault])>=cash,'global wallet backs both declared registrations');
 }
}
function validateCase(c,raw,b,txs,banks,abis){
 const before=banks.get(c.before),after=banks.get(c.after),t=lookup(c.transaction,txs),p=b.pools.find(p=>p.name===c.pool_name);assert(before&&after&&p);assert.equal(before.namespace,c.namespace);assert.equal(after.namespace,c.namespace);
 eq(before.header,t.pre_header,'case native preheader');eq(after.header,t.execution_header,'case native after header');
 assert.equal(t.receipt.status,'0x1');assert.equal(c.expected_native.status,1);
 const own=bank=>bank.pools.find(x=>x.name===c.pool_name).state;
 const s=own(before),a=own(after),wallet=(bank,token,owner)=>BigInt(bank.global_wallets.find(w=>w.token===token).balances[owner]);
 if(c.operation==='swap'){
  assert.equal(t.transaction.to,b.vault);assert.equal(t.method,'swap');assert.equal(t.transaction.from,b.roles.trader);
  const [swap,funds,limit,deadline]=t.args;
  eq(swap,{poolId:p.pool_id,kind:0,assetIn:c.token_in,assetOut:c.token_out,amount:c.amount_in,userData:'0x'},'exact native swap args');
  eq(funds,{sender:b.roles.trader,recipient:b.roles.trader,fromInternalBalance:false,toInternalBalance:false});assert.equal(String(limit),'0');assert.equal(String(deadline),ethers.constants.MaxUint256.toString());
  assert.equal(b.tokens[c.index_in],c.token_in);assert.equal(b.tokens[c.index_out],c.token_out);assert.notEqual(c.index_in,c.index_out);
  const input=BigInt(c.amount_in),output=wallet(after,c.token_out,b.roles.trader)-wallet(before,c.token_out,b.roles.trader);assert(input>0n&&output>0n);
  assert.equal(wallet(before,c.token_in,b.roles.trader)-wallet(after,c.token_in,b.roles.trader),input);
  assert.equal(c.expected_native.actual_amount_in,String(input));assert.equal(c.expected_native.actual_amount_out,String(output));
  const logs=t.receipt.logs.filter(l=>l.address===b.vault).map(l=>{try{return abis.get(b.vault).parseLog(l);}catch{return null;}}).filter(l=>l?.name==='Swap');assert.equal(logs.length,1);const e=logs[0].args;
  const event={pool_id:e.poolId.toLowerCase(),token_in:e.tokenIn.toLowerCase(),token_out:e.tokenOut.toLowerCase(),amount_in:e.amountIn.toString(),amount_out:e.amountOut.toString()};
  eq(event,{pool_id:p.pool_id,token_in:c.token_in,token_out:c.token_out,amount_in:String(input),amount_out:String(output)});eq(c.expected_native.swap_event,event);
  for(let i=0;i<3;i++){
   const delta=i===c.index_in?input:i===c.index_out?-output:0n;
   assert.equal(BigInt(a.token_states[i].info[0])-BigInt(s.token_states[i].info[0]),delta,'own registered cash delta');
   assert.equal(wallet(after,b.tokens[i],b.vault)-wallet(before,b.tokens[i],b.vault),delta,'actual global wallet delta');
   assert.equal(wallet(after,b.tokens[i],b.roles.trader)-wallet(before,b.tokens[i],b.roles.trader),-delta,'actual sender/recipient wallet delta');
   eq(a.token_states[i].info.slice(1),[s.token_states[i].info[1],i===c.index_in||i===c.index_out?String(BigInt(after.header.number)&0xffffffffn):s.token_states[i].info[2],s.token_states[i].info[3]],'own raw managed/change/manager fields');
  }
  eq(a.aum,s.aum,'ordinary swap cannot mint BPT or collect AUM at same clock');
  eq(poolGetterView(a,p),poolGetterView(s,p),'all own Pool getter views unchanged by underlying swap at same clock');
  eq(a.packed,s.packed,'ordinary swap cannot rewrite schedules');
  for(let i=0;i<3;i++){
   const x=before.global_wallets[i],y=after.global_wallets[i],delta=i===c.index_in?input:i===c.index_out?-output:0n;
   assert.equal(y.total_supply,x.total_supply,'ordinary swap cannot mint/burn underlying');
   for(const who of Object.keys(x.balances))assert.equal(BigInt(y.balances[who])-BigInt(x.balances[who]),who===b.vault?delta:who===b.roles.trader?-delta:0n,'complete declared underlying wallet transitions');
  }
 }else{
  assert.equal(c.operation,'collect');assert.equal(t.method,'collectAumManagementFees');assert.equal(t.transaction.to,p.pool);eq(t.args,[]);
  const minted=BigInt(a.aum.total_supply)-BigInt(s.aum.total_supply),manager=BigInt(a.aum.manager_bpt)-BigInt(s.aum.manager_bpt),collector=BigInt(a.aum.collector_bpt)-BigInt(s.aum.collector_bpt);
  assert(minted>0n);assert.equal(minted,manager+collector);assert.equal(a.aum.pending_aum,'0');
  eq([c.expected_native.minted_bpt,c.expected_native.manager_bpt_delta,c.expected_native.collector_bpt_delta],[String(minted),String(manager),String(collector)]);
  assert.equal(minted,BigInt(s.aum.pending_aum),'collection mints original pending view');
  assert.equal(a.aum.params[0],s.aum.params[0]);assert.equal(BigInt(a.aum.params[1]),BigInt(after.header.timestamp)&0xffffffffn);
  assert.equal(BigInt(a.aum.virtual_supply)-BigInt(s.aum.virtual_supply),minted);
  assert.equal(a.aum.actual_supply,s.aum.actual_supply);
  eq(a.aum.bpt_info,s.aum.bpt_info,'collection does not register its BPT mint as Vault cash');
  eq(a.registration,s.registration,'collection does not change Vault token registrations');
  eq(poolGetterView(a,p,['getManagementAumFeeParams','totalSupply','getActualSupply','balanceOf']),poolGetterView(s,p,['getManagementAumFeeParams','totalSupply','getActualSupply','balanceOf']),'collection preserves all non-accounting Pool views');
  eq(read(a,p.pool,'balanceOf',[b.vault]),read(s,p.pool,'balanceOf',[b.vault]),'collection does not donate BPT to Vault');
  eq(a.token_states,s.token_states,'collection does not touch underlying registrations or wallets');eq(after.global_wallets,before.global_wallets,'collection does not touch global underlying wallets');
 }
 for(const other of b.pools.filter(x=>x.name!==p.name)){
  const x=before.pools.find(x=>x.name===other.name).state,y=after.pools.find(x=>x.name===other.name).state;
  eq(y.token_states.map(t=>t.info),x.token_states.map(t=>t.info),'sibling registered cash/managed and change block remain attached to physical pool');eq(y.aum,x.aum,'sibling BPT/accounting unchanged at same clock');
  for(const key of ['weights','sum_weights','fee','weight_schedule','fee_schedule','packed','scales','owner','registration'])eq(y[key],x[key],'same-clock sibling immutable/dynamic view preserved: '+key);
  eq(poolGetterView(y,other),poolGetterView(x,other),'all sibling Pool getters preserved at same clock');
 }
}
function validateControlTimeline(raw,b,txs,banks,local){
 const mode=b.mode,initial=local[0],management=raw.management.filter(m=>m.namespace===mode),cases=raw.cases.filter(c=>c.namespace===mode);
 assert.equal(b.prefix_replay.count,28,'original prefix count');
 const expectedControls=b.pools.filter(p=>mode==='active'||p.name==='A').flatMap(p=>[['updateWeightsGradually',p],['updateSwapFeeGradually',p]]);
 assert.equal(management.length,expectedControls.length);assert.equal(new Set(management.map(m=>m.transaction)).size,management.length,'control calls bijective');
 const nativeControls=b.transactions.filter(t=>['updateWeightsGradually','updateSwapFeeGradually'].includes(t.method));
 eq([...management.map(m=>m.transaction)].sort(),nativeControls.map(t=>t.id).sort(),'all original controls represented once');
 const nativeCases=b.transactions.filter(t=>['swap','collectAumManagementFees'].includes(t.method));
 assert.equal(new Set(cases.map(c=>c.transaction)).size,cases.length,'one case per distinct original execution');
 eq(cases.map(c=>c.transaction).sort(),nativeCases.map(t=>t.id).sort(),'all original swaps/collections represented once');
 for(const p of b.pools){assert.equal(cases.filter(c=>c.pool_name===p.name&&c.operation==='swap').length,12,'both physical pools own 12 native swaps');assert.equal(cases.filter(c=>c.pool_name===p.name&&c.operation==='collect').length,1,'both physical pools own collection');}
 const sourceTime=BigInt(initial.header.timestamp),start=sourceTime+60n,end=start+1001n;
 const targets={A:['100000000000000000','800000000000000000','100000000000000000'],B:['600000000000000000','100000000000000000','300000000000000000']};
 let current=initial;const visited=[initial.id];
 for(let index=0;index<management.length;index++){
  const m=management[index],[method,p]=expectedControls[index],t=lookup(m.transaction,txs),before=banks.get(m.before),after=banks.get(m.after);
  assert(before&&after,'management before/after references');assert.equal(before.namespace,mode);assert.equal(after.namespace,mode);
  assert.equal(m.pool_name,p.name);assert.equal(m.method,method);assert.equal(t.method,method);assert.equal(t.transaction.to,p.pool);assert.equal(t.transaction.from,b.roles.manager);
  assert.equal(before.id,current.id,'management native sequence');eq(before.header,t.pre_header);eq(after.header,t.execution_header);
  assert.equal(BigInt(t.args[0]),start);assert.equal(BigInt(t.args[1]),end);
  const x=before.pools.find(x=>x.name===p.name).state,y=after.pools.find(x=>x.name===p.name).state;
  if(method==='updateWeightsGradually'){eq(t.args.slice(2),[b.tokens,targets[p.name]],'original caller weight endpoint inputs');eq(y.weight_schedule,[String(start),String(end),x.weights,targets[p.name]],'computed start is original before getter without normalization');}
  else {const target=p.name==='A'?'30000000000000000':'25000000000000000';eq(t.args.slice(2),['10000000000000000',target]);eq(y.fee_schedule,[String(start),String(end),'10000000000000000',target]);}
  current=after;visited.push(current.id);
 }
 const cps=raw.checkpoints.filter(c=>c.namespace===mode),ops=b.clock_operations;
 assert.equal(ops.length,6,'all six actual clock operations retained');
 const labels=['start-1','start','start+1','midpoint','end-1','end'],times=[start-1n,start,start+1n,start+500n,end-1n,end];
 const swaps=cases.filter(c=>c.operation==='swap'),collections=cases.filter(c=>c.operation==='collect');
 eq(cases.map(c=>c.operation),[...Array(24).fill('swap'),'collect','collect'],'native case order');
 for(let index=0;index<6;index++){
  const cp=cps[index],op=ops[index],bank=banks.get(cp.bank);assert(bank&&bank.namespace===mode);
  assert.equal(cp.id,mode+'/CP'+index);assert.equal(cp.label,labels[index]);assert.equal(op.label,labels[index]);
  assert.equal(BigInt(cp.timestamp),times[index]);assert.equal(BigInt(op.timestamp),times[index]);
  eq(op.pre_header,current.header,'clock before header follows preceding actual operation');C.header(op.pre_header);C.header(op.header);
  assert.equal(op.header.parentHash,op.pre_header.hash);assert.equal(BigInt(op.header.number),BigInt(op.pre_header.number)+1n);assert.equal(BigInt(op.header.timestamp),times[index]);eq(op.header.transactions,[],'actual clock advance mines an empty block');
  eq(bank.header,op.header,'clock/checkpoint/common bank one-to-one');current=bank;visited.push(bank.id);
  const directions=[['A',0,1],['B',0,1],['A',1,0],['B',1,0]];
  for(let step=0;step<4;step++){
   const c=swaps[index*4+step],[name,i,j]=directions[step];assert.equal(c.pool_name,name);assert.equal(c.index_in,i);assert.equal(c.index_out,j);
   assert.equal(c.amount_in,String(10n**BigInt(b.decimals[i])+BigInt(index+1)),'fixed original call amount is input, not model output');
   assert.equal(c.before,current.id,'A/B/A/B raw execution sequence');current=banks.get(c.after);visited.push(current.id);
  }
 }
 for(let index=0;index<2;index++){const c=collections[index];assert.equal(c.pool_name,['A','B'][index]);assert.equal(c.before,current.id,'collection follows complete native swaps');current=banks.get(c.after);visited.push(current.id);}
 eq(local.map(x=>x.id),visited,'all retained banks occur exactly in original control/clock/swap/collection sequence');
 const headers=new Map();function remember(h){C.header(h);const n=Number(BigInt(h.number));if(headers.has(n))eq(headers.get(n),h,'one original canonical header per height');else headers.set(n,h);}
 for(const h of b.prefix_replay.headers)remember(h);for(const t of b.transactions){remember(t.pre_header);remember(t.execution_header);}for(const op of ops){remember(op.pre_header);remember(op.header);}for(const bank of local)remember(bank.header);
 const maximum=Math.max(...headers.keys());assert.equal(headers.size,maximum+1,'complete original canonical ancestry');
 for(let n=1;n<=maximum;n++)assert.equal(headers.get(n).parentHash,headers.get(n-1).hash,'original parent continuity');
 const nonces=new Map();for(const t of b.prefix_replay.transactions)nonces.set(t.transaction.from,BigInt(t.transaction.nonce)+1n);
 for(let index=0;index<b.transactions.length;index++){const t=b.transactions[index];assert.equal(t.id,'T'+index);const from=t.transaction.from;assert.equal(BigInt(t.transaction.nonce),nonces.get(from)||0n,'original signer nonce sequence');nonces.set(from,BigInt(t.transaction.nonce)+1n);}
 const controlled=new Set([...management.map(m=>m.transaction),...cases.map(c=>c.transaction)]),initialHeight=BigInt(initial.header.number);
 eq(b.transactions.filter(t=>BigInt(t.pre_header.number)>=initialHeight).map(t=>t.id).sort(),[...controlled].sort(),'no unaccounted original operation after initialized source');
}
function checkObservations(raw){
 assert.equal(raw.schema_version,1);assert.equal(raw.profile,PROFILE);assert.equal(raw.authority,'independent-local-original-managed-scheduled-shared');assert.equal(raw.production_ready,false);assert.equal(raw.all_requested_features_complete,false);
 eq(raw.inputs.original,require('../balancer-managed-v2-scheduled/inputs.cjs').closure(),'inherited original source authority');eq(Object.keys(raw.branches),['mixed','active']);
 const original=I.originalBank(),{builds,pins}=A.load(),sourceMeta=raw.original_reference;
 eq(sourceMeta.source_manifest,I.sourceManifest());eq(sourceMeta.source_pins,pins);assert.equal(sourceMeta.source_pins_sha256,original.reference.source_pins_sha256);eq(sourceMeta.compiler_jobs,original.reference.compiler_jobs);eq(sourceMeta.provenance,require('../balancer-managed-v2-scheduled/generate.cjs').provenance());
 assert.equal(raw.banks.length,72);assert.equal(raw.cases.length,52);assert.equal(raw.checkpoints.length,12);assert.equal(raw.management.length,6);assert.equal(raw.orders.length,12);
 const banks=new Map(raw.banks.map(b=>[b.id,b]));assert.equal(banks.size,72);assert.equal(new Set(raw.cases.map(c=>c.id)).size,52,'globally unique namespace-qualified cases');
 const prefix=original.reference.identities.map(r=>ref(original,r,'identity')).filter(i=>BigInt(ref(original,ref(original,i.creation_transaction,'transaction').execution_header,'header').number)<=28n);
 const roles0=ref(original,original.cases[0].execution.before,'snapshot').state.roles,expectedRoles=Object.fromEntries(['admin','manager','lp','trader'].map(k=>[k,roles0[k]]));
 const coverage={modes:['mixed','active'],physical_pools_per_mode:2,shared_underlyings:3,decimals:[18,6,8],native_swap_cases:48,native_collection_cases:4,common_header_banks:72,clock_checkpoints:12,funded_split_budgets:12};eq(raw.coverage,coverage);
 for(const [mode,b]of Object.entries(raw.branches)){
  assert.equal(b.mode,mode);eq(b.roles,expectedRoles);eq(b.prefix_identities,prefix);eq(b.chain_options,original.reference.chain_options);
  assert.equal(b.chain_options.chain.allowUnlimitedContractSize,false);assert.equal(b.chain_options.chain.allowUnlimitedInitCodeSize,false);
  assert.equal(b.vault,prefix.find(i=>i.label==='Vault').address);assert.equal(b.factory,prefix.find(i=>i.label==='ManagedPoolFactory').address);assert.equal(b.fee_provider,prefix.find(i=>i.label==='ProtocolFeePercentagesProvider').address);
  eq(b.decimals,[18,6,8]);assert.equal(b.tokens.length,3);eq(b.tokens,[...new Set(b.tokens)].sort());eq(b.pools.map(p=>p.name),['A','B']);
  assert.equal(new Set(b.pools.map(p=>p.pool)).size,2);assert.equal(b.new_identities.length,5);
  eq(b.new_identities.map(i=>i.address).sort(),[...b.tokens,...b.pools.map(p=>p.pool)].sort(),'exact created physical pools and shared tokens');
  const ids=[...b.prefix_identities,...b.new_identities];assert.equal(new Set(ids.map(i=>i.address)).size,ids.length,'identities cannot shadow original prefix');
  const abis=new Map(ids.map(i=>[i.address,new ethers.utils.Interface(A.locate(builds,i.artifact.build,i.artifact.name).c.abi)]));
  const txs=new Map(b.transactions.map(t=>[t.id,t]));assert.equal(txs.size,b.transactions.length);
  for(const t of b.transactions){C.transaction(t,abis);assert.equal(t.receipt.status,'0x1');assert(Object.values(b.roles).includes(t.transaction.from));}
  for(const i of b.new_identities)validateIdentity(i,b,txs,builds);
  for(const p of b.pools){
   assert.equal(p.requested_protocol,p.name==='B'&&mode==='mixed'?'balancer-v2-managed-aum':'balancer-v2-managed-scheduled');
   eq(p.settings.tokens,b.tokens);assert.equal(p.settings.aumFeeId,3);assert.equal(p.settings.swapEnabledOnStart,true);assert.equal(p.settings.mustAllowlistLPs,false);eq(p.params.assetManagers,Array(3).fill(ethers.constants.AddressZero));
   const create=lookup(p.creation,txs);assert.equal(create.method,'create');assert.equal(create.transaction.to,b.factory);assert.equal(create.transaction.from,b.roles.admin);eq(create.args,[p.params,p.settings,b.roles.manager,ethers.constants.HashZero]);
   const event=create.receipt.logs.filter(l=>l.address===b.factory).map(l=>{try{return abis.get(b.factory).parseLog(l);}catch{return null;}}).find(e=>e?.name==='PoolCreated');assert(event);assert.equal(event.args.pool.toLowerCase(),p.pool);
   const init=lookup(p.initialize,txs);assert.equal(init.method,'joinPool');assert.equal(init.transaction.to,b.vault);assert.equal(init.transaction.from,b.roles.lp);assert.equal(init.args[0],p.pool_id);eq(init.args.slice(1,3),[b.roles.lp,b.roles.lp]);eq(init.args[3].assets,[p.pool,...b.tokens]);assert.equal(init.args[3].fromInternalBalance,false);
   C.verifyBranch(poolContext(raw,b,p,original),mode+'/'+p.name,original,builds);
  }
  const local=raw.banks.filter(x=>x.namespace===mode);assert.equal(local.length,mode==='mixed'?35:37);eq(b.bank_ids,local.map(x=>x.id));
  for(const bank of local)validateBank(bank,b,ids,abis);
  const nativeCases=raw.cases.filter(x=>x.namespace===mode);assert.equal(nativeCases.length,26);assert.equal(new Set(nativeCases.map(c=>c.id)).size,26);assert(nativeCases.every(c=>c.id.startsWith(mode+'/')),'case namespace identity');
  for(const c of nativeCases)validateCase(c,raw,b,txs,banks,abis);
  validateControlTimeline(raw,b,txs,banks,local);
  const cps=raw.checkpoints.filter(c=>c.namespace===mode);eq(cps.map(c=>c.label),['start-1','start','start+1','midpoint','end-1','end']);
  for(const cp of cps){const bank=banks.get(cp.bank);assert(bank&&bank.namespace===mode);assert.equal(cp.timestamp,Number(BigInt(bank.header.timestamp)));}
  for(const o of raw.orders.filter(x=>x.namespace===mode)){const bank=banks.get(o.source_bank);assert(bank&&bank.namespace===mode);eq([o.token_in,o.token_out,o.caller,o.recipient],[b.tokens[0],b.tokens[1],b.roles.trader,b.roles.trader]);assert.equal(o.amount_in,String(5000n*10n**18n));assert(BigInt(bank.global_wallets[0].balances[o.caller])>=BigInt(o.amount_in));}
 }
 return {banks:72,physical_states:144,swaps:48,collections:4,checkpoints:12,funded_orders:12};
}
// Only check() qualifies a producer record; checkObservations validates the
// retained data graph and original authority without blessing new source bytes.
function check(raw){eq(raw.inputs,I.closure(),'complete new and inherited original input authority');return checkObservations(raw);}
module.exports={check,checkObservations,poolContext,validateBank,validateCase};
