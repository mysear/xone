'use strict';
const assert=require('node:assert/strict');
const I=require('./inputs.cjs'),{Runtime,eq,num,MAX}=require('./runtime.cjs');
function pool(r,name='A'){return r.h.pools.find(p=>p.name===name);}
function state(r,name='A'){return r.current.pools.find(p=>p.name===name).state;}
async function run(namespace){
 const r=new Runtime(namespace);
 try{
  await r.setup();const t0=num(r.current.header.timestamp);
  if(namespace==='settled'){
   await r.tick(t0+3600,'one-hour-nonzero-AUM');
   for(const p of r.h.pools)for(let i=0;i<3;i++)await r.pair(p.name,i,'settled:'+p.name+':asset-'+i);
  }else if(namespace==='scheduled'){
   const start=t0+60,end=start+1001,targets={A:['100000000000000000','800000000000000000','100000000000000000'],B:['600000000000000000','100000000000000000','300000000000000000']};
   for(const p of r.h.pools){
    await r.control(p.name,'updateWeightsGradually',[start,end,r.h.tokenAddresses,targets[p.name]],'schedule-weights-'+p.name);
    await r.control(p.name,'updateSwapFeeGradually',[start,end,'10000000000000000',p.name==='A'?'30000000000000000':'25000000000000000'],'schedule-fee-'+p.name);
   }
   for(const [label,time]of [['start-1',start-1],['start',start],['start+1',start+1],['midpoint',start+500],['end-1',end-1],['end',end]]){
    const checkpoint=await r.tick(time,label);r.checkpoints.push({id:namespace+'/CP'+r.checkpoints.length,namespace,label,bank:checkpoint.id,timestamp:time});
    for(const p of r.h.pools)for(let i=0;i<3;i++)await r.pair(p.name,i,label+':'+p.name+':asset-'+i);
   }
  }else if(namespace==='controls'){
   const p=pool(r),token=r.h.tokenAddresses[0],other=r.h.tokenAddresses[1];
   await r.tick(t0+3600,'control-seed-AUM');
   const seed=await r.swap('seed-trader-BPT','A',token,p.pool,'1000000000000000000',3,1),exitAmount=BigInt(seed.expected_native.actual_amount_out)/10n;
   await r.tick(t0+3660,'pending-before-disabled-join-exit');
   await r.control('A','setJoinExitEnabled',[false],'disable-join-exit');
   await r.swap('join-disabled','A',token,p.pool,'1000000000000000000',3,0);
   await r.swap('exit-disabled','A',p.pool,other,exitAmount,3,0);
   await r.swap('regular-with-join-disabled','A',token,other,'1000000000000000000',3,1);
   await r.control('A','setJoinExitEnabled',[true],'enable-join-exit');
   await r.control('A','setSwapEnabled',[false],'disable-all-swaps');
   await r.swap('join-swaps-disabled','A',token,p.pool,'1000000000000000000',3,0);
   await r.swap('exit-swaps-disabled','A',p.pool,other,exitAmount,3,0);
   await r.swap('regular-swaps-disabled','A',token,other,'1000000000000000000',3,0);
   await r.control('A','setSwapEnabled',[true],'enable-all-swaps');
   await r.control('A','setMustAllowlistLPs',[true],'require-LP-allowlist');
   await r.swap('join-not-allowlisted','A',token,p.pool,'1000000000000000000',3,0);
   await r.swap('exit-with-allowlist','A',p.pool,other,exitAmount,3,1);
   await r.swap('exit-LP-with-allowlist','A',p.pool,other,'1000000000000000000',2,1);
   await r.swap('regular-with-allowlist','A',token,other,'1000000000000000000',3,1);
   await r.control('A','addAllowedAddress',[r.h.roles.trader],'allow-trader');
   await r.swap('join-allowlisted','A',token,p.pool,'1000000000000000000',3,1);
   await r.control('A','removeAllowedAddress',[r.h.roles.trader],'remove-trader');
   await r.swap('join-after-remove-allowlist','A',token,p.pool,'1000000000000000000',3,0);
   await r.control('A','setMustAllowlistLPs',[false],'disable-LP-allowlist');
   await r.tick(t0+3720,'before-real-circuit-observations');
   await r.control('A','setCircuitBreakers',[[token],['1500000000000000000'],['990000000000000000'],['2000000000000000000']],'set-real-circuit-bound');
   await r.swap('circuit-small-join','A',token,p.pool,'1000000000000000000',3,1);
   await r.tick(t0+3780,'pending-before-tripped-circuit');
   await r.swap('circuit-large-join','A',token,p.pool,'1000000000000000000000',3,0);
  }else if(namespace==='limits'){
   const p=pool(r),tokens=r.h.tokenAddresses;
   await r.tick(t0+3600,'boundary-nonzero-AUM');
   await r.swap('zero-underlying-input','A',tokens[0],p.pool,'0',3,0);
   await r.swap('zero-BPT-input','A',p.pool,tokens[1],'0',3,0);
   await r.swap('minimum-18-join','A',tokens[0],p.pool,'1',3,1);
   await r.swap('minimum-6-join','A',tokens[1],p.pool,'1',3,1);
   await r.swap('minimum-8-join','A',tokens[2],p.pool,'1',3,1);
   await r.swap('minimum-BPT-exit','A',p.pool,tokens[1],'1',3,1);
   for(let i=0;i<3;i++){
    let supply=BigInt(state(r).aum.actual_supply);
    await r.swap('BPT-exit-30pct-asset-'+i,'A',p.pool,tokens[i],supply*30n/100n,2,1);
    supply=BigInt(state(r).aum.actual_supply);
    await r.swap('BPT-exit-30pct-plus-one-asset-'+i,'A',p.pool,tokens[i],supply*30n/100n+1n,2,null);
    supply=BigInt(state(r).aum.actual_supply);
    await r.swap('BPT-exit-31pct-asset-'+i,'A',p.pool,tokens[i],supply*31n/100n,2,0);
   }
   await r.swap('BPT-uint256-max-input','A',p.pool,tokens[0],MAX.toString(),2,0);
   await r.swap('BPT-above-actual-supply','A',p.pool,tokens[0],BigInt(state(r).aum.actual_supply)+1n,2,0);
  }
  return {metadata:r.branch(),banks:r.h.banks,cases:r.cases,controls:r.controls,checkpoints:r.checkpoints};
 }finally{await r.close();}
}
async function generate(){
 const inputs=I.closure(),original=I.originalBank(),branches={},banks=[],cases=[],controls=[],checkpoints=[];
 for(const name of ['settled','scheduled','controls','limits']){const b=await run(name);branches[name]=b.metadata;banks.push(...b.banks);cases.push(...b.cases);controls.push(...b.controls);checkpoints.push(...b.checkpoints);}
 const raw={schema_version:1,profile:'balancer-v2-managed-bpt-original-native',authority:'independent-local-original-managed-v2-bpt',production_ready:false,all_requested_features_complete:false,public_bpt_profile_candidate:'balancer-v2/20230411-managed-bpt-1 (not yet integrated)',
  inputs,original_reference:{source_manifest:I.sourceManifest(),source_pins:I.A.load().pins,source_pins_sha256:original.reference.source_pins_sha256,compiler_jobs:original.reference.compiler_jobs,provenance:require('../balancer-managed-v2-scheduled/generate.cjs').provenance()},
  branches,banks,cases,controls,checkpoints,
  coverage:{namespaces:Object.keys(branches),banks:banks.length,swap_attempts:cases.length,successful_swaps:cases.filter(c=>c.expected_native.status===1).length,reverted_swaps:cases.filter(c=>c.expected_native.status===0).length,BPT_join_attempts:cases.filter(c=>c.operation==='bpt_join').length,BPT_exit_attempts:cases.filter(c=>c.operation==='bpt_exit').length,clock_checkpoints:checkpoints.length},
  limitations:['Finite original 20230411 local chain31337 Shanghai source observations, not live-chain/trie/public execution proof.',
   'Underlying and BPT outputs are original wallet deltas and Vault Swap events; no Go/Rust pricing oracle.',
   'joinExit/swap/allowlist/circuit boundaries are actual native outcomes; not a claim that current local profiles implement them.',
   'Same physical Pool owns underlying/BPT supply, registration and actual wallet state; BPT outputs are not extra minting.',
   'GIVEN_OUT/ExactOut, enabled-breaker local math, full mixed/rates/other versions and complete design admission remain separate requirements.']};
 require('./checker.cjs').check(raw);eq(I.closure(),inputs,'original producer sources preserved');return raw;
}
module.exports={generate,run};
