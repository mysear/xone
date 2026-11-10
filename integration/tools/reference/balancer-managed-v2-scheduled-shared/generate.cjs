'use strict';
const assert=require('node:assert/strict');
const I=require('./inputs.cjs'),{Harness,eq,num,MAX}=require('./harness.cjs');
function wallet(bank,token,owner){const row=bank.global_wallets.find(w=>w.token===token);assert(row&&Object.hasOwn(row.balances,owner),'observed native wallet');return BigInt(row.balances[owner]);}
async function branch(mode){
 const h=new Harness(mode),cases=[],checkpoints=[],management=[],orders=[];
 try{
  await h.replay();await h.setup();let current=await h.capture('both-pools-initialized');
  const now=num(current.header.timestamp),start=now+60,end=start+1001;
  const targets={A:['100000000000000000','800000000000000000','100000000000000000'],B:['600000000000000000','100000000000000000','300000000000000000']};
  for(const p of h.pools){
   if(p.name==='B'&&mode==='mixed')continue;
   const pool=h.contracts.get(p.pool);
   for(const [method,args]of [['updateWeightsGradually',[start,end,h.tokenAddresses,targets[p.name]]],['updateSwapFeeGradually',[start,end,'10000000000000000',p.name==='A'?'30000000000000000':'25000000000000000']]]){
    const before=current,tx=await h.send(mode+':'+p.name+':'+method,pool,method,args,1);current=await h.capture(p.name+':after-'+method);
    management.push({pool_name:p.name,method,before:before.id,after:current.id,transaction:tx.id});
   }
  }
  const points=[['start-1',start-1],['start',start],['start+1',start+1],['midpoint',start+500],['end-1',end-1],['end',end]];
  for(let index=0;index<points.length;index++){
   const [label,time]=points[index];await h.clock(time,label);current=await h.capture(label+':common-checkpoint');
   checkpoints.push({id:mode+'/CP'+index,namespace:mode,label,bank:current.id,timestamp:time});
   const order=String(5000n*10n**BigInt(h.decimals[0]));assert(wallet(current,h.tokenAddresses[0],h.roles.trader)>=BigInt(order),'native trader funds the API split budget');
   orders.push({id:mode+'/O'+index,namespace:mode,source_bank:current.id,token_in:h.tokenAddresses[0],token_out:h.tokenAddresses[1],amount_in:order,caller:h.roles.trader,recipient:h.roles.trader,authority:'actual funded sender wallet; no expected swap amount or local math generated'});
   for(const [poolName,i,j]of [['A',0,1],['B',0,1],['A',1,0],['B',1,0]]){
    const p=h.pools.find(p=>p.name===poolName),before=current,amount=String(10n**BigInt(h.decimals[i])+BigInt(index+1));
    const tx=await h.send(mode+':'+label+':'+poolName+':swap-'+i+'-'+j,h.vault,'swap',[
     {poolId:p.pool_id,kind:0,assetIn:h.tokenAddresses[i],assetOut:h.tokenAddresses[j],amount,userData:'0x'},
     {sender:h.roles.trader,recipient:h.roles.trader,fromInternalBalance:false,toInternalBalance:false},0,MAX],3);
    current=await h.capture(label+':after-'+poolName+'-'+i+'-'+j);
    const input=String(wallet(before,h.tokenAddresses[i],h.roles.trader)-wallet(current,h.tokenAddresses[i],h.roles.trader));
    const output=String(wallet(current,h.tokenAddresses[j],h.roles.trader)-wallet(before,h.tokenAddresses[j],h.roles.trader));
    assert.equal(input,amount);assert(BigInt(output)>0n,'native positive recipient wallet delta');
    const events=tx.receipt.logs.filter(l=>l.address===h.vault.address.toLowerCase()).map(l=>{try{return h.vault.interface.parseLog(l);}catch{return null;}}).filter(e=>e?.name==='Swap');assert.equal(events.length,1);
    const e=events[0].args,swapEvent={pool_id:e.poolId.toLowerCase(),token_in:e.tokenIn.toLowerCase(),token_out:e.tokenOut.toLowerCase(),amount_in:e.amountIn.toString(),amount_out:e.amountOut.toString()};
    eq(swapEvent,{pool_id:p.pool_id,token_in:h.tokenAddresses[i],token_out:h.tokenAddresses[j],amount_in:amount,amount_out:output},'actual original Vault event and wallet credit');
    cases.push({id:mode+'/C'+cases.length,namespace:mode,operation:'swap',pool_name:poolName,before:before.id,after:current.id,transaction:tx.id,index_in:i,index_out:j,token_in:h.tokenAddresses[i],token_out:h.tokenAddresses[j],amount_in:amount,
     expected_native:{status:1,actual_amount_in:input,actual_amount_out:output,swap_event:swapEvent,authority:'actual original ERC20 sender debit/recipient credit and original Vault Swap receipt'}});
   }
  }
  for(const p of h.pools){
   const before=current,b=before.pools.find(x=>x.name===p.name).state.aum;
   assert(BigInt(b.pending_aum)>0n,'nonzero original AUM available');
   const tx=await h.send(mode+':'+p.name+':collect',h.contracts.get(p.pool),'collectAumManagementFees',[],3);current=await h.capture(p.name+':after-original-aum-collection');
   const a=current.pools.find(x=>x.name===p.name).state.aum,minted=BigInt(a.total_supply)-BigInt(b.total_supply),manager=BigInt(a.manager_bpt)-BigInt(b.manager_bpt),collector=BigInt(a.collector_bpt)-BigInt(b.collector_bpt);
   assert(minted>0n);assert.equal(minted,manager+collector);assert.equal(a.pending_aum,'0');
   cases.push({id:mode+'/C'+cases.length,namespace:mode,operation:'collect',pool_name:p.name,before:before.id,after:current.id,transaction:tx.id,
    expected_native:{status:1,minted_bpt:String(minted),manager_bpt_delta:String(manager),collector_bpt_delta:String(collector),authority:'actual original BPT totalSupply and owner/collector wallet differences'}});
  }
  return {metadata:h.branch(),banks:h.banks,cases,checkpoints,management,orders};
 }finally{await h.close();}
}
async function generate(){
 const inputs=I.closure(),original=I.originalBank(),branches={},banks=[],cases=[],checkpoints=[],management=[],orders=[];
 for(const mode of ['mixed','active']){
  const b=await branch(mode);branches[mode]=b.metadata;banks.push(...b.banks);cases.push(...b.cases);checkpoints.push(...b.checkpoints);management.push(...b.management.map(x=>({...x,namespace:mode})));orders.push(...b.orders);
 }
 const result={schema_version:1,profile:'balancer-v2-managed-scheduled-shared-native',authority:'independent-local-original-managed-scheduled-shared',production_ready:false,all_requested_features_complete:false,
  inputs,original_reference:{source_manifest:I.sourceManifest(),source_pins:I.A.load().pins,source_pins_sha256:original.reference.source_pins_sha256,compiler_jobs:original.reference.compiler_jobs,provenance:require('../balancer-managed-v2-scheduled/generate.cjs').provenance()},
  branches,banks,cases,checkpoints,management,orders,
  coverage:{modes:['mixed','active'],physical_pools_per_mode:2,shared_underlyings:3,decimals:[18,6,8],native_swap_cases:48,native_collection_cases:4,common_header_banks:72,clock_checkpoints:12,funded_split_budgets:12},
  limitations:['Finite January 2026 local original-bytecode observations; no live-chain or storage-trie proof.',
   'Mixed mode explicitly requests one scheduled and one old static aum profile; active mode requests both scheduled. No implicit legacy promotion.',
   'Native pool registration, global underlying wallets and per-physical-pool BPT supplies remain separate.',
   'No Go/Rust or SDK price outputs used as expected native swap values.',
   'API-selected split execution, parser/domain/math consumers and process gates require separate evidence; not claimed by this raw reference.',
   'BPT routing, rate/mixed unsupported accounting, circuit breakers, public VERIFIED/build and protocol-wide acceptance remain unfinished.']};
 require('./checker.cjs').check(result);eq(I.closure(),inputs,'producer input closure preserved');return result;
}
module.exports={generate,branch};
