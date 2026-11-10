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
 if(process.argv.includes('--allocation-grid')){
  await provider.send('evm_increaseTime',[17]);await provider.send('evm_mine',[]);
  const bank=await capture(),source={number:bank.header.number,hash:bank.header.hash,timestamp:bank.header.timestamp};
  let gridCheckpoint=await provider.send('evm_snapshot',[]);
  const routeIndices=[[0,2],[0,1,2],[0,1,3,2]],target=500000000000000000000003n,parts=4,maxInput=1000000000000000000000000000000n,proposals=[];
  function enumerate(used,done,steps){if(done===parts){proposals.push(structuredClone(steps));return;}if(steps.length>=3)return;
   for(let i=0;i<routeIndices.length;i++){if(used.includes(i))continue;for(let end=done+1;end<=parts;end++){const amount=target*BigInt(end)/BigInt(parts)-target*BigInt(done)/BigInt(parts);if(amount===0n)continue;enumerate([...used,i],end,[...steps,{path_index:i,amount_out:amount.toString()}]);}}
  }
  enumerate([],0,[]);must(proposals.length===39,'EXACT_OUT_ORIGINAL_GRID_SCOPE');const results=[];
  for(const[index,proposal]of proposals.entries()){
   must(await provider.send('evm_revert',[gridCheckpoint]),'EXACT_OUT_GRID_RESET');gridCheckpoint=await provider.send('evm_snapshot',[]);
   const balance=await tokens[0].balanceOf(sender),recipientBalance=await tokens[2].balanceOf(recipient);let spent=0n,feasible=true,error=null;const observations=[];
   for(const step of proposal){
    const route=routeIndices[step.path_index].map(i=>tokens[i].address);
    try{const amounts=await router.getAmountsIn(step.amount_out,route),limit=maxInput-spent;
     const tx=await(await router.swapTokensForExactTokens(step.amount_out,limit.toString(),route,recipient,deadline,{gasLimit:7000000})).wait();
     spent=BigInt(balance.sub(await tokens[0].balanceOf(sender)).toString());
     observations.push({path_index:step.path_index,path:route.map(address),amount_out:step.amount_out,amounts:amounts.map(v=>v.toString()),gas_used:tx.gasUsed.toString(),transaction_hash:tx.transactionHash});
    }catch(failure){feasible=false;error=failure.reason||failure.message;break;}
   }
   const actualSpent=balance.sub(await tokens[0].balanceOf(sender)).toString(),received=(await tokens[2].balanceOf(recipient)).sub(recipientBalance).toString();
   if(feasible)must(BigInt(actualSpent)===spent&&BigInt(received)===target&&spent<=maxInput,'EXACT_OUT_ORIGINAL_GRID_RECIPIENT');
   results.push({id:index,proposal,feasible,error,required_input:feasible?actualSpent:null,observed_prefix_input:actualSpent,recipient_delta:received,observations,after:await capture(source)});
  }
  const successful=results.filter(r=>r.feasible);must(successful.length>0,'EXACT_OUT_GRID_NO_PLAN');successful.sort((a,b)=>BigInt(a.required_input)<BigInt(b.required_input)?-1:BigInt(a.required_input)>BigInt(b.required_input)?1:a.proposal.length-b.proposal.length);
  const identities={};for(const[role,c]of Object.entries({factory,weth,router,...Object.fromEntries(tokens.map((t,i)=>['token'+i,t])),...Object.fromEntries(pairs.map((p,i)=>['pair'+i,p]))})){const code=await provider.getCode(c.address);identities[role]={address:address(c.address),runtime_sha256:sha(Buffer.from(code.slice(2),'hex'))};}
  const dependencies=Object.keys(require.cache).filter(f=>f.startsWith(root+path.sep)).sort().map(f=>({path:path.relative(root,f),sha256:sha(fs.readFileSync(f)),bytes:fs.statSync(f).size}));
  const value={schema_version:1,passed:true,authority:'exhaustive finite ordered output-grid proposals executed by unchanged original Router02/Pair code in controlled snapshots; no quote formula, atomic executor or public chain claim',context:{sender,recipient,deadline},source_pins:{path:path.relative(root,pinsPath),sha256:sha(fs.readFileSync(pinsPath))},generator_sha256:sha(fs.readFileSync(__filename)),code_ledger_encoding:'sha256-hex-result/v1',code_ledger:codeLedger,loaded_dependencies:dependencies,identities,bank,paths:routeIndices.map(route=>route.map(i=>address(tokens[i].address))),target_output:target.toString(),max_input:maxInput.toString(),grid_parts:parts,max_splits:3,plans:results,best_plan_id:successful[0].id,best_required_input:successful[0].required_input,all_requested_features_complete:false,production_ready:false};
  fs.writeFileSync(path.join(root,'integration/artifacts/local/core-v2-exact-out-grid-original-20261109.json'),JSON.stringify(value,null,2)+'\n');console.log(JSON.stringify({passed:true,grid_plans:results.length,feasible:successful.length,best_required_input:value.best_required_input,best_routes:successful[0].proposal.length}));return;
 }
 for(const route of [[0,1],[1,0],[0,1,2],[2,1,0],[3,4],[4,3],[0,1,3,2],[2,3,1,0]]){
  const outputs=route[0]>=3?['1','1000','10003']:['1','10003','1000000000000000000','100000000000000000000000'];
  for(const desired of outputs){for(const donation of ['none','input','output']){
   await reset();await provider.send('evm_increaseTime',[17]);await provider.send('evm_mine',[]);const pathTokens=route.map(i=>tokens[i].address),inputToken=tokens[route[0]],outputToken=tokens[route.at(-1)];
   if(donation!=='none'){const first=await factory.getPair(pathTokens[0],pathTokens[1]),last=await factory.getPair(pathTokens.at(-2),pathTokens.at(-1));await(await(donation==='input'?inputToken:outputToken).transfer(donation==='input'?first:last,route[0]>=3?'19':'1000000000000')).wait();}
   const before=await capture(),source={number:before.header.number,hash:before.header.hash,timestamp:before.header.timestamp};
   const required=await router.getAmountsIn(desired,pathTokens);const limit=required[0].add(17),senderBefore=await inputToken.balanceOf(sender),recipientBefore=await outputToken.balanceOf(recipient);
   let tooSmallRejected=false;try{await router.callStatic.swapTokensForExactTokens(desired,required[0].sub(1),pathTokens,recipient,deadline);}catch(error){tooSmallRejected=true;}must(tooSmallRejected,'EXACT_OUT_ORIGINAL_MAX_BOUND');
   const receipt=await(await router.swapTokensForExactTokens(desired,limit,pathTokens,recipient,deadline,{gasLimit:7000000})).wait();
   const consumed=senderBefore.sub(await inputToken.balanceOf(sender)),received=(await outputToken.balanceOf(recipient)).sub(recipientBefore);must(consumed.eq(required[0])&&received.eq(desired),'EXACT_OUT_REAL_RECIPIENT_DELTA');
   const after=await capture(source);
   cases.push({id:cases.length,mode:donation,path:pathTokens.map(address),desired_output:desired,max_input:limit.toString(),source:before,expected:{required_input:consumed.toString(),amounts:required.map(v=>v.toString()),recipient_delta:received.toString(),unused_input_budget:limit.sub(consumed).toString(),max_input_minus_one_rejected:tooSmallRejected,after,gas_used:receipt.gasUsed.toString(),transaction_hash:receipt.transactionHash}});
  }}
 }
 const invalid=[];for(const desired of ['0','1800000000000000000000000']){await reset();let rejected=false;try{await router.getAmountsIn(desired,[tokens[0].address,tokens[1].address]);}catch(error){rejected=true;}must(rejected,'EXACT_OUT_INVALID_TARGET');invalid.push({desired_output:desired,rejected});}
 const identities={};for(const[role,c]of Object.entries({factory,weth,router,...Object.fromEntries(tokens.map((t,i)=>['token'+i,t])),...Object.fromEntries(pairs.map((p,i)=>['pair'+i,p]))})){const code=await provider.getCode(c.address);identities[role]={address:address(c.address),runtime_sha256:sha(Buffer.from(code.slice(2),'hex')),runtime_bytes:(code.length-2)/2};}
 const loaded_dependencies=Object.keys(require.cache).filter(file=>file.startsWith(root+path.sep)).sort().map(file=>({path:path.relative(root,file),bytes:fs.statSync(file).size,sha256:sha(fs.readFileSync(file))}));
 const output={schema_version:1,passed:true,runtime:{node_version:process.version,node_executable_sha256:sha(fs.readFileSync(process.execPath)),ethers_version:ethers.version,ganache_version:require(path.join(modules,'ganache/package.json')).version,loaded_dependencies},authority:'unchanged published original Factory/Pair/Router02/ERC20/WETH; ordinary signed local exact-output transactions and actual sender/recipient balance changes, no live chain claim',source_pins:{path:path.relative(root,pinsPath),sha256:sha(fs.readFileSync(pinsPath))},generator_sha256:sha(fs.readFileSync(__filename)),context:{sender,recipient,deadline},code_ledger_encoding:"sha256-hex-result/v1",code_ledger:codeLedger,identities,cases,invalid,all_requested_features_complete:false,production_ready:false};
 fs.writeFileSync(path.join(root,'integration/artifacts/local/core-v2-exact-out-original-20261109.json'),JSON.stringify(output,null,2)+'\n');console.log(JSON.stringify({passed:true,cases:cases.length,invalid:invalid.length,original_transactions:cases.length}));
}finally{await transport.disconnect();}})().catch(error=>{console.error(error);process.exitCode=1;});
