// SPDX-License-Identifier: GPL-2.0-or-later
// Execute unchanged pinned Pool, LM and MasterChef contracts in controlled banks.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../../..'),dir=path.join(root,'integration/tools/reference/pancake-v3-lm');
const modules=path.join(root,'integration/.runtime/stable-reference/node_modules');
const solc07=require(path.join(modules,'solc07')),wrapper=require(path.join(modules,'solc/wrapper'));
const {ethers}=require(path.join(modules,'ethers')),ganache=require(path.join(modules,'ganache'));
const sha=x=>crypto.createHash('sha256').update(x).digest('hex');
const requireValue=(value,why)=>{if(!value)throw Error(why);};
const metadata=JSON.parse(fs.readFileSync(path.join(dir,'source-metadata.json')));
const lfj=JSON.parse(fs.readFileSync(path.join(root,'integration/tools/reference/lfj-v21/source-metadata.json')));
const compiler=path.join(root,'integration/.runtime/lfj-solc',lfj.compiler.path);
requireValue('0x'+sha(fs.readFileSync(compiler))===lfj.compiler.sha256,'PANCAKE_COMPILER_HASH');
const solc08=wrapper(require(compiler));
requireValue(solc07.version().startsWith('0.7.6+')&&solc08.version().startsWith('0.8.10+'),'PANCAKE_COMPILER_VERSIONS');
const originals={};
for(const entry of metadata.files){const data=fs.readFileSync(path.join(dir,entry.path));requireValue(sha(data)===entry.sha256,'PANCAKE_SOURCE_HASH:'+entry.path);if(entry.path.endsWith('.sol'))originals[entry.path]={content:data.toString()};}
for(const entry of metadata.dependencies.openzeppelin.files){const data=fs.readFileSync(path.join(dir,entry.path));requireValue(sha(data)===entry.sha256,'PANCAKE_DEPENDENCY_HASH:'+entry.path);originals[entry.package_import]={content:data.toString()};}
function compile(engine,entries,helpers={}){
 const sources={...helpers},pending=[...entries];
 while(pending.length){const name=pending.pop();if(sources[name])continue;const source=originals[name];requireValue(source,'PANCAKE_IMPORT_MISSING:'+name);sources[name]=source;
  for(const match of source.content.matchAll(/import\s+(?:[^;]*?from\s+)?['"]([^'"]+)['"]/g)){
   let dep=match[1];if(dep.startsWith('.'))dep=path.posix.normalize(path.posix.join(path.posix.dirname(name),dep));
   else if(dep.startsWith('@pancakeswap/'))dep=dep.replace('@pancakeswap/','projects/');
   pending.push(dep);
  }
 }
 // Rewrite only import path spellings into the original monorepo layout.
 // The Solidity statements and all executable contract bodies remain unchanged.
 for(const [name,source]of Object.entries(sources))sources[name]={content:source.content.replace(/(['"])@pancakeswap\//g,'$1projects/')};
 const settings={optimizer:{enabled:true,runs:200},outputSelection:{'*':{'':['ast'],'*':['abi','evm.bytecode.object','evm.deployedBytecode.object','evm.deployedBytecode.immutableReferences','storageLayout']}}};
 const out=JSON.parse(engine.compile(JSON.stringify({language:'Solidity',sources,settings})));
 const errors=(out.errors||[]).filter(e=>e.severity==='error');requireValue(!errors.length,errors.map(e=>e.formattedMessage).join('\n'));
 const immutableNames={};function visit(node){if(!node||typeof node!=='object')return;if(node.nodeType==='VariableDeclaration'&&node.mutability==='immutable')immutableNames[node.id]=node.name;for(const value of Object.values(node)){if(Array.isArray(value))value.forEach(visit);else if(value&&typeof value==='object')visit(value);}}Object.values(out.sources).forEach(visit);
 return {contracts:out.contracts,immutable_names:immutableNames,input_sha256:sha(Buffer.from(JSON.stringify({language:'Solidity',sources,settings}))),sources:Object.keys(sources)};
}
const helper=`// SPDX-License-Identifier: GPL-2.0-or-later
pragma solidity =0.7.6;
import 'projects/v3-lm-pool/contracts/PancakeV3LmPool.sol';
import 'projects/v3-core/contracts/libraries/TickMath.sol';
contract ReferenceMath {function sqrt(int24 tick)external pure returns(uint160){return TickMath.getSqrtRatioAtTick(tick);}}
interface Pool {function swap(address,bool,int256,uint160,bytes calldata)external returns(int256,int256);function token0()external view returns(address);function token1()external view returns(address);}
interface Token {function transfer(address,uint256)external returns(bool);}
contract ReferenceToken {mapping(address=>uint256)public balanceOf;function mint(address to,uint256 value)external{balanceOf[to]+=value;}function transfer(address to,uint256 value)external returns(bool){require(balanceOf[msg.sender]>=value);balanceOf[msg.sender]-=value;balanceOf[to]+=value;return true;}}
contract ReferenceLmDeveloper {address public pool;address public masterChef;function parameters()external view returns(address,address){return(pool,masterChef);}function create(address p,address m)external returns(address){pool=p;masterChef=m;return address(new PancakeV3LmPool());}}
contract ReferenceCaller {function run(address p,address recipient,bool zero,int256 amount,uint160 limit)external returns(int256,int256){return Pool(p).swap(recipient,zero,amount,limit,abi.encode(p));}function pancakeV3SwapCallback(int256 a,int256 b,bytes calldata data)external{require(msg.sender==abi.decode(data,(address)));if(a>0)require(Token(Pool(msg.sender).token0()).transfer(msg.sender,uint256(a)));if(b>0)require(Token(Pool(msg.sender).token1()).transfer(msg.sender,uint256(b)));}}
`;
const corePath='projects/v3-core/contracts/',lmPath='projects/v3-lm-pool/contracts/',mcPath='projects/masterchef-v3/contracts/';
const core=compile(solc07,[corePath+'PancakeV3PoolDeployer.sol',corePath+'PancakeV3Factory.sol',lmPath+'PancakeV3LmPool.sol'],{'Reference.sol':{content:helper}});
const mc=compile(solc08,[mcPath+'MasterChefV3.sol']);
const get=(build,file,name)=>build.contracts[file][name];
const poolArtifact=get(core,corePath+'PancakeV3Pool.sol','PancakeV3Pool'),lmArtifact=get(core,lmPath+'PancakeV3LmPool.sol','PancakeV3LmPool'),mcArtifact=get(mc,mcPath+'MasterChefV3.sol','MasterChefV3');
const factoryArtifact=get(core,corePath+'PancakeV3Factory.sol','PancakeV3Factory');
const currentArtifacts={schema_version:1,source_commit:metadata.commit,source_metadata_sha256:sha(fs.readFileSync(path.join(dir,'source-metadata.json'))),compilers:{core:solc07.version(),masterChef:solc08.version(),masterChef_compiler_sha256:lfj.compiler.sha256},compilation:{core_input_sha256:core.input_sha256,masterChef_input_sha256:mc.input_sha256},immutable_names:{core:core.immutable_names,masterChef:mc.immutable_names},artifacts:{pool:poolArtifact,lm:lmArtifact,masterChef:mcArtifact,factory:factoryArtifact,token0:get(core,'Reference.sol','ReferenceToken'),token1:get(core,'Reference.sol','ReferenceToken'),caller:get(core,'Reference.sol','ReferenceCaller')}};
if(process.argv.includes('--artifacts-only')){const target=process.argv[process.argv.indexOf('--artifacts-only')+1];requireValue(target&&path.isAbsolute(target),'PANCAKE_ABSOLUTE_ARTIFACT_TARGET');fs.writeFileSync(target,JSON.stringify(currentArtifacts,null,2)+'\n');console.log(JSON.stringify({passed:true,artifacts_only:true}));process.exit(0);}
const word=n=>'0x'+BigInt.asUintN(256,BigInt(n)).toString(16).padStart(64,'0');
const mapSlot=(key,base,type='uint256')=>ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode([type,'uint256'],[key,base]));
const serial=value=>typeof value==='bigint'?value.toString():value;
const transport=ganache.provider({logging:{quiet:true},chain:{hardfork:'london',time:new Date('2026-11-08T00:00:00Z'),allowUnlimitedContractSize:true},miner:{timestampIncrement:0,blockGasLimit:50000000},wallet:{deterministic:true}});
const provider=new ethers.providers.Web3Provider(transport);provider.pollingInterval=20;
(async()=>{
 try{
  const signer=provider.getSigner(0),sender=await signer.getAddress(),recipient='0x000000000000000000000000000000000000beef';
  const deploy=async(artifact,args=[])=>{const c=await new ethers.ContractFactory(artifact.abi,artifact.evm.bytecode.object,signer).deploy(...args,{gasLimit:40000000});await c.deployed();return c;};
  const tokenA=await deploy(get(core,'Reference.sol','ReferenceToken')),tokenB=await deploy(get(core,'Reference.sol','ReferenceToken'));
  const tokens=[tokenA,tokenB].sort((a,b)=>a.address.toLowerCase().localeCompare(b.address.toLowerCase()));
  const poolDeployer=await deploy(get(core,corePath+'PancakeV3PoolDeployer.sol','PancakeV3PoolDeployer'));
  const factory=await deploy(get(core,corePath+'PancakeV3Factory.sol','PancakeV3Factory'),[poolDeployer.address]);
  await(await poolDeployer.setFactoryAddress(factory.address)).wait();await(await factory.createPool(tokens[0].address,tokens[1].address,500,{gasLimit:12000000})).wait();
  const pool=new ethers.Contract(await factory.getPool(tokens[0].address,tokens[1].address,500),poolArtifact.abi,signer);
  await(await pool.initialize('79228162514264337593543950336')).wait();
  const masterChef=await deploy(mcArtifact,[tokens[0].address,sender,tokens[1].address]);
  const developer=await deploy(get(core,'Reference.sol','ReferenceLmDeveloper'));
  const lmAddress=await developer.callStatic.create(pool.address,masterChef.address,{gasLimit:8000000});await(await developer.create(pool.address,masterChef.address,{gasLimit:8000000})).wait();
  const lm=new ethers.Contract(lmAddress,lmArtifact.abi,signer);await(await factory.setLmPool(pool.address,lm.address)).wait();
  const caller=await deploy(get(core,'Reference.sol','ReferenceCaller'));
  const math=await deploy(get(core,'Reference.sol','ReferenceMath'));const limits=[(await math.sqrt(-650)).toString(),(await math.sqrt(650)).toString()];
  const now=(await provider.getBlock('latest')).timestamp,indices=[-600,-120,120,600];
  let checkpoint=await provider.send('evm_snapshot',[]);
  const label=(artifact,name)=>{const item=artifact.storageLayout.storage.find(s=>s.label===name);requireValue(item,'PANCAKE_STORAGE_FIELD:'+name);return item;};
  async function set(address,slot,value){return provider.send('evm_setAccountStorageAt',[address,word(slot),word(value)]);}
  async function fields(address,artifact,type,slot,values){
   const definition=artifact.storageLayout.types[type];
   const members=definition.members;requireValue(members,'PANCAKE_EXPECTED_STRUCT:'+type);
   const words=new Map();
   for(const [name,value]of Object.entries(values)){
    const member=members.find(m=>m.label===name);requireValue(member,'PANCAKE_STRUCT_MEMBER:'+name);
    const width=Number(artifact.storageLayout.types[member.type].numberOfBytes)*8,at=BigInt(slot)+BigInt(member.slot);
    words.set(at,(words.get(at)||0n)|(BigInt.asUintN(width,BigInt(value))<<BigInt(member.offset*8)));
   }
   for(const [at,value]of words)await set(address,at,value);
  }
  async function rootFields(address,artifact,values){
   const slots=new Map();for(const [name,value]of Object.entries(values)){const item=label(artifact,name),width=Number(artifact.storageLayout.types[item.type].numberOfBytes)*8,at=BigInt(item.slot);slots.set(at,(slots.get(at)||0n)|(BigInt.asUintN(width,BigInt(value))<<BigInt(item.offset*8)));}
   for(const [at,value]of slots)await set(address,at,value);
  }
  const tuple=(value,names)=>Object.fromEntries(names.map((name,i)=>[name,typeof value[i]==='boolean'?value[i]:value[i].toString()]));
  async function capture(){
   const reads=[];
   async function read(contract,method,args=[]){const data=contract.interface.encodeFunctionData(method,args),result=await provider.call({to:contract.address,data,from:sender});reads.push({to:contract.address.toLowerCase(),data:data.toLowerCase(),from:sender.toLowerCase(),result:result.toLowerCase()});const decoded=contract.interface.decodeFunctionResult(method,result);return decoded.length===1?decoded[0]:decoded;}

   const slot0=tuple(await read(pool,'slot0'),['sqrtPriceX96','tick','observationIndex','observationCardinality','observationCardinalityNext','feeProtocol','unlocked']);
   const ticks=[],lmTicks=[],words=[];for(const index of indices){ticks.push({index,...tuple(await read(pool,'ticks',[index]),['liquidityGross','liquidityNet','feeGrowthOutside0X128','feeGrowthOutside1X128','tickCumulativeOutside','secondsPerLiquidityOutsideX128','secondsOutside','initialized'])});lmTicks.push({index,...tuple(await read(lm,'lmTicks',[index]),['liquidityGross','liquidityNet','rewardGrowthOutsideX128'])});}
   for(let i=-1;i<=0;i++)words.push({index:i,bitmap:(await read(pool,'tickBitmap',[i])).toString()});
   const observations=[];for(let i=0;i<4;i++)observations.push({index:i,...tuple(await read(pool,'observations',[i]),['blockTimestamp','tickCumulative','secondsPerLiquidityCumulativeX128','initialized'])});
   const [periodRate,periodEnd]=await read(masterChef,'getLatestPeriodInfo',[pool.address]);
   for(const method of ['factory','token0','token1','fee','tickSpacing','maxLiquidityPerTick','lmPool'])await read(pool,method);
   await read(factory,'getPool',[tokens[0].address,tokens[1].address,500]);await read(factory,'poolDeployer');
   for(const method of ['CAKE','WETH','nonfungiblePositionManager'])await read(masterChef,method);
   const chef={pid:(await read(masterChef,'v3PoolAddressPid',[pool.address])).toString(),poolInfo:tuple(await read(masterChef,'poolInfo',[1]),['allocPoint','v3Pool','token0','token1','fee','totalLiquidity','totalBoostLiquidity']),totalAllocPoint:(await read(masterChef,'totalAllocPoint')).toString(),latestPeriodCakePerSecond:(await read(masterChef,'latestPeriodCakePerSecond')).toString(),latestPeriodEndTime:(await read(masterChef,'latestPeriodEndTime')).toString()};
   const block=await provider.getBlock('latest');
   const state={masterChef:chef,slot0,liquidity:(await read(pool,'liquidity')).toString(),feeGrowthGlobal0X128:(await read(pool,'feeGrowthGlobal0X128')).toString(),feeGrowthGlobal1X128:(await read(pool,'feeGrowthGlobal1X128')).toString(),protocolFees:tuple(await read(pool,'protocolFees'),['token0','token1']),ticks,words,observations,raw0:(await read(tokens[0],'balanceOf',[pool.address])).toString(),raw1:(await read(tokens[1],'balanceOf',[pool.address])).toString(),lm:{pool:(await read(lm,'pool')).toLowerCase(),masterChef:(await read(lm,'masterChef')).toLowerCase(),rewardGrowthGlobalX128:(await read(lm,'rewardGrowthGlobalX128')).toString(),liquidity:(await read(lm,'lmLiquidity')).toString(),lastRewardTimestamp:(await read(lm,'lastRewardTimestamp')).toString(),ticks:lmTicks},period:{rewardPerSecond:periodRate.toString(),endTime:periodEnd.toString()}};const center=Math.floor(Number(slot0.tick)/10/256),canonicalWords=[];for(let index=center-1;index<=center+1;index++)canonicalWords.push({index,bitmap:(await read(pool,'tickBitmap',[index])).toString()});const canonicalState=structuredClone(state);canonicalState.words=canonicalWords;
   return {state,canonical_state:canonicalState,reads,block:{number:block.number,hash:block.hash,timestamp:block.timestamp}};
  }
  async function bank(mode){
   requireValue(await provider.send('evm_revert',[checkpoint]),'PANCAKE_SNAPSHOT_REVERT');checkpoint=await provider.send('evm_snapshot',[]);
   const active=2000000000000n,range=1000000000000n;
   const s0=label(poolArtifact,'slot0');await fields(pool.address,poolArtifact,s0.type,s0.slot,{sqrtPriceX96:1n<<96n,tick:0,observationIndex:0,observationCardinality:1,observationCardinalityNext:4,feeProtocol:3200+(3200*65536),unlocked:1});
   await rootFields(pool.address,poolArtifact,{liquidity:active,feeGrowthGlobal0X128:123n,feeGrowthGlobal1X128:456n});
   const pf=label(poolArtifact,'protocolFees');await fields(pool.address,poolArtifact,pf.type,pf.slot,{token0:7,token1:9});
   const tick=label(poolArtifact,'ticks'),tickType=poolArtifact.storageLayout.types[tick.type].value;
   const lmTick=label(lmArtifact,'lmTicks'),lmTickType=lmArtifact.storageLayout.types[lmTick.type].value;
   const bitmaps=new Map();
   for(const index of indices){
    const below=index<0,net=below?range:-range;
    await fields(pool.address,poolArtifact,tickType,mapSlot(index,tick.slot,'int24'),{liquidityGross:range,liquidityNet:net,feeGrowthOutside0X128:below?100:0,feeGrowthOutside1X128:below?200:0,tickCumulativeOutside:0,secondsPerLiquidityOutsideX128:0,secondsOutside:below?now-30:0,initialized:1});
    const compressed=index/10,wordPos=Math.floor(compressed/256),bit=((compressed%256)+256)%256;bitmaps.set(wordPos,(bitmaps.get(wordPos)||0n)|(1n<<BigInt(bit)));
    let lmRange=range/2n;if(mode==='zero-liquidity'||mode==='sparse'&&Math.abs(index)===120)lmRange=0n;
    await fields(lm.address,lmArtifact,lmTickType,mapSlot(index,lmTick.slot,'int24'),{liquidityGross:lmRange,liquidityNet:below?lmRange:-lmRange,rewardGrowthOutsideX128:below&&lmRange!==0n?50:0});
   }
   const bitmap=label(poolArtifact,'tickBitmap');for(const [key,value]of bitmaps)await set(pool.address,mapSlot(key,bitmap.slot,'int16'),value);
   const observation=label(poolArtifact,'observations'),observationType=poolArtifact.storageLayout.types[observation.type].base;
   await fields(pool.address,poolArtifact,observationType,observation.slot,{blockTimestamp:now-30,tickCumulative:0,secondsPerLiquidityCumulativeX128:0,initialized:1});
   await rootFields(lm.address,lmArtifact,{rewardGrowthGlobalX128:mode==='growth-wrap'?(1n<<256n)-10n:100n,lmLiquidity:mode==='zero-liquidity'||mode==='invalid-lm-curve'?0:mode==='sparse'?range/2n:range,lastRewardTimestamp:mode==='future'?now+1:mode==='same-block'?now:now-30});
   await rootFields(masterChef.address,mcArtifact,{totalAllocPoint:mode==='zero-alloc'?0:300,latestPeriodEndTime:mode==='ended'?now-60:mode==='cutoff'?now-7:mode==='end-wrap'?2n**32n+BigInt(now-7):now+600,latestPeriodCakePerSecond:7000000000000000n});
   const pid=label(mcArtifact,'v3PoolAddressPid');await set(masterChef.address,mapSlot(pool.address,pid.slot,'address'),1);
   const poolInfo=label(mcArtifact,'poolInfo'),poolInfoType=mcArtifact.storageLayout.types[poolInfo.type].value;
   await fields(masterChef.address,mcArtifact,poolInfoType,mapSlot(1,poolInfo.slot),{allocPoint:100,v3Pool:BigInt(pool.address),token0:BigInt(tokens[0].address),token1:BigInt(tokens[1].address),fee:500,totalLiquidity:2000000000000n,totalBoostLiquidity:1000000000000n});
   for(const token of tokens){await(await token.mint(pool.address,'1000000000000000000000000000000')).wait();await(await token.mint(caller.address,'1000000000000000000000000000000')).wait();}
  }
  const cases=[],sequences=[],invalidBankCases=[];
  async function swap(mode,zero,input){
   const beforeBank=await capture(),before=beforeBank.state,balances=await Promise.all(tokens.map(t=>t.balanceOf(recipient)));
   let consumed,output,reverted=false,reason=null,receipt=null;
   const limit=limits[zero?0:1];
   try{
    const result=await caller.callStatic.run(pool.address,recipient,zero,input,limit,{gasLimit:8000000});
    consumed=(zero?result[0]:result[1]).toString();output=(zero?result[1]:result[0]).mul(-1).toString();
    const r=await(await caller.run(pool.address,recipient,zero,input,limit,{gasLimit:8000000})).wait();receipt={transaction_hash:r.transactionHash,block_hash:r.blockHash,status:r.status,gas_used:r.gasUsed.toString()};
   }catch(error){reverted=true;reason=error.reason||error.message;}
   const afterBank=await capture(),after=afterBank.state,deltas=await Promise.all(tokens.map(async(t,i)=>(await t.balanceOf(recipient)).sub(balances[i]).toString()));
   if(reverted)requireValue(JSON.stringify(before)===JSON.stringify(after)&&deltas.every(v=>v==='0'),'PANCAKE_REVERT_NOT_ATOMIC');
   else requireValue(deltas[zero?1:0]===output&&deltas[zero?0:1]==='0','PANCAKE_RECIPIENT_DELTA');
   return {mode,zero_for_one:zero,amount_in:input,sqrt_price_limit_x96:limit,before,source_before:beforeBank,source_after:afterBank,expected:{reverted,reason,consumed_in:consumed,amount_out:output,recipient_deltas:deltas,after,receipt}};
  }
  for(const mode of ['active','same-block','future','ended','cutoff','end-wrap','zero-liquidity','zero-alloc','sparse','growth-wrap'])for(const zero of [true,false])for(const input of ['1','10000','1000000000','20000000000','1000000000000']){
   await bank(mode);cases.push({index:cases.length,...await swap(mode,zero,input)});if(cases.length%10===0)console.log(JSON.stringify({completed_cases:cases.length,mode}));
  }
  for(const zero of [true,false])for(const input of ['20000000000','1000000000000']){await bank('invalid-lm-curve');const c=await swap('invalid-lm-curve',zero,input);requireValue(c.expected.reverted,'PANCAKE_INVALID_LM_CURVE_MUST_REVERT');invalidBankCases.push({index:100+invalidBankCases.length,...c});}
  for(const mode of ['active','cutoff','sparse','growth-wrap']){await bank(mode);const steps=[];for(const zero of [true,true,false])steps.push(await swap(mode,zero,'10000000000'));sequences.push({mode,steps});}
  const deployment={};for(const [name,c]of Object.entries({factory,pool,masterChef,lm,developer,caller,token0:tokens[0],token1:tokens[1]}))deployment[name]={address:c.address.toLowerCase(),code:await provider.getCode(c.address)};
  const report={schema_version:1,passed:true,source_commit:metadata.commit,source_metadata_sha256:sha(fs.readFileSync(path.join(dir,'source-metadata.json'))),compilers:{core:solc07.version(),masterChef:solc08.version(),masterChef_compiler_sha256:lfj.compiler.sha256},compilation:{core_input_sha256:core.input_sha256,masterChef_input_sha256:mc.input_sha256},authority:'unmodified original PancakeV3Pool.swap, PancakeV3LmPool and MasterChefV3.getLatestPeriodInfo; controlled storage, constructors via reference LM developer, actual callback standard-token payments/recipient deltas; no historical deployment/chain qualification',fixed_timestamp:now,immutable_names:{core:core.immutable_names,masterChef:mc.immutable_names},layouts:{pool:poolArtifact.storageLayout,lm:lmArtifact.storageLayout,masterChef:mcArtifact.storageLayout,factory:factoryArtifact.storageLayout},artifacts:{pool:poolArtifact,lm:lmArtifact,masterChef:mcArtifact,factory:factoryArtifact,token0:get(core,'Reference.sol','ReferenceToken'),token1:get(core,'Reference.sol','ReferenceToken'),caller:get(core,'Reference.sol','ReferenceCaller')},deployment,cases,invalid_bank_cases:invalidBankCases,sequences,normal_profile_implemented:false,all_requested_features_complete:false,production_ready:false};
  const target=path.join(root,'integration/artifacts/local/core-pancake-v3-lm-original.json');fs.writeFileSync(target,JSON.stringify(report,(_,v)=>serial(v),2)+'\n');
  console.log(JSON.stringify({passed:true,cases:cases.length,full:cases.filter(c=>!c.expected.reverted&&c.expected.consumed_in===c.amount_in).length,partial:cases.filter(c=>!c.expected.reverted&&c.expected.consumed_in!==c.amount_in).length,reverts:cases.filter(c=>c.expected.reverted).length,invalid_bank_reverts:invalidBankCases.length,sequences:sequences.length,steps:sequences.flatMap(s=>s.steps).length}));
 }finally{await transport.disconnect();}
})().catch(error=>{console.error(error);process.exitCode=1;});
