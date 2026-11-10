// SPDX-License-Identifier: GPL-2.0-or-later
// Unmodified pinned Slipstream Pool/Factory/fee modules in a controlled local bank.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../../..'),dir=path.join(root,'integration/tools/reference/slipstream-extended');
const modules=path.join(root,'integration/.runtime/stable-reference/node_modules');
const solc=require(path.join(modules,'solc07')),ganache=require(path.join(modules,'ganache')),{ethers}=require(path.join(modules,'ethers'));
const requireValue=(v,why)=>{if(!v)throw Error(why);};const sha=raw=>crypto.createHash('sha256').update(raw).digest('hex');
requireValue(solc.version().startsWith('0.7.6+'),'SLIPSTREAM_COMPILER_VERSION');
const upstream=JSON.parse(fs.readFileSync(path.join(dir,'source-metadata.json')));
const sealedPath=path.join(root,'integration/test/source/deployment-catalog/metadata-exact/base-aerodrome-slipstream/exact-standard-input.json');
const sealed=JSON.parse(fs.readFileSync(sealedPath)),sources={...sealed.sources};
const needed=['contracts/core/fees/CustomSwapFeeModule.sol','contracts/core/fees/CustomUnstakedFeeModule.sol','contracts/core/fees/DynamicSwapFeeModule.sol','contracts/core/interfaces/fees/ICustomFeeModule.sol','contracts/core/interfaces/fees/IDynamicFeeModule.sol'];
for(const name of needed){const raw=fs.readFileSync(path.join(dir,name));requireValue(sha(raw)===upstream.files[name],'SLIPSTREAM_MODULE_SOURCE_HASH:'+name);sources[name]={content:raw.toString('utf8')};}
for(const name of ['contracts/core/CLPool.sol'])requireValue(sha(Buffer.from(sources[name].content))===upstream.files[name],'SLIPSTREAM_FIXED_DEPLOYED_SOURCE_MATCH:'+name);
sources['Reference.sol']={content:`// SPDX-License-Identifier: GPL-2.0-or-later
pragma solidity =0.7.6;
interface P {function swap(address,bool,int256,uint160,bytes calldata)external returns(int256,int256);function token0()external view returns(address);function token1()external view returns(address);}
interface T {function transfer(address,uint256)external returns(bool);}
contract ReferenceToken {mapping(address=>uint256)public balanceOf;function mint(address to,uint256 value)external{balanceOf[to]+=value;}function transfer(address to,uint256 value)external returns(bool){require(balanceOf[msg.sender]>=value);balanceOf[msg.sender]-=value;balanceOf[to]+=value;return true;}}
contract ReferenceLegacy {function getPool(address,address,int24)external pure returns(address){return address(0);}}
contract ReferenceVoter {address public factoryRegistry=address(0xfeed);function gauges(address)external pure returns(address){return address(0xbead);}function isAlive(address)external pure returns(bool){return true;}}
contract ReferenceCaller {function run(address pool,address recipient,bool zero,int256 input,uint160 limit)external returns(int256,int256){return P(pool).swap(recipient,zero,input,limit,abi.encode(pool));}function uniswapV3SwapCallback(int256 a,int256 b,bytes calldata data)external{require(msg.sender==abi.decode(data,(address)));if(a>0)require(T(P(msg.sender).token0()).transfer(msg.sender,uint256(a)));if(b>0)require(T(P(msg.sender).token1()).transfer(msg.sender,uint256(b)));}}
`};
// Slipstream uses uniswapV3SwapCallback or uniswapV3 style? Read the pinned source.
const poolSource=sources['contracts/core/CLPool.sol'].content;
if(poolSource.includes('.uniswapV3SwapCallback(')===false){
 requireValue(poolSource.includes('.uniswapV3SwapCallback(')||poolSource.includes('.uniswapV3SwapCallback'),'SLIPSTREAM_CALLBACK_SELECTOR_REQUIRES_REVIEW');
}
const settings={...sealed.settings,outputSelection:{'*':{'*':['abi','evm.bytecode.object','evm.deployedBytecode','storageLayout']}}};
const compiled=JSON.parse(solc.compile(JSON.stringify({language:'Solidity',sources,settings})));
const errors=(compiled.errors||[]).filter(e=>e.severity==='error');requireValue(errors.length===0,errors.map(e=>e.formattedMessage).join('\n'));
const artifact=(name,contract)=>compiled.contracts[name][contract];
const voterSourcePath=path.join(dir,'voter-verified-source.json'),voterSource=JSON.parse(fs.readFileSync(voterSourcePath));
requireValue(voterSource.chainId==='8453'||voterSource.chainId===8453,'SLIPSTREAM_VOTER_SOURCE_CHAIN');
requireValue(voterSource.address.toLowerCase()==='0x16613524e02ad97edfef371bc883f2f5d6c480a5','SLIPSTREAM_VOTER_SOURCE_ADDRESS');
const voterInput=JSON.parse(JSON.stringify(voterSource.stdJsonInput));
voterInput.settings.outputSelection={'*':{'*':['abi','evm.deployedBytecode','storageLayout']}};
const voterCompiler=require(path.join(modules,'solc'));requireValue(voterCompiler.version().startsWith('0.8.19+'),'SLIPSTREAM_VOTER_COMPILER');
const voterCompiled=JSON.parse(voterCompiler.compile(JSON.stringify(voterInput)));
requireValue(!(voterCompiled.errors||[]).some(e=>e.severity==='error'),'SLIPSTREAM_VOTER_COMPILE');
const voterArtifact=voterCompiled.contracts['Voter.sol'].Voter;
const voterRuntime=voterSource.runtimeBytecode.onchainBytecode;
requireValue(sha(Buffer.from(voterRuntime.slice(2),'hex'))==='621bb8e29a36ec7d8319e019a2ec47de06fee83e332465a893bd0b34955dff74','SLIPSTREAM_VOTER_RUNTIME_PIN');
function normalizeVoter(code){const raw=Buffer.from(code.replace(/^0x/,''),'hex'),length=raw.readUInt16BE(raw.length-2),body=Buffer.from(raw.subarray(0,raw.length-length-2));for(const values of Object.values(voterArtifact.evm.deployedBytecode.immutableReferences))for(const item of values){requireValue(item.length===32&&item.start+32<=body.length,'SLIPSTREAM_VOTER_IMMUTABLE_RANGE');body.fill(0,item.start,item.start+32);}return sha(body);}
requireValue(normalizeVoter(voterRuntime)===normalizeVoter(voterArtifact.evm.deployedBytecode.object),'SLIPSTREAM_VOTER_ORIGINAL_SOURCE_BODY');
const voterSlots=Object.fromEntries(voterArtifact.storageLayout.storage.map(v=>[v.label,v.slot]));
requireValue(voterSlots.gauges==='8'&&voterSlots.isAlive==='20','SLIPSTREAM_VOTER_MAPPING_LAYOUT');
requireValue(/mapping\s*\(address\s*=>\s*address\)\s*public\s+gauges\s*;/.test(voterSource.sources['Voter.sol'].content)&&/mapping\s*\(address\s*=>\s*bool\)\s*public\s+isAlive\s*;/.test(voterSource.sources['Voter.sol'].content),'SLIPSTREAM_VOTER_CONTEXT_FREE_GETTERS');
const poolArtifact=artifact('contracts/core/CLPool.sol','CLPool');
requireValue(poolArtifact.evm.deployedBytecode.object.length/2<=24576,'SLIPSTREAM_EIP170_RUNTIME');
const layout=Object.fromEntries(poolArtifact.storageLayout.storage.map(v=>[v.label,v]));
const artifactReport={schema_version:1,authority:'original compiler artifacts for controlled-runtime replay; not live execution or deployment qualification',source_input_sha256:sha(fs.readFileSync(sealedPath)),module_manifest_sha256:sha(fs.readFileSync(path.join(dir,'source-metadata.json'))),compiler:solc.version(),contracts:{pool:poolArtifact,factory:artifact('contracts/core/CLFactory.sol','CLFactory'),custom_swap:artifact('contracts/core/fees/CustomSwapFeeModule.sol','CustomSwapFeeModule'),custom_unstaked:artifact('contracts/core/fees/CustomUnstakedFeeModule.sol','CustomUnstakedFeeModule'),dynamic_swap:artifact('contracts/core/fees/DynamicSwapFeeModule.sol','DynamicSwapFeeModule'),caller:artifact('Reference.sol','ReferenceCaller')},voter:{runtime:voterRuntime,storage_layout:voterArtifact.storageLayout,compiled_runtime:voterArtifact.evm.deployedBytecode,source_sha256:sha(fs.readFileSync(voterSourcePath))}};
fs.writeFileSync(path.join(root,'integration/artifacts/local/core-slipstream-context-runtime-artifacts.json'),JSON.stringify(artifactReport,null,2)+'\n');
if(process.argv.includes('--artifacts-only')){console.log(JSON.stringify({passed:true,scope:'original compile artifacts only',pool_runtime_bytes:poolArtifact.evm.deployedBytecode.object.length/2}));return;}
const transport=ganache.provider({logging:{quiet:true},chain:{hardfork:'shanghai',time:new Date('2026-11-07T00:00:00Z')},miner:{timestampIncrement:0},wallet:{deterministic:true}});
const provider=new ethers.providers.Web3Provider(transport),Q96=1n<<96n,L=1000000000000n;
const word=n=>'0x'+(BigInt(n)&((1n<<256n)-1n)).toString(16).padStart(64,'0');
const signed=(n,bits)=>BigInt(n)&((1n<<BigInt(bits))-1n);
const packed=fields=>fields.reduce((v,[n,offset])=>v+(BigInt(n)<<BigInt(offset)),0n);
const mapping=(key,slot,type='int24')=>ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode([type,'uint256'],[String(key),String(slot)]));
const clean=value=>ethers.BigNumber.isBigNumber(value)?value.toString():Array.isArray(value)?Array.from(value,clean):typeof value==='bigint'?String(value):value;
(async()=>{
 try{
  const signer=provider.getSigner(0),sender=(await signer.getAddress()).toLowerCase(),recipient='0x000000000000000000000000000000000000beef';
  const deploy=async(a,args=[])=>{const c=await new ethers.ContractFactory(a.abi,a.evm.bytecode.object,signer).deploy(...args);await c.deployed();return c;};
  const t0=await deploy(artifact('Reference.sol','ReferenceToken')),t1=await deploy(artifact('Reference.sol','ReferenceToken'));
  const tokens=[t0,t1].sort((a,b)=>a.address.toLowerCase().localeCompare(b.address.toLowerCase()));
  await provider.send('evm_setAccountCode',[voterSource.address.toLowerCase(),voterRuntime]);
  const voter=new ethers.Contract(voterSource.address,voterArtifact.abi,signer),legacy=await deploy(artifact('Reference.sol','ReferenceLegacy'));
  const implementation=await deploy(poolArtifact),factory=await deploy(artifact('contracts/core/CLFactory.sol','CLFactory'),[voter.address,implementation.address]);
  await(await factory.createPool(tokens[0].address,tokens[1].address,50,Q96.toString())).wait();
  const createdPool=await factory.getPool(tokens[0].address,tokens[1].address,50);
  await provider.send('evm_setAccountStorageAt',[voter.address,mapping(createdPool,voterSlots.gauges,'address'),word(0xbead)]);
  await provider.send('evm_setAccountStorageAt',[voter.address,mapping('0x000000000000000000000000000000000000bead',voterSlots.isAlive,'address'),word(1)]);
  const pool=new ethers.Contract(await factory.getPool(tokens[0].address,tokens[1].address,50),poolArtifact.abi,signer),caller=await deploy(artifact('Reference.sol','ReferenceCaller'));
  const custom=await deploy(artifact('contracts/core/fees/CustomSwapFeeModule.sol','CustomSwapFeeModule'),[factory.address]);
  const unstaked=await deploy(artifact('contracts/core/fees/CustomUnstakedFeeModule.sol','CustomUnstakedFeeModule'),[factory.address]);
  const dynamic=await deploy(artifact('contracts/core/fees/DynamicSwapFeeModule.sol','DynamicSwapFeeModule'),[factory.address,1000000,50000,[],[]]);
  const now=(await provider.getBlock('latest')).timestamp;
  const set=async(slot,value)=>provider.send('evm_setAccountStorageAt',[pool.address,typeof slot==='string'?slot:word(slot),word(value)]);
  const field=async(label,value)=>{const item=layout[label];requireValue(item,'SLIPSTREAM_MISSING_LAYOUT:'+label);const bits=8n*BigInt(poolArtifact.storageLayout.types[item.type].numberOfBytes),shift=8n*BigInt(item.offset),mask=((1n<<bits)-1n)<<shift;const raw=await provider.getStorageAt(pool.address,word(BigInt(item.slot)));const old=BigInt(raw==='0x'?'0x0':raw);await set(BigInt(item.slot),(old&(((1n<<256n)-1n)^mask))|(signed(value,Number(bits))<<shift));};
  const readsets=new Map();
  async function nativeReads(state){
   const block=await provider.getBlock('latest');if(readsets.has(block.hash))return readsets.get(block.hash);
   const reads=[];
   async function read(contract,signature,args=[],from=sender){const data=contract.interface.encodeFunctionData(signature,args).toLowerCase();reads.push({to:contract.address.toLowerCase(),data,from:from.toLowerCase(),block_hash:block.hash,block_number:block.number});}
   for(const signature of ['token0()','token1()','factory()','fee()','tickSpacing()','slot0()','liquidity()','gauge()','nft()','factoryRegistry()','feeGrowthGlobal0X128()','feeGrowthGlobal1X128()','rewardGrowthGlobalX128()','rewardRate()','rewardReserve()','periodFinish()','rollover()','stakedLiquidity()','maxLiquidityPerTick()','lastUpdated()','gaugeFees()','unstakedFee()'])await read(pool,signature);
   for(const bitmap of state.tick_words)await read(pool,'tickBitmap(int16)',[bitmap.word_pos]);
   for(const tick of [-200,-100,100,200])await read(pool,'ticks(int24)',[tick]);
   for(let index=0;index<8;index++)await read(pool,'observations(uint256)',[index]);
   for(const signature of ['voter()','poolImplementation()','factoryRegistry()','swapFeeModule()','unstakedFeeModule()','defaultUnstakedFee()'])await read(factory,signature);
   for(const signature of ['swapFeeModule()','unstakedFeeModule()'])await read(factory,signature,[],'0x'+'0'.repeat(40));
   await read(factory,'tickSpacingToFee(int24)',[50]);await read(factory,'isPool(address)',[pool.address]);await read(factory,'getPool(address,address,int24)',[tokens[0].address,tokens[1].address,50]);
   await read(voter,'gauges(address)',[pool.address]);await read(voter,'isAlive(address)',[state.module_state.gauge]);
   await read(unstaked,'factory()');await read(unstaked,'customFee(address)',[pool.address]);
   if(state.module_state.kind==='custom'){await read(custom,'factory()');await read(custom,'customFee(address)',[pool.address]);}
   if(state.module_state.kind==='dynamic'){for(const signature of ['factory()','defaultScalingFactor()','defaultFeeCap()','secondsAgo()'])await read(dynamic,signature);await read(dynamic,'dynamicFeeConfig(address)',[pool.address]);await read(dynamic,'discounted(address)',[sender]);}
   await read(tokens[0],'balanceOf(address)',[pool.address]);await read(tokens[1],'balanceOf(address)',[pool.address]);
   await Promise.all(reads.map(async item=>{item.result=await provider.call({to:item.to,data:item.data,from:item.from},block.number);}));
   requireValue((await provider.getBlock(block.number)).hash===block.hash,'SLIPSTREAM_NATIVE_GETTER_ANCHOR_CHANGED');
   const value={reads,header:{number:block.number,hash:block.hash,parent_hash:block.parentHash,timestamp:block.timestamp}};readsets.set(block.hash,value);return value;
  }
  const snapshot=async()=>{
   const result={},labels=['slot0','liquidity','stakedLiquidity','maxLiquidityPerTick','feeGrowthGlobal0X128','feeGrowthGlobal1X128','rewardGrowthGlobalX128','gaugeFees','rewardRate','rewardReserve','periodFinish','rollover','lastUpdated','gauge','nft','factoryRegistry'];const values=await Promise.all(labels.map(label=>pool[label]()));for(let i=0;i<labels.length;i++)result[labels[i]]=clean(values[i]);
   result.ticks=[];for(const tick of [-200,-100,100,200])result.ticks.push({tick,values:clean(await pool.ticks(tick))});
   result.observations=[];for(let index=0;index<8;index++)result.observations.push({index,values:clean(await pool.observations(index))});
   const center=Math.floor(Math.floor(Number(result.slot0[1])/50)/256);result.tick_words=[];for(const pos of [center-1,center,center+1])result.tick_words.push({word_pos:pos,bitmap:(await pool.tickBitmap(pos)).toString()});
   result.raw_balance0=(await tokens[0].balanceOf(pool.address)).toString();result.raw_balance1=(await tokens[1].balanceOf(pool.address)).toString();
   const swapAddress=(await factory.swapFeeModule()).toLowerCase(),unstakedAddress=(await factory.unstakedFeeModule()).toLowerCase();
   result.module_state={factory_registered:await factory.isPool(pool.address),factory_voter:(await factory.voter()).toLowerCase(),factory_default_fee:(await factory.tickSpacingToFee(50)).toString(),factory_default_unstaked_fee:(await factory.defaultUnstakedFee()).toString(),swap_fee_module:swapAddress,unstaked_fee_module:unstakedAddress,gauge:(await voter.gauges(pool.address)).toLowerCase(),gauge_alive:await voter.isAlive(await voter.gauges(pool.address)),unstaked_custom_fee:(await unstaked.customFee(pool.address)).toString(),allowed_origin:sender};
   if(swapAddress===custom.address.toLowerCase()){result.module_state.kind='custom';result.module_state.custom_fee=(await custom.customFee(pool.address)).toString();}
   else if(swapAddress===dynamic.address.toLowerCase()){
    result.module_state.kind='dynamic';result.module_state.dynamic_fee_config=clean(await dynamic.dynamicFeeConfig(pool.address));result.module_state.default_scaling_factor=(await dynamic.defaultScalingFactor()).toString();result.module_state.default_fee_cap=(await dynamic.defaultFeeCap()).toString();result.module_state.seconds_ago=(await dynamic.secondsAgo()).toString();result.module_state.origin_discount=(await dynamic.discounted(sender)).toString();
   }else{requireValue(swapAddress==='0x'+'0'.repeat(40),'SLIPSTREAM_UNKNOWN_MODULE');result.module_state.kind='none';}

   result.module_state.view_swap_fee=(await pool.fee()).toString();result.module_state.view_unstaked_fee=(await pool.unstakedFee()).toString();
   Object.defineProperty(result,'original_readset',{value:await nativeReads(result),enumerable:false});
   return result;
  };
  let point=await provider.send('evm_snapshot',[]);const cases=[],sequences=[];
  for(const mode of ['none','custom','dynamic'])for(const stake of [0,1,2])for(const zero of [true,false])for(const amount of [1n,1000000n,1000000000n,6000000000n,7000000000n,10000000000n]){
   await provider.send('evm_revert',[point]);point=await provider.send('evm_snapshot',[]);
   const staked=L*BigInt(stake)/2n;
   await field('liquidity',L);await field('stakedLiquidity',staked);await field('gauge',0xbead);
   await field('feeGrowthGlobal0X128',1000000);await field('feeGrowthGlobal1X128',2000000);await field('rewardGrowthGlobalX128',3000000);
   await field('rewardRate',1000);await field('rewardReserve',1000000);await field('periodFinish',now+100);await field('lastUpdated',now-5);
   await set(BigInt(layout.slot0.slot),packed([[Q96,0],[3,184],[4,200],[8,216],[1,232]]));
   for(let i=0;i<8;i++){
    const initialized=i<4,at=now-8+i*2;
    const cumulative=-50*at;
    await set(BigInt(layout.observations.slot)+BigInt(i),packed([[initialized?at:1,0],[signed(initialized?cumulative:0,56),32],[initialized?BigInt(at)*(1n<<128n)/L:0,88],[initialized?1:0,248]]));
   }
   const positions=[-200,-100,100,200];
   for(let i=0;i<positions.length;i++){
    const tick=positions[i],net=i<2?L/2n:-L/2n,stakedNet=net*BigInt(stake)/2n;
    const start=BigInt(mapping(tick,layout.ticks.slot));
    await set(start,packed([[L/2n,0],[signed(net,128),128]]));await set(start+1n,signed(stakedNet,128));
    await set(start+2n,11+i);await set(start+3n,21+i);await set(start+4n,31+i);
    await set(start+5n,packed([[signed(-100,56),0],[200,56],[now-10,216],[1,248]]));
   }
   await set(mapping(-1,layout.tickBitmap.slot,'int16'),(1n<<252n)|(1n<<254n));await set(mapping(0,layout.tickBitmap.slot,'int16'),(1n<<2n)|(1n<<4n));
   await(await tokens[0].mint(pool.address,'1000000000000000000')).wait();await(await tokens[1].mint(pool.address,'1000000000000000000')).wait();
   await(await tokens[0].mint(caller.address,'1000000000000000000')).wait();await(await tokens[1].mint(caller.address,'1000000000000000000')).wait();
   await(await factory.setUnstakedFeeModule(unstaked.address)).wait();await(await unstaked.setCustomFee(pool.address,200000)).wait();
   if(mode==='custom'){await(await factory.setSwapFeeModule(custom.address)).wait();await(await custom.setCustomFee(pool.address,750)).wait();}
   if(mode==='dynamic'){await(await factory.setSwapFeeModule(dynamic.address)).wait();await(await dynamic.setSecondsAgo(4)).wait();await(await dynamic.setInitialFee(pool.address,100)).wait();await(await dynamic.registerDiscounted(sender,250000)).wait();}
   const before=await snapshot(),payerBefore=await tokens[zero?0:1].balanceOf(caller.address);const fee=(await pool.fee()).toString(),unstakedFee=(await pool.unstakedFee()).toString();
   const limit=zero?'4295128740':'1461446703485210103287273052203988822378723970341';
   const item={mode,stake,zero_for_one:zero,amount_in:String(-amount),target_output:String(amount),fee,unstaked_fee:unstakedFee,before,before_getter_reads:before.original_readset.reads,original_before_header:before.original_readset.header};
   try{
    const preview=await caller.callStatic.run(pool.address,recipient,zero,String(-amount),limit);
    const r=await(await caller.run(pool.address,recipient,zero,String(-amount),limit,{gasLimit:10000000})).wait();
    const received=await tokens[zero?1:0].balanceOf(recipient),after=await snapshot();
    const consumed=zero?preview[0]:preview[1],output=(zero?preview[1]:preview[0]).mul(-1);
    requireValue(received.toString()===output.toString(),'SLIPSTREAM_ACTUAL_RECIPIENT_DELTA');
    const payerDelta=payerBefore.sub(await tokens[zero?0:1].balanceOf(caller.address));requireValue(payerDelta.eq(consumed),'SLIPSTREAM_EXACT_OUT_PAYER_DELTA');
    item.expected={payer_delta:payerDelta.toString(),consumed_in:consumed.toString(),amount_out:output.toString(),after,after_getter_reads:after.original_readset.reads,original_after_header:after.original_readset.header,receipt:{transaction_hash:r.transactionHash,block_hash:r.blockHash,status:r.status,gas_used:r.gasUsed.toString()}};
    if(mode==='dynamic'&&stake===1&&(amount===1000000000n||amount===6000000000n)){
     const sequence={initial_case_index:cases.length,steps:[{amount_in:String(-amount),target_output:String(amount),fee,...item.expected}]};
     for(const nextInput of [1000000n,1000000n]){
      const priorPayer=await tokens[zero?0:1].balanceOf(caller.address),prior=await tokens[zero?1:0].balanceOf(recipient),nextFee=(await pool.fee()).toString();
      const preview=await caller.callStatic.run(pool.address,recipient,zero,String(-nextInput),limit);
      await(await caller.run(pool.address,recipient,zero,String(-nextInput),limit,{gasLimit:10000000})).wait();
      const delta=(await tokens[zero?1:0].balanceOf(recipient)).sub(prior);
      requireValue(delta.toString()===(zero?preview[1]:preview[0]).mul(-1).toString(),'SLIPSTREAM_SEQUENCE_DELTA');
      const payerDelta=priorPayer.sub(await tokens[zero?0:1].balanceOf(caller.address));requireValue(payerDelta.eq(zero?preview[0]:preview[1]),'SLIPSTREAM_SEQUENCE_PAYER_DELTA');
      const after=await snapshot();sequence.steps.push({amount_in:String(-nextInput),target_output:String(nextInput),payer_delta:payerDelta.toString(),fee:nextFee,consumed_in:(zero?preview[0]:preview[1]).toString(),amount_out:delta.toString(),after,after_getter_reads:after.original_readset.reads,original_after_header:after.original_readset.header});
     }
     sequences.push(sequence);
    }
   }catch(error){if(error.code!=='CALL_EXCEPTION')throw error;item.expected={reverted:true};}
   cases.push(item);
  }
  function codeTemplate(artifact,runtime,bindings){
   const raw=Buffer.from(runtime.slice(2),'hex'),metadataBytes=raw.readUInt16BE(raw.length-2),bodyLength=raw.length-metadataBytes-2;
   const normalized=Buffer.from(raw.subarray(0,bodyLength)),refs=[];
   for(const locations of Object.values(artifact.evm.deployedBytecode.immutableReferences||{}))for(const item of locations){
    requireValue(item.length===32&&item.start+32<=bodyLength,'SLIPSTREAM_IMMUTABLE_REF_RANGE');
    const original=raw.subarray(item.start,item.start+32).toString('hex');
    const role=Object.keys(bindings).find(k=>bindings[k].slice(2).toLowerCase().padStart(64,'0')===original);
    requireValue(role,'SLIPSTREAM_UNBOUND_IMMUTABLE');refs.push({offset:item.start,role});normalized.fill(0,item.start,item.start+32);
   }
   const original=Buffer.from(artifact.evm.deployedBytecode.object,'hex');
   requireValue(sha(normalized)===sha(original.subarray(0,bodyLength)),'SLIPSTREAM_ORIGINAL_TEMPLATE_IMMUTABLE_MATCH');
   return {runtime_bytes:raw.length,body_bytes:bodyLength,metadata_bytes:metadataBytes,normalized_execution_body_sha256:sha(normalized),immutable_bindings:refs.sort((a,b)=>a.offset-b.offset)};
  }
  const templates={
   implementation:codeTemplate(poolArtifact,await provider.getCode(implementation.address),{}),
   factory:codeTemplate(artifact('contracts/core/CLFactory.sol','CLFactory'),await provider.getCode(factory.address),{voter:voter.address,pool_implementation:implementation.address,factory_registry:await factory.factoryRegistry()}),
   custom_swap:codeTemplate(artifact('contracts/core/fees/CustomSwapFeeModule.sol','CustomSwapFeeModule'),await provider.getCode(custom.address),{factory:factory.address}),
   custom_unstaked:codeTemplate(artifact('contracts/core/fees/CustomUnstakedFeeModule.sol','CustomUnstakedFeeModule'),await provider.getCode(unstaked.address),{factory:factory.address}),
   dynamic_swap:codeTemplate(artifact('contracts/core/fees/DynamicSwapFeeModule.sol','DynamicSwapFeeModule'),await provider.getCode(dynamic.address),{factory:factory.address}),
  };
  const loaded_dependencies=Object.keys(require.cache).filter(file=>file.startsWith(root+path.sep)).sort().map(file=>({path:path.relative(root,file),bytes:fs.statSync(file).size,sha256:sha(fs.readFileSync(file))}));
  const output={schema_version:1,passed:true,quote_kind:'EXACT_OUT',generator_sha256:sha(fs.readFileSync(__filename)),runtime:{node_version:process.version,node_executable_sha256:sha(fs.readFileSync(process.execPath)),loaded_dependencies},source_commit:upstream.commit,pinned_source_manifest_sha256:sha(fs.readFileSync(path.join(dir,'source-metadata.json'))),retained_source_input_sha256:sha(fs.readFileSync(sealedPath)),factory_source_authority:'exact retained Base factory source; its constructor/legacy-pool migration differs from upstream f8717faa, fee getters unchanged and preserved',factory_source_sha256:sha(Buffer.from(sources['contracts/core/CLFactory.sol'].content)),compiler:solc.version(),authority:'unmodified original Slipstream Pool/Factory/modules, controlled initialized state, callback Token payments and actual recipient balances; no real Base source/production claim',voter_source_sha256:sha(fs.readFileSync(voterSourcePath)),voter_runtime_sha256:sha(Buffer.from(voterRuntime.slice(2),'hex')),voter_recompiled_execution_body_sha256:normalizeVoter(voterRuntime),voter_source_scope:'unmodified verified Base Voter runtime, recompiled execution body and public gauge/isAlive mapping getters; controlled mapping values only; no live bank claim',fixed_timestamp:now,code_templates:templates,deployment:{sender,recipient,voter:voter.address.toLowerCase(),voter_code:await provider.getCode(voter.address),factory_registry:(await factory.factoryRegistry()).toLowerCase(),caller:caller.address.toLowerCase(),pool:pool.address.toLowerCase(),pool_code:await provider.getCode(pool.address),factory:factory.address.toLowerCase(),factory_code:await provider.getCode(factory.address),implementation:implementation.address.toLowerCase(),implementation_code:await provider.getCode(implementation.address),token0:tokens[0].address.toLowerCase(),token0_code:await provider.getCode(tokens[0].address),token1:tokens[1].address.toLowerCase(),token1_code:await provider.getCode(tokens[1].address),swap_custom:custom.address.toLowerCase(),swap_custom_code:await provider.getCode(custom.address),unstaked_custom:unstaked.address.toLowerCase(),unstaked_custom_code:await provider.getCode(unstaked.address),swap_dynamic:dynamic.address.toLowerCase(),swap_dynamic_code:await provider.getCode(dynamic.address)},source_layout:poolArtifact.storageLayout,cases,sequences,production_ready:false,normal_profile_implemented:false};
  fs.writeFileSync(path.join(root,'integration/artifacts/local/core-slipstream-exact-out-original-20261110.json'),JSON.stringify(output,null,2)+'\n');
  console.log(JSON.stringify({passed:true,cases:cases.length,sequences:sequences.length,reverts:cases.filter(c=>c.expected.reverted).length,partial_fills:cases.filter(c=>c.expected.amount_out&&c.expected.amount_out!==c.target_output).length}));
 }finally{await transport.disconnect();}
})().catch(error=>{console.error(error);process.exitCode=1;});
