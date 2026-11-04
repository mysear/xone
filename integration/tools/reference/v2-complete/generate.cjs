'use strict';
// Run unchanged published artifacts through ordinary signed transactions only.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),zlib=require('node:zlib');
const assert=require('node:assert/strict'),{createRequire}=require('node:module');
const ROOT=path.resolve(__dirname,'../../../..'),OLD=path.join(ROOT,'integration/tools/reference/evm-execution'),RUNTIME=path.join(ROOT,'integration/.runtime/stable-reference');
const rr=createRequire(path.join(RUNTIME,'package.json')),{ethers}=rr('ethers'),ganache=rr('ganache'),ganacheVersion=rr('ganache/package.json').version;
const sha=b=>crypto.createHash('sha256').update(b).digest('hex'),json=x=>JSON.stringify(x,null,2)+'\n';
const codeSha=x=>sha(Buffer.from(x.slice(2),'hex')),addr=x=>x.toLowerCase(),word=x=>'0x'+BigInt(x==='0x'?0:x).toString(16).padStart(64,'0');
const num=x=>{const n=BigInt(x);assert(n>=0n&&n<=BigInt(Number.MAX_SAFE_INTEGER));return Number(n);};
const mapping=(key,slot)=>ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['address','uint256'],[key,slot]));
const nested=(a,b,s)=>mapping(b,mapping(a,s));
const PROFILE='uniswap-v2-complete',ADAPTER='uniswap-v2/core-1.0.1-full-swap-1';
function artifact(pkg,name){const file=`packages/${pkg}/build/${name}.json`,a=JSON.parse(fs.readFileSync(path.join(OLD,file)));assert.equal(a.bytecode.replace(/^0x/,''),a.evm.bytecode.object.replace(/^0x/,''));return {file,abi:a.abi,name,creation:'0x'+a.evm.bytecode.object.replace(/^0x/,''),template:'0x'+a.evm.deployedBytecode.object.replace(/^0x/,''),immutables:a.evm.deployedBytecode.immutableReferences||{}};}
function pins(){const p=JSON.parse(fs.readFileSync(path.join(OLD,'source-pins.json'))),seen=new Set();for(const pkg of p.packages){for(const f of pkg.files){assert(!seen.has(f.path));seen.add(f.path);assert.equal(sha(fs.readFileSync(path.join(OLD,f.path))),f.sha256);}const name=pkg.name==='@uniswap/v2-core'?'core':pkg.name==='@uniswap/lib'?'lib':'periphery';assert.equal(sha(fs.readFileSync(path.join(OLD,`${name}-registry.json`))),pkg.metadata_sha256);}assert.equal(seen.size,93);function walk(dir){for(const e of fs.readdirSync(dir,{withFileTypes:true})){assert(!e.isSymbolicLink());const p=path.join(dir,e.name);if(e.isDirectory())walk(p);else assert(seen.has(path.relative(OLD,p)));}}walk(path.join(OLD,'packages'));return p;}
function footprint(trace,target,created){
 const slots=new Map(),addresses=new Set([target]),contexts=new Map([[trace.structLogs[0].depth,target]]),accesses=new Map();let previous;
 for(const log of trace.structLogs){if(previous&&log.depth>previous.depth){const op=previous.op;assert.equal(log.depth,previous.depth+1);let context;if(['CREATE','CREATE2'].includes(op)){assert(created,'known official creation required');context=created;}else {assert(['CALL','STATICCALL','DELEGATECALL','CALLCODE'].includes(op));context=['DELEGATECALL','CALLCODE'].includes(op)?contexts.get(previous.depth):addr('0x'+previous.stack.at(-2).slice(-40));}contexts.set(log.depth,context);}
  const context=contexts.get(log.depth);assert(context);addresses.add(context);
  if(['CALL','STATICCALL','DELEGATECALL','CALLCODE'].includes(log.op))addresses.add(addr('0x'+log.stack.at(-2).slice(-40)));
  if(['BALANCE','EXTCODEHASH','EXTCODESIZE','EXTCODECOPY'].includes(log.op))addresses.add(addr('0x'+log.stack.at(-1).slice(-40)));
  assert(log.op!=='SELFDESTRUCT');
  if(['SLOAD','SSTORE'].includes(log.op)){const slot=word('0x'+log.stack.at(-1));if(!slots.has(context))slots.set(context,new Set());slots.get(context).add(slot);const key=context+'/'+slot;if(!accesses.has(key))accesses.set(key,{address:context,slot,operations:new Set()});accesses.get(key).operations.add(log.op);}
  previous=log;
 }
 return {slots,addresses,accesses:[...accesses.values()].sort((a,b)=>(a.address+a.slot).localeCompare(b.address+b.slot)).map(a=>({...a,operations:[...a.operations].sort()}))};
}
async function generate({traceDir=path.join(__dirname,'traces')}={}){
 fs.mkdirSync(traceDir,{recursive:true});
 const source_pins=pins(),A={factory:artifact('core','UniswapV2Factory'),pair:artifact('core','UniswapV2Pair'),router:artifact('periphery','UniswapV2Router02'),token:artifact('periphery','ERC20'),weth:artifact('periphery','WETH9')};
 assert.equal(ethers.utils.keccak256(A.pair.creation),'0x96e8ac4277198ff8b6f785478aa9a39f403cb768dd02cbee326c3e7da348845f');
 const keys=[1,2,3,4].map(i=>'0x'+i.toString(16).padStart(64,'0')),owners=keys.map(k=>addr(new ethers.Wallet(k).address));
 const chainOptions={chain:{chainId:31337,networkId:31337,hardfork:'shanghai',time:new Date('2026-10-01T00:00:00Z'),allowUnlimitedContractSize:false,allowUnlimitedInitCodeSize:false,vmErrorsOnRPCResponse:false},wallet:{accounts:keys.map(secretKey=>({secretKey,balance:ethers.utils.parseEther('10000').toHexString()})),seed:'t33-v2-complete-official'},miner:{timestampIncrement:0,coinbase:owners[3],blockGasLimit:30000000,defaultGasPrice:2000000000},logging:{quiet:true}};
 const raw=ganache.provider(chainOptions),rpc=(method,params=[])=>raw.request({method,params}),provider=new ethers.providers.Web3Provider(raw),wallet=new ethers.Wallet(keys[0],provider);
 const txs=[],identities=[],known=new Set(owners),tokens=[],pairs=[],cases=[];let factory,weth,router;
 const options={gasLimit:7000000,gasPrice:2000000000,type:0,chainId:31337};
 async function header(){return rpc('eth_getBlockByNumber',['latest',false]);}
 async function getters(pair,block,anchor){
  const calls=[],tag=block.number;
  async function call(c,m,args=[]){const data=c.interface.encodeFunctionData(m,args).toLowerCase(),result=await rpc('eth_call',[{to:addr(c.address),data},tag]);calls.push({to:addr(c.address),method:m,args:args.map(x=>typeof x==='string'?x.toLowerCase():x),data,result,block_hash:block.hash,block_number:num(block.number)});return c.interface.decodeFunctionResult(m,result);}
  const [token0]=await call(pair,'token0'),[token1]=await call(pair,'token1'),[fac]=await call(pair,'factory'),res=await call(pair,'getReserves');
  const t0=tokens.find(t=>addr(t.address)===addr(token0)),t1=tokens.find(t=>addr(t.address)===addr(token1));assert(t0&&t1);
  const [b0]=await call(t0,'balanceOf',[pair.address]),[b1]=await call(t1,'balanceOf',[pair.address]),[p0]=await call(pair,'price0CumulativeLast'),[p1]=await call(pair,'price1CumulativeLast');
  assert.equal(addr((await call(factory,'getPair',[token0,token1]))[0]),addr(pair.address));assert.equal(addr((await call(factory,'getPair',[token1,token0]))[0]),addr(pair.address));
  const [lock,code,facCode]=await Promise.all([rpc('eth_getStorageAt',[addr(pair.address),word(12),tag]),rpc('eth_getCode',[addr(pair.address),tag]),rpc('eth_getCode',[addr(factory.address),tag])]);
  const source=anchor||block;
  return {block_hash:block.hash,block_number:num(block.number),calls,lock_observation:{address:addr(pair.address),slot:word(12),value:word(lock),block_hash:block.hash,block_number:num(block.number)},pool:{chain_key:'evm:31337',pool_id:addr(pair.address),protocol:PROFILE,token0:addr(token0),token1:addr(token1),reserve0:res[0].toString(),reserve1:res[1].toString(),fee_numerator:3,fee_denominator:1000,adapter_version:ADAPTER,token_behavior:'standard',v2:{factory:addr(fac),code_sha256:codeSha(code),swap:{raw_balance0:b0.toString(),raw_balance1:b1.toString(),block_timestamp_last:res[2],price0_cumulative_last:p0.toString(),price1_cumulative_last:p1.toString(),unlocked:BigInt(lock)==1n,factory_code_sha256:codeSha(facCode),bank_block_number:num(source.number),bank_block_hash:source.hash,bank_unix_timestamp:num(source.timestamp)}}}};
 }
 async function capture(purpose,to,data,{created,newPair,expectedStatus=1}={}){
  if(created)created=addr(created);
  const pre=await header(),pre_getters=[];
  for(const p of pairs)pre_getters.push(await getters(p,pre,pre));
  const nonce=num(await rpc('eth_getTransactionCount',[owners[0],pre.number]));
  let call_observation=null;
  if(expectedStatus===0){try{const result=await rpc('eth_call',[{from:owners[0],to:addr(to),data,gas:ethers.utils.hexValue(options.gasLimit),gasPrice:ethers.utils.hexValue(options.gasPrice)},pre.number]);call_observation={result};}catch(e){const data=typeof e.data==='string'?e.data:e.data?.result;assert(typeof data==='string','revert return bytes');call_observation={error_code:e.code,data};}}
  const signed=await wallet.signTransaction({...options,to,data,nonce});
  const hash=await rpc('eth_sendRawTransaction',[signed]),receipt=await rpc('eth_getTransactionReceipt',[hash]),transaction=await rpc('eth_getTransactionByHash',[hash]);
  assert.equal(num(receipt.status),expectedStatus,`transaction ${purpose}`);
  const block=await rpc('eth_getBlockByNumber',[receipt.blockNumber,false]);assert.equal(block.parentHash,pre.hash);
  const trace=await rpc('debug_traceTransaction',[hash,{disableMemory:true,disableStorage:true,disableStack:false}]);
  const traceText=json(trace),traceHash=sha(traceText),gzip=zlib.gzipSync(traceText,{level:9}),file=traceHash+'.json.gz';fs.writeFileSync(path.join(traceDir,file),gzip);
  const target=to?addr(to):addr(created||receipt.contractAddress),f=footprint(trace,target,created);
  known.forEach(a=>f.addresses.add(a));if(created)f.addresses.add(addr(created));if(newPair)f.addresses.add(addr(newPair.address));
  function add(a,s){a=addr(a);f.addresses.add(a);if(!f.slots.has(a))f.slots.set(a,new Set());f.slots.get(a).add(word(s));}
  for(const t of tokens){for(let s=0;s<=4;s++)add(t.address,s);for(const who of [...owners,...pairs.map(p=>addr(p.address)),...(newPair?[addr(newPair.address)]:[]),...(router?[addr(router.address)]:[])])add(t.address,mapping(who,1));if(router)add(t.address,nested(owners[0],router.address,2));}
  for(const p of [...pairs,...(newPair?[newPair]:[])])for(let s=0;s<=12;s++)add(p.address,s);
  if(factory){for(let s=0;s<=3;s++)add(factory.address,s);for(const p of [...pre_getters.map(g=>g.pool),...(newPair?[{token0:addr(newPair.t0.address),token1:addr(newPair.t1.address)}]:[])]){add(factory.address,nested(p.token0,p.token1,2));add(factory.address,nested(p.token1,p.token0,2));}}
  async function image(tag){return Promise.all([...f.addresses].sort().map(async address=>{const [balance,nonce,code]=await Promise.all([rpc('eth_getBalance',[address,tag]),rpc('eth_getTransactionCount',[address,tag]),rpc('eth_getCode',[address,tag])]);const storage=await Promise.all([...(f.slots.get(address)||[])].sort().map(async slot=>({slot,value:word(await rpc('eth_getStorageAt',[address,slot,tag]))})));return {address,exists:known.has(address)||code!=='0x'||BigInt(nonce)>0n||BigInt(balance)>0n,balance:BigInt(balance).toString(),nonce:num(nonce),code,storage};}));}
  const accounts=await image(pre.number),post_accounts=await image(block.number);
  const post_getters=[];for(const p of [...pairs,...(newPair?[newPair]:[])])post_getters.push(await getters(p,block,pre));
  let revert_reason='';if(call_observation?.data?.startsWith('0x08c379a0'))revert_reason=ethers.utils.defaultAbiCoder.decode(['string'],'0x'+call_observation.data.slice(10))[0];
  const observation={purpose,calldata:data.toLowerCase(),signed_transaction:signed,transaction,receipt,preblock_header:pre,execution_block_header:block,accounts,post_accounts,pre_getters,post_getters,storage_accesses:f.accesses,call_observation,trace:{file,sha256:traceHash,gzip_sha256:sha(gzip),steps:trace.structLogs.length,revert_reason}};
  txs.push(observation);return observation;
 }
 async function identify(a,c,tx,args=[]){const code=await rpc('eth_getCode',[addr(c.address),'latest']);assert((code.length-2)/2<=24576);const actual=Buffer.from(code.slice(2),'hex'),template=Buffer.from(a.template.slice(2),'hex');assert.equal(actual.length,template.length);const refs=Object.entries(a.immutables).map(([id,rs],i)=>({id,value:addr(args[i]),references:rs}));for(const ref of refs)for(const r of ref.references){assert.equal(actual.subarray(r.start,r.start+r.length).toString('hex'),word(ref.value).slice(2));template.copy(actual,r.start,r.start,r.start+r.length);}assert.equal('0x'+actual.toString('hex'),a.template);identities.push({artifact:'integration/tools/reference/evm-execution/'+a.file,address:addr(c.address),creation_transaction_hash:tx.receipt.transactionHash,creation_code_sha256:codeSha(a.creation),runtime_code_sha256:codeSha(code),runtime_template_sha256:codeSha(a.template),immutable_references:refs});known.add(addr(c.address));}
 async function deploy(a,args=[]){const nonce=await provider.getTransactionCount(wallet.address),created=ethers.utils.getContractAddress({from:wallet.address,nonce});const cf=new ethers.ContractFactory(a.abi,a.creation,wallet),c=new ethers.Contract(created,a.abi,wallet);const t=await capture('deploy:'+a.name,undefined,cf.getDeployTransaction(...args).data,{created});await identify(a,c,t,a===A.router?args:[]);return c;}
 async function op(label,c,method,args=[]){return capture(label,c.address,c.interface.encodeFunctionData(method,args));}
 async function createPair(label,r0,r1){let t0=await deploy(A.token,[(1n<<254n).toString()]),t1=await deploy(A.token,[(1n<<254n).toString()]);tokens.push(t0,t1);[t0,t1]=[t0,t1].sort((a,b)=>addr(a.address).localeCompare(addr(b.address)));
  const address=ethers.utils.getCreate2Address(factory.address,ethers.utils.keccak256(ethers.utils.solidityPack(['address','address'],[t0.address,t1.address])),ethers.utils.keccak256(A.pair.creation));const pair=new ethers.Contract(address,A.pair.abi,wallet);pair.t0=t0;pair.t1=t1;
  const t=await capture(label+':createPair',factory.address,factory.interface.encodeFunctionData('createPair',[t0.address,t1.address]),{created:address,newPair:pair});await identify(A.pair,pair,t);pairs.push(pair);
  await op(label+':fund0',t0,'transfer',[pair.address,r0.toString()]);await op(label+':fund1',t1,'transfer',[pair.address,r1.toString()]);await op(label+':mint',pair,'mint',[owners[0]]);
  await op(label+':approve0',t0,'approve',[router.address,ethers.constants.MaxUint256]);await op(label+':approve1',t1,'approve',[router.address,ethers.constants.MaxUint256]);return pair;
 }
 async function clock(timestamp){await rpc('evm_setTime',[timestamp*1000]);}
 async function donation(pair,label,d0,d1){if(d0)await op(label+':donate0',pair.t0,'transfer',[pair.address,d0.toString()]);if(d1)await op(label+':donate1',pair.t1,'transfer',[pair.address,d1.toString()]);}
 async function swap(name,pair,reverse=false,{fail=false,direct=false,service=true,amount=1000000000000000000000n}={}){
  process.stderr.write(`capture ${name}\n`);const pre=await header(),input=reverse?pair.t1:pair.t0,output=reverse?pair.t0:pair.t1;
  let to=router.address,data,quote_view=null;
  if(direct){const reserves=await pair.getReserves();to=pair.address;amount=0n;data=pair.interface.encodeFunctionData('swap',[reverse?reserves[0].sub(1):0,reverse?0:reserves[1].sub(1),owners[1],'0x']);}
  else {const qdata=router.interface.encodeFunctionData('getAmountsOut',[amount.toString(),[input.address,output.address]]),result=await rpc('eth_call',[{to:addr(router.address),data:qdata},pre.number]);quote_view={block_hash:pre.hash,to:addr(router.address),data:qdata,result};data=router.interface.encodeFunctionData('swapExactTokensForTokens',[amount.toString(),0,[input.address,output.address],owners[1],ethers.constants.MaxUint256]);}
  const t=await capture(name,to,data,{expectedStatus:fail?0:1}),p=t.pre_getters.find(g=>g.pool.pool_id===addr(pair.address)).pool,after=t.post_getters.find(g=>g.pool.pool_id===p.pool_id).pool,block=t.execution_block_header;
  const bal=(image,token,who)=>BigInt(image.find(a=>a.address===addr(token)).storage.find(s=>s.slot===mapping(who,1)).value);
  const amount_out=(bal(t.post_accounts,output.address,owners[1])-bal(t.accounts,output.address,owners[1])).toString(),consumed=(bal(t.accounts,input.address,owners[0])-bal(t.post_accounts,input.address,owners[0])).toString();
  const events=t.receipt.logs.filter(l=>addr(l.address)===p.pool_id).map(l=>({log:l,event:pair.interface.parseLog(l)})).filter(v=>v.event.name==='Swap');assert.equal(events.length,fail?0:1);
  const event=events[0],swap_event=event?{log_index:num(event.log.logIndex),sender:addr(event.event.args.sender),recipient:addr(event.event.args.to),amount0_in:event.event.args.amount0In.toString(),amount1_in:event.event.args.amount1In.toString(),amount0_out:event.event.args.amount0Out.toString(),amount1_out:event.event.args.amount1Out.toString()}:null;
  cases.push({schema_version:1,name,pool:p,state_anchor:{chain_key:'evm:31337',block_number:num(pre.number),block_hash:pre.hash,parent_hash:pre.parentHash,timestamp:num(pre.timestamp),genesis_hash:genesis.hash},execution_timestamp:num(block.timestamp),service_eligible:service,
   execution_environment:{chain_id:31337,fork:'shanghai',block_number:num(block.number),block_hash:block.hash,parent_hash:block.parentHash,timestamp:num(block.timestamp),coinbase:addr(block.miner),gas_limit:num(block.gasLimit),base_fee:BigInt(block.baseFeePerGas).toString(),prevrandao:block.mixHash},
   accounts:t.accounts,transaction:{chain_key:'evm:31337',from:owners[0],to:addr(to),data:data.toLowerCase(),value:'0'},transaction_context:{nonce:num(t.transaction.nonce),gas_limit:num(t.transaction.gas),gas_price:BigInt(t.transaction.gasPrice).toString(),chain_id:31337},
   direction:reverse?'token1-to-token0':'token0-to-token1',token_in:addr(input.address),token_out:addr(output.address),recipient:owners[1],amount_in:amount.toString(),quote_view,
   expected:{status:fail?'revert':'success',receiptstatus:fail?0:1,amount_out,sender_input_consumed:consumed,gas_used:BigInt(t.receipt.gasUsed).toString(),tx_hash:t.receipt.transactionHash,pool_after:after,post_accounts:t.post_accounts,swap_event},evidence:t});
 }
 let genesis;
 try{
  genesis=await header();factory=await deploy(A.factory,[owners[0]]);weth=await deploy(A.weth);router=await deploy(A.router,[factory.address,weth.address]);
  const pair=await createPair('ordinary',10n**24n,2n*10n**24n),D=50n*10n**18n;
  await swap('sync-0-to-1',pair);await swap('sync-1-to-0',pair,true);
  for(const [label,d0,d1] of [['donate0',D,0n],['donate1',0n,D],['donate-both',D,D]])for(const reverse of [false,true]){await op(label+':reset-sync',pair,'sync');await donation(pair,label,d0,d1);await swap(label+(reverse?'-1-to-0':'-0-to-1'),pair,reverse);}
  await donation(pair,'sequence',D,D);await swap('donation-sequence-forward-1',pair);await swap('donation-sequence-forward-2',pair);await swap('donation-sequence-reverse-1',pair,true);await swap('donation-sequence-reverse-2',pair,true);
  await donation(pair,'sync-boundary',D,D);await op('sync-boundary:sync',pair,'sync');await swap('after-sync-0-to-1',pair);await swap('after-sync-1-to-0',pair,true);
  await donation(pair,'skim-boundary',D,D);await op('skim-boundary:skim',pair,'skim',[owners[1]]);await swap('after-skim-0-to-1',pair);await swap('after-skim-1-to-0',pair,true);
  await clock(2**32-2);await op('uint32:prime-clock',pair,'sync');await swap('uint32-before-wrap',pair,false,{service:false});await clock(2**32+3);await swap('uint32-after-wrap',pair,true,{service:false});
  const limit=(1n<<112n)-1n;
  for(const reverse of [false,true]){const skew=await createPair(reverse?'skew1':'skew0',reverse?limit:1n,reverse?1n:limit),now=num((await header()).timestamp);
   await clock(now+2**32-1);await op('cumulative:prime',skew,'sync');await clock(now+2*(2**32-1));await swap(reverse?'cumulative1-wrap':'cumulative0-wrap',skew,reverse,{amount:1n,service:false});}
  for(const reverse of [false,true]){const current=await (reverse?pair.t1:pair.t0).balanceOf(pair.address),amount=(1n<<112n)+100000n-BigInt(current.toString());await donation(pair,'overflow',reverse?0n:amount,reverse?amount:0n);await swap(reverse?'reserve-overflow-1':'reserve-overflow-0',pair,reverse,{fail:true,service:false});await op('overflow:recover-skim',pair,'skim',[owners[0]]);}
  for(const reverse of [false,true]){await donation(pair,'K',reverse?0n:D,reverse?D:0n);await swap(reverse?'pair-k-revert-1':'pair-k-revert-0',pair,reverse,{fail:true,direct:true,service:false});await op('K:recover-skim',pair,'skim',[owners[0]]);}
  const sequenceNames=[['donation-sequence-forward-1','donation-sequence-forward-2','donation-sequence-reverse-1','donation-sequence-reverse-2'],['after-sync-0-to-1','after-sync-1-to-0'],['after-skim-0-to-1','after-skim-1-to-0'],['uint32-before-wrap','uint32-after-wrap']];
  const sequences=sequenceNames.map((names,i)=>{
   const initial=cases.find(c=>c.name===names[0]),source=initial.state_anchor;
   function derived(pool){const p=structuredClone(pool);Object.assign(p.v2.swap,{bank_block_number:source.block_number,bank_block_hash:source.block_hash,bank_unix_timestamp:source.timestamp});return p;}
   return {name:['donation-forward-reverse','sync-shared','skim-shared','uint32-shared'][i],source_anchor:source,pool:initial.pool,cases:names,steps:names.map(name=>{const c=cases.find(c=>c.name===name),expected=structuredClone(c.expected);expected.pool_after=derived(expected.pool_after);return {case:name,execution_timestamp:c.execution_timestamp,token_in:c.token_in,token_out:c.token_out,amount_in:c.amount_in,pool:derived(c.pool),expected};})};
  });
  const sourceFiles=['generate.cjs','validate.cjs','README.md','runtime-pins.json'];
  const scripts=sourceFiles.map(f=>({path:path.relative(ROOT,path.join(__dirname,f)),sha256:sha(fs.readFileSync(path.join(__dirname,f)))}));
  for(const f of ['integration/tools/ci/v2-complete-golden.cjs','integration/tools/ci/v2-complete-check.py','integration/tools/ci/v2-complete-guards.py','integration/tools/reference/evm-execution/source-pins.json','integration/tools/reference/evm-execution/core-registry.json','integration/tools/reference/evm-execution/periphery-registry.json','integration/tools/reference/evm-execution/lib-registry.json','integration/.runtime/stable-reference/package.json','integration/.runtime/stable-reference/package-lock.json'])scripts.push({path:f,sha256:sha(fs.readFileSync(path.join(ROOT,f)))});
  const loaded_dependencies=Object.keys(require.cache).filter(p=>p.startsWith(path.join(RUNTIME,'node_modules')+path.sep)).sort().map(p=>({path:path.relative(ROOT,p),sha256:sha(fs.readFileSync(p))}));
  return {schema_version:1,profile:PROFILE,adapter_version:ADAPTER,authority:'original-local-node-artifact-replay-not-live',source_pins,reference:{case_inventory:cases.map(c=>c.name),case_count:cases.length,success_count:cases.filter(c=>c.expected.status==='success').length,revert_count:cases.filter(c=>c.expected.status==='revert').length,sequence_count:sequences.length,transaction_count:txs.length,chain_options:JSON.parse(json(chainOptions)),genesis_header:genesis,transactions:txs,runtime_identities:identities,scripts,loaded_dependencies,node:{version:process.version,executable:process.execPath,sha256:sha(fs.readFileSync(process.execPath))},ethers_version:ethers.version,ganache_version:ganacheVersion,source_pins_sha256:sha(fs.readFileSync(path.join(OLD,'source-pins.json')))},cases,sequences};
 } finally{await raw.disconnect();}
}
module.exports={generate};
