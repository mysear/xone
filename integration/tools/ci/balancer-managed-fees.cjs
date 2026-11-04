// Original e281ff40 ManagedPool initialization and fee-minting swap transactions.
'use strict';
const fs=require('fs'),path=require('path'),crypto=require('crypto');
const root=path.resolve(__dirname,'../../..'),ref=path.join(root,'integration/tools/reference/balancer-managed-fees');
const receiptPath=path.join(ref,'generation.json');
function atomic(file,value){const tmp=file+'.tmp-'+process.pid;fs.writeFileSync(tmp,JSON.stringify(value,null,2)+'\n');fs.renameSync(tmp,file);}
// Invalidate before imports, pin parsing, source verification, compilation or EVM startup.
atomic(receiptPath,{passed:false,state:'RUNNING',started_at_unix_ms:Date.now()});
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const rev='e281ff40583c39bf1d46c322334b2b13936e0e33',E=10n**18n,MAX=(1n<<256n)-1n;
let provider,ganacheProvider;
async function main(){
 const runtime=path.join(root,'integration/.runtime/stable-reference/node_modules');
 const {ethers}=require(path.join(runtime,'ethers')),solc=require(path.join(runtime,'solc07')),ganache=require(path.join(runtime,'ganache'));
 const manifestPath=path.join(root,'integration/tools/reference/balancer-multi',rev+'.json');
 const manifest=JSON.parse(fs.readFileSync(manifestPath));if(manifest.revision!==rev)throw Error('revision mismatch');
 if(!solc.version().startsWith('0.7.6+'))throw Error('solc 0.7.6 required');
 const inputs={},sources={};
 function record(p){const bytes=fs.readFileSync(p);inputs[path.relative(root,p)]=sha(bytes);return bytes;}
 record(manifestPath);record(__filename);
 for(const f of manifest.files){const p=path.join(root,'integration/tools/reference/balancer-multi/sources',rev,f.path);const bytes=record(p);if(sha(bytes)!==f.sha256)throw Error('source hash '+f.path);sources[f.path]={content:bytes.toString()};}
 sources['ManagedFeesHarness.sol']={content:record(path.join(ref,'ManagedFeesHarness.sol')).toString()};
 // Hash every actual JavaScript/WASM/native dependency byte under this dedicated runtime.
 function walk(d){for(const n of fs.readdirSync(d).sort()){const p=path.join(d,n),s=fs.lstatSync(p);if(s.isDirectory())walk(p);else if(s.isFile()&&/\.(js|cjs|json|wasm|node)$/.test(n))record(p);}}
 walk(runtime);
 const settings={optimizer:{enabled:true,runs:200},outputSelection:{'*':{'':['ast'],'*':['abi','storageLayout','evm.bytecode.object','evm.deployedBytecode.object','evm.deployedBytecode.immutableReferences']}}};
 const output=JSON.parse(solc.compile(JSON.stringify({language:'Solidity',sources,settings}),{import:p=>{const k=p.startsWith('@balancer-labs/v2-')?'pkg/'+p.slice(18):p;return sources[k]?{contents:sources[k].content}:{error:'Unpinned import '+p};}}));
 if((output.errors||[]).some(e=>e.severity==='error'))throw Error(output.errors.map(e=>e.formattedMessage).join('\n'));
 const poolFile='pkg/pool-weighted/contracts/smart/ManagedPool.sol',artifact=output.contracts[poolFile].ManagedPool;
 const bytecode=artifact.evm.deployedBytecode,spans=Object.values(bytecode.immutableReferences).flat().sort((a,b)=>a.start-b.start),template=Buffer.from(bytecode.object,'hex');
 for(const s of spans)template.fill(0,s.start,s.start+s.length);
 const immutable_ast={};
 function astWalk(n,source){if(!n||typeof n!=='object')return;if(n.nodeType==='VariableDeclaration'&&bytecode.immutableReferences[String(n.id)])immutable_ast[n.id]={name:n.name,source,src:n.src,type:n.typeDescriptions?.typeString,mutability:n.mutability};for(const v of Object.values(n)){if(Array.isArray(v))for(const x of v)astWalk(x,source);else if(v&&typeof v==='object')astWalk(v,source);}}
 for(const [source,v] of Object.entries(output.sources))astWalk(v.ast,source);
 if(Object.keys(immutable_ast).length!==Object.keys(bytecode.immutableReferences).length)throw Error('unmapped immutable AST');
 const metadata={eip170_deployable:template.length<=24576,admission_status:template.length<=24576?'requires-standard-deployment-proof':'reference-only-over-eip170',revision:rev,solc:solc.version(),compiler_settings:settings,pool_source:poolFile,storageLayout:artifact.storageLayout,immutableReferences:bytecode.immutableReferences,immutable_ast,immutable_spans:spans,deployedBytecode:bytecode.object,normalized_runtime_sha256:sha(template),runtime_bytes:template.length,source_and_dependency_sha256:inputs};
 atomic(path.join(ref,'runtime-metadata.json'),metadata);
 atomic(path.join(ref,'compiler-identity.json'),Object.fromEntries(['eip170_deployable','admission_status','revision','solc','pool_source','runtime_bytes','normalized_runtime_sha256','immutableReferences','immutable_ast','immutable_spans','storageLayout'].map(k=>[k,metadata[k]])));
 console.log(JSON.stringify({runtime_metadata:'integration/tools/reference/balancer-managed-fees/runtime-metadata.json',normalized_runtime_sha256:metadata.normalized_runtime_sha256,immutable_spans:spans.length,runtime_bytes:template.length}));
 if(process.argv.includes('--compile-only'))return;
 if(!metadata.eip170_deployable&&!process.argv.includes('--reference-only'))throw Error('EIP170: original runtime exceeds 24576 bytes; --reference-only is required for isolated arithmetic evidence, no service admission');
 // Prove the code-size restriction using normal EVM contract creation on
 // a separately owned provider, before the explicit reference-only provider.
 const normalGanache=ganache.provider({logging:{quiet:true},chain:{hardfork:'shanghai',allowUnlimitedContractSize:false,time:new Date('2026-01-01T00:00:00Z')},miner:{blockGasLimit:60000000},wallet:{deterministic:true}});
 try {
  const normal=new ethers.providers.Web3Provider(normalGanache),signer=normal.getSigner(0),who=await signer.getAddress();
  async function deployNormal(file,name,args=[]){const c=output.contracts[file][name],x=await new ethers.ContractFactory(c.abi,c.evm.bytecode.object,signer).deploy(...args,{gasLimit:55000000});await x.deployed();return x;}
  const vault=await deployNormal('ManagedFeesHarness.sol','ManagedFeesVault',[ethers.constants.AddressZero]);
  const t0=await deployNormal('ManagedFeesHarness.sol','ManagedFeesToken',[18]),t1=await deployNormal('ManagedFeesHarness.sol','ManagedFeesToken',[18]);
  const t=[t0.address,t1.address].sort((a,b)=>a.toLowerCase().localeCompare(b.toLowerCase()));
  const params=['Managed fee reference','MFREF',t,[String(E/2n),String(E/2n)],t.map(()=>ethers.constants.AddressZero),String(10n**16n),true,false,String(3n*E/10n),'0','0',vault.address];
  const f=new ethers.ContractFactory(artifact.abi,artifact.evm.bytecode.object,signer),request=f.getDeployTransaction(params,vault.address,who,100000,100000);
  let estimateError;
  try {await normal.estimateGas(request);}catch(e){estimateError=e.error?.message||e.reason||e.message;}
  if(!metadata.eip170_deployable){
   if(!estimateError||!estimateError.includes('code size to deposit exceeds maximum code size'))throw Error('expected explicit EIP170 rejection');
   const tx=await signer.sendTransaction({...request,gasLimit:55000000});let r;
   try {r=await tx.wait();}catch(e){r=e.receipt;}
   if(!r||r.status!==0||await normal.getCode(r.contractAddress)!=='0x')throw Error('normal deployment rejection not confirmed');
   atomic(path.join(ref,'standard-evm-deployment.json'),{revision:rev,solc:solc.version(),runtime_bytes:metadata.runtime_bytes,normalized_runtime_sha256:metadata.normalized_runtime_sha256,eip170_limit:24576,code_size_limit_enforced:true,deployed:false,error:estimateError,transaction_hash:r.transactionHash,status:r.status,gas_used:String(r.gasUsed)});
  }else{
   const x=await f.deploy(params,vault.address,who,100000,100000,{gasLimit:55000000});await x.deployed();
   atomic(path.join(ref,'standard-evm-deployment.json'),{revision:rev,solc:solc.version(),runtime_bytes:metadata.runtime_bytes,normalized_runtime_sha256:metadata.normalized_runtime_sha256,code_size_limit_enforced:true,deployed:true,address:x.address,runtime:await normal.getCode(x.address)});
  }
 }finally{await normalGanache.disconnect();}
 ganacheProvider=ganache.provider({logging:{quiet:true},chain:{hardfork:'shanghai',allowUnlimitedContractSize:true,time:new Date('2026-01-01T00:00:00Z')},miner:{blockGasLimit:60000000,timestampIncrement:0},wallet:{deterministic:true}});
 provider=new ethers.providers.Web3Provider(ganacheProvider);
 const signer=provider.getSigner(0),owner=await signer.getAddress();
 async function deploy(file,name,args=[]){const c=output.contracts[file][name],x=await new ethers.ContractFactory(c.abi,c.evm.bytecode.object,signer).deploy(...args,{gasLimit:55000000});await x.deployed();return x;}
 const observation=await deploy('ManagedFeesHarness.sol','ManagedFeesObservation');
 const cases=[],sequences=[],guards=[];

 function balReason(e){
  const values=[e.data,e.error?.data,e.error?.error?.data].map(v=>typeof v==='string'?v:v?.result);
  for(const d of values)if(typeof d==='string'&&d.startsWith('0x08c379a0'))return ethers.utils.defaultAbiCoder.decode(['string'],'0x'+d.slice(10))[0];
  throw e;
 }
 async function getState(pool,same){return {total_supply:String(await pool.totalSupply()),protocol_swap_fee:String(await pool.getProtocolSwapFeePercentageCache()),management_swap_fee:String(await pool.getManagementSwapFeePercentage()),owner_balance:String(await pool.balanceOf(owner)),protocol_collector_balance:String(await pool.balanceOf(await pool.getProtocolFeesCollector())),same_recipient:same};}
 for(const count of [2,3,8,20,38]){
  const pairs=[];
  for(let k=0;k<count;k++){const decimals=[18,6,0,12][k%4],t=await deploy('ManagedFeesHarness.sol','ManagedFeesToken',[decimals]);pairs.push([t.address,decimals]);}
  pairs.sort((a,b)=>a[0].toLowerCase().localeCompare(b[0].toLowerCase()));
  const tokens=pairs.map(p=>p[0]),decimals=pairs.map(p=>p[1]),scales=decimals.map(d=>10n**BigInt(36-d));
  const w=Array(count).fill(E/BigInt(count));w[count-1]=E-w[0]*BigInt(count-1);
  const initial=decimals.map((d,k)=>BigInt(k+1)*1000000n*10n**BigInt(d));
  const profiles=[['protocol_only',3n*E/10n,0n,false,10n**16n],['management_only',0n,E/10n,false,10n**16n],['both',3n*E/10n,E/10n,false,10n**16n],['zero',0n,0n,false,10n**12n],['full_management',E/2n,E,false,8n*E/10n],['same_recipient',3n*E/10n,E/10n,true,10n**16n]];
  for(const [profile,p,m,same,swapFee] of profiles){
   const vault=await deploy('ManagedFeesHarness.sol','ManagedFeesVault',[same?owner:ethers.constants.AddressZero]);
   const params=['Managed fee reference','MFREF',tokens,w.map(String),tokens.map(()=>ethers.constants.AddressZero),String(swapFee),true,false,String(p),String(m),'0',vault.address];
   const pool=await deploy(poolFile,'ManagedPool',[params,vault.address,owner,100000,100000]);
   const poolId=await pool.getPoolId(),collector=await pool.getProtocolFeesCollector();
   const join=await(await vault.initialize(pool.address,owner,initial.map(String),{gasLimit:25000000})).wait();
   const deployedCode=await provider.getCode(pool.address);
   if(count===2&&profile==='protocol_only'){
    const deployment={eip170_deployable:metadata.eip170_deployable,admission_status:metadata.admission_status,code_size_limit_enforced:false,revision:rev,solc:solc.version(),pool:pool.address,vault:vault.address,owner,protocol_collector:collector,pool_id:poolId,deployed_runtime:deployedCode,runtime_sha256:sha(Buffer.from(deployedCode.slice(2),'hex')),normalized_runtime_sha256:metadata.normalized_runtime_sha256,runtime_bytes:metadata.runtime_bytes,initialization_transaction:join.transactionHash,block:await provider.getBlock('latest'),state:await getState(pool,same),immutables:Object.fromEntries(Object.entries(bytecode.immutableReferences).map(([id,ss])=>[id,{...immutable_ast[id],spans:ss,values:ss.map(v=>'0x'+deployedCode.slice(2+v.start*2,2+(v.start+v.length)*2))}]))};
    atomic(path.join(ref,'original-deployment.json'),deployment);
    console.log(JSON.stringify({original_deployment:'integration/tools/reference/balancer-managed-fees/original-deployment.json',pool:pool.address,runtime_sha256:deployment.runtime_sha256}));
   }
   const rawCode=Buffer.from(deployedCode.slice(2),'hex');
   for(const span of spans)rawCode.fill(0,span.start,span.start+span.length);
   if(rawCode.length!==metadata.runtime_bytes||sha(rawCode)!==metadata.normalized_runtime_sha256)throw Error('deployed normalized runtime mismatch');
   const initialized=await getState(pool,same);
   if(BigInt(initialized.total_supply)<=1000000n||initialized.owner_balance!==(BigInt(initialized.total_supply)-1000000n).toString()||(!same&&initialized.protocol_collector_balance!=='0'))throw Error('original initialization mint');
   let weights=(await pool.getNormalizedWeights()).map(String),fee=String(await pool.getSwapFeePercentage());
   async function internal(raw,input,i,j){
    const a=[raw[i],raw[j],weights[i],weights[j],scales[i],scales[j],fee,input].map(String),v=(await observation.observe(a)).map(String);
    return {net_scaled:v[0],reconstructed_gross_scaled:v[1],amount_out_scaled:v[2],pre_invariant:v[3],post_invariant:v[4],pre_balances:[v[5],v[6]],post_balances:[v[7],v[8]],reconstructed_gross_differs:BigInt(v[1])!==input*scales[i]/E,pre_downscale_differs:BigInt(v[2])!==(BigInt(v[2])*E/scales[j])*scales[j]/E};
   }
   async function run(raw,input,i,j,extra={}){
    const before=await getState(pool,same),storedBefore=(await vault.getPoolTokens(poolId))[1].map(String),block=await provider.getBlock('latest');
    const c={profile,balances:raw.map(String),weights:[...weights],scales:scales.map(String),fee,input:String(input),index_in:i,index_out:j,state:before,pool:pool.address,vault:vault.address,owner,protocol_collector:collector,initialization_transaction:join.transactionHash,block_number:block.number,timestamp:block.timestamp,...extra};
    let quote,reason;
    try{quote=await vault.callStatic.quote(pool.address,raw.map(String),String(input),i,j,{gasLimit:25000000});}catch(e){reason=balReason(e);if(!/^BAL#[0-9]{3}$/.test(reason))throw e;}
    let txReceipt;
    try{txReceipt=await(await vault.quote(pool.address,raw.map(String),String(input),i,j,{gasLimit:25000000})).wait();}catch(e){if(!reason||!e.receipt||e.receipt.status!==0)throw e;txReceipt=e.receipt;}
    const after=await getState(pool,same),storedAfter=(await vault.getPoolTokens(poolId))[1].map(String);
    if(reason){
     if(txReceipt.status!==0||JSON.stringify(after)!==JSON.stringify(before)||JSON.stringify(storedAfter)!==JSON.stringify(storedBefore)||txReceipt.logs.length)throw Error('revert failed atomic rollback');
     c.expected={reverted:true,reason,next_state:after,next_balances:storedAfter,transaction_hash:txReceipt.transactionHash,rollback_verified:true};
    }else{
     if(txReceipt.status!==1)throw Error('quote/transaction disagreement');
     const minted=[];
     for(const log of txReceipt.logs.filter(l=>l.address.toLowerCase()===pool.address.toLowerCase())){const event=pool.interface.parseLog(log);if(event.name==='Transfer'&&event.args.from===ethers.constants.AddressZero)minted.push({to:event.args.to,amount:String(event.args.value)});}
     let protocol=0n,management=0n;
     if(same){if(minted.length>2)throw Error('unexpected same recipient mints');if(p>0n&&minted.length>0&&BigInt(minted[0].amount)>0n){protocol=BigInt(minted.shift().amount);}if(minted.length)management=BigInt(minted.shift().amount);}
     else for(const mint of minted){if(mint.to.toLowerCase()===collector.toLowerCase())protocol+=BigInt(mint.amount);else if(mint.to.toLowerCase()===owner.toLowerCase())management+=BigInt(mint.amount);else throw Error('unexpected mint recipient');}
     if(BigInt(after.total_supply)!==BigInt(before.total_supply)+protocol+management)throw Error('total supply mint discrepancy');
     if(same){if(after.owner_balance!==after.protocol_collector_balance||BigInt(after.owner_balance)!==BigInt(before.owner_balance)+protocol+management)throw Error('same recipient coalescing');}
     else if(BigInt(after.owner_balance)!==BigInt(before.owner_balance)+management||BigInt(after.protocol_collector_balance)!==BigInt(before.protocol_collector_balance)+protocol)throw Error('recipient balances');
     if(JSON.stringify(storedAfter)!==JSON.stringify(quote.next.map(String)))throw Error('raw Vault balances mismatch');
     c.internal=await internal(raw,input,i,j);
     c.expected={amount_out:String(quote.out),next_balances:storedAfter,next_state:after,protocol_bpt:String(protocol),management_bpt:String(management),transaction_hash:txReceipt.transactionHash};
    }
    return c;
   }
   const i=0,j=count-1,edge=(initial[i]*3n/10n*E+(E-swapFee)-1n)/(E-swapFee);
   for(const input of [0n,1n,2n,10n**BigInt(decimals[i])+1n,initial[i]/100n+1n,edge,edge+1n,initial[i],MAX]){
    const snap=await provider.send('evm_snapshot',[]);cases.push(await run(initial,input,i,j));if(!await provider.send('evm_revert',[snap]))throw Error('snapshot revert failed');
   }
   for(const [a,b] of [[count-1,0],[0,Math.min(1,count-1)]]){
    const snap=await provider.send('evm_snapshot',[]);cases.push(await run(initial,initial[a]/100n+7n,a,b));if(!await provider.send('evm_revert',[snap]))throw Error('snapshot revert failed');
   }
   const seq={profile,balances:initial.map(String),weights:[...weights],scales:scales.map(String),fee,state:await getState(pool,same),steps:[]};let raw=[...initial];
   for(let k=0;k<4;k++){const a=k%count,b=(k+1)%count,c=await run(raw,1000n*10n**BigInt(decimals[a])+11n,a,b);if(c.expected.reverted||c.expected.amount_out==='0')throw Error('sequential swap failed');seq.steps.push(c);raw=c.expected.next_balances.map(BigInt);}
   sequences.push(seq);
   if(profile==='both'){
    const before=await provider.getBlock('latest'),start=before.timestamp+100,end=start+1000;
    const endWeights=Array(count).fill(E/100n);endWeights[0]=E-BigInt(count-1)*E/100n;
    await(await pool.updateWeightsGradually(start,end,endWeights.map(String),{gasLimit:10000000})).wait();
    await(await pool.updateSwapFeeGradually(start,end,fee,String(8n*E/10n),{gasLimit:10000000})).wait();
    for(const time of [start-1,start,start+333,end,end+1]){
     await provider.send('evm_setTime',[time*1000]);await provider.send('evm_mine',[]);
     weights=(await pool.getNormalizedWeights()).map(String);fee=String(await pool.getSwapFeePercentage());
     cases.push(await run(raw,1000n*10n**BigInt(decimals[0])+17n,0,count-1,{weight_schedule:true}));raw=cases[cases.length-1].expected.next_balances.map(BigInt);
    }
   }
   if(count===2&&profile==='management_only'){
    // Set k exactly to 1 so all preceding uint256 math succeeds and the
    // original ERC20 totalSupply addition itself overflows on management mint.
    const snap=await provider.send('evm_snapshot',[]),obs=await internal(initial,initial[0]/100n,0,1);
    const growth=BigInt(obs.post_invariant)*E/BigInt(obs.pre_invariant),delta=growth-E;
    const tiny=(2n*E+delta-1n)/delta;
    await(await pool.setManagementSwapFeePercentage(String(tiny))).wait();
    const supplySlot=artifact.storageLayout.storage.find(x=>x.label==='_totalSupply');
    if(!supplySlot||supplySlot.offset!==0)throw Error('missing totalSupply layout');
    await provider.send('evm_setAccountStorageAt',[pool.address,ethers.utils.hexZeroPad(ethers.utils.hexlify(BigInt(supplySlot.slot)),32),ethers.utils.hexZeroPad(ethers.utils.hexlify(MAX),32)]);
    const c=await run(initial,initial[0]/100n,0,1,{storage_injection:'totalSupply=max uint256; tiny fee gives k=1'});
    if(!c.expected.reverted||c.expected.reason!=='BAL#000')throw Error('fee mint overflow expected');
    cases.push(c);guards.push({kind:'fee_mint_overflow_rollback',case_index:cases.length-1,...c.expected});
    if(!await provider.send('evm_revert',[snap]))throw Error('snapshot revert failed');
   }
   if(count===2&&profile==='full_management'){
    const snap=await provider.send('evm_snapshot',[]),raw2=[...initial];raw2[0]=(1n<<112n)-1n;
    const c=await run(raw2,1n,0,1);if(!c.expected.reverted)throw Error('Vault cash overflow expected');cases.push(c);guards.push({kind:'vault_uint112_rollback',case_index:cases.length-1,...c.expected});
    if(!await provider.send('evm_revert',[snap]))throw Error('snapshot revert failed');
   }
  }
  console.log(JSON.stringify({completed_token_count:count,cases:cases.length,sequences:sequences.length}));
 }

 for(const [p,h] of Object.entries(inputs))if(sha(fs.readFileSync(path.join(root,p)))!==h)throw Error('input changed '+p);
 const corpus={schema_version:1,reference:{eip170_deployable:metadata.eip170_deployable,admission_status:metadata.admission_status,revision:rev,solc:solc.version(),flow:'Unmodified original ManagedPool constructor -> original onJoinPool INIT -> original onSwap transactions -> private _payProtocolAndManagementFees; test Vault raw cash facade; independent original WeightedMath selected invariant observer',runtime_normalized_sha256:metadata.normalized_runtime_sha256,source_files:manifest.files.length},cases,sequences,guards};
 const text=JSON.stringify(corpus,null,2)+'\n';
 for(const svc of ['integration','dex-router']){const file=path.join(root,svc,'test/contracts/balancer-managed-fees.json');const tmp=file+'.tmp-'+process.pid;fs.writeFileSync(tmp,text);fs.renameSync(tmp,file);}
 atomic(receiptPath,{passed:true,evidence_scope:'original-callback-reference-only',eip170_deployable:metadata.eip170_deployable,service_admission:false,revision:rev,finished_at_unix_ms:Date.now(),corpus_sha256:sha(text),runtime_metadata_sha256:sha(fs.readFileSync(path.join(ref,'runtime-metadata.json'))),cases:cases.length,sequences:sequences.length,steps:sequences.reduce((n,s)=>n+s.steps.length,0),guards:guards.length,source_and_dependency_sha256:inputs});
 console.log(JSON.stringify({passed:true,cases:cases.length,sequences:sequences.length,guards:guards.length,corpus_sha256:sha(text)}));
}
main().catch(e=>{atomic(receiptPath,{passed:false,state:'FAILED',error:String(e)});console.error(e);process.exitCode=1;}).finally(async()=>{if(ganacheProvider)await ganacheProvider.disconnect();});
