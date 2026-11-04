'use strict';
const fs=require('node:fs'),path=require('node:path');
const ref=path.resolve(__dirname,'../reference/infinity-bin-detached'),receiptPath=path.join(ref,'execution-receipt.json');
fs.writeFileSync(receiptPath,JSON.stringify({passed:false,state:'RUNNING'})+'\n');
let raw;
async function main(){
 const {root,base,sha,requireThat,compile,frozenService}=require('./infinity-bin-detached-lib.cjs');const frozen=frozenService();
 const owned=['integration/tools/ci/infinity-bin-detached-lib.cjs','integration/tools/ci/infinity-bin-detached-golden.cjs','integration/tools/reference/infinity-bin-detached/InfinityBinDetached.sol','integration/tools/reference/singleton-cl/source-metadata.json'];
 const inputs=Object.fromEntries(owned.map(p=>[p,sha(fs.readFileSync(path.join(root,p)))]));
 const {artifact,input,solc,loaded,load,manifest_sha256}=compile();
 const ganache=require(path.join(base,'node_modules/ganache')),{ethers}=require(path.join(base,'node_modules/ethers'));
 raw=ganache.provider({logging:{quiet:true},chain:{hardfork:'shanghai',chainId:31337,allowUnlimitedContractSize:false,time:new Date('2026-11-02T00:00:00Z')},wallet:{deterministic:true},miner:{blockGasLimit:100000000,timestampIncrement:1}});
 const provider=new ethers.providers.Web3Provider(raw);provider.pollingInterval=10;const signer=provider.getSigner(),owner=(await signer.getAddress()).toLowerCase(),A=8358608;
 const contract=await new ethers.ContractFactory(artifact.abi,artifact.evm.bytecode.object,signer).deploy({gasLimit:18000000});await contract.deployed();
 const runtime=(await provider.getCode(contract.address)).toLowerCase();requireThat(runtime.slice(2)===artifact.evm.deployedBytecode.object,'deployed original-library harness');
 const W=x=>ethers.utils.hexZeroPad(ethers.BigNumber.from(x.toString()).toHexString(),32).toLowerCase(),map=(key,slot)=>ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['bytes32','uint256'],[W(key),slot]));
 const salt=W(73),MAX='1000000000000000000000000000000';let clean=await provider.send('evm_snapshot',[]);
 const profiles=[{name:'dust1',x:'1000000',y:'1000000',supply:'1000000000000',fee:3000,p0:4000,p1:17},{name:'dust1000',x:'1000000',y:'1000000',supply:'1000000',fee:3000,p0:4000,p1:17},{name:'asymmetric',x:'1000000000',y:'1000000',supply:'1000000',fee:100000,p0:4000,p1:4000},{name:'zero-fee',x:'1000000',y:'1000000',supply:'1000000',fee:0,p0:0,p1:0}];
 const cases=[];
 async function snapshot(ids,profile){
  const packed=await contract.slot0(),active=Number(BigInt(packed)&0xffffffn),out={active_id:active,bin_step:1,lp_fee:profile.fee,protocol_fee:profile.p0+(profile.p1<<12),protocol_fee0:profile.p0,protocol_fee1:profile.p1,bins:{},detached_active:null,raw_bins:{},raw_storage:{},tree:{level0:null,level1:{},level2:{}}};
  for(const offset of [0,4])out.raw_storage[W(offset)]=await contract.raw(W(offset));
  for(const id of ids){const b=await contract.bin(id),t=await contract.tree(id),key=await contract.positionKey(id),position=await contract.position(id);
   const value={id,reserve0:b[0].toString(),reserve1:b[1].toString(),liquidity:b[2].toString(),total_shares:b[3].toString(),price:(await contract.price(id)).toString(),in_tree:t[3],position_key:key,position_share:position.toString(),next:{true:Number(await contract.next(true,id)),false:Number(await contract.next(false,id))}};
   out.raw_bins[id]=value;if(t[3])out.bins[id]=value;else if(id===active)out.detached_active=value;
   out.tree.level0=t[0];out.tree.level1[W(id>>16)]=t[1];out.tree.level2[W(id>>8)]=t[2];
   for(const slot of [map(id,1),map(id,2),map(key,3),map(id>>16,5),map(id>>8,6)])out.raw_storage[slot]=await contract.raw(slot);
  }
  return out;
 }
 async function trade(ids,profile,label,amount,zero){
  const pre=await snapshot(ids,profile),quote=await contract.callStatic.attempt(amount.toString(),zero,{gasLimit:15000000});
  await(await contract.attempt(amount.toString(),zero,{gasLimit:15000000})).wait();const post=await snapshot(ids,profile);
  if(!quote.ok)requireThat(JSON.stringify(pre)===JSON.stringify(post),'native rollback');
  const expected={native_reverted:!quote.ok,revert_data:quote.reason,delta0:quote.delta0.toString(),delta1:quote.delta1.toString(),amount_out:quote.ok?(zero?quote.delta1:quote.delta0).toString():'0',swap_fee:Number(quote.swapFee),protocol_amount:quote.protocolAmount.toString(),post};
  return {label,amount_in:amount.toString(),zero_for_one:zero,state:pre,expected};
 }
 for(const profile of profiles)for(const hasTree of [false,true])for(const zero of [true,false]){
  requireThat(await provider.send('evm_revert',[clean]),'restore fixture');clean=await provider.send('evm_snapshot',[]);
  const neighbors=hasTree?[A-65537,A-257,A+257,A+65537]:[],ids=[...neighbors,A].sort((a,b)=>a-b),rows=ids.map(id=>id===A?[id,profile.x,profile.y,profile.supply]:[id,'1000000000','1000000000','1000000000000']);
  await(await contract.seed(A,1,profile.fee,profile.p0+(profile.p1<<12),rows,owner,salt,{gasLimit:15000000})).wait();const preBurn=await snapshot(ids,profile),burnShares=(BigInt(profile.supply)-1000n).toString(),burn=await contract.callStatic.burn(A,burnShares);
  await(await contract.burn(A,burnShares)).wait();const afterBurn=await snapshot(ids,profile);
  requireThat(afterBurn.active_id===A&&afterBurn.detached_active?.total_shares==='1000'&&!afterBurn.bins[A]&&afterBurn.raw_bins[A].position_share==='0','native detached burn');
  const c={name:`${profile.name}/${hasTree?'real-tree':'empty-root'}/zero-${zero}`,seed:{active_id:A,bin_step:1,lp_fee:profile.fee,protocol_fee:profile.p0+(profile.p1<<12),owner,salt,bins:Object.fromEntries(rows.map(r=>[r[0],{reserve0:r[1],reserve1:r[2],total_shares:r[3]}]))},burn:{id:A,shares:burnShares,pre:preBurn,expected:{delta0:burn.delta0.toString(),delta1:burn.delta1.toString(),amounts:burn.amount,post:afterBurn}},trades:[]};
  const cap=BigInt((await contract.fill(A,MAX,zero)).used);requireThat(cap>0n,'native ghost capacity');
  if(hasTree){
   c.trades.push(await trade(ids,profile,'cross-from-detached',cap+1000n,zero));requireThat(!c.trades[0].expected.native_reverted&&c.trades[0].expected.post.detached_active===null,'detached drops on crossing');
   const current=c.trades.at(-1).expected.post.active_id,backCap=BigInt((await contract.fill(current,MAX,!zero)).used);
   c.trades.push(await trade(ids,profile,'reverse-skips-detached',backCap+1000n,!zero));requireThat(!c.trades.at(-1).expected.native_reverted,'reverse real-tree native success');
   requireThat(c.trades.at(-1).expected.post.active_id!==A&&c.trades.at(-1).expected.post.detached_active===null,'reverse cannot visit removed node');
   c.trades.push(await trade(ids,profile,'local-loop-after',1000n,zero));
  }else{
   const amount=profile.name==='dust1'?cap:(cap>2n?cap/2n:cap);
   c.trades.push(await trade(ids,profile,'small-detached-only',amount,zero));
   const active=c.trades.at(-1).expected.post.active_id,remaining=BigInt((await contract.fill(active,MAX,zero)).used);
   c.trades.push(await trade(ids,profile,'exhaust-empty-root',remaining+1n,zero));requireThat(c.trades.at(-1).expected.native_reverted,'empty root exhausted must revert');
   const reverseCap=BigInt((await contract.fill(A,MAX,!zero)).used);
   c.trades.push(await trade(ids,profile,'reverse-detached-only',reverseCap>2n?reverseCap/2n:(reverseCap||1n),!zero));
  }
  cases.push(c);
  if(cases.length===1){fs.writeFileSync(path.join(ref,'native-burn-probe.json'),JSON.stringify({passed:true,case:c.name,burn:c.burn},null,2)+'\n');console.log(JSON.stringify({progress:'native-burn',active_id:A,reserves:[afterBurn.raw_bins[A].reserve0,afterBurn.raw_bins[A].reserve1],shares:afterBurn.raw_bins[A].total_shares,root:afterBurn.tree.level0}));}
 }
 // Read pinned original Manager source; current maxBinStep is checked by initialize only.
 const managerName='infinity/src/pool-bin/BinPoolManager.sol',managerSource=load(managerName),lines=managerSource.split('\n');
 const maxReferences=lines.map((line,i)=>({line:i+1,text:line})).filter(x=>/maxBinStep|setMaxBinStep/.test(x.text));
 const managerSourceProof={file:managerName,sha256:sha(managerSource),references:maxReferences,scope:'original source checks current maxBinStep in initialize; setter updates the global maximum without rewriting existing pool parameters; no full-manager swap settlement claim'};
 const nativeArtifact={abi:artifact.abi,storage_layout:artifact.storageLayout,creation:'0x'+artifact.evm.bytecode.object,runtime_template:runtime,selectors:artifact.evm.methodIdentifiers};
 const artifactBytes=JSON.stringify(nativeArtifact,null,2)+'\n';fs.writeFileSync(path.join(ref,'Detached.artifact.json'),artifactBytes);
 const dependencyNames=[['solc','0.8.26'],['ganache','7.9.2'],['ethers','5.8.0']],dependencies={};for(const [name,version]of dependencyNames){const p=path.join(base,'node_modules',name,'package.json'),b=fs.readFileSync(p);requireThat(JSON.parse(b).version===version,'dependency version '+name);dependencies[name]={version,path:path.relative(root,p),sha256:sha(b)};}
 const loadedDependencies=Object.keys(require.cache).filter(p=>p.startsWith(path.join(base,'node_modules')+path.sep)).sort().map(p=>({path:path.relative(root,p),sha256:sha(fs.readFileSync(p))}));
 const binaryDependencies=[{path:path.relative(root,path.join(base,'node_modules/solc/soljson.js')),sha256:sha(fs.readFileSync(path.join(base,'node_modules/solc/soljson.js')))},{path:process.execPath,sha256:sha(fs.readFileSync(process.execPath))}];
 const reference={schema:'infinity-bin-detached-source/1',revision:'3c5ef1be1f71442d8bc60cc6fd11f9c677b714df',compiler:solc.version(),settings:input.settings,compile_input_sha256:sha(JSON.stringify(input)),manifest_sha256,files:[...loaded.values()].sort((a,b)=>a.local.localeCompare(b.local)),inputs,dependencies,loaded_dependencies:loadedDependencies,binary_dependencies:binaryDependencies,dependency_lock_sha256:sha(fs.readFileSync(path.join(base,'package-lock.json'))),artifact_sha256:sha(artifactBytes),runtime_sha256:sha(Buffer.from(runtime.slice(2),'hex')),runtime_keccak256:ethers.utils.keccak256(runtime),runtime_bytes:(runtime.length-2)/2,unlimited_contract_size:false,engine:{name:'ganache',version:'7.9.2',hardfork:'shanghai',chain_id:31337},minimum_share:'1000',frozen_service:frozen,manager_source_proof:managerSourceProof,scope:{original_BinPool_burn_swap:true,original_BinPosition_key_and_position:true,full_manager_swap_settlement_verified:false}};
 const fixture={schema:'infinity-bin-detached/1',reference,cases};const bytes=JSON.stringify(fixture,null,2)+'\n';for(const d of ['integration','chain-parser-go','dex-router'])fs.writeFileSync(path.join(root,d,'test/contracts/infinity-bin-detached.json'),bytes);
 const metadataBytes=JSON.stringify(reference,null,2)+'\n';fs.writeFileSync(path.join(ref,'source-metadata.json'),metadataBytes);
 for(const [p,h]of Object.entries(inputs))requireThat(sha(fs.readFileSync(path.join(root,p)))===h,'input changed '+p);for(const p of reference.files)requireThat(sha(fs.readFileSync(path.join(base,'sources',p.local)))===p.sha256,'source changed '+p.local);for(const p of [...loadedDependencies,...binaryDependencies])requireThat(sha(fs.readFileSync(path.isAbsolute(p.path)?p.path:path.join(root,p.path)))===p.sha256,'dependency changed '+p.path);frozenService();
 return {passed:true,inputs,fixture_sha256:sha(bytes),source_metadata_sha256:sha(metadataBytes),artifact_sha256:sha(artifactBytes),cases:cases.length,trades:cases.reduce((n,c)=>n+c.trades.length,0),native_reverts:cases.flatMap(c=>c.trades).filter(t=>t.expected.native_reverted).length,runtime_bytes:reference.runtime_bytes,frozen_service_unchanged:true};
}
(async()=>{let result;try{result=await main();}catch(e){result={passed:false,error:String(e),stack:e.stack};console.error(e);process.exitCode=1;}finally{if(raw)try{await raw.disconnect();}catch(e){result={passed:false,error:'cleanup '+e};process.exitCode=1;}fs.writeFileSync(receiptPath,JSON.stringify(result,null,2)+'\n');}if(result.passed)console.log(JSON.stringify(result));})();
