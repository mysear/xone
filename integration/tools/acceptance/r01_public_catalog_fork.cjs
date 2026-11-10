#!/usr/bin/env node
'use strict';
// Public-origin fixed-H bank. All writes terminate inside an owned Ganache EVM;
// the only remote connection is an allowlisted read-only loopback archive.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const ROOT=path.resolve(__dirname,'../../..');
const {createRequire}=require('node:module'),r=createRequire(path.join(ROOT,'integration/.runtime/stable-reference/package.json'));
const {ethers}=r('ethers');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const num=x=>Number(BigInt(x)),dec=x=>BigInt(x).toString(),lower=x=>x.toLowerCase();
function quantity(x){if(x===undefined||x===null||BigInt(x)===0n)return '0x';let v=BigInt(x).toString(16);return '0x'+(v.length%2?'0':'')+v;}
function accessList(x){return (x||[]).map(v=>Array.isArray(v)?v:[v.address,v.storageKeys]);}
function serializeTransaction(t){
 const type=num(t.type||'0x0'),to=t.to||'0x',data=t.input||t.data||'0x';
 let fields;
 if(type===0)fields=[quantity(t.nonce),quantity(t.gasPrice),quantity(t.gas),to,quantity(t.value),data,quantity(t.v),quantity(t.r),quantity(t.s)];
 else{
  fields=[quantity(t.chainId),quantity(t.nonce)];
  if(type===1)fields.push(quantity(t.gasPrice));else fields.push(quantity(t.maxPriorityFeePerGas),quantity(t.maxFeePerGas));
  fields.push(quantity(t.gas),to,quantity(t.value),data,accessList(t.accessList));
  if(type===3)fields.push(quantity(t.maxFeePerBlobGas),t.blobVersionedHashes);
  if(type===4)fields.push((t.authorizationList||[]).map(a=>[quantity(a.chainId),a.address,quantity(a.nonce),quantity(a.yParity??a.v),quantity(a.r),quantity(a.s)]));
  assert([1,2,3,4].includes(type),'RPC_TRANSACTION_TYPE');
  fields.push(quantity(t.yParity??t.v),quantity(t.r),quantity(t.s));
 }
 const encoded=ethers.utils.RLP.encode(fields),raw=type?'0x'+type.toString(16).padStart(2,'0')+encoded.slice(2):encoded;
 assert.equal(ethers.utils.keccak256(raw),t.hash,'RPC_TRANSACTION_SIGNED_HASH');return raw;
}
function traceBoundary(trace,target){
 const contexts=new Map(),accounts=new Map(),touch=(a,slot)=>{a=lower(a);if(!accounts.has(a))accounts.set(a,new Set());if(slot)accounts.get(a).add(slot);};
 let previous;
 for(const log of trace.structLogs){
  if(!contexts.has(log.depth)){
   if(!previous)contexts.set(log.depth,lower(target));
   else if(log.depth>previous.depth){
    assert(['CALL','STATICCALL','DELEGATECALL','CALLCODE'].includes(previous.op),'TRACE_NEW_CONTEXT');
    const callee='0x'+previous.stack.at(-2).slice(-40);contexts.set(log.depth,['DELEGATECALL','CALLCODE'].includes(previous.op)?contexts.get(previous.depth):callee);
   }
  }
  if(previous&&log.depth>previous.depth){const callee='0x'+previous.stack.at(-2).slice(-40);contexts.set(log.depth,['DELEGATECALL','CALLCODE'].includes(previous.op)?contexts.get(previous.depth):callee);}
  const context=contexts.get(log.depth);assert(context,'TRACE_CONTEXT');touch(context);
  if(['SLOAD','SSTORE'].includes(log.op))touch(context,'0x'+log.stack.at(-1).padStart(64,'0'));
  if(['CALL','STATICCALL','DELEGATECALL','CALLCODE'].includes(log.op))touch('0x'+log.stack.at(-2).slice(-40));
  assert(!['CREATE','CREATE2','SELFDESTRUCT'].includes(log.op),'UNSUPPORTED_EXECUTION_LIFECYCLE');previous=log;
 }
 return Object.fromEntries([...accounts].map(([a,slots])=>[a,[...slots].sort()]));
}
async function run(mode,directory,url){
 const anchorFile=JSON.parse(fs.readFileSync(path.join(directory,'anchor.json'))),anchor=anchorFile.anchor;
 const full=JSON.parse(fs.readFileSync(path.join(ROOT,anchorFile.header.path))).response.result;
 const raws=full.transactions.map(serializeTransaction);
 if(mode==='capture'&&!fs.existsSync(path.join(directory,'raw-transactions.json')))fs.writeFileSync(path.join(directory,'raw-transactions.json'),JSON.stringify(raws,null,2)+'\n',{flag:'wx'});
 else assert.deepEqual(raws,JSON.parse(fs.readFileSync(path.join(directory,'raw-transactions.json'))),'BLOCK_TRANSACTIONS_REPLAY');
 assert(/^http:\/\/127\.0\.0\.1:\d+$/.test(url),'OWNED_READ_ONLY_ARCHIVE_REQUIRED');
 const sender='0x28c6c06298d514db089934071355e5743bf21d60',recipient='0x000000000000000000000000000000000000beef';
 const usdc='0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48',weth='0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2',dai='0x6b175474e89094c44da98b954eedeac495271d0f';
 const tokenIn=weth,tokenOut=dai,amount='100000000000000';
 const ganache=r('ganache'),raw=ganache.provider({fork:{url,blockNumber:anchor.height},chain:{chainId:1,networkId:1,hardfork:'shanghai',time:new Date(anchor.timestamp*1000)},
 wallet:{totalAccounts:0,unlockedAccounts:[sender]},miner:{timestampIncrement:1,blockGasLimit:30000000,coinbase:'0x000000000000000000000000000000000000cafe'},logging:{quiet:true}});
 const rpc=(method,params=[])=>raw.request({method,params}),provider=new ethers.providers.Web3Provider(raw);
 const erc20=new ethers.utils.Interface(['function balanceOf(address) view returns(uint256)','function allowance(address,address) view returns(uint256)','function approve(address,uint256) returns(bool)']);
 const balance=async(token,owner)=>BigInt(await rpc('eth_call',[{to:token,data:erc20.encodeFunctionData('balanceOf',[owner])},'latest']));
 const fee=(BigInt(full.baseFeePerGas)*2n+2000000000n).toString();
 const original={chain_id:num(await rpc('eth_chainId')),sender_native_balance:dec(await rpc('eth_getBalance',[sender,'latest'])),sender_nonce:dec(await rpc('eth_getTransactionCount',[sender,'latest'])),sender_input_token_balance:(await balance(tokenIn,sender)).toString(),recipient_output_token_balance:(await balance(tokenOut,recipient)).toString()};
 assert(BigInt(original.sender_native_balance)>1000000000000000n,'PUBLIC_SENDER_GAS_BALANCE');assert(BigInt(original.sender_native_balance)>BigInt(amount)+1000000000000000n,'PUBLIC_SENDER_NATIVE_INPUT_BALANCE');
 const cases=[],union={},environmentOpcodes={};
 async function transact(to,data,label,value='0x0'){
  const beforeHeader=await rpc('eth_getBlockByNumber',['latest',false]);
  const hash=await rpc('eth_sendTransaction',[{from:sender,to,data,value,gas:'0x1e8480',gasPrice:ethers.utils.hexlify(BigInt(fee)),type:'0x0'}]);
  const receipt=await rpc('eth_getTransactionReceipt',[hash]),transaction=await rpc('eth_getTransactionByHash',[hash]);
  if(num(receipt.status)!==1){
   const trace=await rpc('debug_traceTransaction',[hash,{disableMemory:true,disableStorage:true}]);let call;
   try{call={result:await rpc('eth_call',[{from:sender,to,data,gas:'0x1e8480'},beforeHeader.number])}}catch(error){call={error:error.message,data:error.data}}
   const target=path.join(directory,'failed-'+label+'-'+Date.now()+'.json');fs.writeFileSync(target,JSON.stringify({label,anchor,receipt,transaction,beforeHeader,returnValue:trace.returnValue,last_steps:trace.structLogs.slice(-15),call},null,2)+'\n',{flag:'wx'});
  }
  assert.equal(num(receipt.status),1,'ORIGINAL_RUNTIME_TRANSACTION_REVERT '+label);
  const header=await rpc('eth_getBlockByNumber',[receipt.blockNumber,false]);
  const trace=await rpc('debug_traceTransaction',[hash,{disableMemory:true,disableStorage:true}]);
  for(const step of trace.structLogs)if(['BLOCKHASH','COINBASE','PREVRANDAO','DIFFICULTY','BASEFEE','TIMESTAMP','NUMBER','CHAINID','GASPRICE','GASLIMIT'].includes(step.op))environmentOpcodes[step.op]=(environmentOpcodes[step.op]||0)+1;
  const boundary=traceBoundary(trace,to);for(const [a,slots] of Object.entries(boundary)){union[a]||=new Set();slots.forEach(s=>union[a].add(s));}
  return {label,before_header:beforeHeader,header,transaction,receipt,trace_sha256:sha(JSON.stringify(trace)),trace_steps:trace.structLogs.length,boundary};
 }
 try{
  const specs=[{name:'uniswap-v2-original-mainnet-state',router:'0x7a250d5630b4cf539739df2c5dacb4c659f2488d',pool:'0xa478c2975ab1ea89e8196811f51a7b7ade33eb11',protocol:'uniswap-v2'},
   {name:'uniswap-v3-original-mainnet-state',router:'0xe592427a0aece92de3edee1f18e0157c05861564',pool:'0xc2e9f25be6257c210d7adf0d4cd6e3e881ba25f8',protocol:'uniswap-v3'}];
  for(const spec of specs){
   const snapshot=await rpc('evm_snapshot');
   const buyBefore=await balance(tokenOut,recipient),sellBeforeInitial=await balance(tokenIn,sender),nativeBefore=BigInt(await rpc('eth_getBalance',[sender,'latest']));
   const deposit=await transact(weth,'0xd0e30db0','deposit-original-WETH',ethers.utils.hexlify(BigInt(amount)));
   const sellBefore=await balance(tokenIn,sender);
   const allowance=await rpc('eth_call',[{to:tokenIn,data:erc20.encodeFunctionData('allowance',[sender,spec.router])},'latest']);
   const approval=await transact(tokenIn,erc20.encodeFunctionData('approve',[spec.router,amount]),'approve-original-WETH');
   const approvalState={router:spec.router,sender,actual_transaction_from:approval.transaction.from,allowance:dec(await rpc('eth_call',[{to:tokenIn,data:erc20.encodeFunctionData('allowance',[sender,spec.router])},'latest'])),sender_balance:(await balance(tokenIn,sender)).toString(),receipt_logs:approval.receipt.logs};
   try{approvalState.transfer_from_call=await rpc('eth_call',[{from:spec.router,to:tokenIn,data:new ethers.utils.Interface(['function transferFrom(address,address,uint256) returns(bool)']).encodeFunctionData('transferFrom',[sender,spec.pool,amount])},'latest'])}catch(error){approvalState.transfer_from_error={message:error.message,data:error.data}}
   fs.writeFileSync(path.join(directory,'approval-state-'+spec.protocol+'-'+Date.now()+'.json'),JSON.stringify(approvalState,null,2)+'\n',{flag:'wx'});
   assert.equal(BigInt(approvalState.allowance),BigInt(amount),'APPROVE_STORAGE_UPDATE');
   const deadline=anchor.timestamp+3600;
   let data;
   if(spec.protocol==='uniswap-v2')data=new ethers.utils.Interface(['function swapExactTokensForTokens(uint256,uint256,address[],address,uint256) returns(uint256[])']).encodeFunctionData('swapExactTokensForTokens',[amount,'1',[tokenIn,tokenOut],recipient,deadline]);
   else data=new ethers.utils.Interface(['function exactInputSingle((address tokenIn,address tokenOut,uint24 fee,address recipient,uint256 deadline,uint256 amountIn,uint256 amountOutMinimum,uint160 sqrtPriceLimitX96)) payable returns(uint256)']).encodeFunctionData('exactInputSingle',[[tokenIn,tokenOut,3000,recipient,deadline,amount,'1','0']]);
   const swap=await transact(spec.router,data,spec.name);
   const buyAfter=await balance(tokenOut,recipient),sellAfter=await balance(tokenIn,sender),nativeAfter=BigInt(await rpc('eth_getBalance',[sender,'latest']));
   const delta=buyAfter-buyBefore;assert(delta>0n,'ACTUAL_NET_RECEIPT_MISSING');assert.equal(sellBefore-sellAfter,BigInt(amount),'ACTUAL_INPUT_CONSUMPTION');
   cases.push({...spec,anchor,deposit,approval,swap,original_allowance:dec(allowance),sender,recipient,token_in:tokenIn,token_out:tokenOut,amount_in:amount,native_value_deposited:amount,sender_input_before_deposit:sellBeforeInitial.toString(),recipient_before:buyBefore.toString(),recipient_after:buyAfter.toString(),actual_net_recipient:delta.toString(),sender_sell_before:sellBefore.toString(),sender_sell_after:sellAfter.toString(),sender_native_gas_spent:(nativeBefore-nativeAfter-BigInt(amount)).toString(),original_chain_mined_transaction:false});
   assert(await rpc('evm_revert',[snapshot]),'FORK_SNAPSHOT_REVERT');
  }
  // Materialize every opcode-derived original slot, including zero/absent slots,
  // so the parent can request proofs for a complete explicit prestate boundary.
  const remote=new ethers.providers.JsonRpcProvider(url);
  for(const [a,slots] of Object.entries(union)){
   await remote.send('eth_getCode',[a,ethers.utils.hexValue(anchor.height)]);
   await remote.send('eth_getBalance',[a,ethers.utils.hexValue(anchor.height)]);
   await remote.send('eth_getTransactionCount',[a,ethers.utils.hexValue(anchor.height)]);
   for(const key of slots)await remote.send('eth_getStorageAt',[a,key,ethers.utils.hexValue(anchor.height)]);
  }
  const result={schema_version:1,scope:'original-public-chain-state-at-fixed-H-with-explicit-local-simulation-environment',anchor,original,public_prestate_only:true,private_key_required:false,public_transactions_sent:0,
   overrides:{sender_authority:'public-sender-impersonated-only-in-owned-local-node',balance_overrides:[],storage_overrides:[],code_overrides:[],local_chain_id:1,local_hardfork:'shanghai',original_chain_hardfork_equivalence_claimed:false,
    timestamp:'each local block increments original H timestamp by 1 second',coinbase:'0x000000000000000000000000000000000000cafe',block_gas_limit:30000000,gas_price:fee,prior_setup:'original WETH deposit using existing sender ETH, then approve; all setup transactions retained per case'},
   opcode_boundary:Object.fromEntries(Object.entries(union).map(([a,s])=>[a,[...s].sort()])),ganache_version:r('ganache/package.json').version,ethers_version:ethers.version,cases};
  if(mode==='capture')fs.writeFileSync(path.join(directory,'fork-execution.json'),JSON.stringify(result,null,2)+'\n',{flag:'wx'});
  else assert.deepEqual(result,JSON.parse(fs.readFileSync(path.join(directory,'fork-execution.json'))),'EXACT_ORIGINAL_STATE_FORK_REPLAY');
  return {passed:true,scope:result.scope,anchor,cases:cases.map(c=>({protocol:c.protocol,actual_net_recipient:c.actual_net_recipient,input_consumed:amount})),public_transactions_sent:0,overrides:result.overrides,environment_opcode_counts:environmentOpcodes};
 }finally{await raw.disconnect();}
}
module.exports={serializeTransaction,traceBoundary,run};
if(require.main===module){process.on('uncaughtException',e=>{process.stderr.write('R01_FORK_UNCAUGHT: '+e.message+'\n'+String(e.stack).split('\n').slice(1,8).join('\n')+'\n');process.exit(1);});process.on('unhandledRejection',e=>{process.stderr.write('R01_FORK_REJECTION: '+String(e&&e.message||e)+'\n');process.exit(1);});const [mode,directory,url]=process.argv.slice(2);assert(['capture','replay'].includes(mode));run(mode,path.resolve(directory),url).then(result=>console.log(JSON.stringify(result))).catch(error=>{console.error(error.stack);process.exit(1);});}
