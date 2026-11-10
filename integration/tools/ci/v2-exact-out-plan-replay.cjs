// SPDX-License-Identifier: GPL-3.0-or-later
// Original published Router02 exact-output swaps; controlled banks and tokens.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../../..'),old=path.join(root,'integration/tools/reference/evm-execution'),modules=path.join(root,'integration/.runtime/stable-reference/node_modules');
const {ethers}=require(path.join(modules,'ethers')),ganache=require(path.join(modules,'ganache'));
const must=(v,m)=>{if(!v)throw Error(m);},sha=b=>crypto.createHash('sha256').update(b).digest('hex'),address=x=>x.toLowerCase();
const pinsPath=path.join(old,'source-pins.json'),pins=JSON.parse(fs.readFileSync(pinsPath));
for(const pkg of pins.packages)for(const entry of pkg.files){must(!path.isAbsolute(entry.path)&&!entry.path.split('/').includes('..'),'EXACT_OUT_PIN_PATH');must(sha(fs.readFileSync(path.join(old,entry.path)))===entry.sha256,'EXACT_OUT_ORIGINAL_PIN:'+entry.path);}
function artifact(pkg,name){const relative=`packages/${pkg}/build/${name}.json`,value=JSON.parse(fs.readFileSync(path.join(old,relative)));return{...value,source_path:path.join('integration/tools/reference/evm-execution',relative)};}
const artifacts={factory:artifact('core','UniswapV2Factory'),pair:artifact('core','UniswapV2Pair'),router:artifact('periphery','UniswapV2Router02'),weth:artifact('periphery','WETH9'),token:artifact('periphery','ERC20')};
const transport=ganache.provider({logging:{quiet:true},chain:{chainId:31337,hardfork:'shanghai',time:new Date('2026-11-09T00:00:00Z'),allowUnlimitedContractSize:false},miner:{timestampIncrement:0,blockGasLimit:30000000},wallet:{deterministic:true}});
const provider=new ethers.providers.Web3Provider(transport);provider.pollingInterval=20;
let recording=null;const codeLedger={};const originalSend=provider.send.bind(provider);
provider.send=async(method,params)=>{const result=await originalSend(method,params);if(recording&&['eth_call','eth_getCode','eth_getStorageAt','eth_getBlockByNumber'].includes(method)){
 const item={method,params:structuredClone(params)};
 if(method==='eth_getCode'){const digest=sha(Buffer.from(result.slice(2),'hex'));if(codeLedger[digest]!==undefined)must(codeLedger[digest]===result,'EXACT_OUT_CODE_LEDGER_CONFLICT');codeLedger[digest]=result;item.result_code_sha256=digest;}else item.result=structuredClone(result);
 recording.push(item);
}return result;};
(async()=>{try{
 const signer=provider.getSigner(0),sender=address(await signer.getAddress()),recipient='0x000000000000000000000000000000000000beef';
 async function deploy(a,args=[]){const c=await new ethers.ContractFactory(a.abi,a.evm.bytecode.object,signer).deploy(...args,{gasLimit:15000000});await c.deployed();return c;}
 const factory=await deploy(artifacts.factory,[sender]),weth=await deploy(artifacts.weth),router=await deploy(artifacts.router,[factory.address,weth.address]);
 const tokens=[];for(let i=0;i<5;i++){const t=await deploy(artifacts.token,['1000000000000000000000000000000000000']);await(await t.approve(router.address,ethers.constants.MaxUint256)).wait();tokens.push(t);}
 const pairs=[];
 async function pair(a,b,rin,rout){await(await factory.createPair(tokens[a].address,tokens[b].address)).wait();const c=new ethers.Contract(await factory.getPair(tokens[a].address,tokens[b].address),artifacts.pair.abi,signer);await(await tokens[a].transfer(c.address,rin)).wait();await(await tokens[b].transfer(c.address,rout)).wait();await(await c.mint(sender)).wait();pairs.push(c);return c;}
 await pair(0,1,'1000000000000000000000000','1800000000000000000000000');
 await pair(1,2,'1400000000000000000000000','1100000000000000000000000');
 await pair(3,4,'997000','1001000');
 await pair(0,2,'1000000000000000000000000','1300000000000000000000000');
 await pair(1,3,'1000000000000000000000000','2000000000000000000000000');
 await pair(3,2,'1500000000000000000000000','1000000000000000000000000');
 const deadline=(await provider.getBlock('latest')).timestamp+3600;
 async function snapshotPool(c,source){
  const at={blockTag:source.number},[t0,t1,r,c0,c1,fac,code]=await Promise.all([c.token0(at),c.token1(at),c.getReserves(at),c.price0CumulativeLast(at),c.price1CumulativeLast(at),c.factory(at),provider.getCode(c.address,source.number)]);
  const token0=new ethers.Contract(t0,artifacts.token.abi,signer),token1=new ethers.Contract(t1,artifacts.token.abi,signer);
  const binding=await Promise.all([factory.getPair(t0,t1,at),factory.getPair(t1,t0,at)]);must(binding.every(p=>address(p)===address(c.address)),'EXACT_OUT_ORIGINAL_FACTORY_PAIR');
  const[raw0,raw1,facCode,lock]=await Promise.all([token0.balanceOf(c.address,at),token1.balanceOf(c.address,at),provider.getCode(fac,source.number),provider.getStorageAt(c.address,12,source.number)]);
  return{chain_key:'evm:31337',pool_id:address(c.address),protocol:'uniswap-v2-complete',adapter_version:'uniswap-v2/core-1.0.1-full-swap-1',token0:address(t0),token1:address(t1),reserve0:r[0].toString(),reserve1:r[1].toString(),fee_numerator:3,fee_denominator:1000,token_behavior:'standard',v2:{factory:address(fac),code_sha256:sha(Buffer.from(code.slice(2),'hex')),swap:{raw_balance0:raw0.toString(),raw_balance1:raw1.toString(),block_timestamp_last:Number(r[2]),price0_cumulative_last:c0.toString(),price1_cumulative_last:c1.toString(),unlocked:BigInt(lock)===1n,factory_code_sha256:sha(Buffer.from(facCode.slice(2),'hex')),bank_block_number:source.number,bank_block_hash:source.hash,bank_unix_timestamp:source.timestamp}}};
 }
 async function capture(sourceOverride){recording=[];const h=await provider.getBlock('latest');const rows=[],canonical=[];for(const c of pairs){const p=await snapshotPool(c,h);canonical.push(structuredClone(p));if(sourceOverride){p.v2.swap.bank_block_number=sourceOverride.number;p.v2.swap.bank_block_hash=sourceOverride.hash;p.v2.swap.bank_unix_timestamp=sourceOverride.timestamp;}rows.push(p);}const reads=recording;recording=null;return{header:{number:h.number,hash:h.hash,parent_hash:h.parentHash,timestamp:h.timestamp},reads,pools:rows,canonical_pools:canonical};}
 let checkpoint=await provider.send('evm_snapshot',[]);const cases=[];
 async function reset(){must(await provider.send('evm_revert',[checkpoint]),'EXACT_OUT_REFERENCE_RESET');checkpoint=await provider.send('evm_snapshot',[]);}

 const args=process.argv.slice(2);must(args.length===3,'EXACT_OUT_REPLAY_INPUT_MODEL_OUTPUT');
 const inputRaw=fs.readFileSync(args[0]),input=JSON.parse(inputRaw),model=JSON.parse(fs.readFileSync(args[1]));
 must(input.schema_version===1&&model.passed===true&&model.input_sha256===sha(inputRaw)
  &&input.records.length===model.records.length&&input.records.length>0,'EXACT_OUT_REPLAY_MODEL_INPUT');
 const referencePath=path.join(root,'integration/test/contracts/v2-exact-out.json'),reference=JSON.parse(fs.readFileSync(referencePath));
 function comparable(rows,anchor){return rows.map(p=>{p=structuredClone(p);p.v2.swap.bank_block_number=anchor.number_or_slot;p.v2.swap.bank_block_hash=anchor.hash;p.v2.swap.bank_unix_timestamp=anchor.timestamp_ms/1000;return p;}).sort((a,b)=>a.pool_id.localeCompare(b.pool_id));}
 function stable(v){if(Array.isArray(v))return v.map(stable);if(v&&typeof v==='object')return Object.fromEntries(Object.keys(v).sort().map(k=>[k,stable(v[k])]));return v;}
 function equal(a,b,message){must(JSON.stringify(stable(a))===JSON.stringify(stable(b)),message);}
 const results=[];let transactions=0;
 for(const[index,r]of input.records.entries()){
  const projection=model.records[index],req=r.request,plan=r.execution_plan;
  must(projection.case_index===r.case_index&&projection.quote_id===r.quote.quoteId&&projection.plan_hash===plan.plan_hash,'EXACT_OUT_REPLAY_RECORD_ID');
  equal(projection.request,req,'EXACT_OUT_REPLAY_REQUEST');equal(projection.snapshot,r.snapshot,'EXACT_OUT_REPLAY_SNAPSHOT');
  const context=plan.context,allocation=plan.allocation,anchor=r.snapshot.reference.block;
  must(context.caller===sender&&context.recipient===recipient&&context.sell_token===req.sellToken&&context.buy_token===req.buyToken
   &&allocation.target_output===req.buyAmount&&allocation.input_budget===req.maxSellAmount
   &&projection.required_input===allocation.required_input&&projection.target_output===allocation.target_output
   &&projection.max_sell_amount===plan.max_sell_amount&&projection.steps.length===allocation.routes.length,'EXACT_OUT_REPLAY_CONTEXT');
  await reset();await provider.send('evm_increaseTime',[17]);await provider.send('evm_mine',[]);
  if(r.case_index!=='grid'){
   const c=reference.cases[r.case_index];must(c&&c.id===r.case_index,'EXACT_OUT_REPLAY_CASE');
   if(c.mode!=='none'){
    const original=c.path.map(p=>tokens.find(t=>address(t.address)===p));must(original.every(Boolean),'EXACT_OUT_REPLAY_ORIGINAL_PATH');
    const first=await factory.getPair(original[0].address,original[1].address),last=await factory.getPair(original.at(-2).address,original.at(-1).address);
    const small=c.path[0]===reference.identities.token3.address||c.path[0]===reference.identities.token4.address;
    await(await(c.mode==='input'?original[0]:original.at(-1)).transfer(c.mode==='input'?first:last,small?'19':'1000000000000')).wait();
   }
  }
  const bank=await capture();must(bank.header.timestamp===anchor.timestamp_ms/1000,'EXACT_OUT_REPLAY_SOURCE_CLOCK');
  equal(comparable(bank.pools,anchor),comparable(r.snapshot.pools,anchor),'EXACT_OUT_REPLAY_ORIGINAL_SOURCE_BANK');
  const inToken=tokens.find(t=>address(t.address)===req.sellToken),outToken=tokens.find(t=>address(t.address)===req.buyToken);
  must(inToken&&outToken,'EXACT_OUT_REPLAY_TOKENS');
  const beforeIn=await inToken.balanceOf(sender),beforeOut=await outToken.balanceOf(recipient);let spent=0n;const observations=[];
  for(const[routeIndex,route]of allocation.routes.entries()){
   const step=projection.steps[routeIndex];equal(step.route,route,'EXACT_OUT_REPLAY_ROUTE');
   equal(comparable((await capture()).pools,anchor),comparable(step.before,anchor),'EXACT_OUT_REPLAY_ROUTE_PRESTATE');
   const native=await router.getAmountsIn(route.amount_out,route.tokens);equal(native.map(v=>v.toString()),route.amounts,'EXACT_OUT_REPLAY_NATIVE_REQUIRED_INPUT');
   const limit=BigInt(plan.max_sell_amount)-spent;must(limit>=BigInt(route.amount_in),'EXACT_OUT_REPLAY_SLIPPAGE_CAP');
   const receipt=await(await router.swapTokensForExactTokens(route.amount_out,limit.toString(),route.tokens,recipient,Math.floor(context.deadline_ms/1000),{gasLimit:7000000})).wait();transactions++;
   spent=BigInt(beforeIn.sub(await inToken.balanceOf(sender)).toString());
   const after=await capture();equal(comparable(after.pools,anchor),comparable(step.after,anchor),'EXACT_OUT_REPLAY_COMPLETE_POSTSTATE');
   observations.push({path:route.tokens,amounts:native.map(v=>v.toString()),gas_used:receipt.gasUsed.toString(),transaction_hash:receipt.transactionHash});
  }
  const received=(await outToken.balanceOf(recipient)).sub(beforeOut).toString();
  must(spent.toString()===allocation.required_input&&received===req.buyAmount
   &&spent<=BigInt(plan.max_sell_amount)&&BigInt(allocation.input_budget)-spent===BigInt(allocation.unused_input_budget),'EXACT_OUT_REPLAY_RECIPIENT_AND_BUDGET');
  equal(comparable((await capture()).pools,anchor),comparable(projection.final_pools,anchor),'EXACT_OUT_REPLAY_FINAL_SCOPE');
  results.push({case_index:r.case_index,quote_id:r.quote.quoteId,required_input:spent.toString(),recipient_delta:received,route_transactions:observations});
 }
 const dependencies=Object.keys(require.cache).filter(file=>file.startsWith(root+path.sep)).sort().map(file=>({path:path.relative(root,file),bytes:fs.statSync(file).size,sha256:sha(fs.readFileSync(file))}));
 const report={schema_version:1,passed:true,input_sha256:sha(inputRaw),model_sha256:sha(fs.readFileSync(args[1])),reference_sha256:sha(fs.readFileSync(referencePath)),generator_sha256:sha(fs.readFileSync(__filename)),source_pins_sha256:sha(fs.readFileSync(pinsPath)),runtime:{node_version:process.version,node_executable_sha256:sha(fs.readFileSync(process.execPath)),loaded_dependencies:dependencies},authority:'actual normal ExactOut API plans executed with published original Router02/Factory/Pair; real sender/recipient deltas and complete Pair poststate; ordered routes are separate local transactions',records:results,original_transactions:transactions,all_requested_features_complete:false,production_ready:false};
 fs.writeFileSync(args[2],JSON.stringify(report,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify({passed:true,api_records:results.length,original_transactions:transactions}));
}finally{await transport.disconnect();}})().catch(error=>{console.error(error);process.exitCode=1;});
