// Executed original WeightedPool + packed BalanceAllocation, independently of service code.
const fs=require('fs'),path=require('path'),crypto=require('crypto');
const root=path.resolve(__dirname,'../../..'),runtime=path.join(root,'integration/.runtime/stable-reference');
fs.writeFileSync(path.join(root,'integration/artifacts/local/weighted-static-golden-generation.json'),JSON.stringify({passed:false,state:'RUNNING'})+'\n');
const {ethers}=require(path.join(runtime,'node_modules/ethers')),ganache=require(path.join(runtime,'node_modules/ganache')),solc=require(path.join(runtime,'node_modules/solc07'));
const manifest=JSON.parse(fs.readFileSync(path.join(root,'integration/tools/reference/weighted-model/source-metadata.json')));
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const oracleFiles=['integration/tools/ci/weighted-static-golden.cjs','integration/tools/reference/weighted-model/Harness.sol','integration/tools/reference/weighted-model/StaticMathRef.sol','integration/tools/reference/weighted-model/source-metadata.json','integration/tools/reference/stable-models/package.json','integration/tools/reference/stable-models/package-lock.json'];
const oracleInputs=Object.fromEntries(oracleFiles.map(p=>[p,sha(fs.readFileSync(path.join(root,p)))]));
function runtimeDigest(){const h=crypto.createHash('sha256'),base=path.join(runtime,'node_modules');function walk(dir){for(const name of fs.readdirSync(dir).sort()){const p=path.join(dir,name),st=fs.lstatSync(p);if(st.isDirectory())walk(p);else if(st.isFile()&&/\.(js|cjs|json|wasm|node)$/.test(name)){h.update(path.relative(base,p));h.update(fs.readFileSync(p));}}}walk(base);return h.digest('hex');}
const runtimeBefore=runtimeDigest(),started=Date.now();
const sources={};
for(const f of manifest.files){const b=fs.readFileSync(path.join(root,'integration/.runtime/weighted-reference/sources',f.path));if(sha(b)!==f.sha256)throw Error('Source hash mismatch '+f.path);sources[f.path]={content:b.toString()};}
const frozen=['integration/test/contracts/weighted-quotes.json','integration/test/contracts/weighted-frames.json','integration/test/contracts/weighted-state.json'];const legacyInputs=Object.fromEntries(frozen.map(p=>[p,sha(fs.readFileSync(path.join(root,p)))]));
const harness=fs.readFileSync(path.join(root,'integration/tools/reference/weighted-model/Harness.sol'),'utf8');sources['Harness.sol']={content:harness};sources['StaticMathRef.sol']={content:fs.readFileSync(path.join(root,'integration/tools/reference/weighted-model/StaticMathRef.sol'),'utf8')};
const result=JSON.parse(solc.compile(JSON.stringify({language:'Solidity',sources,settings:{optimizer:{enabled:true,runs:200},outputSelection:{'*':{'*':['abi','evm.bytecode.object']}}}}),{import:p=>{let key=p.startsWith('@balancer-labs/v2-')?'pkg/'+p.slice('@balancer-labs/v2-'.length):p;return sources[key]?{contents:sources[key].content}:{error:'Unpinned import '+p};}}));
if((result.errors||[]).some(e=>e.severity==='error'))throw Error(result.errors.map(e=>e.formattedMessage).join('\n'));
const provider=new ethers.providers.Web3Provider(ganache.provider({logging:{quiet:true},chain:{hardfork:'shanghai'},wallet:{deterministic:true}}));
const E=10n**18n,addr=n=>'0x'+n.toString(16).padStart(40,'0');
function pool(b,scales,fee,weights){return {chain_key:'evm:31337',pool_id:addr(3),protocol:'balancer-v2-weighted2-static',token0:addr(1),token1:addr(2),reserve0:b[0].toString(),reserve1:b[1].toString(),fee_numerator:Number(fee),fee_denominator:Number(E),adapter_version:'balancer-v2-weighted2/static-1',token_behavior:'standard',weighted:{code_sha256:'aa'.repeat(32),vault:addr(5),vault_code_sha256:'bb'.repeat(32),pool_id:addr(3)+'0002'+'0'.repeat(19)+'1',scaling0:scales[0].toString(),scaling1:scales[1].toString(),weight0:weights[0].toString(),weight1:weights[1].toString(),token0_code_sha256:'cc'.repeat(32),token1_code_sha256:'dd'.repeat(32)}};}
async function main(){
 const signer=provider.getSigner(0), owner=await signer.getAddress();
 async function deploy(file,name,args=[]){const a=result.contracts[file][name],f=new ethers.ContractFactory(a.abi,a.evm.bytecode.object,signer),c=await f.deploy(...args);await c.deployed();return c;}
 const vault=await deploy('Harness.sol','WeightedVaultRef');const tokens=[await deploy('Harness.sol','RefToken'),await deploy('Harness.sol','RefToken')].sort((a,b)=>a.address.toLowerCase().localeCompare(b.address.toLowerCase()));
 const tokenAddresses=tokens.map(t=>t.address),cases=[],sequences=[];let deployed=0;
 async function run(c,b,input,zero){try{const q=await vault.callStatic.quote(c.address,b.map(String),tokenAddresses,input.toString(),zero);return {amount_out:q.out.toString(),next_balances:q.next.map(String)};}catch(e){const values=[e.data,e.error?.data,e.error?.error?.data].map(v=>typeof v==='string'?v:v?.result).filter(v=>typeof v==='string'&&v.startsWith('0x08c379a0'));const reasons=values.map(v=>{try{return ethers.utils.defaultAbiCoder.decode(['string'],'0x'+v.slice(10))[0];}catch(_){return '';}});if(e.code!=='CALL_EXCEPTION'||!reasons.some(v=>/^BAL#[0-9]{3}$/.test(v)))throw e;return {reverted:true};}}
 for(const weight0 of [10n**16n,10n**16n+1n,333333333333333333n,370000000000000000n,E/2n,800000000000000000n,990000000000000000n])for(const decimals of [[18,18],[18,6],[6,18],[18,0],[0,18]])for(const fee of [10n**12n,10n**15n,10n**17n]){
  const weights=[weight0,E-weight0];
  for(let i=0;i<2;i++)await (await tokens[i].setDecimals(decimals[i])).wait();
  const c=await deploy('pkg/pool-weighted/contracts/WeightedPool.sol','WeightedPool',[vault.address,'Reference','REF',tokenAddresses,weights.map(String),[ethers.constants.AddressZero,ethers.constants.AddressZero],String(fee),0,0,owner]);deployed++;
  const scales=decimals.map(d=>10n**BigInt(36-d));
  for(const imbalance of [1n,7n])for(const zero of [true,false]){
   const b=decimals.map((d,i)=>1000000n*10n**BigInt(d)*(i?1n:imbalance)),i=zero?0:1;
   const target=b[i]*3n/10n,edge=(target*E+(E-fee)-1n)/(E-fee);
   const inputs=[0n,1n,2n,3n,10n**BigInt(decimals[i]),b[i]/100n,edge-1n,edge,edge+1n,b[i],(1n<<256n)-1n];
   for(const input of inputs)cases.push({pool:pool(b,scales,fee,weights),input:input.toString(),zero_for_one:zero,expected:await run(c,b,input,zero)});
  }
  // Original full onSwap flow on the fixed-point rounding counterexample.
  if(decimals[0]===18 && decimals[1]===18){const b=[E,10n**24n];for(const input of [1n,2n,3n,4n,5n])cases.push({pool:pool(b,scales,fee,weights),input:input.toString(),zero_for_one:true,expected:await run(c,b,input,true)});}
  const max=(1n<<112n)-1n;for(const b of [[max,max],[max-10n,max],[0n,10n**24n]])for(const zero of [true,false])cases.push({pool:pool(b,scales,fee,weights),input:'100',zero_for_one:zero,expected:await run(c,b,100n,zero)});
  for(const pattern of [[true,true,false,false],[false,false,true,true],[true,false,true,false]]){
   let b=decimals.map(d=>1000000n*10n**BigInt(d));const initial=pool(b,scales,fee,weights),steps=[];
   for(let n=0;n<pattern.length;n++){const zero=pattern[n],input=BigInt(n+1)*1000n*10n**BigInt(decimals[zero?0:1]),expected=await run(c,b,input,zero);if(expected.reverted||expected.amount_out==='0')throw Error('Positive sequence failed');steps.push({input:input.toString(),zero_for_one:zero,expected});b=expected.next_balances.map(BigInt);}
   sequences.push({pool:initial,steps});
  }
 }
 const powers=[],math=await deploy('StaticMathRef.sol','StaticMathRef');
 const mild=(1n<<254n)/(10n**20n);
 async function powerCall(method,x,y){try{return {value:(await math[method](x.toString(),y.toString())).toString()};}catch(e){const candidates=[e.data,e.error?.data,e.error?.error?.data].map(v=>typeof v==='string'?v:v?.result);const reason=candidates.filter(v=>typeof v==='string'&&v.startsWith('0x08c379a0')).some(v=>{try{return /^BAL#[0-9]{3}$/.test(ethers.utils.defaultAbiCoder.decode(['string'],'0x'+v.slice(10))[0]);}catch(_){return false;}});if(e.code!=='CALL_EXCEPTION'||!reason)throw e;return {reverted:true};}}
 for(const x of [0n,1n,10n**10n,E/10n,7n*E/10n,9n*E/10n-1n,9n*E/10n,9n*E/10n+1n,E-1n,E,E+1n,11n*E/10n-1n,11n*E/10n,11n*E/10n+1n,13n*E/10n,2n*E,10n**36n,(1n<<255n)-1n,1n<<255n,(1n<<256n)-1n])for(const y of [0n,1n,E/100n,E/4n,E,2n*E,4n*E,99n*E,130n*E,1000n*E,mild-1n,mild])powers.push({x:x.toString(),y:y.toString(),raw:await powerCall('rawPow',x,y),up:await powerCall('powUp',x,y)});
 const powersText=JSON.stringify({schema_version:1,revision:manifest.revision,cases:powers},null,2)+'\n';for(const service of ['integration','dex-router'])fs.writeFileSync(path.join(root,service,'test/contracts/weighted-static-powers.json'),powersText);
 const data={schema_version:1,reference:{revision:manifest.revision,source_files:manifest.files.length,flow:'Original unmodified WeightedPool constructor and onSwap + original BalanceAllocation; Vault facade without ERC20 transfer/full Vault',solc:solc.version(),harness_sha256:sha(harness)},cases,sequences};
 const text=JSON.stringify(data,null,2)+'\n';for(const service of ['integration','dex-router'])fs.writeFileSync(path.join(root,service,'test/contracts/weighted-static-quotes.json'),text);
 if(frozen.some(p=>legacyInputs[p]!==sha(fs.readFileSync(path.join(root,p)))))throw Error('Legacy corpus changed');
 if(runtimeBefore!==runtimeDigest()||oracleFiles.some(p=>oracleInputs[p]!==sha(fs.readFileSync(path.join(root,p)))))throw Error('Oracle inputs/runtime changed during execution');
 const report={passed:true,oracle_inputs:oracleInputs,legacy_inputs:legacyInputs,power_cases:powers.length,powers_sha256:sha(powersText),runtime_sha256:runtimeBefore,started_at_unix_ms:started,measured_at_unix_ms:Date.now(),cases:cases.length,sequences:sequences.length,steps:sequences.reduce((n,s)=>n+s.steps.length,0),deployed_pools:deployed,corpus_sha256:sha(text),reference:data.reference};fs.writeFileSync(path.join(root,'integration/artifacts/local/weighted-static-golden-generation.json'),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify(report));
}
main().catch(e=>{fs.writeFileSync(path.join(root,'integration/artifacts/local/weighted-static-golden-generation.json'),JSON.stringify({passed:false,error:String(e)})+'\n');console.error(e);process.exitCode=1;}).finally(()=>provider.provider.disconnect());
