'use strict';
// No imports of generators: every quote uses pinned original Solidity libraries.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../../..'),ref=path.join(root,'integration/tools/reference/infinity-bin-multi');
const receiptPath=path.join(ref,'execution-receipt.json');
// Must precede optional require, source parsing, pin validation and compilation.
fs.writeFileSync(receiptPath,JSON.stringify({passed:false,state:'RUNNING'})+'\n');
const sha=x=>crypto.createHash('sha256').update(x).digest('hex');
let raw;
async function main(){
 const runtime=path.join(root,'integration/.runtime/singleton-reference'),mods=path.join(runtime,'node_modules');
 const deps={};
 for(const [name,version] of [['solc','0.8.26'],['ganache','7.9.2'],['ethers','5.8.0']]){
  const file=path.join(mods,name,'package.json'),bytes=fs.readFileSync(file);assert.equal(JSON.parse(bytes).version,version);
  deps[name]={version,package_sha256:sha(bytes),entry_sha256:sha(fs.readFileSync(require.resolve(path.join(mods,name))))};
 }
 const solc=require(path.join(mods,'solc')),ganache=require(path.join(mods,'ganache')),{ethers}=require(path.join(mods,'ethers'));
 deps.solc.soljson_sha256=sha(fs.readFileSync(path.join(mods,'solc/soljson.js')));
 const pinPath='integration/tools/reference/singleton-advanced/source-metadata.json',pinBytes=fs.readFileSync(path.join(root,pinPath)),manifest=JSON.parse(pinBytes);
 const pins=new Map(manifest.files.map(f=>[f.local,f]));
 const harnessName='InfinityBinMultiReference.sol',harness=fs.readFileSync(path.join(ref,harnessName),'utf8');
 const ownedInputs=[path.relative(root,__filename),path.relative(root,path.join(ref,harnessName)),pinPath];
 const inputs=Object.fromEntries(ownedInputs.map(p=>[p,sha(fs.readFileSync(path.join(root,p)))]));
 const closure=new Set(),input={language:'Solidity',sources:{[harnessName]:{content:harness}},settings:{optimizer:{enabled:true,runs:200},viaIR:true,evmVersion:'shanghai',metadata:{bytecodeHash:'none',appendCBOR:false},outputSelection:{'*':{'*':['abi','evm.bytecode.object','evm.deployedBytecode.object']}}}};
 const compiled=JSON.parse(solc.compile(JSON.stringify(input),{import:name=>{
  const pin=pins.get(name);if(!pin)return {error:'Unpinned '+name};
  const bytes=fs.readFileSync(path.join(runtime,'sources',name));assert.equal(sha(bytes),pin.sha256,name);closure.add(name);return {contents:bytes.toString()};
 }}));
 assert.deepEqual((compiled.errors||[]).filter(e=>e.severity==='error'),[]);
 const artifact=compiled.contracts[harnessName].InfinityBinMultiReference;
 const runtimeBytes=Buffer.from(artifact.evm.deployedBytecode.object,'hex');
 assert.ok(runtimeBytes.length<=24576,`EIP170 harness too large: ${runtimeBytes.length}`);
 raw=ganache.provider({logging:{quiet:true},chain:{hardfork:'shanghai',chainId:31337,allowUnlimitedContractSize:false},wallet:{deterministic:true},miner:{blockGasLimit:100000000}});
 const provider=new ethers.providers.Web3Provider(raw),signer=provider.getSigner();provider.pollingInterval=10;
 const contract=await new ethers.ContractFactory(artifact.abi,artifact.evm.bytecode.object,signer).deploy({gasLimit:20000000});await contract.deployed();
 assert.equal(sha(Buffer.from((await provider.getCode(contract.address)).slice(2),'hex')),sha(runtimeBytes));
 function snapshot(s){return {active_id:Number(s.activeId),bins:Object.fromEntries(s.bins.map(b=>[String(b.id),{reserve0:b.reserve0.toString(),reserve1:b.reserve1.toString()}])),membership:s.membership,protocol0:s.protocol0.toString(),protocol1:s.protocol1.toString()};}
 function model(s){return {active_id:s.active_id,bin_step:s.bin_step,lp_fee:s.lp_fee,protocol_fee0:s.protocol_fee0,protocol_fee1:s.protocol_fee1,max_steps:s.max_steps??64,tree_min_id:0,tree_max_id:0xffffff,bins:s.bins};}
 const cases=[],A=8388608,M=(1n<<128n)-1n,S=(1n<<127n)-1n;
 const mk=(offsets,zero,lp=3000,step=1,p0=4000,p1=17)=>({active_id:A,bin_step:step,lp_fee:lp,protocol_fee0:p0,protocol_fee1:p1,bins:Object.fromEntries(offsets.map(o=>[A+o,{reserve0:'1000000',reserve1:'1000000'}]))});
 async function seed(s){const bins=Object.entries(s.bins).map(([id,b])=>[Number(id),b.reserve0,b.reserve1]);await (await contract.seed([s.active_id,s.bin_step,s.lp_fee,s.protocol_fee0+(s.protocol_fee1<<12),bins],{gasLimit:12000000})).wait();}
 // Tree/fee trace uses native lookup and native helpers. No JS price/fee math.
 async function trace(s,amount,zero){let id=s.active_id,left=BigInt(amount),steps=0;const fills=[];
  for(let n=0;n<=Object.keys(s.bins).length;n++){
   steps++;
   const q=await contract.fill(id,left.toString(),zero);
   if(q.used.gt(0)){fills.push({id,amount_in:q.used.toString(),amount_out:q.out.toString(),total_fee:q.fee.toString(),protocol_amount:q.protocol.toString(),price_x128:q.price.toString()});left-=BigInt(q.used.toString());}
   if(left===0n)return {steps,fills};
   id=Number(await contract.next(zero,id));if(id===0||id===0xffffff)return {steps,fills,out_of_liquidity:true};
  }throw Error('native tree failed to terminate');
 }
 async function run(name,s,requests,budget){
  await seed(s);
  for(let index=0;index<requests.length;index++){
   const [amount,zero]=requests[index],before=snapshot(await contract.snapshot());
   let tr;try{tr=await trace(s,amount,zero);}catch(e){tr={helper_reverted:true};}
   const q=await contract.callStatic.attempt(String(amount),zero,{gasLimit:20000000});
   await (await contract.attempt(String(amount),zero,{gasLimit:20000000})).wait();
   const post=snapshot(await contract.snapshot());assert.deepEqual(post,snapshot(q.post));assert.ok(post.membership.every(Boolean));
   const c={name:`${name}/${index}`,state:model(s),input:String(amount),zero_for_one:zero,expected:{}};
   if(budget!==undefined)c.state.max_steps=budget;
   if(!q.ok){assert.notEqual(q.reason,'0x');assert.deepEqual(post,before,'atomic rollback');c.expected={native_reverted:true,revert_data:q.reason,post};}
   else{
    assert.ok(!tr.helper_reverted&&!tr.out_of_liquidity);
    const out=(zero?q.delta1:q.delta0).toString(),p=q.protocolAmount.toString();
    assert.equal((zero?q.delta0:q.delta1).toString(),'-'+String(amount));
    assert.equal(tr.fills.reduce((n,f)=>n+BigInt(f.amount_out),0n).toString(),out);
    assert.equal(tr.fills.reduce((n,f)=>n+BigInt(f.protocol_amount),0n).toString(),p);
    c.expected={amount_out:out,consumed_in:String(amount),remaining_in:'0',protocol_amount:p,swap_fee:Number(q.swapFee),delta0:q.delta0.toString(),delta1:q.delta1.toString(),steps:tr.steps,bin_fills:tr.fills,post};
    if(budget!==undefined&&tr.steps>budget)c.expected.support_rejection='INFINITY_BIN_STEP_BUDGET';
   }
   cases.push(c);s={...s,active_id:post.active_id,bins:post.bins};
  }
 }
 for(const count of [2,3,8,12])for(const zero of [true,false])for(const lp of [0,1,3000,100000]){
  const direction=zero?-1:1,offsets=Array.from({length:count},(_,i)=>direction*i*(i%2?257:1));
  const s=mk(offsets,zero,lp,1,4000,4000);
  for(const amount of ['1','2','999999','1000000','1000001',String(count*800000),String(count*4000000)])
   await run(`bins-${count}/zero-${zero}/lp-${lp}/amount-${amount}`,s,[[amount,zero]]);
 }
 // Every initialized bin receives a partial fill and an exact depletion at zero fee.
 // Native helper determines the precise gross cap at each sparse ID.
 for(const zero of [true,false])for(const step of [1,25,65535]){
  const direction=zero?-1:1;
  const offsets=(step===65535?[0,1,2]:step===1?[0,1,255,256,257,65535,65536,65537]:[0,1,257,65536]).map(o=>o*direction);
  const s=mk(offsets,zero,0,step,0,0);await seed(s);
  let cap=0n;
  for(const id of offsets.map(o=>A+o)){
   // Extreme id/step can make the original helper revert; covered as native error.
   let f;try{f=await contract.fill(id,S.toString(),zero);}catch(e){
    assert.ok(e.error?.data||e.data,'missing native helper revert');
    await run(`extreme-price/zero-${zero}/step-${step}/id-${id}`,s,[['4000001',zero]]);continue;
   }
   cap+=BigInt(f.used.toString());
    await run(`partial-each/zero-${zero}/step-${step}/id-${id}`,s,[[(cap-BigInt(f.used.toString())+BigInt(f.used.toString())/2n+1n).toString(),zero]]);
    await run(`deplete-each/zero-${zero}/step-${step}/id-${id}`,s,[[cap.toString(),zero]]);await seed(s);
  }
 }
 for(const zero of [true,false]){
  const s=mk([0,zero?-1:1,zero?-256:256,zero?-65536:65536],zero,100000,1);
  s.bins[A][zero?'reserve1':'reserve0']='0';s.bins[A+(zero?-1:1)][zero?'reserve1':'reserve0']='0';
  await run(`retained-empty/zero-${zero}`,s,[['100',zero],['100',!zero],['1000000000000',zero],['17',!zero]]);
  const absent=mk([zero?-1:1,zero?-257:257],zero,3000);await run(`uninitialized-active/zero-${zero}`,absent,[['100',zero]]);
  await run(`budget/zero-${zero}`,mk([0,zero?-257:257],zero,3000),[['1500000',zero]],1);
  await run(`sequence/zero-${zero}`,mk([0,-1,1,-257,257,-65536,65536,2],zero,100000),[['2500000',zero],['2000000',!zero],['999999999999999',zero],['10000',!zero],['1',zero]]);
 }
 for(const zero of [true,false])for(const amount of [S.toString(),(S+1n).toString()]){
  const s=mk([0,zero?-1:1],zero,0,1,0,0);for(const b of Object.values(s.bins)){b.reserve0=S.toString();b.reserve1=S.toString();}
  await run(`signed-bound/zero-${zero}/amount-${amount}`,s,[[amount,zero]]);
 }
 for(const zero of [true,false]){
  const s=mk([0,zero?-1:1],zero,0,1,0,0);s.bins[A][zero?'reserve0':'reserve1']=M.toString();await run(`reserve-overflow/zero-${zero}`,s,[['100',zero]]);
  const cap=mk([0,zero?-1:1],zero,100000);for(const b of Object.values(cap.bins)){b.reserve0='1000000000000000000000000000000000';b.reserve1=b.reserve0;}await run(`liquidity-cap/zero-${zero}`,cap,[['100',zero]]);
  const width=mk([0,zero?-1:1],zero,100000);width.active_id=A+(zero?-1:1);width.bins[width.active_id][zero?'reserve1':'reserve0']=M.toString();await run(`max-input-u128/zero-${zero}`,width,[['100',zero]]);
 }
 // Isolate final signed input/output bounds at tiny price, avoiding the
 // liquidity cap that would otherwise mask them at the central price.
 const low=mk([-600000,-600001],true,0,1,0,0);low.active_id=A-600000;
 await seed(low);const lowPrice=BigInt((await contract.fill(low.active_id,'1',true)).price.toString());
 assert.ok(lowPrice>2n);
 for(const amount of [S,S+1n]){
  const s=structuredClone(low);s.bins[s.active_id]={reserve0:'1',reserve1:(lowPrice-1n).toString()};
  await run(`isolated-signed-input/${amount}`,s,[[amount.toString(),true]]);
 }
 for(const reserve of [S,S+1n]){
  const s=structuredClone(low);s.bins[s.active_id]={reserve0:reserve.toString(),reserve1:'1'};await seed(s);
  const cap=await contract.fill(s.active_id,S.toString(),false);
  await run(`isolated-signed-output/${reserve}`,s,[[cap.used.toString(),false]]);
 }
 const summed=structuredClone(low);for(const b of Object.values(summed.bins)){b.reserve0=M.toString();b.reserve1='1';}
 summed.active_id=A-600001;await seed(summed);let sum=0n;
 for(const id of Object.keys(summed.bins)){sum+=BigInt((await contract.fill(Number(id),S.toString(),false)).used.toString());}
 await run('aggregate-output-u128-overflow',summed,[[sum.toString(),false]]);
 for(const zero of [true,false]){
  const empty=mk([0,zero?-257:257],zero,3000);for(const b of Object.values(empty.bins)){b.reserve0='0';b.reserve1='0';}
  await run(`all-depleted/zero-${zero}`,empty,[['100',zero]]);
  const feeOverflow=mk([0,zero?-1:1],zero,100000);feeOverflow.bins[A][zero?'reserve1':'reserve0']=M.toString();
  await run(`gross-cap-u128-overflow/zero-${zero}`,feeOverflow,[['100',zero]]);
 }
 const liqOverflow=mk([600000,600001],true,0,1,0,0);liqOverflow.active_id=A+600000;
 liqOverflow.bins[liqOverflow.active_id]={reserve0:(M/2n).toString(),reserve1:'1'};
 await run('liquidity-u256-overflow',liqOverflow,[['1',true]]);
 // Dynamic LP fee is a snapshot property: no hook/override execution is asserted.
 await run('dynamic-lp-snapshot',mk([0,-1,-257],true,99999),[['1500000',true],['5000',false]]);
 const sourceFiles=[...closure].sort().map(p=>pins.get(p));
 const metadata={schema:'infinity-bin-multi-source/1',upstream_manifest_sha256:sha(pinBytes),compiler:solc.version(),dependencies:deps,files:sourceFiles,harness_sha256:sha(harness),compile_input_sha256:sha(JSON.stringify(input)),creation_sha256:sha(Buffer.from(artifact.evm.bytecode.object,'hex')),runtime_sha256:sha(runtimeBytes),runtime_bytes:runtimeBytes.length,eip170_limit:24576,unlimited_contract_size:false,reference_scope:'original BinPool.swap plus TreeMath/BinHelper; no manager or settlement'};
 const fixture={schema:'infinity-bin-multi/1',reference:metadata,cases};const bytes=JSON.stringify(fixture,null,2)+'\n';
 for(const base of ['integration','dex-router'])fs.writeFileSync(path.join(root,base,'test/contracts/infinity-bin-multi.json'),bytes);
 fs.writeFileSync(path.join(ref,'source-metadata.json'),JSON.stringify(metadata,null,2)+'\n');
 for(const [p,h]of Object.entries(inputs))assert.equal(sha(fs.readFileSync(path.join(root,p))),h,p+' changed during generation');
 for(const p of sourceFiles)assert.equal(sha(fs.readFileSync(path.join(runtime,'sources',p.local))),p.sha256);
 const result={passed:true,inputs,dependency_lock_sha256:sha(fs.readFileSync(path.join(runtime,'package-lock.json'))),node:process.version,compiler:solc.version(),runtime_sha256:sha(runtimeBytes),runtime_bytes:runtimeBytes.length,source_metadata_sha256:sha(fs.readFileSync(path.join(ref,'source-metadata.json'))),cases:cases.length,native_success:cases.filter(c=>!c.expected.native_reverted).length,native_reverts:cases.filter(c=>c.expected.native_reverted).length,support_budget_rejections:cases.filter(c=>c.expected.support_rejection).length,fixture_sha256:sha(bytes)};
 return result;
}
(async()=>{let result;try{result=await main();}catch(e){result={passed:false,error:String(e),stack:e.stack};console.error(e);process.exitCode=1;}finally{if(raw){try{await raw.disconnect();}catch(e){result={passed:false,error:'provider cleanup: '+e};process.exitCode=1;}}fs.writeFileSync(receiptPath,JSON.stringify(result,null,2)+'\n');}if(result.passed)console.log(JSON.stringify(result));})();
