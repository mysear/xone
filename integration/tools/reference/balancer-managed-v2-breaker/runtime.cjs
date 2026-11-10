'use strict';
// Original bytecode only. Extra pure-library calls preserve independent
// cache-miss ratio observations; no local swap or power oracle executes here.
const assert=require('node:assert/strict');
const I=require('./inputs.cjs'),{Runtime,clean,eq,num,MAX}=require('../balancer-managed-v2-bpt/runtime.cjs');
const {ethers}=I.E.rr('ethers');
const PROFILE='balancer-v2-managed-breaker';
class BreakerRuntime extends Runtime {
 async setup(){await super.setup();for(const p of this.h.pools)p.requested_protocol=PROFILE;}
 async capture(label){
  const bank=await super.capture(label),h=this.h;
  const library=h.byLabel('CircuitBreakerLib');
  bank.circuit_math_calls=[];
  for(const p of h.pools){
   const state=bank.pools.find(row=>row.name===p.name).state;
   for(let index=0;index<h.tokenAddresses.length;index++){
    const token=h.tokenAddresses[index],stored=bank.control_storage.find(s=>s.address===p.pool&&s.kind==='circuit'&&s.token===token);
    assert(stored,'complete original circuit storage');const word=BigInt(stored.value==='0x'?'0x0':stored.value),weight=BigInt(state.weights[index]),reference=(word>>32n)&((1n<<64n)-1n);
    for(const lower of [true,false]){
     const bound=((word>>(lower?0n:16n))&65535n)<<48n;
     if(bound===0n||weight===reference)continue;
     const ordinal=state.getters.length;
     const value=await h.call(library,'calcAdjustedBound',[String(bound),String(weight),lower],bank.header,state.getters);
     bank.circuit_math_calls.push({pool_name:p.name,token,is_lower:lower,getter_ordinal:ordinal,ratio:value[0]});
    }
   }
  }
  const anchor=await h.header(bank.header.number);eq(anchor,bank.header,'breaker pure calls retain source H');
  bank.header_anchor_after=anchor;for(const row of bank.pools)row.state.header_anchor_after=anchor;
  return bank;
 }
 currentPool(name='A'){return this.h.pools.find(p=>p.name===name);}
 state(name='A'){return this.current.pools.find(p=>p.name===name).state;}
 // Calibration is an explicit manager INPUT built from observed values. It is
 // never an expected swap result, and original setCircuitBreakers may round it.
 prices(name='A',multipliers=[1n,1n,1n]){
  const s=this.state(name),supply=BigInt(s.aum.actual_supply);
  return s.token_states.map((t,i)=>String(supply*BigInt(s.weights[i])*multipliers[i]/(BigInt(t.info[0])+BigInt(t.info[1]))));
 }
 async set(name,lower,upper,multipliers=[1n,1n,1n],label='set-breakers'){
  return this.control(name,'setCircuitBreakers',[this.h.tokenAddresses,this.prices(name,multipliers),this.h.tokenAddresses.map(()=>String(lower)),this.h.tokenAddresses.map(()=>String(upper))],label);
 }
 async controlAttempt(name,method,args,label,expected=0,signer=1){
  const h=this.h,p=this.currentPool(name),c=h.contracts.get(p.pool),before=this.current,who=h.roles[['admin','manager','lp','trader'][signer]],pre=await h.header();eq(pre,before.header);
  const data=c.interface.encodeFunctionData(method,args);let preflight;
  try{preflight={result:await h.rpc('eth_call',[{from:who,to:p.pool,data,gas:ethers.utils.hexValue(3000000),gasPrice:'0x77359400'},pre.number])};}catch(e){preflight={error:e.message,data:typeof e.data==='string'?e.data:e.data?.result};}
  const signed=await h.wallets[signer].signTransaction({type:0,chainId:31337,nonce:num(await h.rpc('eth_getTransactionCount',[who,'latest'])),to:p.pool,data,gasLimit:3000000,gasPrice:2000000000});
  const hash=await h.rpc('eth_sendRawTransaction',[signed]),receipt=await h.rpc('eth_getTransactionReceipt',[hash]),transaction=await h.rpc('eth_getTransactionByHash',[hash]),header=await h.header();
  assert.equal(hash,ethers.utils.keccak256(signed));assert.equal(num(receipt.status),expected,label);assert.equal(header.parentHash,pre.hash);assert.equal(header.timestamp,pre.timestamp);
  const t={id:'T'+h.transactions.length,purpose:label,to:p.pool,method,args:clean(args),signed_transaction:signed,transaction,receipt,pre_header:pre,execution_header:header,call_observation:preflight,events:h.events(receipt)};h.transactions.push(t);
  const after=await this.capture('after:'+label);this.controls.push({namespace:this.namespace,pool_name:name,method,args:clean(args),actor:who,before:before.id,after:after.id,transaction:t.id,expected_native:{status:expected}});return t;
 }
}
module.exports={BreakerRuntime,PROFILE,clean,eq,num,MAX};
