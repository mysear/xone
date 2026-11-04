'use strict';
const fs=require('fs'),path=require('path');
const receiptPath=path.resolve(__dirname,'../../artifacts/local/balancer-multi-managed-generation.json');
fs.writeFileSync(receiptPath,JSON.stringify({passed:false,state:'RUNNING'})+'\n');
const {root,ref,ethers,solc,sha,read,verifyManifest,compile,runtimeDigest,legacyDigest,revertResult,provider,E,MAX}=require('./balancer-multi-golden.cjs');
const rev='e281ff40583c39bf1d46c322334b2b13936e0e33';
async function main(){
 const started=Date.now(),legacy=legacyDigest(),runtime=runtimeDigest();
 const inputs=['integration/tools/ci/balancer-multi-managed.cjs','integration/tools/ci/balancer-multi-golden.cjs',`integration/tools/reference/balancer-multi/${rev}.json`,'integration/tools/reference/balancer-multi/WeightedHarness.sol','integration/tools/reference/balancer-multi/ManagedHarness.sol'];
 const hashes=Object.fromEntries(inputs.map(p=>[p,sha(read(p))]));
 const manifest=JSON.parse(fs.readFileSync(path.join(ref,rev+'.json')));
 const contracts=compile(verifyManifest(manifest,path.join(ref,'sources',rev)),['WeightedHarness.sol','ManagedHarness.sol']);
 const signer=provider.getSigner(0),owner=await signer.getAddress();
 async function deploy(file,name,args=[]){const c=contracts[file][name];const x=await new ethers.ContractFactory(c.abi,c.evm.bytecode.object,signer).deploy(...args,{gasLimit:55000000});await x.deployed();return x;}
 const vault=await deploy('WeightedHarness.sol','MultiVaultRef'),decoder=await deploy('ManagedHarness.sol','ManagedWeightsRef');
 const cases=[],weightsCases=[],sequences=[],guards=[];
 for(const count of [3,8,20,38]){
  const pairs=[];for(let k=0;k<count;k++){const decimals=[18,6,0][k%3];const t=await deploy('WeightedHarness.sol','MultiTokenRef',[decimals]);pairs.push([t.address,decimals]);}pairs.sort((a,b)=>a[0].toLowerCase().localeCompare(b[0].toLowerCase()));
  const tokens=pairs.map(p=>p[0]),decimals=pairs.map(p=>p[1]),scales=decimals.map(d=>10n**BigInt(36-d));
  const start=Array(count).fill(E/BigInt(count));start[count-1]=E-start[0]*BigInt(count-1);
  const end=Array(count).fill(E/100n);end[0]=E-BigInt(count-1)*E/100n;
  const params=['Managed','MREF',tokens,start.map(String),tokens.map(()=>ethers.constants.AddressZero),(10n**15n).toString(),true,true,'0','0','0',vault.address];
  const pool=await deploy('pkg/pool-weighted/contracts/smart/ManagedPool.sol','ManagedPool',[params,vault.address,owner,100000,100000]);
  const block=await provider.getBlock('latest'),t0=block.timestamp+100,t1=t0+1000;
  await(await pool.updateWeightsGradually(t0,t1,end.map(String),{gasLimit:10000000})).wait();
  await(await pool.updateSwapFeeGradually(t0,t1,(10n**15n).toString(),(10n**17n).toString(),{gasLimit:10000000})).wait();
  const b=decimals.map((d,k)=>BigInt(k+1)*1000000n*10n**BigInt(d));
  async function run(b,input,i,j){return revertResult(async()=>{const q=await vault.callStatic.quote(pool.address,await pool.getPoolId(),b.map(String),tokens,input.toString(),i,j,{gasLimit:25000000});return {amount_out:q.out.toString(),next_balances:q.next.map(String)};});}
  for(const target of [t0-1,t0,t0+1,t0+333,t0+999,t1,t1+1]){
   await provider.send('evm_setTime',[target*1000]);await provider.send('evm_mine',[]);
   const observed=await provider.getBlock('latest'),weights=(await pool.getNormalizedWeights()).map(String),fee=(await pool.getSwapFeePercentage()).toString();
   const state={balances:b.map(String),weights,scales:scales.map(String),fee,timestamp:observed.timestamp,block_number:observed.number};
   for(const [i,j] of [[0,count-1],[count-1,0],[1,2]])for(const input of [0n,1n,2n,10n**BigInt(decimals[i]),b[i]/100n,b[i],(1n<<256n)-1n])cases.push({...state,input:input.toString(),index_in:i,index_out:j,expected:await run(b,input,i,j)});
   const steps=[];let next=[...b];
   for(const [i,j] of [[0,1],[1,2],[2,0],[0,count-1]]){const input=1000n*10n**BigInt(decimals[i]),expected=await run(next,input,i,j);if(expected.reverted||expected.amount_out==='0')throw Error('managed sequence');steps.push({input:input.toString(),index_in:i,index_out:j,expected});next=expected.next_balances.map(BigInt);}
   sequences.push({...state,steps});
  }
  await(await pool.setSwapEnabled(false)).wait();const expected=await run(b,1000n*10n**BigInt(decimals[0]),0,1);if(!expected.reverted)throw Error('disabled swap accepted');guards.push({count,swaps_enabled:false,expected});
 }
 const starts=['461168601842738','461168601842738','922337203685477'],ends=[...starts].reverse();
 const now=(await provider.getBlock('latest')).timestamp;
 for(const [s,e] of [[now-100,now+100],[now+100,now+200],[now-200,now-100],[now,now]])for(const sum of [E,13n*E/10n]){const out=await decoder.weights(starts,ends,sum.toString(),s,e);weightsCases.push({start_compressed:starts,end_compressed:ends,denormalized_sum:sum.toString(),start_time:s,end_time:e,timestamp:now,expected:out.map(String)});}
 verifyManifest(manifest,path.join(ref,'sources',rev));
 if(JSON.stringify(legacy)!==JSON.stringify(legacyDigest())||runtime!==runtimeDigest()||inputs.some(p=>hashes[p]!==sha(read(p))))throw Error('inputs changed');
 const text=JSON.stringify({schema_version:1,reference:{revision:rev,source_files:manifest.files.length,solc:solc.version(),flow:'Unmodified original ManagedPool constructor/updateWeightsGradually/updateSwapFeeGradually/onSwap with zero management/protocol swap fees; LP allowlist enabled'},cases,sequences,weight_cases:weightsCases,guards},null,2)+'\n';
 for(const svc of ['integration','dex-router'])fs.writeFileSync(path.join(root,svc,'test/contracts/balancer-multi-managed.json'),text);
 const report={passed:true,started_at_unix_ms:started,finished_at_unix_ms:Date.now(),revision:rev,oracle_inputs:hashes,runtime_sha256:runtime,corpus_sha256:sha(text),cases:cases.length,sequences:sequences.length,steps:sequences.reduce((n,s)=>n+s.steps.length,0),weight_cases:weightsCases.length,guard_cases:guards.length,legacy_fixture_hashes:legacy};
 fs.writeFileSync(receiptPath,JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify({passed:true,cases:cases.length,sequences:sequences.length,weight_cases:weightsCases.length,corpus_sha256:sha(text)}));
}
main().catch(e=>{console.error(e);process.exitCode=1;}).finally(async()=>{await provider.ready;await provider.provider.disconnect();});
