'use strict';
const assert=require('node:assert/strict');
const I=require('./inputs.cjs'),{BreakerRuntime,eq,num}=require('./runtime.cjs');
const ONE=10n**18n;
function rejection(t){const p=t.call_observation||{};if(typeof p.data==='string'&&p.data.startsWith('0x08c379a0')){try{return I.E.rr('ethers').ethers.utils.defaultAbiCoder.decode(['string'],'0x'+p.data.slice(10))[0];}catch{}}
 return typeof p.error==='string'&&p.error.includes('BAL#356')?'BAL#356':null;}
async function rejectedSwap(r,label,name,tokenIn,tokenOut,amount,signer=3){
 const actor=r.h.roles[['admin','manager','lp','trader'][signer]];assert(r.wallet(r.current,tokenIn,actor)>=BigInt(amount),label+' original prestate funds must cover the input');
 const c=await r.swap(label,name,tokenIn,tokenOut,amount,signer,0),t=r.h.transactions.find(t=>t.id===c.transaction);
 assert.equal(rejection(t),'BAL#356',label+' must fail for the original circuit breaker, not capacity/funds');c.expected_native.rejection_reason='BAL#356';return c;
}
async function checkpoint(r,time,label){const bank=await r.tick(time,label);r.checkpoints.push({id:r.namespace+'/CP'+r.checkpoints.length,namespace:r.namespace,label,bank:bank.id,timestamp:time});return bank;}
async function smallMatrix(r,label){
 for(const p of r.h.pools){for(let i=0;i<3;i++)await r.pair(p.name,i,label+':'+p.name+':asset-'+i);
  for(const [i,j]of [[0,1],[1,0]])await r.swap(label+':'+p.name+':regular-'+i+'-'+j,p.name,r.h.tokenAddresses[i],r.h.tokenAddresses[j],10n**BigInt(r.h.decimals[i]),3,1);
 }
}
async function run(namespace){
 const r=new BreakerRuntime(namespace);
 try{
  await r.setup();const start=num(r.current.header.timestamp),tokens=r.h.tokenAddresses,p=r.currentPool('A');r.orders=[];
  if(namespace==='cached'){
   await checkpoint(r,start+3600,'pending-AUM-before-cached-bounds');
   for(const q of r.h.pools)await r.set(q.name,9n*ONE/10n,11n*ONE/10n,[1n,1n,1n],'cached-bounds-'+q.name);
   await r.swap('zero-underlying-input','A',tokens[0],p.pool,0n,3,0);
   await r.swap('zero-BPT-input','A',p.pool,tokens[0],0n,3,0);
   for(let i=0;i<3;i++)await r.swap('minimum-underlying-'+i,'A',tokens[i],p.pool,1n,3,1);
   await r.swap('minimum-BPT-input','A',p.pool,tokens[1],1n,3,1);
   await smallMatrix(r,'cached-original');
   await rejectedSwap(r,'cached-large-join','A',tokens[0],p.pool,4000n*ONE);
   await rejectedSwap(r,'cached-large-exit','A',p.pool,tokens[1],BigInt(r.state().aum.actual_supply)/5n,2);
  }else if(namespace==='dynamic'){
   await checkpoint(r,start+3600,'pending-AUM-before-active-weight-breakers');
   await r.set('A',ONE/10n,10n*ONE,[1n,2n,1n],'dynamic-reference-A');
   await r.set('B',ONE/10n,10n*ONE,[2n,1n,1n],'dynamic-reference-B');
   const a=start+3660,z=a+1001;
   for(const [name,weights]of [['A',['100000000000000000','800000000000000000','100000000000000000']],['B',['600000000000000000','100000000000000000','300000000000000000']]])await r.control(name,'updateWeightsGradually',[a,z,tokens,weights],'first-schedule-'+name);
   for(const [label,time]of [['start-1',a-1],['start',a],['start+1',a+1]]){await checkpoint(r,time,label);await smallMatrix(r,label);}
   const b=a+11,end=a+510;
   for(const [name,weights]of [['A',['600000000000000000','100000000000000000','300000000000000000']],['B',['100000000000000000','700000000000000000','200000000000000000']]])await r.control(name,'updateWeightsGradually',[b,end,tokens,weights],'reschedule-computed-start-'+name);
   for(const [name,fee]of [['A','30000000000000000'],['B','25000000000000000']])await r.control(name,'updateSwapFeeGradually',[b,end,'10000000000000000',fee],'active-fee-'+name);
   for(const [label,time]of [['restart-1',b-1],['restart',b],['restart+1',b+1],['end-1',end-1],['end',end],['end+1',end+1]]){await checkpoint(r,time,label);await smallMatrix(r,label);}
  }else if(namespace==='boundaries'){
   await checkpoint(r,start+3600,'boundary-pending-AUM');
   for(let i=0;i<3;i++){
    const token=tokens[i],unit=10n**BigInt(r.h.decimals[i]);let price=r.prices()[i];
    if(i>0)price=String(BigInt(price)+1n); // Original downscale discards manager-input dust.
    await r.control('A','setCircuitBreakers',[[token],[price],['990000000000000000'],['2000000000000000000']],'narrow-lower-asset-'+i);
    await r.pair('A',i,'narrow-small-asset-'+i);
    await rejectedSwap(r,'lower-tripped-asset-'+i,'A',token,p.pool,4000n*unit);
    await r.control('A','setCircuitBreakers',[[token],r.prices().slice(i,i+1),['0'],['0']],'both-zero-disabled-asset-'+i);
    await r.swap('disabled-large-join-asset-'+i,'A',token,p.pool,4000n*unit,3,1);
    await r.control('A','setCircuitBreakers',[[token],r.prices().slice(i,i+1),['0'],['1010000000000000000']],'upper-only-asset-'+i);
    await r.swap('upper-small-exit-asset-'+i,'A',p.pool,token,ONE,2,1);
    await rejectedSwap(r,'upper-tripped-exit-asset-'+i,'A',p.pool,token,BigInt(r.state().aum.actual_supply)/5n,2);
    await r.control('A','setCircuitBreakers',[[token],['0'],['990000000000000000'],['2000000000000000000']],'zero-reference-price-asset-'+i);
    await r.pair('A',i,'zero-price-disabled-asset-'+i);
    await r.control('A','setCircuitBreakers',[[token],r.prices().slice(i,i+1),['0'],['1']],'upper-quantizes-to-zero-asset-'+i);
    await r.pair('A',i,'quantized-disabled-asset-'+i);
   }
   // Public exits create a legitimate tiny 6-decimal reserve. This is not
   // storage injection. The disabled controls then witness the SAME input
   // succeeding with zero raw output, while original scaled-bound checks trip.
   const dustPool=r.currentPool('B');
   await r.control('B','setCircuitBreakers',[[tokens[0]],r.prices('B').slice(0,1),['0'],['900000000000000000']],'legal-subunit-upper-currently-tripped');
   await rejectedSwap(r,'subunit-upper-small-join-trips','B',tokens[0],dustPool.pool,ONE,3);
   await r.swap('subunit-upper-large-join-recovers','B',tokens[0],dustPool.pool,1200n*ONE,3,1);
   for(let attempt=0;BigInt(r.state('B').token_states[1].info[0])>2n;attempt++){
    assert(attempt<36,'bounded original tiny-reserve preparation');
    const input=BigInt(r.state('B').aum.actual_supply)*3n/10n,wallet=r.current.bpt_wallets.find(w=>w.pool_name==='B').balances;
    const signer=BigInt(wallet[r.h.roles.lp])>=input?2:3;assert(BigInt(wallet[r.h.roles[signer===2?'lp':'trader']])>=input,'original funded tiny-reserve preparation actor');
    await r.swap('prepare-real-tiny-reserve-'+attempt,'B',dustPool.pool,tokens[1],input,signer,1);
   }
   assert(BigInt(r.state('B').token_states[1].info[0])<=2n);
   let dustPrice=r.prices('B')[1];
   await r.control('B','setCircuitBreakers',[[tokens[1]],[dustPrice],['0'],['1010000000000000000']],'dust-BPT-upper-reference');
   const dustInput=BigInt(r.state('B').aum.actual_supply)/10n,dustWallet=r.current.bpt_wallets.find(w=>w.pool_name==='B').balances,dustSigner=BigInt(dustWallet[r.h.roles.lp])>=dustInput?2:3;
   assert(BigInt(dustWallet[r.h.roles[dustSigner===2?'lp':'trader']])>=dustInput,'funded dust exit actor');
   await rejectedSwap(r,'dust-BPT-exit-internal-bound-trips','B',dustPool.pool,tokens[1],dustInput,dustSigner);
   await r.control('B','setCircuitBreakers',[[tokens[1]],[dustPrice],['0'],['0']],'dust-BPT-disable-bound');
   const zeroExit=await r.swap('dust-BPT-exit-disabled-zero-credit','B',dustPool.pool,tokens[1],dustInput,dustSigner,1);assert.equal(zeroExit.expected_native.actual_amount_out,'0');
   dustPrice=r.prices('B')[1];
   await r.control('B','setCircuitBreakers',[[tokens[1]],[dustPrice],['0'],['1000010000000000000']],'dust-regular-upper-reference');
   await rejectedSwap(r,'dust-regular-internal-bound-trips','B',tokens[0],tokens[1],ONE,3);
   await r.control('B','setCircuitBreakers',[[tokens[1]],[dustPrice],['0'],['0']],'dust-regular-disable-bound');
   const zeroRegular=await r.swap('dust-regular-disabled-zero-credit','B',tokens[0],tokens[1],ONE,3,1);assert.equal(zeroRegular.expected_native.actual_amount_out,'0');
   const price=r.prices()[0];
   for(const [label,token,lower,upper]of [
    ['below-lower-minimum',tokens[0],String(ONE/10n-1n),String(2n*ONE)],
    ['lower-above-one',tokens[0],String(ONE+1n),String(2n*ONE)],
    ['upper-above-ten',tokens[0],'0',String(10n*ONE+1n)],
    ['upper-below-lower',tokens[0],String(9n*ONE/10n),String(8n*ONE/10n)],
    ['BPT-is-not-underlying',p.pool,'0',String(2n*ONE)],
   ])await r.controlAttempt('A','setCircuitBreakers',[[token],[price],[lower],[upper]],label,0);
   await r.controlAttempt('A','setCircuitBreakers',[[tokens[0]],['79228162514264337593543950336'],['0'],[String(2n*ONE)]],'stored-price-overflows-u96',0);
   await r.controlAttempt('A','setCircuitBreakers',[[tokens[0]],[price],['0'],[String(2n*ONE)]],'non-owner-cannot-set-breaker',0,3);
  }else if(namespace==='shared'){
   await checkpoint(r,start+3600,'shared-pending-AUM');
   for(const q of r.h.pools)await r.set(q.name,ONE/10n,10n*ONE,[1n,1n,1n],'shared-reference-'+q.name);
   const first=await r.swap('shared-A-join','A',tokens[0],p.pool,ONE,3,1);
   const exit=await r.swap('shared-A-exit','A',p.pool,tokens[1],BigInt(first.expected_native.actual_amount_out)/2n,3,1);
   const second=r.currentPool('B'),join=await r.swap('shared-B-join','B',tokens[1],second.pool,BigInt(exit.expected_native.actual_amount_out),3,1);
   await r.swap('shared-B-exit','B',second.pool,tokens[0],BigInt(join.expected_native.actual_amount_out)/2n,3,1);
   const source=r.current,amount=BigInt(r.state('A').aum.actual_supply)*4n/10n,parts=[amount/2n,amount-amount/2n],routes=[];
   for(const [index,input]of parts.entries()){
    const out=await r.swap('shared-capacity-A-exit-'+index,'A',p.pool,tokens[index],input,2,1);
    const into=await r.swap('shared-capacity-B-join-'+index,'B',tokens[index],second.pool,BigInt(out.expected_native.actual_amount_out),2,1);
    routes.push({index,token:tokens[index],amount_in:String(input),actual_amount_out:into.expected_native.actual_amount_out,cases:[out.id,into.id]});
   }
   const actor=r.h.roles.lp;r.orders.push({id:namespace+'/O0',namespace,source_bank:source.id,after:r.current.id,token_in:p.pool,token_out:second.pool,amount_in:String(amount),caller:actor,recipient:actor,reference_routes:routes,
    expected_native:{actual_amount_in:String(r.wallet(source,p.pool,actor)-r.wallet(r.current,p.pool,actor)),actual_amount_out:String(r.wallet(r.current,second.pool,actor)-r.wallet(source,second.pool,actor))},scope:'explicit original sequential witness; not an optimizer quote oracle'});
   assert.equal(r.orders[0].expected_native.actual_amount_in,String(amount));
  }else throw Error('unknown exact original scenario');
  return {branch:r.branch(),banks:r.h.banks,cases:r.cases,controls:r.controls,checkpoints:r.checkpoints,orders:r.orders};
 }finally{await r.close();}
}
async function generate(){
 const inputs=I.closure();I.originalBpt();const original=I.originalBank(),branches={},banks=[],cases=[],controls=[],checkpoints=[],orders=[];
 for(const namespace of ['cached','dynamic','boundaries','shared']){const r=await run(namespace);branches[namespace]=r.branch;banks.push(...r.banks);cases.push(...r.cases);controls.push(...r.controls);checkpoints.push(...r.checkpoints);orders.push(...r.orders);}
 const raw={schema_version:1,authority:'independent-local-original-managed-v2-breaker',profile:'published-20230411-managed-breaker-reference',production_ready:false,all_requested_features_complete:false,inputs,
  original_reference:{source_manifest:I.sourceManifest(),source_pins:I.A.load().pins,source_pins_sha256:original.reference.source_pins_sha256,compiler_jobs:original.reference.compiler_jobs,provenance:require('../balancer-managed-v2-scheduled/generate.cjs').provenance()},branches,banks,cases,controls,checkpoints,orders,
  limitations:['Historical owned original VM31337; no live-chain/trie/public atomic execution proof.','Expected swap amounts come only from original ERC20/BPT wallet deltas and Vault Swap receipts.','Extra original pure-library observations qualify cache-miss getter bindings; no local swap/power model generates expected amounts.','The shared sequential witness is an explicit funded input scope, not the optimizer output oracle.','Production breaker state/parser/math/router integration and all broader design acceptance remain separate.']};
 require('./checker.cjs').check(raw);eq(I.closure(),inputs,'breaker generation closure unchanged');return raw;
}
module.exports={generate,run,rejection};
