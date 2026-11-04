// Original Solidity callbacks. No Rust/math transcription is an oracle input.
'use strict';
const fs=require('fs'),path=require('path'),crypto=require('crypto');
const root=path.resolve(__dirname,'../../..'),ref=path.join(root,'integration/tools/reference/balancer-multi');
const receiptPath=path.join(root,'integration/artifacts/local/balancer-multi-weighted-generation.json');
if(require.main===module)fs.writeFileSync(receiptPath,JSON.stringify({passed:false,state:'RUNNING'})+'\n');
const runtime=path.join(root,'integration/.runtime/stable-reference');
const {ethers}=require(path.join(runtime,'node_modules/ethers'));
const ganache=require(path.join(runtime,'node_modules/ganache')),solc=require(path.join(runtime,'node_modules/solc07'));
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const read=p=>fs.readFileSync(path.join(root,p));
const baseline=JSON.parse(read('integration/tools/reference/weighted-model/source-metadata.json'));
const revision=baseline.revision,E=10n**18n,MAX=(1n<<112n)-1n;
function verifyManifest(manifest,base){const sources={};for(const f of manifest.files){const b=fs.readFileSync(path.join(base,f.path));if(sha(b)!==f.sha256)throw Error('source hash '+f.path);sources[f.path]={content:b.toString()};}return sources;}
function compile(sources,extras){for(const f of extras)sources[path.basename(f)]={content:fs.readFileSync(path.join(ref,f),'utf8')};
 const output=JSON.parse(solc.compile(JSON.stringify({language:'Solidity',sources,settings:{optimizer:{enabled:true,runs:200},outputSelection:{'*':{'*':['abi','evm.bytecode.object']}}}}),{import:p=>{const k=p.startsWith('@balancer-labs/v2-')?'pkg/'+p.slice('@balancer-labs/v2-'.length):p;return sources[k]?{contents:sources[k].content}:{error:'Unpinned import '+p};}}));
 if((output.errors||[]).some(e=>e.severity==='error'))throw Error(output.errors.map(e=>e.formattedMessage).join('\n'));return output.contracts;}
function runtimeDigest(){const h=crypto.createHash('sha256'),base=path.join(runtime,'node_modules');function walk(d){for(const n of fs.readdirSync(d).sort()){const p=path.join(d,n),st=fs.lstatSync(p);if(st.isDirectory())walk(p);else if(st.isFile()&&/\.(js|cjs|json|wasm|node)$/.test(n)){h.update(path.relative(base,p));h.update(fs.readFileSync(p));}}}walk(base);return h.digest('hex');}
function legacyDigest(){const rows={};for(const svc of ['integration','dex-router'])for(const name of fs.readdirSync(path.join(root,svc,'test/contracts')).filter(n=>!n.startsWith('balancer-multi')&&!n.startsWith('balancer-composable')).sort()){const p=`${svc}/test/contracts/${name}`;if(fs.statSync(path.join(root,p)).isFile())rows[p]=sha(read(p));}return rows;}
const provider=new ethers.providers.Web3Provider(ganache.provider({logging:{quiet:true},chain:{hardfork:'shanghai',allowUnlimitedContractSize:true},miner:{blockGasLimit:60000000},wallet:{deterministic:true}}));
async function revertResult(call){try{return await call();}catch(e){const data=[e.data,e.error?.data,e.error?.error?.data].map(v=>typeof v==='string'?v:v?.result);for(const d of data)if(typeof d==='string'&&d.startsWith('0x08c379a0')){const reason=ethers.utils.defaultAbiCoder.decode(['string'],'0x'+d.slice(10))[0];if(/^BAL#[0-9]{3}$/.test(reason))return {reverted:true,reason};}throw e;}}
async function main(){
 const started=Date.now(),legacy=legacyDigest(),runtimeSha=runtimeDigest();
 const inputs=['integration/tools/ci/balancer-multi-golden.cjs','integration/tools/reference/balancer-multi/WeightedHarness.sol','integration/tools/reference/weighted-model/source-metadata.json','integration/tools/reference/stable-models/package.json','integration/tools/reference/stable-models/package-lock.json'];
 const oracleInputs=Object.fromEntries(inputs.map(p=>[p,sha(read(p))]));
 const sources=verifyManifest(baseline,path.join(root,'integration/.runtime/weighted-reference/sources'));
 const contracts=compile(sources,['WeightedHarness.sol']);
 const signer=provider.getSigner(0),owner=await signer.getAddress();
 async function deploy(file,name,args=[]){const c=contracts[file][name];const x=await new ethers.ContractFactory(c.abi,c.evm.bytecode.object,signer).deploy(...args,{gasLimit:55000000});await x.deployed();return x;}
 const vault=await deploy('WeightedHarness.sol','MultiVaultRef'),cases=[],sequences=[];
 let pools=0;
 async function run(c,b,t,input,i,j){return revertResult(async()=>{const q=await vault.callStatic.quote(c.address,await c.getPoolId(),b.map(String),t,input.toString(),i,j,{gasLimit:25000000});return {amount_out:q.out.toString(),next_balances:q.next.map(String)};});}
 for(const count of [3,4,8,20])for(const mixed of [false,true]){
  const decimals=Array.from({length:count},(_,i)=>mixed?[18,6,0,12][i%4]:18);
  // Sort token addresses before construction, keeping decimals attached to identity.
  const pairs=[];for(const d of decimals){const t=await deploy('WeightedHarness.sol','MultiTokenRef',[d]);pairs.push([t.address,d]);}pairs.sort((a,b)=>a[0].toLowerCase().localeCompare(b[0].toLowerCase()));
  const tokens=pairs.map(x=>x[0]),ds=pairs.map(x=>x[1]),scales=ds.map(d=>10n**BigInt(36-d));
  const weights=Array.from({length:count},()=>E/BigInt(count));weights[count-1]=E-weights[0]*BigInt(count-1);
  // A strongly asymmetric profile exercises nontrivial log/exp for every N.
  if(mixed){for(let k=1;k<count;k++)weights[k]=E/100n;weights[0]=E-BigInt(count-1)*E/100n;}
  const fee=mixed?10n**17n:10n**12n;
  const c=await deploy('pkg/pool-weighted/contracts/WeightedPool.sol','WeightedPool',[vault.address,'Multi','MULTI',tokens,weights.map(String),tokens.map(()=>ethers.constants.AddressZero),fee.toString(),0,0,owner]);pools++;
  if((await c.getNormalizedWeights()).map(String).join()!==weights.map(String).join())throw Error('weight snapshot');
  const initial=ds.map((d,i)=>BigInt(i+1)*1000000n*10n**BigInt(d));
  const state=b=>({balances:b.map(String),weights:weights.map(String),scales:scales.map(String),fee:fee.toString()});
  const pairsToQuote=count<=4?tokens.flatMap((_,i)=>tokens.map((_,j)=>[i,j])).filter(([i,j])=>i!==j):[[0,count-1],[count-1,0],[1,2],[2,1],[count-2,count-1]];
  for(const [i,j] of pairsToQuote){const target=initial[i]*3n/10n,edge=(target*E+E-fee-1n)/(E-fee);
   for(const input of [0n,1n,2n,3n,10n**BigInt(ds[i]),initial[i]/100n,edge-1n,edge,edge+1n,initial[i],(1n<<256n)-1n])
    cases.push({...state(initial),input:input.toString(),index_in:i,index_out:j,expected:await run(c,initial,tokens,input,i,j)});
  }
  for(const i of [0,count-1])for(const b0 of [MAX,MAX-1000n]){const b=[...initial];b[i]=b0;const j=(i+1)%count;cases.push({...state(b),input:'1000',index_in:i,index_out:j,expected:await run(c,b,tokens,1000n,i,j)});}
  let b=[...initial];const steps=[];
  for(let k=0;k<count+2;k++){const i=k%count,j=(k+1)%count,input=1000n*10n**BigInt(ds[i]);const expected=await run(c,b,tokens,input,i,j);if(expected.reverted||expected.amount_out==='0')throw Error('sequence failed');steps.push({input:input.toString(),index_in:i,index_out:j,expected});b=expected.next_balances.map(BigInt);}
  sequences.push({...state(initial),steps});
 }
 const text=JSON.stringify({schema_version:1,reference:{revision,source_files:baseline.files.length,flow:'Original WeightedPool constructor/onSwap and BalanceAllocation, test Vault facade',solc:solc.version()},cases,sequences},null,2)+'\n';
 verifyManifest(baseline,path.join(root,'integration/.runtime/weighted-reference/sources'));
 if(JSON.stringify(legacy)!==JSON.stringify(legacyDigest()))throw Error('legacy fixtures changed');
 if(runtimeSha!==runtimeDigest()||inputs.some(p=>oracleInputs[p]!==sha(read(p))))throw Error('oracle/runtime inputs changed');
 const receipt={passed:true,started_at_unix_ms:started,finished_at_unix_ms:Date.now(),revision,baseline_source_hash:'a21f6566d37138802aab731388c6da6e75c0405b490564de58abc084352ec5a5',baseline_source_hash_authority:'user-supplied identifier; actual source file hashes verified from source-metadata.json',oracle_inputs:oracleInputs,runtime_sha256:runtimeSha,legacy_fixture_hashes:legacy,corpus_sha256:sha(text),cases:cases.length,sequences:sequences.length,steps:sequences.reduce((n,x)=>n+x.steps.length,0),pools};
 for(const svc of ['integration','dex-router'])fs.writeFileSync(path.join(root,svc,'test/contracts/balancer-multi-quotes.json'),text);
 fs.writeFileSync(receiptPath,JSON.stringify(receipt,null,2)+'\n');
 console.log(JSON.stringify({passed:true,cases:receipt.cases,sequences:receipt.sequences,steps:receipt.steps,pools,corpus_sha256:receipt.corpus_sha256}));
}
module.exports={root,ref,runtime,ethers,solc,sha,read,verifyManifest,compile,runtimeDigest,legacyDigest,revertResult,provider,E,MAX};
if(require.main===module)main().catch(e=>{console.error(e);process.exitCode=1;}).finally(async()=>{await provider.ready;await provider.provider.disconnect();});
