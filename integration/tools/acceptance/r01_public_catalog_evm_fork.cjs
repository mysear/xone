#!/usr/bin/env node
'use strict';
// All mutations below terminate in an owned in-process Ganache VM. The loopback
// archive refuses public writes and records every fixed-H public prestate read.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const ROOT=path.resolve(__dirname,'../../..'),{createRequire}=require('node:module'),r=createRequire(path.join(ROOT,'integration/.runtime/stable-reference/package.json')),{ethers}=r('ethers');
const {traceBoundary}=require('./r01_public_catalog_fork.cjs');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex'),load=p=>JSON.parse(fs.readFileSync(p));
const HELPER=`// SPDX-License-Identifier: MIT
pragma solidity 0.8.19;
interface T { function transfer(address,uint256) external returns(bool); function transferFrom(address,address,uint256) external returns(bool); }
interface V2 { function swap(uint256,uint256,address,bytes calldata) external; }
interface V3 { function swap(address,bool,int256,uint160,bytes calldata) external returns(int256,int256); }
interface LB { function swap(bool,address) external returns(bytes32); }
contract PublicStateCallback {
 address private activePool; address private input;
 function pay(address t,address from,address to,uint256 n) private { (bool ok,bytes memory v)=t.call(abi.encodeWithSelector(T.transferFrom.selector,from,to,n));require(ok&&(v.length==0||abi.decode(v,(bool))),"input transfer"); }
 function v2(address p,address tin,uint256 n,uint256 out,bool inputIs0,address recipient) external {pay(tin,msg.sender,p,n);V2(p).swap(inputIs0?0:out,inputIs0?out:0,recipient,"");}
 function v3(address p,address tin,uint256 n,bool zero,address recipient) external {require(activePool==address(0),"active");pay(tin,msg.sender,address(this),n);activePool=p;input=tin;V3(p).swap(recipient,zero,int256(n),zero?4295128740:1461446703485210103287273052203988822378723970341,"");activePool=address(0);input=address(0);}
 function lb(address p,address tin,uint256 n,bool toY,address recipient) external {pay(tin,msg.sender,p,n);LB(p).swap(toY,recipient);}
 function uniswapV3SwapCallback(int256 a,int256 b,bytes calldata) external {callback(a,b);}
 function pancakeV3SwapCallback(int256 a,int256 b,bytes calldata) external {callback(a,b);}
 function callback(int256 a,int256 b) private {require(msg.sender==activePool&&activePool!=address(0),"callback sender");uint256 n=uint256(a>0?a:b);(bool ok,bytes memory v)=input.call(abi.encodeWithSelector(T.transfer.selector,msg.sender,n));require(ok&&(v.length==0||abi.decode(v,(bool))),"callback payment");}
}
`;
function helperArtifact(){
 const version='0.8.19+commit.7dd6d404',manifest=load(path.join(ROOT,'integration/test/source/deployment-catalog/public-source-build-inputs.json')),pin=manifest.compilers.find(p=>p.version===version);assert(pin,'PUBLIC_HELPER_COMPILER_PIN');const compilerFile=path.join(ROOT,pin.blob.path);assert.equal(sha(fs.readFileSync(compilerFile)),pin.blob.sha256,'PUBLIC_HELPER_COMPILER_HASH');
 const compiler=r('solc/wrapper')(require(compilerFile)),input={language:'Solidity',sources:{'PublicStateCallback.sol':{content:HELPER}},settings:{optimizer:{enabled:true,runs:200},evmVersion:'paris',outputSelection:{'*':{'*':['abi','metadata','evm.bytecode','evm.deployedBytecode']}}}};
 const output=JSON.parse(compiler.compile(JSON.stringify(input)));assert(!(output.errors||[]).some(e=>e.severity==='error'));return {compiler:compiler.version(),compiler_sha256:pin.blob.sha256,input,contract:output.contracts['PublicStateCallback.sol'].PublicStateCallback};
}
function forkSource(remote,url,chain,height){
 if(chain!=='avalanche')return {url,blockNumber:height,disableCache:true};
 const reads=new Set(['eth_chainId','net_version','eth_blockNumber','eth_getBlockByNumber','eth_getBlockByHash','eth_getTransactionByHash','eth_getTransactionReceipt','eth_getBalance','eth_getTransactionCount','eth_getCode','eth_getStorageAt']);
 return {blockNumber:height,disableCache:true,provider:{request:({method,params=[]})=>{assert(reads.has(method),'PUBLIC_FORK_READ_ONLY');return remote.send(method,params);}}};
}
async function run(mode,directory,url){
 assert(/^http:\/\/127\.0\.0\.1:\d+$/.test(url),'OWNED_READ_ONLY_ARCHIVE');const anchor=load(path.join(directory,'anchor.json')).anchor,plan=load(path.join(directory,'execution-plan.json')),chainId=plan.chain_id,tag=ethers.utils.hexValue(anchor.height),remote=new ethers.providers.JsonRpcProvider(url,{name:'public-fixed-H',chainId});
 const originalHeader=load(path.join(ROOT,load(path.join(directory,'anchor.json')).header.path)).response.result;
 const requiredNative=(BigInt(originalHeader.baseFeePerGas||0)*2n+2000000000n)*12000000n+1000000000000000n;let sender;
 if(mode==='replay')sender=load(path.join(directory,'fork-execution.json')).sender;
 else {
  const extraFile=path.join(directory,'sender-candidates.json'),extra=fs.existsSync(extraFile)?load(extraFile).addresses:[];const candidates=[...new Set([...extra,...originalHeader.transactions.map(t=>t.from?.toLowerCase()).filter(Boolean)])];
  for(const a of candidates.slice(0,24)){
   const balance=BigInt(await remote.send('eth_getBalance',[a,tag]));if(balance<(requiredNative>100000000000000000n?requiredNative:100000000000000000n))continue;
   const code=await remote.send('eth_getCode',[a,tag]);if(code==='0x'){sender=a;break;}
  }
  assert(sender,'PUBLIC_FUNDED_EOA_UNAVAILABLE');
 }
 const senderBalance=await remote.send('eth_getBalance',[sender,tag]);assert(BigInt(senderBalance)>100000000000000000n);assert.equal(await remote.send('eth_getCode',[sender,tag]),'0x','PUBLIC_SENDER_EOA');
 const {input,contract,...pin}=helperArtifact();const helperReport={...pin,input,contract,authority:'new local callback helper; not an original public deployment'};
 if(mode==='capture')fs.writeFileSync(path.join(directory,'local-helper-compiler.json'),JSON.stringify(helperReport,null,2)+'\n',{flag:'wx'});else assert.deepEqual(helperReport,load(path.join(directory,'local-helper-compiler.json')));
 const ganache=r('ganache'),raw=ganache.provider({fork:forkSource(remote,url,plan.chain,anchor.height),chain:{chainId,networkId:chainId,hardfork:'shanghai',time:new Date(anchor.timestamp*1000)},wallet:{totalAccounts:0,unlockedAccounts:[sender]},miner:{timestampIncrement:1,blockGasLimit:30000000,coinbase:'0x000000000000000000000000000000000000cafe'},logging:{quiet:true}}),rpc=(method,params=[])=>raw.request({method,params}),provider=new ethers.providers.Web3Provider(raw);
 assert.equal(raw.getOptions().fork.disableCache,true,'UNVERIFIED_PERSISTENT_FORK_CACHE');
 const iface=new ethers.utils.Interface(contract.abi),erc=new ethers.utils.Interface(['function balanceOf(address) view returns(uint256)','function approve(address,uint256) returns(bool)','function token0() view returns(address)','function getAmountOut(uint256,address) view returns(uint256)','function getReserves() view returns(uint112,uint112,uint32)','function getTokenX() view returns(address)','function getTokenY() view returns(address)']);
 const recipient='0x000000000000000000000000000000000000beef',fee=(BigInt(originalHeader.baseFeePerGas||0)*2n+2000000000n).toString(),cases=[],union={},environment={};
 const merge=trace=>{for(const [a,slots] of Object.entries(trace)){union[a]=[...new Set([...(union[a]||[]),...slots])].sort();}};
 const view=async(to,fn,args=[])=>{const data=erc.encodeFunctionData(fn,args);return erc.decodeFunctionResult(fn,await rpc('eth_call',[{to,data},'latest']))[0];};
 const balance=async(t,a)=>BigInt(await view(t,'balanceOf',[a]));
 async function transact(to,data,value='0x0'){
  const before=await rpc('eth_getBlockByNumber',['latest',false]);const tx={from:sender,data,value,gas:'0x5b8d80',gasPrice:ethers.utils.hexValue(BigInt(fee)),type:'0x0'};if(to)tx.to=to;
  const hash=await rpc('eth_sendTransaction',[tx]),receipt=await rpc('eth_getTransactionReceipt',[hash]),transaction=await rpc('eth_getTransactionByHash',[hash]),header=await rpc('eth_getBlockByNumber',[receipt.blockNumber,false]);
  const trace=await rpc('debug_traceTransaction',[hash,{disableMemory:true,disableStorage:true}]);merge(traceBoundary(trace,to||receipt.contractAddress));for(const l of trace.structLogs)if(['TIMESTAMP','NUMBER','COINBASE','BASEFEE','GASLIMIT','PREVRANDAO','DIFFICULTY','BLOCKHASH','CHAINID','ORIGIN','GASPRICE'].includes(l.op))environment[l.op]=(environment[l.op]||0)+1;
  return {transaction,receipt,before_header:before,header,trace_sha256:sha(JSON.stringify(trace)),return_value:trace.returnValue||'',status:Number(BigInt(receipt.status))};
 }
 try{
  for(const c of plan.cases){const snapshot=await rpc('evm_snapshot',[]);let result={...c,passed:false,transactions:[],original_chain_mined_transaction:false};
   try{
    const before=await balance(c.token_out,recipient),inputBefore=await balance(c.token_in,sender),nativeBefore=await rpc('eth_getBalance',[sender,'latest']);
    const creation=await transact(null,'0x'+contract.evm.bytecode.object);result.transactions.push(creation);assert.equal(creation.status,1,'LOCAL_HELPER_DEPLOYMENT');const helper=creation.receipt.contractAddress.toLowerCase();result.local_helper=helper;const code=await rpc('eth_getCode',[helper,'latest']);assert.equal(code,'0x'+contract.evm.deployedBytecode.object,'LOCAL_HELPER_RUNTIME');
    const deposit=await transact(c.token_in,'0xd0e30db0',ethers.utils.hexValue(BigInt(c.amount_in)));result.transactions.push(deposit);assert.equal(deposit.status,1,'ORIGINAL_WRAPPED_DEPOSIT');
    const approve=await transact(c.token_in,erc.encodeFunctionData('approve',[helper,c.amount_in]));result.transactions.push(approve);assert.equal(approve.status,1,'ORIGINAL_TOKEN_APPROVE');
    let data;
    if(c.kind==='v3'){
     const token0=(await view(c.pool,'token0')).toLowerCase();data=iface.encodeFunctionData('v3',[c.pool,c.token_in,c.amount_in,token0===c.token_in,recipient]);
    }else if(c.kind==='lb'){
     const tokenY=(await view(c.pool,'getTokenY')).toLowerCase();data=iface.encodeFunctionData('lb',[c.pool,c.token_in,c.amount_in,c.token_out===tokenY,recipient]);
    }else{
     const token0=(await view(c.pool,'token0')).toLowerCase();let quoted;
     if(c.kind==='v2-solidly')quoted=await view(c.pool,'getAmountOut',[c.amount_in,c.token_in]);
     else {const reserves=erc.decodeFunctionResult('getReserves',await rpc('eth_call',[{to:c.pool,data:erc.encodeFunctionData('getReserves')},'latest']));const rin=BigInt(reserves[token0===c.token_in?0:1]),rout=BigInt(reserves[token0===c.token_in?1:0]),adjusted=BigInt(c.amount_in)*9975n;quoted=adjusted*rout/(rin*10000n+adjusted);}
     result.original_pool_quote=quoted.toString();data=iface.encodeFunctionData('v2',[c.pool,c.token_in,c.amount_in,quoted.toString(),token0===c.token_in,recipient]);
    }
    const swap=await transact(helper,data);result.transactions.push(swap);assert.equal(swap.status,1,'ORIGINAL_POOL_SWAP_REVERTED');
    const after=await balance(c.token_out,recipient),inputAfter=await balance(c.token_in,sender);assert(after>before,'ORIGINAL_NET_RECIPIENT_POSITIVE');assert.equal(inputAfter,inputBefore,'ORIGINAL_WRAPPED_INPUT_NET_ZERO_AFTER_DEPOSIT_SWAP');
    Object.assign(result,{passed:true,recipient,recipient_before:before.toString(),recipient_after:after.toString(),actual_net_recipient:(after-before).toString(),sender_input_before:inputBefore.toString(),sender_input_after:inputAfter.toString(),sender_native_before:nativeBefore,sender_native_after:await rpc('eth_getBalance',[sender,'latest'])});
   }catch(error){result.error=error.reason||error.message;result.error_code=error.code||null;}
   cases.push(result);await rpc('evm_revert',[snapshot]);
  }
  // Explicitly retain original factory/pool/implementation identities at the same H.
  for(const c of plan.cases){for(const a of [c.factory,c.pool,c.pool_implementation?.address].filter(Boolean)){await remote.send('eth_getCode',[a,tag]);for(const slot of ['0x360894a13ba1a3210667c828492db98dca3e2076cc3735a920a3ca505d382bbc','0xa3f0ad74e5423aebfd80d3ef4346578335a9a72aeaee59ff6cb3582b35133d50'])await remote.send('eth_getStorageAt',[a,slot,tag]);}}
  for(const [a,slots] of Object.entries(union)){
   // Local helper did not exist in the original bank. Its local storage is an
   // explicit setup effect, not a claim about original pool persistent state.
   if(cases.some(c=>c.local_helper===a))continue;
   await remote.send('eth_getCode',[a,tag]);await remote.send('eth_getBalance',[a,tag]);await remote.send('eth_getTransactionCount',[a,tag]);for(const slot of slots)await remote.send('eth_getStorageAt',[a,slot,tag]);
  }
  const result={schema_version:2,chain:plan.chain,chain_id:chainId,anchor,sender,sender_original_native_balance:senderBalance,recipient,cases,opcode_boundary:union,environment_opcode_counts:environment,local_hardfork:'shanghai',current_native_VM_equivalence_claimed:false,local_helper_newly_deployed:true,balance_overrides:[],storage_overrides:[],code_overrides:[],public_transactions_sent:0,original_chain_mined_transaction:false,original_native_block_transactions_replayed:false,ganache_version:r('ganache/package.json').version,gas_price:fee};
  if(mode==='capture')fs.writeFileSync(path.join(directory,'fork-execution.json'),JSON.stringify(result,null,2)+'\n',{flag:'wx'});else assert.deepEqual(result,load(path.join(directory,'fork-execution.json')),'EXACT_MULTICHAIN_ORIGINAL_POOL_REPLAY');
  return {passed:true,cases:cases.map(c=>({id:c.catalog_deployment_id,passed:c.passed,actual_net_recipient:c.actual_net_recipient,error:c.error})),chain:plan.chain,environment_opcode_counts:environment,public_transactions_sent:0};
 }finally{await raw.disconnect();}
}
module.exports={HELPER,helperArtifact,forkSource,run};
if(require.main===module){const [mode,directory,url]=process.argv.slice(2);assert(['capture','replay'].includes(mode));run(mode,path.resolve(directory),url).then(r=>console.log(JSON.stringify(r))).catch(e=>{console.error(e.stack);process.exit(1)});}
