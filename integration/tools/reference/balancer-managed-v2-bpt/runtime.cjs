'use strict';
const assert=require('node:assert/strict');
const I=require('./inputs.cjs'),{E,A,V,sha}=I,{ethers}=E.rr('ethers');
const {Harness,clean,num,eq,MAX}=require('../balancer-managed-v2-scheduled-shared/harness.cjs');
const H=require('../balancer-managed-v2-scheduled/helpers.cjs');
class Runtime {
 constructor(namespace){this.namespace=namespace;this.h=new Harness('active');this.cases=[];this.controls=[];this.checkpoints=[];this.current=null;}
 async setup(){
  await this.h.replay();await this.h.setup();this.h.mode=this.namespace;
  for(const p of this.h.pools)for(const signer of [2,3])await this.h.send('approve-BPT-'+p.name+'-'+signer,this.h.contracts.get(p.pool),'approve',[this.h.vault.address,MAX],signer);
  this.slots={};for(const name of ['_circuitBreakerState','_allowedAddresses']){const field=this.h.layout.variables.find(v=>v.name===name);assert(field,'original AST mapping '+name);this.slots[name]=field.slot;}
  assert.equal(this.slots._circuitBreakerState,12);assert.equal(this.slots._allowedAddresses,13);
  await this.capture('initialized-with-BPT-approvals');
 }
 mapping(address,slot){return ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['address','uint256'],[address,slot]));}
 async capture(label){
  const h=this.h,bank=await h.capture(label),collector=bank.pools[0].state.getters.find(g=>g.to===h.vault.address.toLowerCase()&&g.method==='getProtocolFeesCollector').decoded[0],holders=[...new Set([...Object.values(h.roles),collector,ethers.constants.AddressZero,h.vault.address.toLowerCase(),...h.pools.map(p=>p.pool)])];
  bank.bpt_wallets=[];bank.actor_allowlists=[];bank.control_storage=[];
  for(const p of h.pools){
   const pool=h.contracts.get(p.pool),state=bank.pools.find(x=>x.name===p.name).state,balances={},allowances={},members={};
   for(const who of holders)balances[who]=(await h.call(pool,'balanceOf',[who],bank.header,state.getters))[0];
   for(const who of Object.values(h.roles)){
    allowances[who]=(await h.call(pool,'allowance',[who,h.vault.address],bank.header,state.getters))[0];
    members[who]=(await h.call(pool,'isAddressOnAllowlist',[who],bank.header,state.getters))[0];
    const key=this.mapping(who,this.slots._allowedAddresses),value=await h.rpc('eth_getStorageAt',[p.pool,key,bank.header.number]);
    bank.control_storage.push({address:p.pool,kind:'allowlist',member:who,slot:this.slots._allowedAddresses,key,value});
   }
   for(const token of h.tokenAddresses){const key=this.mapping(token,this.slots._circuitBreakerState),value=await h.rpc('eth_getStorageAt',[p.pool,key,bank.header.number]);bank.control_storage.push({address:p.pool,kind:'circuit',token,slot:this.slots._circuitBreakerState,key,value});}
   bank.bpt_wallets.push({pool_name:p.name,token:p.pool,balances,allowances});
   bank.actor_allowlists.push({pool_name:p.name,enabled:state.getters.find(g=>g.to===p.pool&&g.method==='getMustAllowlistLPs').decoded[0],members});
  }
  bank.storage.push(...bank.control_storage.map(({address,key,value})=>({address,key,value})));
  const anchor=await h.header(bank.header.number);eq(anchor,bank.header,'BPT/control read-after source anchor');bank.header_anchor_after=anchor;
  for(const p of bank.pools)p.state.header_anchor_after=anchor;
  this.current=bank;return bank;
 }
 wallet(bank,asset,holder){
  const bpt=bank.bpt_wallets.find(w=>w.token===asset);if(bpt){assert(Object.hasOwn(bpt.balances,holder));return BigInt(bpt.balances[holder]);}
  const token=bank.global_wallets.find(w=>w.token===asset);assert(token&&Object.hasOwn(token.balances,holder));return BigInt(token.balances[holder]);
 }
 async tick(timestamp,label){await this.h.clock(timestamp,label);return this.capture(label);}
 async control(poolName,method,args,label){
  const p=this.h.pools.find(p=>p.name===poolName),before=this.current,t=await this.h.send(label||method,this.h.contracts.get(p.pool),method,args,1),after=await this.capture('after-control:'+label);
  this.controls.push({namespace:this.namespace,pool_name:poolName,method,args:clean(args),before:before.id,after:after.id,transaction:t.id});return t;
 }
 async swap(label,poolName,assetIn,assetOut,amount,signer=3,expected=null){
  const h=this.h,p=h.pools.find(p=>p.name===poolName),before=this.current,who=h.roles[['admin','manager','lp','trader'][signer]];
  assert(p);const args=[{poolId:p.pool_id,kind:0,assetIn,assetOut,amount:String(amount),userData:'0x'},
   {sender:who,recipient:who,fromInternalBalance:false,toInternalBalance:false},0,MAX];
  const pre=await h.header();eq(pre,before.header,'every original swap has captured complete prestate');
  const data=h.vault.interface.encodeFunctionData('swap',args);let preflight;
  try{preflight={result:await h.rpc('eth_call',[{from:who,to:h.vault.address.toLowerCase(),data,gas:ethers.utils.hexValue(3000000),gasPrice:'0x77359400'},pre.number])};}catch(e){preflight={error:e.message,data:typeof e.data==='string'?e.data:e.data?.result};}
  const signed=await h.wallets[signer].signTransaction({type:0,chainId:31337,nonce:num(await h.rpc('eth_getTransactionCount',[who,'latest'])),to:h.vault.address,data,gasLimit:3000000,gasPrice:2000000000});
  const hash=await h.rpc('eth_sendRawTransaction',[signed]),receipt=await h.rpc('eth_getTransactionReceipt',[hash]),transaction=await h.rpc('eth_getTransactionByHash',[hash]),header=await h.header();
  assert.equal(hash,ethers.utils.keccak256(signed));assert.equal(header.parentHash,pre.hash);assert.equal(header.timestamp,pre.timestamp);
  const status=num(receipt.status);if(expected!==null)assert.equal(status,expected,label+' unexpected original outcome: '+JSON.stringify(preflight));
  const t={id:'T'+h.transactions.length,purpose:label,to:h.vault.address.toLowerCase(),method:'swap',args:clean(args),signed_transaction:signed,transaction,receipt,pre_header:pre,execution_header:header,call_observation:preflight,events:h.events(receipt)};h.transactions.push(t);
  const after=await this.capture('after:'+label),input=this.wallet(before,assetIn,who)-this.wallet(after,assetIn,who),output=this.wallet(after,assetOut,who)-this.wallet(before,assetOut,who);
  const operation=assetOut===p.pool?'bpt_join':assetIn===p.pool?'bpt_exit':'token_swap';
  const x=before.pools.find(x=>x.name===poolName).state.aum,y=after.pools.find(x=>x.name===poolName).state.aum;
  let event=null;
  if(status===1){
   assert.equal(input,BigInt(amount),'actual original sender debit');assert(output>=0n);
   const rows=receipt.logs.filter(l=>l.address===h.vault.address.toLowerCase()).map(l=>{try{return h.vault.interface.parseLog(l);}catch{return null;}}).filter(e=>e?.name==='Swap');assert.equal(rows.length,1);const e=rows[0].args;
   event={pool_id:e.poolId.toLowerCase(),token_in:e.tokenIn.toLowerCase(),token_out:e.tokenOut.toLowerCase(),amount_in:e.amountIn.toString(),amount_out:e.amountOut.toString()};
   eq(event,{pool_id:p.pool_id,token_in:assetIn,token_out:assetOut,amount_in:String(input),amount_out:String(output)},'actual original event and ERC20 wallets');
  }else{assert.equal(status,0);assert.equal(input,0n);assert.equal(output,0n);eq(receipt.logs,[],'actual reverted swap retains no events');}
  const c={id:this.namespace+'/C'+this.cases.length,namespace:this.namespace,label,operation,pool_name:poolName,actor:who,token_in:assetIn,token_out:assetOut,amount_in:String(amount),before:before.id,after:after.id,transaction:t.id,
   expected_native:{status,actual_amount_in:String(input),actual_amount_out:String(output),swap_event:event,aum_minted_bpt:String(BigInt(y.total_supply)-BigInt(x.total_supply)),manager_bpt_delta:String(BigInt(y.manager_bpt)-BigInt(x.manager_bpt)),collector_bpt_delta:String(BigInt(y.collector_bpt)-BigInt(x.collector_bpt)),authority:'actual original ERC20 wallets, BPT supply and Vault Swap receipt; no local pricing model'}};
  this.cases.push(c);return c;
 }
 async pair(poolName,index,label){const p=this.h.pools.find(p=>p.name===poolName),input=10n**BigInt(this.h.decimals[index]);const join=await this.swap(label+':join',poolName,this.h.tokenAddresses[index],p.pool,input,3,1);const output=BigInt(join.expected_native.actual_amount_out);assert(output>1n);return this.swap(label+':exit',poolName,p.pool,this.h.tokenAddresses[index],output/2n,3,1);}
 branch(){return {...this.h.branch(),namespace:this.namespace,control_mapping_slots:this.slots};}
 async close(){return this.h.close();}
}
module.exports={Runtime,clean,num,eq,MAX};
