'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../../..'),ref=path.join(root,'integration/tools/reference/infinity-bin-service');
const receiptPath=path.join(ref,'execution-receipt.json');
// Invalidate before any optional dependency, source, compiler, or artifact work.
fs.writeFileSync(receiptPath,JSON.stringify({passed:false,state:'RUNNING'})+'\n');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
function requireThat(ok,message){if(!ok)throw Error(message);}
let raw;
async function main(){
 const owned=['integration/tools/ci/infinity-bin-service-compile.cjs','integration/tools/ci/infinity-bin-service-golden.cjs','integration/tools/reference/infinity-bin-service/InfinityBinServiceMath.sol','integration/tools/reference/singleton-cl/source-metadata.json'];
 const inputs=Object.fromEntries(owned.map(p=>[p,sha(fs.readFileSync(path.join(root,p)))]));
 const {compile}=require('./infinity-bin-service-compile.cjs');
 const {result:compileEvidence,out,base,load,loaded,solc}=compile();
 const dependencies={};
 for(const [name,version] of [['solc','0.8.26'],['ganache','7.9.2'],['ethers','5.8.0']]){
  const p=path.join(base,'node_modules',name,'package.json'),b=fs.readFileSync(p);requireThat(JSON.parse(b).version===version,'dependency version '+name);
  dependencies[name]={version,package_path:path.relative(root,p),package_sha256:sha(b)};
 }
 const ganache=require(path.join(base,'node_modules/ganache')),{ethers}=require(path.join(base,'node_modules/ethers'));
 const W=x=>ethers.utils.hexZeroPad(ethers.BigNumber.from(x.toString()).toHexString(),32).toLowerCase();
 const add=(s,n)=>W(BigInt(s)+BigInt(n));
 const map=(key,slot)=>ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['bytes32','uint256'],[W(key),slot.toString()]));
 const zero=ethers.constants.AddressZero;
 const harnessPath=path.join(ref,'InfinityBinServiceMath.sol');
 const hi={language:'Solidity',sources:{'InfinityBinServiceMath.sol':{content:fs.readFileSync(harnessPath,'utf8')}},settings:{optimizer:{enabled:true,runs:200},viaIR:true,evmVersion:'shanghai',metadata:{bytecodeHash:'none',appendCBOR:false},outputSelection:{'*':{'*':['abi','storageLayout','evm.bytecode.object','evm.deployedBytecode.object','evm.deployedBytecode.immutableReferences','evm.methodIdentifiers']}}}};
 const ho=JSON.parse(solc.compile(JSON.stringify(hi),{import:n=>({contents:load(n)})}));
 requireThat(!(ho.errors||[]).some(e=>e.severity==='error'),JSON.stringify(ho.errors));
 const artifacts={BinPoolManager:out.contracts['infinity/src/pool-bin/BinPoolManager.sol'].BinPoolManager,Vault:out.contracts['infinity/src/Vault.sol'].Vault,Math:ho.contracts['InfinityBinServiceMath.sol'].InfinityBinServiceMath,Currency:ho.contracts['InfinityBinServiceMath.sol'].InfinityBinServiceCurrency};
 for(const [n,a] of Object.entries(artifacts))requireThat(a.evm.deployedBytecode.object.length/2<=24576,'EIP170 '+n);
 raw=ganache.provider({logging:{quiet:true},chain:{hardfork:'shanghai',chainId:31337,allowUnlimitedContractSize:false,time:new Date('2026-11-02T00:00:00Z')},wallet:{deterministic:true},miner:{blockGasLimit:100000000,timestampIncrement:1},chainId:31337});
 const provider=new ethers.providers.Web3Provider(raw),signer=provider.getSigner();provider.pollingInterval=10;
 async function deploy(a,args=[]){const c=await new ethers.ContractFactory(a.abi,a.evm.bytecode.object,signer).deploy(...args,{gasLimit:18000000});await c.deployed();return c;}
 const vault=await deploy(artifacts.Vault),manager=await deploy(artifacts.BinPoolManager,[vault.address]);
 await (await vault.registerApp(manager.address)).wait();
 const currencies=[await deploy(artifacts.Currency),await deploy(artifacts.Currency)].sort((a,b)=>a.address.toLowerCase().localeCompare(b.address.toLowerCase()));
 const tokens=currencies.map(c=>({address:c.address.toLowerCase(),decimals:18,symbol:'IBS'}));
 const M=manager.address.toLowerCase(),V=vault.address.toLowerCase(),A=8388608;
 // Every patch location must equal the real Vault address; all remaining bytes equal solc output.
 const managerRuntime=(await provider.getCode(M)).toLowerCase(),vaultRuntime=(await provider.getCode(V)).toLowerCase();
 let template=managerRuntime.slice(2);const refs=artifacts.BinPoolManager.evm.deployedBytecode.immutableReferences;
 requireThat(Object.keys(refs).length===1,'sole immutable must be Vault');
 for(const patches of Object.values(refs))for(const r of patches){requireThat(r.length===32,'immutable width');requireThat(template.slice(r.start*2,(r.start+r.length)*2)===W(V).slice(2),'Vault immutable bytes');template=template.slice(0,r.start*2)+'0'.repeat(r.length*2)+template.slice((r.start+r.length)*2);}
 requireThat(template===artifacts.BinPoolManager.evm.deployedBytecode.object,'original manager template');
 requireThat(vaultRuntime.slice(2)===artifacts.Vault.evm.deployedBytecode.object,'original Vault runtime');
 requireThat((await manager.vault()).toLowerCase()===V,'Vault getter');
 const keyType='tuple(address currency0,address currency1,address hooks,address poolManager,uint24 fee,bytes32 parameters)';
 const defs=[{name:'pool0',step:1,fee:3000,p0:4000,p1:17},{name:'pool1',step:10,fee:100000,p0:123,p1:4000}];
 const pools=[],maths=[],knownStorage=new Map();
 function remember(address,slot){address=address.toLowerCase();if(!knownStorage.has(address))knownStorage.set(address,new Set());knownStorage.get(address).add(slot.toLowerCase());}
 async function write(address,slot,value){remember(address,slot);await provider.send('evm_setAccountStorageAt',[address,slot,W(value)]);}
 for(const d of defs){
  const key={currency0:tokens[0].address,currency1:tokens[1].address,hooks:zero,poolManager:M,fee:d.fee,parameters:W(BigInt(d.step)<<16n)};
  const id=ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode([keyType],[key]));
  const c=await deploy(artifacts.Math);maths.push(c);
  // Crosses both leaf and parent boundaries; initialized depleted entries stay in all tree levels.
  const offsets=[-65537,-257,-1,0,1,257,65537];
  const rows=offsets.map(o=>({id:A+o,reserve0:o===-1||o===1?'0':'1000000',reserve1:o===-1||o===1?'0':'1000000',shares:'1000000000000'}));
  await (await c.seed(A,d.step,d.fee,d.p0+(d.p1<<12),rows.map(b=>[b.id,b.reserve0,b.reserve1,b.shares]),{gasLimit:10000000})).wait();
  const poolBase=map(id,5),keyBase=map(id,6);
  const pool={id,name:d.name,key,bin_step:d.step,lp_fee:d.fee,protocol_fee0:d.p0,protocol_fee1:d.p1,bin_ids:rows.map(b=>b.id),pool_base:poolBase,key_base:keyBase,math_harness:c.address.toLowerCase()};pools.push(pool);
  // Original PoolKey struct packing is obtained from compiler layout below, including fee+manager slot.
  const kt=Object.values(artifacts.BinPoolManager.storageLayout.types).find(t=>t.label==='struct PoolKey');
  requireThat(kt&&kt.members.length===6,'compiler PoolKey layout');
  const keyWords={};for(const member of kt.members){const n=BigInt(key[member.label]);keyWords[member.slot]=(keyWords[member.slot]||0n)|(n<<BigInt(member.offset*8));}
  for(const [slot,value]of Object.entries(keyWords))await write(M,add(keyBase,slot),value);
 }
 async function projectPool(i){
  const p=pools[i],c=maths[i];await write(M,p.pool_base,await c.slot0());
  const l1=new Set(),l2=new Set();
  for(const id of p.bin_ids){
   const b=await c.bin(id);await write(M,map(id,add(p.pool_base,1)),BigInt(b[0])|(BigInt(b[1])<<128n));await write(M,map(id,add(p.pool_base,2)),b[3]);
   const tree=await c.tree(id);requireThat(tree[3],'initialized member');await write(M,add(p.pool_base,4),tree[0]);
   if(!l1.has(id>>16)){await write(M,map(id>>16,add(p.pool_base,5)),tree[1]);l1.add(id>>16);}
   if(!l2.has(id>>8)){await write(M,map(id>>8,add(p.pool_base,6)),tree[2]);l2.add(id>>8);}
  }
 }
 for(let i=0;i<pools.length;i++)await projectPool(i);
 const initialProtocol=[13n,17n],bank=[0n,0n];
 for(const c of maths)for(const id of pools[maths.indexOf(c)].bin_ids){const b=await c.bin(id);bank[0]+=BigInt(b[0]);bank[1]+=BigInt(b[1]);}
 const vaultOuter=map(M,6);
 for(let k=0;k<2;k++){bank[k]+=initialProtocol[k];await write(V,map(tokens[k].address,vaultOuter),bank[k]);await write(M,map(tokens[k].address,2),initialProtocol[k]);}
 remember(V,map(M,5));for(const n of [0,1,2,3,4,5,6])remember(M,W(n));for(const n of [3,4,5,6])remember(V,W(n));
 const blockStart=await provider.getBlock('latest');
 const calls=[];
 function call(c,name,args=[]){calls.push({address:c.address.toLowerCase(),data:c.interface.encodeFunctionData(name,args).toLowerCase(),name,args});}
 for(const n of ['vault','maxBinStep','protocolFeeController','paused'])call(manager,n);
 for(const p of pools){call(manager,'getSlot0',[p.id]);call(manager,'pools',[p.id]);call(manager,'poolIdToPoolKey',[p.id]);for(const id of p.bin_ids){call(manager,'getBin',[p.id,id]);for(const d of [true,false])call(manager,'getNextNonEmptyBin',[p.id,d,id]);}}
 for(const t of tokens){call(vault,'reservesOfApp',[M,t.address]);call(manager,'protocolFeesAccrued',[t.address]);}
 call(vault,'isAppRegistered',[M]);for(const c of currencies){call(c,'decimals');call(c,'symbol');call(c,'balanceOf',[V]);}
 for(const [a,slots]of knownStorage)if(a===M){for(const s of slots)call(manager,'extsload(bytes32)',[s]);call(manager,'extsload(bytes32[])',[[...slots]]);}
 async function capture(id){
  const block=await provider.getBlock('latest'),tag=block.number;
  const snapshot={id,block:{num:tag,hash:block.hash.toLowerCase(),timestamp:block.timestamp},storage:{},callmap:{},code:{},native_balances:{},pools:[],app_reserves:{},protocol_fees:{}};
  for(const a of [M,V,...tokens.map(t=>t.address),...maths.map(c=>c.address.toLowerCase())]){snapshot.code[a]=(await provider.getCode(a,tag)).toLowerCase();snapshot.native_balances[a]=W(await provider.getBalance(a,tag));}
  requireThat(snapshot.code[M]===managerRuntime&&snapshot.code[V]===vaultRuntime,'immutable runtime at block');
  for(const [a,slots]of knownStorage){snapshot.storage[a]={};for(const s of slots)snapshot.storage[a][s]=ethers.utils.hexZeroPad(await provider.getStorageAt(a,s,tag),32).toLowerCase();}
  for(const q of calls){snapshot.callmap[q.address]??={};snapshot.callmap[q.address][q.data]=(await provider.call({to:q.address,data:q.data},tag)).toLowerCase();}
  function decoded(c,name,args){return c.interface.decodeFunctionResult(name,snapshot.callmap[c.address.toLowerCase()][c.interface.encodeFunctionData(name,args).toLowerCase()]);}
  requireThat(decoded(vault,'isAppRegistered',[M])[0]===true,'registered original manager');
  for(const t of tokens){snapshot.app_reserves[t.address]=decoded(vault,'reservesOfApp',[M,t.address])[0].toString();snapshot.protocol_fees[t.address]=decoded(manager,'protocolFeesAccrued',[t.address])[0].toString();}
  for(let i=0;i<pools.length;i++){
   const p=pools[i],slot=decoded(manager,'getSlot0',[p.id]),s={pool_id:p.id,active_id:Number(slot[0]),protocol_fee:Number(slot[1]),lp_fee:Number(slot[2]),bins:{},tree:{level0:snapshot.storage[M][add(p.pool_base,4)],level1:{},level2:{}},app_reserves:{...snapshot.app_reserves},protocol_fees:{...snapshot.protocol_fees}};
   for(const id of p.bin_ids){const b=decoded(manager,'getBin',[p.id,id]),h=await maths[i].bin(id,{blockTag:tag});requireThat(b.map(String).join(',')===h.map(String).join(','),'original manager/harness bin agreement');
    const next={};for(const d of [true,false]){const actual=Number(decoded(manager,'getNextNonEmptyBin',[p.id,d,id])[0]);requireThat(actual===Number(await maths[i].next(d,id,{blockTag:tag})),'native tree agreement');next[d?'true':'false']=actual;}
    s.bins[id]={reserve0:b[0].toString(),reserve1:b[1].toString(),liquidity:b[2].toString(),total_shares:b[3].toString(),price:(await maths[i].price(id,{blockTag:tag})).toString(),initialized:true,next};
    s.tree.level1[W(id>>16)]=snapshot.storage[M][map(id>>16,add(p.pool_base,5))];s.tree.level2[W(id>>8)]=snapshot.storage[M][map(id>>8,add(p.pool_base,6))];
   }
   snapshot.pools.push(s);
  }
  // extSload and standard storage must have identical bytes at the same block.
  for(const slot of knownStorage.get(M)){const value=decoded(manager,'extsload(bytes32)',[slot])[0];requireThat(value===snapshot.storage[M][slot],'extsload/storage agreement');}
  return snapshot;
 }
 const RPCbanks=[await capture('bank0')],trades=[];let amount=1600000n;
 for(const [i,[poolIndex,zeroForOne]]of [[0,true],[1,false],[1,true],[0,false]].entries()){
  const p=pools[poolIndex],c=maths[poolIndex],pre=RPCbanks.at(-1),prePool=pre.pools[poolIndex],fills=[];let left=amount,id=prePool.active_id;
  for(let visit=0;left>0n;visit++){
   requireThat(visit<32,'bounded tree traversal');const f=await c.fill(id,left.toString(),zeroForOne);const used=BigInt(f.used),outAmt=BigInt(f.out),b=prePool.bins[id];requireThat(b,'tree returned initialized bin');
   const inputField=zeroForOne?'reserve0':'reserve1',outputField=zeroForOne?'reserve1':'reserve0';const after={reserve0:b.reserve0,reserve1:b.reserve1};after[inputField]=(BigInt(b[inputField])+used-BigInt(f.protocol)).toString();after[outputField]=(BigInt(b[outputField])-outAmt).toString();
   fills.push({bin_id:id,price:f.p.toString(),amount_in:used.toString(),amount_out:outAmt.toString(),fee_amount:f.fee.toString(),protocol_amount:f.protocol.toString(),pre:{reserve0:b.reserve0,reserve1:b.reserve1},post:after});left-=used;
   if(left>0n){id=Number(await c.next(zeroForOne,id));requireThat(id!==0&&id!==16777215,'insufficient liquidity');}
  }
  const expected=await c.callStatic.swap(amount.toString(),zeroForOne,{gasLimit:12000000});
  const output=BigInt(zeroForOne?expected.delta1:expected.delta0),protocol=BigInt(expected.protocolAmount);
  requireThat(fills.reduce((n,f)=>n+BigInt(f.amount_out),0n)===output,'helper/full swap outputs');requireThat(fills.reduce((n,f)=>n+BigInt(f.protocol_amount),0n)===protocol,'helper/full swap protocol');
  await (await c.swap(amount.toString(),zeroForOne,{gasLimit:12000000})).wait();await projectPool(poolIndex);
  const inputToken=tokens[zeroForOne?0:1].address,outputToken=tokens[zeroForOne?1:0].address;
  await write(V,map(inputToken,vaultOuter),BigInt(pre.app_reserves[inputToken])+amount);await write(V,map(outputToken,vaultOuter),BigInt(pre.app_reserves[outputToken])-output);
  await write(M,map(inputToken,2),BigInt(pre.protocol_fees[inputToken])+protocol);
  const post=await capture('bank'+(i+1));RPCbanks.push(post);
  for(const f of fills)requireThat(post.pools[poolIndex].bins[f.bin_id].reserve0===f.post.reserve0&&post.pools[poolIndex].bins[f.bin_id].reserve1===f.post.reserve1,'full swap reserves');
  trades.push({id:'trade'+i,pool_id:p.id,zero_for_one:zeroForOne,amount_in:amount.toString(),pre_bank:pre.id,post_bank:post.id,expected:{delta0:expected.delta0.toString(),delta1:expected.delta1.toString(),amount_out:output.toString(),swap_fee:Number(expected.swapFee),protocol_amount:protocol.toString(),fee_amount:fills.reduce((n,f)=>n+BigInt(f.fee_amount),0n).toString(),bin_fills:fills,post:post.pools[poolIndex]}});
  amount=output;
 }
 const nativeArtifacts={};
 for(const [name,a]of Object.entries(artifacts)){
  const runtime='0x'+a.evm.deployedBytecode.object;
  const file=`${name}.artifact.json`,bytes=JSON.stringify({abi:a.abi,storage_layout:a.storageLayout,creation:'0x'+a.evm.bytecode.object,runtime_template:runtime,immutable_references:a.evm.deployedBytecode.immutableReferences,selectors:a.evm.methodIdentifiers},null,2)+'\n';fs.writeFileSync(path.join(ref,file),bytes);
  nativeArtifacts[name]={file,sha256:sha(bytes),runtime_bytes:(runtime.length-2)/2,runtime_template_sha256:sha(Buffer.from(runtime.slice(2),'hex')),runtime_template_keccak256:ethers.utils.keccak256(runtime)};
 }
 fs.writeFileSync(path.join(ref,'compile-evidence.json'),JSON.stringify(compileEvidence,null,2)+'\n');
 const loadedDependencies=Object.keys(require.cache).filter(p=>p.startsWith(path.join(base,'node_modules')+path.sep)).sort().map(p=>({path:path.relative(root,p),sha256:sha(fs.readFileSync(p))}));
 // soljson and executable are explicit even if module cache policies change.
 const binaryDependencies=[{path:path.relative(root,path.join(base,'node_modules/solc/soljson.js')),sha256:sha(fs.readFileSync(path.join(base,'node_modules/solc/soljson.js')))},{path:process.execPath,sha256:sha(fs.readFileSync(process.execPath))}];
 const reference={schema:'infinity-bin-service-source/1',revision:'3c5ef1be1f71442d8bc60cc6fd11f9c677b714df',compiler:solc.version(),original_settings:compileEvidence.settings,compile_input_sha256:compileEvidence.compile_input_sha256,harness_compile_input_sha256:sha(JSON.stringify(hi)),files:[...loaded.values()].sort((a,b)=>a.local.localeCompare(b.local)),dependencies,loaded_dependencies:loadedDependencies,binary_dependencies:binaryDependencies,node:process.version,dependency_lock_sha256:sha(fs.readFileSync(path.join(base,'package-lock.json'))),artifacts:nativeArtifacts,manager_runtime_sha256:sha(Buffer.from(managerRuntime.slice(2),'hex')),manager_runtime_keccak256:ethers.utils.keccak256(managerRuntime),vault_runtime_sha256:sha(Buffer.from(vaultRuntime.slice(2),'hex')),vault_runtime_keccak256:ethers.utils.keccak256(vaultRuntime),immutable_vault:V,immutable_references:refs,unlimited_contract_size:false,eip170_limit:24576,execution_engine:{name:'ganache',version:'7.9.2',hardfork:'shanghai',chain_id:31337},source_profile:{hooks:'0x0000000000000000000000000000000000000000',lp_fee_max:100000,bin_step_min:1,bin_step_max:100,protocol_fee_direction_max:4000},scope:{getters:'deployed unmodified original BinPoolManager and Vault; all banks read at one explicit block tag',math:'original BinPool.swap, BinHelper and TreeMath in isolated harness',fixture_setup:'evm_setAccountStorageAt projects harness state into original manager layout and propagates bank/protocol accounting',full_manager_swap_settlement_verified:false,vault_transient_storage_execution_verified:false},inputs};
 const fixture={schema:'infinity-bin-service/1',reference,chain_id:31337,manager:M,vault:V,tokens,pools,RPCbanks,trades};
 const bytes=JSON.stringify(fixture,null,2)+'\n';
 for(const dir of ['integration','chain-parser-go','dex-router'])fs.writeFileSync(path.join(root,dir,'test/contracts/infinity-bin-service.json'),bytes);
 fs.writeFileSync(path.join(ref,'source-metadata.json'),JSON.stringify(reference,null,2)+'\n');
 for(const [p,h]of Object.entries(inputs))requireThat(sha(fs.readFileSync(path.join(root,p)))===h,'input changed '+p);
 for(const s of reference.files)requireThat(sha(fs.readFileSync(path.join(base,'sources',s.local)))===s.sha256,'source changed '+s.local);
 for(const d of [...loadedDependencies,...binaryDependencies])requireThat(sha(fs.readFileSync(path.isAbsolute(d.path)?d.path:path.join(root,d.path)))===d.sha256,'dependency changed '+d.path);
 return {passed:true,fixture_sha256:sha(bytes),source_metadata_sha256:sha(fs.readFileSync(path.join(ref,'source-metadata.json'))),compile_evidence_sha256:sha(fs.readFileSync(path.join(ref,'compile-evidence.json'))),inputs,banks:RPCbanks.length,trades:trades.length,manager_runtime_bytes:nativeArtifacts.BinPoolManager.runtime_bytes,vault_runtime_bytes:nativeArtifacts.Vault.runtime_bytes,manager:M,vault:V,block_start:blockStart.number,block_end:RPCbanks.at(-1).block.num};
}
(async()=>{let result;try{result=await main();}catch(e){result={passed:false,error:String(e),stack:e.stack};console.error(e);process.exitCode=1;}finally{if(raw)try{await raw.disconnect();}catch(e){result={passed:false,error:'cleanup '+e};process.exitCode=1;}fs.writeFileSync(receiptPath,JSON.stringify(result,null,2)+'\n');}if(result.passed)console.log(JSON.stringify(result));})();
