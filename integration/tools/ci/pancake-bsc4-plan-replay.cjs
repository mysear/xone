// SPDX-License-Identifier: GPL-2.0-or-later
// Replay selected actual API plans with unchanged original Pool/LM/MasterChef code.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../../..'),modules=path.join(root,'integration/.runtime/stable-reference/node_modules');
const ganache=require(path.join(modules,'ganache')),{ethers}=require(path.join(modules,'ethers'));
const sha=b=>crypto.createHash('sha256').update(b).digest('hex'),requireValue=(v,w)=>{if(!v)throw Error(w);};
function read(file){const size=fs.statSync(file).size;requireValue(size<=32*1024*1024,'PANCAKE_NATIVE_INPUT_BYTES');return fs.readFileSync(file);}
function parse(raw){return JSON.parse(raw.toString(),(k,v,c)=>{if(typeof v!=='number')return v;requireValue(typeof c?.source==='string'&&/^-?(?:0|[1-9][0-9]*)$/.test(c.source),'PANCAKE_NATIVE_JSON_INTEGER');const n=BigInt(c.source);return n>=BigInt(Number.MIN_SAFE_INTEGER)&&n<=BigInt(Number.MAX_SAFE_INTEGER)?Number(n):n;});}
function stable(v){if(typeof v==='bigint')return v.toString();if(Array.isArray(v))return '['+v.map(stable).join(',')+']';if(v&&typeof v==='object')return '{'+Object.keys(v).sort().map(k=>JSON.stringify(k)+':'+stable(v[k])).join(',')+'}';if(typeof v==='number')requireValue(Number.isSafeInteger(v),'PANCAKE_NATIVE_INTEGER');return JSON.stringify(v);}
function equal(a,b,why){requireValue(stable(a)===stable(b),why);}
const args=process.argv.slice(2);requireValue(args.length===4,'PANCAKE_NATIVE_ARGUMENTS');
const referenceRaw=read(args[0]),inputRaw=read(args[1]),modelRaw=read(args[2]);
const reference=parse(referenceRaw),input=parse(inputRaw),model=parse(modelRaw);
requireValue(reference.schema_version===1&&input.schema_version===1&&model.schema_version===1&&model.passed===true,'PANCAKE_NATIVE_SCHEMA');
requireValue(model.input_sha256===sha(inputRaw),'PANCAKE_NATIVE_INPUT_BINDING');requireValue(input.records.length>0&&input.records.length<=128&&model.records.length===input.records.length,'PANCAKE_NATIVE_RECORD_SCOPE');
const originalPath=fs.realpathSync(path.resolve(root,reference.original_reference.path));requireValue(originalPath.startsWith(path.join(root,'integration/artifacts/local')+path.sep)&&path.basename(originalPath).startsWith('core-pancake-bsc4-original'),'PANCAKE_NATIVE_REFERENCE_LOCALITY');requireValue(sha(read(originalPath))===reference.original_reference.sha256,'PANCAKE_NATIVE_ORIGINAL_DIGEST');const original=parse(read(originalPath));
for(const role of ['pool','lm','masterChef','factory','token0','token1','caller','first','second','third']){equal(reference.deployment[role],original.deployment[role],'PANCAKE_NATIVE_RUNTIME_BINDING:'+role);if(!['first','second','third'].includes(role))equal(reference.artifacts[role],original.artifacts[role],'PANCAKE_NATIVE_ARTIFACT_BINDING:'+role);}
const precursorPath=path.resolve(root,original.predecessor_source_reference.path);requireValue(sha(read(precursorPath))===original.predecessor_source_reference.sha256,'BSC4_PREDECESSOR_ARTIFACT_DIGEST');const precursor=JSON.parse(read(precursorPath));
for(const role of ['first','second','third'])original.artifacts[role]=precursor.contracts[role].artifact;
const dep=original.deployment;
const word=n=>'0x'+BigInt.asUintN(256,BigInt(n)).toString(16).padStart(64,'0');
const mapping=(key,slot,type='uint256')=>ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode([type,'uint256'],[String(key),String(slot)]));
const mask=bits=>(1n<<BigInt(bits))-1n;
(async()=>{
 const timestamp=Number(model.records[0].snapshot.reference.block.timestamp_ms);requireValue(Number.isSafeInteger(timestamp)&&timestamp===original.fixed_timestamp*1000,'PANCAKE_NATIVE_FIXED_CLOCK');
 const transport=ganache.provider({logging:{quiet:true},chain:{hardfork:'london',time:new Date(timestamp),allowUnlimitedContractSize:true},miner:{timestampIncrement:0,blockGasLimit:30000000},wallet:{deterministic:true}});
 const provider=new ethers.providers.Web3Provider(transport);provider.pollingInterval=20;
 try{
  const signer=provider.getSigner(0),sender=(await signer.getAddress()).toLowerCase();
  for(const role of ['pool','lm','masterChef','factory','token0','token1','caller','first','second','third'])await provider.send('evm_setAccountCode',[dep[role].address,dep[role].code]);
  const contracts=Object.fromEntries(['pool','lm','masterChef','factory','token0','token1','caller','first','second','third'].map(role=>[role,new ethers.Contract(dep[role].address,original.artifacts[role].abi,signer)]));
  let checkpoint=await provider.send('evm_snapshot',[]),count=0;const results=[];
  const label=(role,name)=>{const field=original.artifacts[role].storageLayout.storage.find(f=>f.label===name);requireValue(field,'PANCAKE_NATIVE_STORAGE:'+role+':'+name);return field;};
  const set=(role,slot,value)=>provider.send('evm_setAccountStorageAt',[dep[role].address,word(slot),word(value)]);
  async function fields(role,type,base,values){const layout=original.artifacts[role].storageLayout,definition=layout.types[type];requireValue(definition.members,'PANCAKE_NATIVE_STRUCT');const words=new Map();for(const [name,value]of Object.entries(values)){const member=definition.members.find(m=>m.label===name);requireValue(member,'PANCAKE_NATIVE_MEMBER:'+name);const bits=8*Number(layout.types[member.type].numberOfBytes),at=BigInt(base)+BigInt(member.slot),encoded=(BigInt(value)&mask(bits))<<BigInt(member.offset*8);words.set(at,(words.get(at)||0n)|encoded);}for(const [at,value]of words)await set(role,at,value);}
  async function roots(role,values){const layout=original.artifacts[role].storageLayout,words=new Map();for(const [name,value]of Object.entries(values)){const f=label(role,name),bits=8*Number(layout.types[f.type].numberOfBytes),at=BigInt(f.slot);words.set(at,(words.get(at)||0n)|((BigInt(value)&mask(bits))<<BigInt(f.offset*8)));}for(const [at,value]of words)await set(role,at,value);}
  async function initialize(bank){
   requireValue(await provider.send('evm_revert',[checkpoint]),'PANCAKE_NATIVE_RESET');checkpoint=await provider.send('evm_snapshot',[]);
   const s=bank.state,slot0=label('pool','slot0');await fields('pool',slot0.type,slot0.slot,s.slot0);
   await roots('pool',{liquidity:s.liquidity,feeGrowthGlobal0X128:s.feeGrowthGlobal0X128,feeGrowthGlobal1X128:s.feeGrowthGlobal1X128,lmPool:BigInt(dep.lm.address)});
   const protocol=label('pool','protocolFees');await fields('pool',protocol.type,protocol.slot,s.protocolFees);
   const ticks=label('pool','ticks'),tickType=original.artifacts.pool.storageLayout.types[ticks.type].value;
   for(const t of s.ticks){const {index,...values}=t;await fields('pool',tickType,mapping(index,ticks.slot,'int24'),values);}
   const bitmap=label('pool','tickBitmap');for(const w of (bank.canonical_state||s).words)await set('pool',mapping(w.index,bitmap.slot,'int16'),w.bitmap);
   const observations=label('pool','observations'),observationType=original.artifacts.pool.storageLayout.types[observations.type].base;
   for(const o of s.observations){const {index,...values}=o;await fields('pool',observationType,BigInt(observations.slot)+BigInt(index),values);}
   await roots('lm',{rewardGrowthGlobalX128:s.lm.rewardGrowthGlobalX128,lmLiquidity:s.lm.liquidity,lastRewardTimestamp:s.lm.lastRewardTimestamp,initialization:s.migration.initialized?1:0});
   const lmTicks=label('lm','lmTicks'),lmType=original.artifacts.lm.storageLayout.types[lmTicks.type].value;for(const t of s.lm.ticks){const {index,...values}=t;await fields('lm',lmType,mapping(index,lmTicks.slot,'int24'),values);}
   const localFlags=label('lm','lmTicksFlag');for(const f of s.migration.flags)await set('lm',mapping(f.tick,localFlags.slot,'int24'),f.migrated?1:0);
   for(const [generation,row]of s.migration.predecessors.entries()){
    const role=['first','second','third'][generation];requireValue(row.address===dep[role].address,'BSC4_PREDECESSOR_ADDRESS');
    const ticks=label(role,'lmTicks'),type=original.artifacts[role].storageLayout.types[ticks.type].value;
    for(const tick of row.ticks){await fields(role,type,mapping(tick.tick,ticks.slot,'int24'),{liquidityGross:tick.liquidityGross,liquidityNet:tick.liquidityNet,rewardGrowthOutsideX128:tick.rewardGrowthOutsideX128});if(generation!==0)await set(role,mapping(tick.tick,label(role,'lmTicksFlag').slot,'int24'),tick.flag?1:0);}
   }
   const chef=s.masterChef;await roots('masterChef',{totalAllocPoint:chef.totalAllocPoint,latestPeriodCakePerSecond:chef.latestPeriodCakePerSecond,latestPeriodEndTime:chef.latestPeriodEndTime});
   await set('masterChef',mapping(dep.pool.address,label('masterChef','v3PoolAddressPid').slot,'address'),chef.pid);
   const info=label('masterChef','poolInfo'),infoType=original.artifacts.masterChef.storageLayout.types[info.type].value;await fields('masterChef',infoType,mapping(chef.pid,info.slot),chef.poolInfo);
   const registry=label('factory','getPool');let at=mapping(dep.token0.address,registry.slot,'address');at=mapping(dep.token1.address,at,'address');at=mapping(500,at,'uint24');await set('factory',at,BigInt(dep.pool.address));
   for(const [role,raw]of [['token0',s.raw0],['token1',s.raw1]]){const balances=label(role,'balanceOf');await set(role,mapping(dep.pool.address,balances.slot,'address'),raw);await set(role,mapping(dep.caller.address,balances.slot,'address'),'1000000000000000000000000000000');}
  }
  async function capture(template){
   const p=structuredClone(template),s=p.pancake_bsc_lm.ledger,c=contracts.pool,l=contracts.lm,m=contracts.masterChef;
   requireValue(p.pool_id===dep.pool.address&&p.token0===dep.token0.address&&p.token1===dep.token1.address&&p.protocol==='pancake-v3-bsc-lm-v4'&&p.adapter_version==='pancake-v3/bsc-p400-lm1000000-mc999-v4'&&p.reserve0==='0'&&p.reserve1==='0'&&p.token_behavior==='standard'&&p.fee_denominator===1000000,'PANCAKE_NATIVE_PROFILE');
   for(const [field,role]of [['pool_template_sha256','pool'],['factory_template_sha256','factory']]){const raw=Buffer.from(original.artifacts[role].evm.deployedBytecode.object,'hex'),n=raw.readUInt16BE(raw.length-2)+2;requireValue(s[field]===sha(raw.subarray(0,raw.length-n)),'PANCAKE_NATIVE_TEMPLATE:'+role);}
   for(const [role,state]of [['lm',s.lm],['masterChef',s.master_chef]]){const raw=Buffer.from(original.artifacts[role].evm.deployedBytecode.object,'hex'),n=raw.readUInt16BE(raw.length-2)+2;requireValue(state.template_sha256===sha(raw.subarray(0,raw.length-n))&&state.code_sha256===sha(Buffer.from(dep[role].code.slice(2),'hex'))&&state.address===dep[role].address,'PANCAKE_NATIVE_AUX_CODE:'+role);}
   requireValue(s.core.code_sha256===sha(Buffer.from(dep.pool.code.slice(2),'hex'))&&s.core.factory_code_sha256===sha(Buffer.from(dep.factory.code.slice(2),'hex'))&&s.token0_code_sha256===sha(Buffer.from(dep.token0.code.slice(2),'hex'))&&s.token1_code_sha256===sha(Buffer.from(dep.token1.code.slice(2),'hex')),'PANCAKE_NATIVE_CORE_CODE');
   const slot0=await c.slot0();s.core.sqrt_price_x96=slot0[0].toString();s.core.tick=Number(slot0[1]);s.tick=s.core.tick;s.observation_index=Number(slot0[2]);s.observation_cardinality=Number(slot0[3]);s.observation_cardinality_next=Number(slot0[4]);s.core.fee_protocol=Number(slot0[5]);s.fee_protocol=s.core.fee_protocol;s.unlocked=slot0[6];s.core.liquidity=(await c.liquidity()).toString();
   s.core.factory=(await c.factory()).toLowerCase();p.token0=(await c.token0()).toLowerCase();p.token1=(await c.token1()).toLowerCase();p.fee_numerator=Number(await c.fee());s.core.tick_spacing=Number(await c.tickSpacing());s.max_liquidity_per_tick=(await c.maxLiquidityPerTick()).toString();s.factory_registered=(await contracts.factory.getPool(p.token0,p.token1,p.fee_numerator)).toLowerCase()===p.pool_id;
   for(const w of s.core.tick_words)w.bitmap=(await c.tickBitmap(w.word_pos)).toString();
   for(let i=0;i<s.ticks.length;i++){const t=s.ticks[i],r=await c.ticks(t.tick),core=s.core.ticks[i];requireValue(core.tick===t.tick,'PANCAKE_NATIVE_TICK_INDEX');core.liquidity_gross=r[0].toString();core.liquidity_net=r[1].toString();Object.assign(t,{liquidity_gross:r[0].toString(),liquidity_net:r[1].toString(),fee_growth_outside0:r[2].toString(),fee_growth_outside1:r[3].toString(),tick_cumulative_outside:r[4].toString(),seconds_per_liquidity_outside_x128:r[5].toString(),seconds_outside:Number(r[6]),initialized:r[7]});}
   for(let i=0;i<s.observations.length;i++){const r=await c.observations(i);s.observations[i]={block_timestamp:Number(r[0]),tick_cumulative:r[1].toString(),seconds_per_liquidity_cumulative_x128:r[2].toString(),initialized:r[3]};}
   s.raw_balance0=(await contracts.token0.balanceOf(p.pool_id)).toString();s.raw_balance1=(await contracts.token1.balanceOf(p.pool_id)).toString();s.fee_growth_global0=(await c.feeGrowthGlobal0X128()).toString();s.fee_growth_global1=(await c.feeGrowthGlobal1X128()).toString();const pf=await c.protocolFees();s.protocol_fees0=pf[0].toString();s.protocol_fees1=pf[1].toString();
   requireValue((await c.lmPool()).toLowerCase()===s.lm.address,'PANCAKE_NATIVE_LM_POINTER');s.lm.pool=(await l.pool()).toLowerCase();s.lm.master_chef=(await l.masterChef()).toLowerCase();s.lm.reward_growth_global=(await l.rewardGrowthGlobalX128()).toString();s.lm.liquidity=(await l.lmLiquidity()).toString();s.lm.last_reward_timestamp=Number(await l.lastRewardTimestamp());for(const t of s.lm.ticks){const r=await l.lmTicks(t.tick);t.liquidity_gross=r[0].toString();t.liquidity_net=r[1].toString();t.reward_growth_outside=r[2].toString();}
   const chef=s.master_chef;chef.cake=(await m.CAKE()).toLowerCase();chef.weth=(await m.WETH()).toLowerCase();chef.position_manager=(await m.nonfungiblePositionManager()).toLowerCase();chef.pool_pid=(await m.v3PoolAddressPid(p.pool_id)).toString();const info=await m.poolInfo(chef.pool_pid);Object.assign(chef,{pool_alloc_point:info[0].toString(),pool_address:info[1].toLowerCase(),pool_token0:info[2].toLowerCase(),pool_token1:info[3].toLowerCase(),pool_fee:Number(info[4]),total_alloc_point:(await m.totalAllocPoint()).toString(),latest_period_cake_per_second:(await m.latestPeriodCakePerSecond()).toString(),latest_period_end_time:(await m.latestPeriodEndTime()).toString()});const view=await m.getLatestPeriodInfo(p.pool_id);chef.view_reward_per_second=view[0].toString();chef.view_end_time=view[1].toString();
   const migration=p.pancake_bsc_lm;migration.initialization=await l.initialization();
   for(const f of migration.flags)f.migrated=await l.lmTicksFlag(f.tick);
   for(const [generation,row]of migration.predecessors.entries()){
    const role=['first','second','third'][generation],c=contracts[role];
    requireValue(row.address===dep[role].address&&row.code_sha256===sha(Buffer.from(dep[role].code.slice(2),'hex')),'BSC4_NATIVE_PREDECESSOR_CODE');
    const compiled=Buffer.from(original.artifacts[role].evm.deployedBytecode.object,'hex'),metadata=compiled.readUInt16BE(compiled.length-2)+2;
    requireValue(row.template_sha256===sha(compiled.subarray(0,compiled.length-metadata)),'BSC4_NATIVE_PREDECESSOR_TEMPLATE');
    for(const t of row.ticks){const value=await c.lmTicks(t.tick);t.liquidity_gross=value[0].toString();t.liquidity_net=value[1].toString();t.reward_growth_outside=value[2].toString();}
    for(const f of row.flags)f.migrated=await c.lmTicksFlag(f.tick);
   }
   return p;
  }
  for(let index=0;index<model.records.length;index++){
   const record=model.records[index],actual=input.records[index],q=actual.quote,req=actual.request,bank=reference.source_banks[record.case_index];requireValue(bank&&bank.id===record.case_index&&actual.case_index===record.case_index,'PANCAKE_NATIVE_BANK_ID');
   equal(record.snapshot,actual.snapshot,'PANCAKE_NATIVE_ACTUAL_SNAPSHOT');equal(record.request,req,'PANCAKE_NATIVE_ACTUAL_REQUEST');requireValue(record.plan_hash===actual.execution_plan.plan_hash&&record.quote_id===q.quoteId&&record.gross_buy_amount===q.grossBuyAmount,'PANCAKE_NATIVE_PLAN_QUOTE');
   const originalBank=bank.sequence_index===undefined?original.cases.concat(original.invalid_bank_cases||[]).find(c=>c.index===bank.case_index).source_before:bank.source_step===0?original.sequences[bank.sequence_index].steps[0].source_before:original.sequences[bank.sequence_index].steps[bank.source_step-1].source_after;equal(bank.original_bank,originalBank,'PANCAKE_NATIVE_ORIGINAL_BANK');
   requireValue(req.caller===sender&&req.recipient==='0x000000000000000000000000000000000000beef'&&req.kind==='EXACT_IN'&&req.verification==='FAST','PANCAKE_NATIVE_CONTEXT');requireValue(q.verification.level==='INDICATIVE'&&q.netBuyAmountEstimate===null,'PANCAKE_NATIVE_NO_UPGRADE');
   requireValue(record.snapshot.pools.length===1&&record.steps.length===1&&record.snapshot.reference.block.timestamp_ms===timestamp,'PANCAKE_NATIVE_SNAPSHOT_SCOPE');
   const sourcePool=structuredClone(bank.pool),snapshotPool=record.snapshot.pools[0];for(const key of ['block_number','block_hash'])sourcePool.pancake_bsc_lm.ledger[key]=snapshotPool.pancake_bsc_lm.ledger[key];equal(sourcePool,snapshotPool,'PANCAKE_NATIVE_SOURCE_BANK_BINDING');requireValue(sourcePool.pancake_bsc_lm.ledger.derived===false,'PANCAKE_NATIVE_DERIVED_SOURCE');
   await initialize(originalBank);equal(await capture(sourcePool),sourcePool,'PANCAKE_NATIVE_ORIGINAL_PRESTATE');
   const outputToken=req.buyToken===dep.token0.address?contracts.token0:contracts.token1;requireValue(req.buyToken===dep.token0.address||req.buyToken===dep.token1.address,'PANCAKE_NATIVE_BUY_TOKEN');const start=await outputToken.balanceOf(req.recipient);let output=0n,inputTotal=0n;
   for(const step of record.steps){
    const a=step.before.pancake_bsc_lm.ledger,b=step.after.pancake_bsc_lm.ledger;
    requireValue(a.derived===false&&b.derived===true,'PANCAKE_NATIVE_DERIVED_MARKERS');
    const mutable=new Set(['core','tick','raw_balance0','raw_balance1','fee_growth_global0','fee_growth_global1','protocol_fees0','protocol_fees1','observation_index','observation_cardinality','ticks','observations','lm','derived']);
    for(const key of Object.keys(a).filter(k=>!mutable.has(k)))equal(b[key],a[key],'PANCAKE_NATIVE_MODEL_MARKER:'+key);
    for(const key of Object.keys(a.core).filter(k=>!['sqrt_price_x96','tick','liquidity'].includes(k)))equal(b.core[key],a.core[key],'PANCAKE_NATIVE_CORE_MARKER:'+key);
    for(const key of Object.keys(a.lm).filter(k=>!['reward_growth_global','liquidity','last_reward_timestamp','ticks'].includes(k)))equal(b.lm[key],a.lm[key],'PANCAKE_NATIVE_LM_MARKER:'+key);
    equal(b.lm.ticks.map(t=>t.tick),a.lm.ticks.map(t=>t.tick),'PANCAKE_NATIVE_LM_TICK_SCOPE');
    equal(step.after.pancake_bsc_lm.predecessors,step.before.pancake_bsc_lm.predecessors,'BSC4_PREDECESSOR_READONLY');
    equal(step.after.pancake_bsc_lm.flags.map(f=>f.tick),step.before.pancake_bsc_lm.flags.map(f=>f.tick),'BSC4_MIGRATION_FLAG_SCOPE');
    requireValue(step.after.pancake_bsc_lm.initialization===step.before.pancake_bsc_lm.initialization,'BSC4_INITIALIZATION_READONLY');
    equal(b.ticks.map(t=>t.tick),a.ticks.map(t=>t.tick),'PANCAKE_NATIVE_TICK_SCOPE');requireValue(b.observations.length===a.observations.length,'PANCAKE_NATIVE_OBSERVATION_SCOPE');
    requireValue(step.pool_id===dep.pool.address&&BigInt(step.amount_in)>0n,'PANCAKE_NATIVE_PHYSICAL_STEP');equal(await capture(step.before),step.before,'PANCAKE_NATIVE_STEP_BEFORE');const zero=step.zero_for_one;requireValue((zero?dep.token0.address:dep.token1.address)===req.sellToken,'PANCAKE_NATIVE_DIRECTION');const limit=zero?'4295128740':'1461446703485210103287273052203988822378723970341';const values=await contracts.caller.callStatic.run(dep.pool.address,req.recipient,zero,step.amount_in,limit,{gasLimit:8000000});const used=(zero?values[0]:values[1]).toString(),received=(zero?values[1]:values[0]).mul(-1).toString();requireValue(used===step.amount_in&&received===step.amount_out,'PANCAKE_NATIVE_STEP_OUTPUT');await(await contracts.caller.run(dep.pool.address,req.recipient,zero,step.amount_in,limit,{gasLimit:8000000})).wait();const after=await capture(step.after);equal(after,step.after,'PANCAKE_NATIVE_FULL_AFTER_STATE');output+=BigInt(received);inputTotal+=BigInt(used);count++;}
   const delta=(await outputToken.balanceOf(req.recipient)).sub(start).toString();requireValue(delta===output.toString()&&delta===q.grossBuyAmount&&inputTotal.toString()===req.sellAmount,'PANCAKE_NATIVE_RECIPIENT_AND_INPUT');results.push({case_index:record.case_index,quote_id:q.quoteId,recipient_token_delta:delta,consumed_in:inputTotal.toString(),full_after_state_matched:true});
  }
  fs.writeFileSync(args[3],JSON.stringify({schema_version:1,passed:true,authority:'unchanged original Pool/LM/MasterChef and controlled standard-token callback with actual API inputs, token deltas and complete declared after-state',input_sha256:sha(inputRaw),model_sha256:sha(modelRaw),reference_sha256:sha(referenceRaw),original_swaps:count,records:results,production_ready:false,all_requested_features_complete:false},null,2)+'\n');console.log(JSON.stringify({passed:true,records:results.length,original_swaps:count}));
 }finally{await transport.disconnect();}
})().catch(e=>{console.error(e);process.exitCode=1;});
