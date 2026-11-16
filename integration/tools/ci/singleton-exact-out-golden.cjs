const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../../..'),ref=path.join(root,'integration/tools/reference/singleton-cl'),runtime=path.join(root,'integration/.runtime/singleton-reference');
const solc=require(path.join(runtime,'node_modules/solc')),ganache=require(path.join(runtime,'node_modules/ganache')),{ethers}=require(path.join(runtime,'node_modules/ethers'));
for(const [name,version]of[['solc','0.8.26'],['ganache','7.9.2'],['ethers','5.8.0']])assert.equal(require(path.join(runtime,'node_modules',name,'package.json')).version,version);
const sha=b=>crypto.createHash('sha256').update(b).digest('hex'),meta=JSON.parse(fs.readFileSync(path.join(ref,'source-metadata.json')));
for(const item of meta.files)assert.equal(sha(fs.readFileSync(path.join(runtime,'sources',item.local))),item.sha256);
const outputRef=path.join(root,'integration/tools/reference/singleton-exact-out');
const pinsPath=path.join(outputRef,'source-pins.json'),pins=JSON.parse(fs.readFileSync(pinsPath));
for(const [key,hashKey]of[['original_source_manifest','original_source_manifest_sha256'],['baseline_templates','baseline_templates_sha256'],['package_lock','package_lock_sha256']])assert.equal(sha(fs.readFileSync(path.join(root,pins[key]))),pins[hashKey],key);
const inputPaths=[__filename,pinsPath,...['ReferenceTypes.sol','V4Reference.sol','InfinityReference.sol'].map(p=>path.join(outputRef,p))];
const inputs=Object.fromEntries(inputPaths.map(p=>[path.relative(root,p),sha(fs.readFileSync(p))]));
const source={};for(const file of ['ReferenceTypes.sol','V4Reference.sol','InfinityReference.sol'])source[file]={content:fs.readFileSync(path.join(outputRef,file),'utf8')};
const compiled=JSON.parse(solc.compile(JSON.stringify({language:'Solidity',sources:source,settings:{optimizer:{enabled:true,runs:200},viaIR:true,evmVersion:'shanghai',outputSelection:{'*':{'*':['abi','evm.bytecode.object']}}}}),{import:name=>{const e=meta.files.find(e=>e.local===name);return e?{contents:fs.readFileSync(path.join(runtime,'sources',name),'utf8')}:{error:'unlisted '+name};}}));assert(!(compiled.errors||[]).some(e=>e.severity==='error'),JSON.stringify(compiled.errors));
const engine=ganache.provider({logging:{quiet:true},chain:{chainId:31337,hardfork:'shanghai'},wallet:{deterministic:true}}),provider=new ethers.providers.Web3Provider(engine);
async function main(){
 const corpus=JSON.parse(fs.readFileSync(path.join(root,'integration/test/contracts/singleton-quotes.json'))),cases=[],sequences=[],ordered=[],grids=[];
 for(const [protocol,name]of[['uniswap-v4','V4Reference'],['pancake-infinity-cl','InfinityReference']]){
  const art=compiled.contracts[name+'.sol'][name],c=await new ethers.ContractFactory(art.abi,art.evm.bytecode.object,provider.getSigner()).deploy();await c.deployed();
  const unique=new Map();for(const row of corpus.cases)if(row.pool.protocol===protocol)unique.set(JSON.stringify(row.pool),row.pool);
  function seed(p){const s=p.singleton_cl,t=s.core;return [t.sqrt_price_x96,t.tick,t.liquidity,t.tick_spacing,s.lp_fee,s.protocol_fee0+(s.protocol_fee1<<12),t.tick_words.map(w=>[w.word_pos,w.bitmap]),t.ticks.map(t=>[t.tick,t.liquidity_gross,t.liquidity_net])];}
  async function limit(p,zero){const t=p.singleton_cl.core,k=zero?Math.max(-887271,t.tick_words[0].word_pos*256*t.tick_spacing):Math.min(887271,(t.tick_words.at(-1).word_pos+1)*256*t.tick_spacing-t.tick_spacing);return c.priceAtTick(k);}
  function result(q){return {input:q.amountIn.toString(),output:q.amountOut.toString(),remaining:q.remainingOut.toString(),sqrt:q.sqrtPriceX96.toString(),tick:q.tick,liquidity:q.liquidity.toString(),protocolAmount:q.protocolAmount.toString(),swapFee:q.swapFee};}
  async function one(p,target,zero){let native;try{native=result(await c.callStatic.quote(seed(p),target,zero,await limit(p,zero),{gasLimit:30000000}));}catch(e){let data=e.data||e.error?.data;if(data==='0x'&&e.error?.data)data=e.error.data;if(typeof data==='object')data=data.result;assert(typeof data==='string'&&data.startsWith('0x'),String(e));native={reverted:true,data};}
   const passed=!native.reverted&&native.remaining==='0'&&BigInt(target)<(1n<<127n)&&BigInt(native.input)<=(1n<<127n);
   const after=JSON.parse(JSON.stringify(p));if(!native.reverted)Object.assign(after.singleton_cl.core,{sqrt_price_x96:native.sqrt,tick:native.tick,liquidity:native.liquidity});
   return {pool:p,target,zeroForOne:zero,passed,native,...(passed?{requiredInput:native.input,after}:{})};
  }
  for(const p of unique.values())for(const zero of[true,false])for(const target of['1','2','1000','1000000','20000000','1000000000000000000',String(1n<<127n)])cases.push(await one(p,target,zero));
  const base=[...unique.values()].find(p=>p.singleton_cl.core.tick===0&&p.singleton_cl.core.ticks.length===4&&p.singleton_cl.lp_fee===3000);
  for(const first of[true,false]){
   const targets=['1000','1000000','20000000'],zeros=[first,!first,first],limits=await Promise.all(zeros.map(z=>limit(base,z)));
   const native=await c.callStatic.sequence(seed(base),targets,zeros,limits,{gasLimit:30000000});let current=base;const steps=[];
   for(let i=0;i<3;i++){const row=await one(current,targets[i],zeros[i]);assert(row.passed);assert.deepEqual(row.native,result(native[i]));steps.push(row);current=row.after;}sequences.push({steps});
  }
  for(const zero of[true,false]){
   const targets=['1000','1000000','20000000'],zeros=targets.map(()=>zero),limits=await Promise.all(zeros.map(z=>limit(base,z))),native=await c.callStatic.sequence(seed(base),targets,zeros,limits,{gasLimit:30000000});
   let current=base,total=0n,required=0n;const steps=[];
   for(let i=0;i<targets.length;i++){const row=await one(current,targets[i],zero);assert(row.passed);assert.deepEqual(row.native,result(native[i]));steps.push(row);current=row.after;total+=BigInt(row.target);required+=BigInt(row.requiredInput);}
   ordered.push({pool:base,zeroForOne:zero,target:String(total),requiredInput:String(required),steps});
  }
  const low=[...unique.values()].find(p=>p.singleton_cl.lp_fee===100&&p.singleton_cl.core.tick===0&&p.singleton_cl.core.ticks.length===4);assert(low);assert.notEqual(base.pool_id,low.pool_id);
  for(const zero of[true,false]){
   const target=1000000n,rows=[];
   for(let part=0;part<=4;part++){
    const first=target*BigInt(part)/4n;let required=0n,passed=true;const proposals=[];
    for(const [pool,amount]of[[base,first],[low,target-first]])if(amount>0n){const row=await one(pool,String(amount),zero);proposals.push(row);if(row.passed)required+=BigInt(row.requiredInput);else passed=false;}
    rows.push({passed,requiredInput:passed?String(required):null,proposals});
   }
   const feasible=rows.filter(r=>r.passed);assert(feasible.length>0&&feasible.length<rows.length);const best=feasible.reduce((a,b)=>BigInt(a.requiredInput)<BigInt(b.requiredInput)?a:b);grids.push({pools:[base,low],zeroForOne:zero,target:String(target),bestRequiredInput:best.requiredInput,rows});
  }

 }
 for(const item of meta.files)assert.equal(sha(fs.readFileSync(path.join(runtime,'sources',item.local))),item.sha256);
 for(const p of inputPaths)assert.equal(sha(fs.readFileSync(p)),inputs[path.relative(root,p)]);
 const data={schema_version:1,reference:{inputs,source_manifest_sha256:pins.original_source_manifest_sha256,baseline_templates_sha256:pins.baseline_templates_sha256,scope:'Original positive amountSpecified Pool/CLPool core with explicit seeded storage; no Manager/Vault/ERC20 settlement proof.'},cases,sequences,ordered,grids};
 const text=JSON.stringify(data,null,2)+'\n';
 for(const service of ['integration','dex-router'])fs.writeFileSync(path.join(root,service,'test/contracts/singleton-exact-out.json'),text);
 const report={passed:true,inputs,corpus_sha256:sha(text),cases:cases.length,success:cases.filter(x=>x.passed).length,rejected:cases.filter(x=>!x.passed).length,sequences:sequences.length,sequence_steps:sequences.reduce((n,s)=>n+s.steps.length,0),ordered:ordered.length,ordered_steps:ordered.reduce((n,s)=>n+s.steps.length,0),grids:grids.length,loaded_dependencies:Object.keys(require.cache).filter(p=>fs.existsSync(p)).sort().map(p=>({path:path.relative(root,p),sha256:sha(fs.readFileSync(p))}))};
 fs.writeFileSync(path.join(root,'integration/artifacts/local/singleton-exact-out-generation.json'),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify({...report,loaded_dependencies:report.loaded_dependencies.length}));
}
main().catch(e=>{console.error(e);process.exitCode=1;}).finally(()=>engine.disconnect());
