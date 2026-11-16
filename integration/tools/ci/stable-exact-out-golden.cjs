// Original pinned StableMath GIVEN_OUT solver + fee/scale wrapper; no service imports.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../../..'),runtime=path.join(root,'integration/.runtime/stable-reference');
const {ethers}=require(path.join(runtime,'node_modules/ethers')),ganache=require(path.join(runtime,'node_modules/ganache')),solc=require(path.join(runtime,'node_modules/solc07'));
const sha=b=>crypto.createHash('sha256').update(b).digest('hex'),read=p=>fs.readFileSync(path.join(root,p));
const stableManifest='integration/tools/reference/stable-models/source-metadata.json',weightedManifest='integration/tools/reference/weighted-model/source-metadata.json',harnessPath='integration/tools/reference/stable-exact-out/Harness.sol';
const inputPaths=[path.relative(root,__filename),stableManifest,weightedManifest,harnessPath,'integration/tools/reference/stable-models/package-lock.json'];
const inputs=Object.fromEntries(inputPaths.map(p=>[p,sha(read(p))])),sources={};
function verifySources(){
 for(const f of JSON.parse(read(weightedManifest)).files){const b=read('integration/.runtime/weighted-reference/sources/'+f.path);assert.equal(sha(b),f.sha256,f.path);sources[f.path]={content:b.toString()};}
 for(const f of JSON.parse(read(stableManifest)).files.filter(f=>f.local.startsWith('balancer'))){const b=read('integration/.runtime/stable-reference/sources/'+f.local);assert.equal(sha(b),f.sha256,f.local);const name=f.local==='balancer/StableMath.sol'?'StableMath.sol':f.local.replace('balancer-imports/','');sources[name]={content:b.toString()};}
}
verifySources();sources['Harness.sol']={content:read(harnessPath).toString()};assert(solc.version().startsWith('0.7.6+commit.7338295f'));
const compiled=JSON.parse(solc.compile(JSON.stringify({language:'Solidity',sources,settings:{optimizer:{enabled:true,runs:200},outputSelection:{'*':{'*':['abi','evm.bytecode.object']}}}}),{import:p=>{const k=p.replace(/^@balancer-labs\/v2-/,'pkg/');return sources[k]?{contents:sources[k].content}:{error:'Unpinned import '+p};}}));
assert(!(compiled.errors||[]).some(e=>e.severity==='error'),JSON.stringify(compiled.errors));
const engine=ganache.provider({logging:{quiet:true},chain:{chainId:31337,hardfork:'shanghai',time:new Date('2026-11-15T00:00:00Z')},wallet:{deterministic:true}}),provider=new ethers.providers.Web3Provider(engine);
const E=10n**18n,MAX=(1n<<112n)-1n,addr=n=>'0x'+n.toString(16).padStart(40,'0');
function model(b,scales,amp,fee,id=100){return {chain_key:'evm:31337',pool_id:addr(id),protocol:'balancer-v2-stable2',token0:addr(1),token1:addr(2),reserve0:String(b[0]),reserve1:String(b[1]),fee_numerator:Number(fee),fee_denominator:Number(E),adapter_version:'balancer-v2-stable2/stable-1',token_behavior:'standard',stable:{code_sha256:'aa'.repeat(32),vault:addr(500),vault_code_sha256:'bb'.repeat(32),pool_id:addr(id)+'0000'+'0'.repeat(19)+'1',rate0:String(scales[0]),rate1:String(scales[1]),amplification:String(amp),admin_fee:0,max_iterations:255}};}
async function main(){
 const a=compiled.contracts['Harness.sol'].StableOutputRef,c=await new ethers.ContractFactory(a.abi,a.evm.bytecode.object,provider.getSigner()).deploy();await c.deployed();
 const cases=[],sequences=[],grids=[],ordered=[];
 async function run(b,scales,amp,fee,target,i,j){assert(i!==j);try{const q=await c.quote(b.map(String),String(amp),scales.map(String),String(fee),String(target),i===0);return {amount_in:q.input.toString(),next_balances:q.next.map(String)};}catch(e){for(const d of [e.data,e.error?.data,e.error?.error?.data].map(v=>typeof v==='string'?v:v?.result)){if(typeof d==='string'&&d.startsWith('0x08c379a0')){const reason=ethers.utils.defaultAbiCoder.decode(['string'],'0x'+d.slice(10))[0];if(/^BAL#\d{3}$/.test(reason))return {reverted:true,reason};}}throw e;}}
 for(const ds of [[18,18],[18,6],[6,18],[0,18],[18,0]])for(const amp of [1000n,35791n,5000000n])for(const fee of [10n**12n,3000000000000001n,10n**17n]){
  const scales=ds.map(d=>10n**BigInt(36-d)),initial=ds.map((d,i)=>BigInt(i+1)*1000000n*10n**BigInt(d));
  for(const [i,j]of [[0,1],[1,0]])for(const target of [1n,10n**BigInt(ds[j]),initial[j]/100n,initial[j]*4n/10n,initial[j]-1n,initial[j],initial[j]+1n])cases.push({pool:model(initial,scales,amp,fee),i,j,target:String(target),expected:await run(initial,scales,amp,fee,target,i,j)});
  for(const i of [0,1]){const b=[...initial];b[i]=MAX;const j=1-i;cases.push({pool:model(b,scales,amp,fee),i,j,target:'1000',expected:await run(b,scales,amp,fee,1000n,i,j)});}
  let current=[...initial];const steps=[];
  for(let k=0;k<4;k++){const i=k%2,j=1-i,target=100n*10n**BigInt(ds[j]),expected=await run(current,scales,amp,fee,target,i,j);assert(!expected.reverted);steps.push({pool:model(current,scales,amp,fee),i,j,target:String(target),expected});current=expected.next_balances.map(BigInt);}sequences.push({steps});
  if(ds[0]===18&&ds[1]===6&&fee===3000000000000001n){
   const other=[initial[0]*3n,initial[1]*2n],target=initial[1]/20n,rows=[];
   for(let part=0;part<=4;part++){const first=target*BigInt(part)/4n;let required=0n;const proposals=[];
    for(const[b,out,id]of[[initial,first,100],[other,target-first,101]])if(out>0n){const expected=await run(b,scales,amp,fee,out,0,1);assert(!expected.reverted);required+=BigInt(expected.amount_in);proposals.push({pool:model(b,scales,amp,fee,id),i:0,j:1,target:String(out),expected});}rows.push({requiredInput:String(required),proposals});}
   const best=rows.reduce((a,b)=>BigInt(a.requiredInput)<BigInt(b.requiredInput)?a:b);grids.push({pools:[model(initial,scales,amp,fee),model(other,scales,amp,fee,101)],i:0,j:1,target:String(target),bestRequiredInput:best.requiredInput,rows});
   current=[...initial];let required=0n,total=0n;const same=[];
   for(let k=1;k<=3;k++){const output=initial[1]*BigInt(k)/10000n,expected=await run(current,scales,amp,fee,output,0,1);assert(!expected.reverted);same.push({pool:model(current,scales,amp,fee),i:0,j:1,target:String(output),expected});required+=BigInt(expected.amount_in);total+=output;current=expected.next_balances.map(BigInt);}ordered.push({pool:model(initial,scales,amp,fee),target:String(total),requiredInput:String(required),steps:same});
  }
 }
 verifySources();for(const p of inputPaths)assert.equal(sha(read(p)),inputs[p]);
 const data={schema_version:1,reference:{revision:'e281ff40583c39bf1d46c322334b2b13936e0e33',solc:solc.version(),inputs,scope:'Original StableMath _calcInGivenOut + fee/scale wrapper + original BalanceAllocation; no StablePool/Vault deployment or ERC20 execution proof.'},cases,sequences,grids,ordered};
 const text=JSON.stringify(data,null,2)+'\n';for(const service of ['integration','dex-router'])fs.writeFileSync(path.join(root,service,'test/contracts/stable-exact-out.json'),text);
 const report={passed:true,inputs,corpus_sha256:sha(text),cases:cases.length,success:cases.filter(c=>!c.expected.reverted).length,rejected:cases.filter(c=>c.expected.reverted).length,sequences:sequences.length,steps:sequences.reduce((s,x)=>s+x.steps.length,0),grids:grids.length,ordered_allocations:ordered.length,loaded_dependencies:Object.keys(require.cache).filter(p=>fs.existsSync(p)).sort().map(p=>({path:path.relative(root,p),sha256:sha(fs.readFileSync(p))}))};
 fs.writeFileSync(path.join(root,'integration/artifacts/local/stable-exact-out-generation.json'),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify({...report,loaded_dependencies:report.loaded_dependencies.length}));
}
main().catch(e=>{console.error(e);process.exitCode=1;}).finally(()=>engine.disconnect());
