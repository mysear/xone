// Original pinned WeightedPool GIVEN_OUT callbacks; never imports service math.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../../..'),runtime=path.join(root,'integration/.runtime/stable-reference');
const {ethers}=require(path.join(runtime,'node_modules/ethers')),ganache=require(path.join(runtime,'node_modules/ganache')),solc=require(path.join(runtime,'node_modules/solc07'));
const sha=b=>crypto.createHash('sha256').update(b).digest('hex'),read=p=>fs.readFileSync(path.join(root,p));
const manifestPath='integration/tools/reference/weighted-model/source-metadata.json',manifest=JSON.parse(read(manifestPath));
const harnessPath='integration/tools/reference/weighted-exact-out/Harness.sol';
const inputPaths=[path.relative(root,__filename),harnessPath,manifestPath,'integration/tools/reference/stable-models/package-lock.json'];
const inputs=Object.fromEntries(inputPaths.map(p=>[p,sha(read(p))]));
assert(solc.version().startsWith('0.7.6+commit.7338295f'));
const sources={};
function verifySources(){for(const f of manifest.files){const b=read('integration/.runtime/weighted-reference/sources/'+f.path);assert.equal(sha(b),f.sha256,f.path);sources[f.path]={content:b.toString()};}}
verifySources();sources['Harness.sol']={content:read(harnessPath).toString()};
const compiled=JSON.parse(solc.compile(JSON.stringify({language:'Solidity',sources,settings:{optimizer:{enabled:true,runs:200},outputSelection:{'*':{'*':['abi','evm.bytecode.object']}}}}),{import:p=>{const k=p.replace(/^@balancer-labs\/v2-/,'pkg/');return sources[k]?{contents:sources[k].content}:{error:'Unpinned import '+p};}}));
assert(!(compiled.errors||[]).some(e=>e.severity==='error'),JSON.stringify(compiled.errors));
const engine=ganache.provider({logging:{quiet:true},chain:{chainId:31337,hardfork:'shanghai',time:new Date('2026-11-14T00:00:00Z')},wallet:{deterministic:true}}),provider=new ethers.providers.Web3Provider(engine);
const E=10n**18n,MAX=(1n<<112n)-1n,addr=n=>'0x'+n.toString(16).padStart(40,'0');
function model(b,scales,weights,fee,protocol,id=100){
 const p={chain_key:'evm:31337',pool_id:addr(id),protocol,token0:addr(1),token1:addr(2),reserve0:String(b[0]),reserve1:String(b[1]),fee_numerator:Number(fee),fee_denominator:Number(E),adapter_version:protocol==='balancer-v2-weighted-multi'?'balancer-v2-weighted-multi/static-1':protocol==='balancer-v2-weighted2-equal'?'balancer-v2-weighted2/equal-1':'balancer-v2-weighted2/static-1',token_behavior:'standard'};
 const common={code_sha256:'aa'.repeat(32),vault:addr(500),vault_code_sha256:'bb'.repeat(32),pool_id:addr(id)+(b.length===2?'0002':'0001')+'0'.repeat(19)+'1'};
 if(b.length===2)p.weighted={...common,scaling0:String(scales[0]),scaling1:String(scales[1]),weight0:String(weights[0]),weight1:String(weights[1]),token0_code_sha256:'cc'.repeat(32),token1_code_sha256:'dd'.repeat(32)};
 else p.multi_asset={...common,assets:b.map((balance,i)=>({token:addr(i+1),balance:String(balance),scaling:String(scales[i]),code_sha256:'cc'.repeat(32)})),weights:weights.map(String),amplification:'0',admin_fee:0};
 return p;
}
async function main(){
 const signer=provider.getSigner(),owner=await signer.getAddress();
 async function deploy(file,name,args=[]){const a=compiled.contracts[file][name],c=await new ethers.ContractFactory(a.abi,a.evm.bytecode.object,signer).deploy(...args);await c.deployed();return c;}
 const vault=await deploy('Harness.sol','OutputVaultRef'),cases=[],sequences=[],grids=[],ordered=[];
 async function run(c,b,t,target,i,j){try{const q=await vault.callStatic.quote(c.address,b.map(String),t,String(target),i,j);return {amount_in:q.input.toString(),next_balances:q.next.map(String)};}catch(e){for(const d of [e.data,e.error?.data,e.error?.error?.data].map(v=>typeof v==='string'?v:v?.result)){if(typeof d==='string'&&d.startsWith('0x08c379a0')){const reason=ethers.utils.defaultAbiCoder.decode(['string'],'0x'+d.slice(10))[0];if(/^BAL#\d{3}$/.test(reason))return {reverted:true,reason};}}throw e;}}
 for(const spec of [{n:2,w:50,mixed:false},{n:2,w:80,mixed:true},{n:2,w:1,mixed:false},{n:3,w:60,mixed:true},{n:4,w:25,mixed:false},{n:20,w:81,mixed:true}]){
  const pairs=[];for(let i=0;i<spec.n;i++){const d=spec.mixed?[18,6,0,12][i%4]:18,c=await deploy('Harness.sol','OutputTokenRef',[d]);pairs.push([c.address,d]);}pairs.sort((a,b)=>a[0].toLowerCase().localeCompare(b[0].toLowerCase()));
  const tokens=pairs.map(x=>x[0]),ds=pairs.map(x=>x[1]),scales=ds.map(d=>10n**BigInt(36-d)),weights=Array(spec.n).fill((E-E*BigInt(spec.w)/100n)/BigInt(spec.n-1));weights[0]=E-weights[1]*BigInt(spec.n-1);
  const protocol=spec.n>2?'balancer-v2-weighted-multi':spec.w===50?'balancer-v2-weighted2-equal':'balancer-v2-weighted2-static';
  for(const fee of [10n**12n,10n**17n]){
   const c=await deploy('pkg/pool-weighted/contracts/WeightedPool.sol','WeightedPool',[vault.address,'Output','OUT',tokens,weights.map(String),tokens.map(()=>ethers.constants.AddressZero),String(fee),0,0,owner]);
   const initial=ds.map((d,i)=>BigInt(i+1)*1000000n*10n**BigInt(d));
   const pairsToTest=spec.n<=4?tokens.flatMap((_,i)=>tokens.map((_,j)=>[i,j])).filter(([i,j])=>i!==j):[[0,19],[19,0],[1,2],[2,1]];
   for(const [i,j]of pairsToTest)for(const target of [1n,10n**BigInt(ds[j]),initial[j]/100n,initial[j]*3n/10n,initial[j]*3n/10n+1n])cases.push({pool:model(initial,scales,weights,fee,protocol),i,j,target:String(target),expected:await run(c,initial,tokens,target,i,j)});
   for(const i of [0,spec.n-1]){const b=[...initial];b[i]=MAX;const j=(i+1)%spec.n;cases.push({pool:model(b,scales,weights,fee,protocol),i,j,target:'1000',expected:await run(c,b,tokens,1000n,i,j)});}
   let b=[...initial];const steps=[];
   for(let k=0;k<4;k++){const i=k%spec.n,j=(i+1)%spec.n,target=100n*10n**BigInt(ds[j]),expected=await run(c,b,tokens,target,i,j);assert(!expected.reverted);steps.push({pool:model(b,scales,weights,fee,protocol),i,j,target:String(target),expected});b=expected.next_balances.map(BigInt);}sequences.push({steps});
   if(spec.n===3){
    const other=initial.map(x=>x*2n),target=initial[2]/20n,rows=[];
    for(let part=0;part<=4;part++){const a=target*BigInt(part)/4n,z=target-a;let input=0n;const proposals=[];for(const [b,out,id]of [[initial,a,100],[other,z,101]])if(out>0n){const expected=await run(c,b,tokens,out,1,2);assert(!expected.reverted);input+=BigInt(expected.amount_in);proposals.push({pool:model(b,scales,weights,fee,protocol,id),i:1,j:2,target:String(out),expected});}rows.push({requiredInput:String(input),proposals});}
    const best=rows.reduce((a,b)=>BigInt(a.requiredInput)<BigInt(b.requiredInput)?a:b);grids.push({pools:[model(initial,scales,weights,fee,protocol),model(other,scales,weights,fee,protocol,101)],i:1,j:2,target:String(target),bestRequiredInput:best.requiredInput,rows});
    let current=[...initial],required=0n,total=0n;const steps=[];
    for(let k=1;k<=3;k++){const output=initial[2]*BigInt(k)/10000n,expected=await run(c,current,tokens,output,1,2);assert(!expected.reverted);steps.push({pool:model(current,scales,weights,fee,protocol),i:1,j:2,target:String(output),expected});required+=BigInt(expected.amount_in);total+=output;current=expected.next_balances.map(BigInt);}
    ordered.push({pool:model(initial,scales,weights,fee,protocol),target:String(total),requiredInput:String(required),steps});
   }
  }
 }
 verifySources();for(const p of inputPaths)assert.equal(sha(read(p)),inputs[p]);
 const data={schema_version:1,reference:{revision:manifest.revision,solc:solc.version(),inputs,scope:'Original WeightedPool GIVEN_OUT callback + original BalanceAllocation. Facade: no ERC20 transfers or full Vault.'},cases,sequences,grids,ordered};
 const text=JSON.stringify(data,null,2)+'\n';for(const service of ['integration','dex-router'])fs.writeFileSync(path.join(root,service,'test/contracts/weighted-exact-out.json'),text);
 const report={passed:true,inputs,corpus_sha256:sha(text),cases:cases.length,success:cases.filter(c=>!c.expected.reverted).length,rejected:cases.filter(c=>c.expected.reverted).length,sequences:sequences.length,steps:sequences.reduce((s,x)=>s+x.steps.length,0),grids:grids.length,ordered_allocations:ordered.length,loaded_dependencies:Object.keys(require.cache).filter(p=>fs.existsSync(p)).sort().map(p=>({path:path.relative(root,p),sha256:sha(fs.readFileSync(p))}))};
 fs.writeFileSync(path.join(root,'integration/artifacts/local/weighted-exact-out-generation.json'),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify(report));
}
main().catch(e=>{console.error(e);process.exitCode=1;}).finally(()=>engine.disconnect());
