// SPDX-License-Identifier: MIT
// Original getSwapIn targets and actual Pair output, including output quantization. No service admission.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../../..'),dir=path.join(root,'integration/tools/reference/lfj-v21');
const nodeModules=path.join(root,'integration/.runtime/stable-reference/node_modules');
const wrapper=require(path.join(nodeModules,'solc/wrapper'));
const ganache=require(path.join(nodeModules,'ganache')),{ethers}=require(path.join(nodeModules,'ethers'));
const metadata=JSON.parse(fs.readFileSync(path.join(dir,'source-metadata.json')));
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const compilerPath=path.join(root,'integration/.runtime/lfj-solc',metadata.compiler.path);
if('0x'+sha(fs.readFileSync(compilerPath))!==metadata.compiler.sha256)throw Error('LFJ_COMPILER_HASH');
const solc=wrapper(require(compilerPath));
if(!solc.version().startsWith('0.8.10+commit.fc410830'))throw Error('LFJ_COMPILER_VERSION');
const sources={};
for(const [name,digest] of Object.entries(metadata.files)){
 const raw=fs.readFileSync(path.join(dir,name));if(sha(raw)!==digest)throw Error('LFJ_SOURCE_HASH:'+name);
 sources[name]={content:raw.toString('utf8')};
}
for(const [name,entry] of Object.entries(metadata.external_files)){
 const raw=fs.readFileSync(path.join(dir,name));if(sha(raw)!==entry.sha256)throw Error('LFJ_EXTERNAL_SOURCE_HASH:'+name);
 sources[name]={content:raw.toString('utf8')};
}
sources['ReferenceToken.sol']={content:`// SPDX-License-Identifier: MIT
pragma solidity 0.8.10;
contract ReferenceToken {
 mapping(address=>uint256) public balanceOf;
 function mint(address to,uint256 amount) external { balanceOf[to]+=amount; }
 function transfer(address to,uint256 amount) external returns(bool) { balanceOf[msg.sender]-=amount;balanceOf[to]+=amount;return true; }
}`};
const compiled=JSON.parse(solc.compile(JSON.stringify({language:'Solidity',sources,settings:{optimizer:{enabled:true,runs:800},outputSelection:{'*':{'*':['abi','evm.bytecode.object','evm.deployedBytecode','storageLayout']}}}})));
const errors=(compiled.errors||[]).filter(e=>e.severity==='error');if(errors.length)throw Error(errors.map(e=>e.formattedMessage).join('\n'));
const pairArtifact=compiled.contracts['src/LBPair.sol'].LBPair;
const tokenArtifact=compiled.contracts['ReferenceToken.sol'].ReferenceToken;
const storage=Object.fromEntries(pairArtifact.storageLayout.storage.map(s=>[s.label,s.slot]));
for(const [name,value] of Object.entries({_status:'3',_parameters:'4',_reserves:'5',_protocolFees:'6',_bins:'7',_tree:'8',_oracle:'11'}))
 if(storage[name]!==value)throw Error('LFJ_ORIGINAL_STORAGE_LAYOUT:'+name);
const transport=ganache.provider({logging:{quiet:true},chain:{hardfork:'london',time:new Date('2026-11-07T00:00:00Z')},miner:{timestampIncrement:0},wallet:{deterministic:true}});
const provider=new ethers.providers.Web3Provider(transport);provider.pollingInterval=20;
const word=n=>'0x'+BigInt(n).toString(16).padStart(64,'0');
const packed=(x,y)=>BigInt(x)+(BigInt(y)<<128n);
const slot=(key,index)=>ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['uint256','uint256'],[key.toString(),index.toString()]));
const encode=fields=>fields.reduce((n,[value,offset])=>n+(BigInt(value)<<BigInt(offset)),0n);
const amount=v=>v.toString();
(async()=>{
 try{
  const signer=provider.getSigner(0),sender=await signer.getAddress(),recipient='0x000000000000000000000000000000000000beef';
  const deploy=async(artifact,args=[])=>{const c=await new ethers.ContractFactory(artifact.abi,artifact.evm.bytecode.object,signer).deploy(...args);await c.deployed();return c;};
  const x=await deploy(tokenArtifact),y=await deploy(tokenArtifact),factory='0x000000000000000000000000000000000000feed';
  await provider.send('evm_setAccountCode',[factory,'0x00']);
  const implementation=await deploy(pairArtifact,[factory]);
  const pairAddress='0x000000000000000000000000000000000000aabb',step=20;
  const clone='0x363d3d373d3d3d3d61002c806035363936013d73'+implementation.address.slice(2)+'5af43d3d93803e603357fd5bf3'+x.address.slice(2)+y.address.slice(2)+step.toString(16).padStart(4,'0')+'002c';
  await provider.send('evm_setAccountCode',[pairAddress,clone]);
  const pair=new ethers.Contract(pairAddress,pairArtifact.abi,signer);
  if((await pair.getTokenX()).toLowerCase()!==x.address.toLowerCase()||(await pair.getTokenY()).toLowerCase()!==y.address.toLowerCase()||(await pair.getBinStep())!==step)throw Error('LFJ_CLONE_ARG_BINDING');
  let checkpoint=await provider.send('evm_snapshot',[]);
  const set=async(index,value)=>provider.send('evm_setAccountStorageAt',[pairAddress,typeof index==='string'?index:word(index),word(value)]);
  const all=[],sequences=[];const now=BigInt((await provider.getBlock('latest')).timestamp);
  const receiptFields=r=>({transaction_hash:r.transactionHash,block_hash:r.blockHash,block_number:r.blockNumber,status:r.status,gas_used:r.gasUsed.toString()});
  async function captureState(before) {
   const parameters=BigInt(await provider.getStorageAt(pairAddress,4)),reserves=BigInt(await provider.getStorageAt(pairAddress,5)),protocolFees=BigInt(await provider.getStorageAt(pairAddress,6));
   const mask=(1n<<128n)-1n;
   const after={...before,parameters:String(parameters),raw_reserve0:String(reserves&mask),raw_reserve1:String(reserves>>128n),protocol_fees0:String(protocolFees&mask),protocol_fees1:String(protocolFees>>128n),bins:[],oracle_samples:[]};
   for(const bin of before.bins){const value=BigInt(await provider.getStorageAt(pairAddress,slot(bin.id,7)));after.bins.push({...bin,reserve0:String(value&mask),reserve1:String(value>>128n)});}
   for(const sample of before.oracle_samples)after.oracle_samples.push({index:sample.index,data:String(BigInt(await provider.getStorageAt(pairAddress,11+sample.index-1)))});
   return after;
  }
  const readCache=new Map();
  async function originalReads(before,block) {
   if(readCache.has(block.hash))return readCache.get(block.hash);
   const calls=[];
   async function read(contract,signature,args=[]) {
    const data=contract.interface.encodeFunctionData(signature,args).toLowerCase();
    const result=await provider.call({to:contract.address,data},block.number);
    calls.push({to:contract.address.toLowerCase(),data,result,block_hash:block.hash,block_number:block.number});
   }
   for(const signature of ['getFactory()','getTokenX()','getTokenY()','getBinStep()','getActiveId()','getStaticFeeParameters()','getVariableFeeParameters()','getReserves()','getProtocolFees()'])await read(pair,signature);
   await read(x,'balanceOf(address)',[pairAddress]);await read(y,'balanceOf(address)',[pairAddress]);
   await read(pair,'getNextNonEmptyBin(bool,uint24)',[false,0]);
   for(const bin of before.bins){
    await read(pair,'getBin(uint24)',[bin.id]);await read(pair,'totalSupply(uint256)',[bin.id]);
    await read(pair,'getNextNonEmptyBin(bool,uint24)',[false,bin.id]);await read(pair,'getNextNonEmptyBin(bool,uint24)',[true,bin.id]);
   }
   const storage={};
   for(const index of [3,4,5,6,...before.oracle_samples.map(s=>11+s.index-1)])storage['0x'+index.toString(16)]=await provider.getStorageAt(pairAddress,index,block.number);
   if((await provider.getBlock(block.number)).hash!==block.hash)throw Error('LFJ_ORIGINAL_READ_ANCHOR_CHANGED');
   const result={getter_reads:calls,storage_reads:storage};readCache.set(block.hash,result);return result;
  }
  async function executeTarget(before,target,zero) {
   const header=await provider.getBlock('latest'),reads=await originalReads(before,header);
   const original=await pair.getSwapIn(String(target),zero);
   const observed={target_output:String(target),zero_for_one:zero,before,...reads,original_before_block:{number:header.number,hash:header.hash,parent_hash:header.parentHash,timestamp:header.timestamp},original_input_view:{required_input:original.amountIn.toString(),amount_out_left:original.amountOutLeft.toString(),fee:original.fee.toString()}};
   if(!original.amountOutLeft.isZero()){observed.expected={incomplete_target:true,swapped:false};return observed;}
   const input=original.amountIn,inputToken=zero?x:y,outputToken=zero?y:x;
   const forward=await pair.getSwapOut(input,zero),less=await pair.getSwapOut(input.sub(1),zero);
   observed.original_forward_view={amount_out:forward.amountOut.toString(),amount_in_left:forward.amountInLeft.toString(),fee:forward.fee.toString()};
   observed.minimum_input_control={input:input.sub(1).toString(),amount_out:less.amountOut.toString(),below_target:less.amountOut.lt(String(target))};
   await(await inputToken.mint(sender,input)).wait();const startInput=await inputToken.balanceOf(sender),previous=await outputToken.balanceOf(recipient);
   const funding=await(await inputToken.transfer(pairAddress,input)).wait();
   const receipt=await(await pair.swap(zero,recipient,{gasLimit:10000000})).wait();
   const received=(await outputToken.balanceOf(recipient)).sub(previous),paid=startInput.sub(await inputToken.balanceOf(sender));
   if(received.lt(String(target))||!received.eq(forward.amountOut)||!paid.eq(input)||!forward.amountInLeft.isZero())throw Error('LFJ_MINIMUM_TARGET_FUNDS_MISMATCH');
   const after=await captureState(before),block=await provider.getBlock(receipt.blockNumber),afterReads=await originalReads(after,block);
   if(BigInt(block.timestamp)!==now||(await x.balanceOf(pairAddress)).toString()!==after.raw_reserve0||(await y.balanceOf(pairAddress)).toString()!==after.raw_reserve1)throw Error('LFJ_MINIMUM_TARGET_POST_STATE');
   observed.expected={incomplete_target:false,swapped:true,actual_output:received.toString(),output_surplus:received.sub(String(target)).toString(),payer_token_delta:paid.toString(),after,...afterReads,original_after_block:{number:block.number,hash:block.hash,parent_hash:block.parentHash,timestamp:block.timestamp},funding_receipt:receiptFields(funding),receipt:receiptFields(receipt)};
   return observed;
  }
  for(const activeOffset of [-500,0,500])for(const zero of [true,false])for(const dynamic of [false,true])for(const oracle of [false,true])for(const elapsed of [0,10,1000])for(const input of [1n,10000n,2500000n,100000000n]){
   await provider.send('evm_revert',[checkpoint]);checkpoint=await provider.send('evm_snapshot',[]);
   const active=8388608+activeOffset,ids=[active-2,active-1,active,active+1,active+2];
   const bins=ids.map((id,i)=>({id,reserve0:1000000n+BigInt(i)*1234n,reserve1:1000000n+BigInt(4-i)*4321n,total_shares:1000000n}));
   const initialParams=encode([[20,0],[5,16],[120,28],[5000,40],[dynamic?50000:0,54],[1000,78],[100000,92],[20000,112],[1000,132],[active-1,152],[now-BigInt(elapsed),176],[oracle?1:0,216],[active,232]]);
   const fees=[11n,13n],raw=[bins.reduce((n,b)=>n+b.reserve0,fees[0]),bins.reduce((n,b)=>n+b.reserve1,fees[1])];
   await set(3,1);await set(4,initialParams);await set(5,packed(...raw));await set(6,packed(...fees));
   let rootBits=0n;const level1=new Map(),level2=new Map();
   for(const b of bins){
    await set(slot(b.id,7),packed(b.reserve0,b.reserve1));await set(slot(b.id,1),b.total_shares);
    const key2=b.id>>>8,key1=key2>>>8;
    rootBits|=1n<<BigInt(key1);level1.set(key1,(level1.get(key1)||0n)|(1n<<BigInt(key2&255)));level2.set(key2,(level2.get(key2)||0n)|(1n<<BigInt(b.id&255)));
   }
   await set(8,rootBits);for(const [key,bits]of level1)await set(slot(key,9),bits);for(const [key,bits]of level2)await set(slot(key,10),bits);
   const samples=oracle?[{index:1,data:encode([[2,0],[17,16],[19,80],[23,144],[0,208],[now-200n,216]])},{index:2,data:encode([[2,0],[1,16],[1,80],[1,144],[0,208],[now-1000n,216]])}]:[];
   for(const sample of samples)await set(11+sample.index-1,sample.data);
   await(await x.mint(pairAddress,String(raw[0]))).wait();await(await y.mint(pairAddress,String(raw[1]))).wait();
   const sourceBlock=await provider.getBlock('latest');
   const before={timestamp:String(now),bin_step:step,parameters:String(initialParams),raw_reserve0:String(raw[0]),raw_reserve1:String(raw[1]),protocol_fees0:String(fees[0]),protocol_fees1:String(fees[1]),bins:bins.map(b=>({...b,reserve0:amount(b.reserve0),reserve1:amount(b.reserve1),total_shares:amount(b.total_shares)})),oracle_samples:samples.map(s=>({...s,data:amount(s.data)}))};
   const observed=await executeTarget(before,input,zero);observed.active_offset=activeOffset;observed.dynamic=dynamic;observed.oracle=oracle;observed.elapsed=elapsed;
   const caseIndex=all.length;all.push(observed);
   if(observed.expected.swapped&&dynamic&&oracle&&elapsed===10&&input===2500000n){
    const sequence={initial_case_index:caseIndex,steps:[observed]};let current=observed.expected.after;
    for(const target of [10000n,5000n]){const next=await executeTarget(current,target,zero);if(!next.expected.swapped)throw Error('LFJ_TARGET_SEQUENCE_PARTIAL');sequence.steps.push(next);current=next.expected.after;}
    sequences.push(sequence);
   }
  }
  const loaded_dependencies=Object.keys(require.cache).filter(file=>file.startsWith(root+path.sep)).sort().map(file=>({path:path.relative(root,file),bytes:fs.statSync(file).size,sha256:sha(fs.readFileSync(file))}));
  const report={schema_version:1,passed:true,quote_kind:'ORIGINAL_GET_SWAP_IN_MINIMUM_TARGET',generator_sha256:sha(fs.readFileSync(__filename)),runtime:{node_version:process.version,node_executable_sha256:sha(fs.readFileSync(process.execPath)),loaded_dependencies},source_tag:metadata.tag,source_metadata_sha256:sha(fs.readFileSync(path.join(dir,'source-metadata.json'))),compiler:solc.version(),storage_layout:pairArtifact.storageLayout,deployment:{factory:factory.toLowerCase(),factory_code:await provider.getCode(factory),implementation:implementation.address.toLowerCase(),implementation_code:await provider.getCode(implementation.address),token0:x.address.toLowerCase(),token0_code:await provider.getCode(x.address),token1:y.address.toLowerCase(),token1_code:await provider.getCode(y.address),pool:pairAddress.toLowerCase(),pool_code:clone.toLowerCase(),sender:sender.toLowerCase(),recipient:recipient.toLowerCase()},authority:'unchanged original v2.1.0 getSwapIn/getSwapOut and LBPair swap, controlled clone/storage/factory sentinel, separately prefunded standard Token transfers and recipient deltas; not an original Router or atomic funding executor, real Avalanche bank or service admission',cases:all,sequences,original_pair_swap_executed:true,exact_out_admission:false,all_requested_features_complete:false,production_ready:false};
  const output=path.join(root,'integration/artifacts/local/core-lfj-v21-exact-out-original-20261110.json');fs.writeFileSync(output,JSON.stringify(report,null,2)+'\n');
  console.log(JSON.stringify({passed:true,cases:all.length,sequences:sequences.length,sequence_steps:sequences.reduce((n,s)=>n+s.steps.length,0),full_targets:all.filter(c=>c.expected.swapped).length,incomplete_targets:all.filter(c=>c.expected.incomplete_target).length,excess_output_cases:all.filter(c=>c.expected.swapped&&c.expected.output_surplus!=='0').length,nonminimal_getSwapIn_cases:all.filter(c=>c.minimum_input_control&&!c.minimum_input_control.below_target).length,path:output}));
 }finally{await transport.disconnect();}
})().catch(error=>{console.error(error);process.exitCode=1;});
