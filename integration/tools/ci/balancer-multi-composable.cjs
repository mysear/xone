'use strict';
const fs=require('fs'),path=require('path');
const receiptPath=path.resolve(__dirname,'../../artifacts/local/balancer-multi-composable-generation.json');
fs.writeFileSync(receiptPath,JSON.stringify({passed:false,state:'RUNNING'})+'\n');
const {root,ref,ethers,solc,sha,read,verifyManifest,compile,runtimeDigest,legacyDigest,revertResult,provider,E,MAX}=require('./balancer-multi-golden.cjs');
const rev='e91a2b643a49856f51a648d175667c1b48cf3377';
async function main(){
 const started=Date.now(),legacy=legacyDigest(),runtime=runtimeDigest();
 const inputs=['integration/tools/ci/balancer-multi-composable.cjs','integration/tools/ci/balancer-multi-golden.cjs',`integration/tools/reference/balancer-multi/${rev}.json`,'integration/tools/reference/balancer-multi/ComposableMathHarness.sol'];
 const hashes=Object.fromEntries(inputs.map(p=>[p,sha(read(p))]));
 const manifest=JSON.parse(fs.readFileSync(path.join(ref,rev+'.json')));
 const contracts=compile(verifyManifest(manifest,path.join(ref,'sources',rev)),['ComposableMathHarness.sol']);
 const c=contracts['ComposableMathHarness.sol'].ComposableMathRef,signer=provider.getSigner(0);
 const math=await new ethers.ContractFactory(c.abi,c.evm.bytecode.object,signer).deploy({gasLimit:25000000});await math.deployed();
 const regular=[],bpt=[],sequences=[],bptSequences=[];
 async function runRegular(b,amp,s,fee,input,i,j,k){return revertResult(async()=>({amount_out:(await math.regular([b.map(String),amp,s.map(String),fee.toString(),input.toString(),i,j,k],{gasLimit:25000000})).toString()}));}
 async function runBpt(b,amp,s,fee,input,supply,index,join){return revertResult(async()=>{const r=await math.bpt([b.map(String),amp,s.map(String),fee.toString(),input.toString(),supply.toString(),index,join],{gasLimit:25000000});return {amount_out:r.out.toString(),next_actual_supply:r.nextSupply.toString()};});}
 for(const count of [2,3,5])for(const amp of [1000,100000,5000000])for(const mixed of [false,true]){
  const decimals=Array.from({length:count},(_,i)=>mixed?[18,6,0,12,18][i]:18),scales=decimals.map((d,i)=>10n**BigInt(36-d)*(mixed&&i===1?1234567890123456789n:E)/E);
  const fee=mixed?10n**16n:0n,balances=decimals.map((d,i)=>1000000n*BigInt(i+1)*10n**BigInt(d)),supply=BigInt(count)*1000000n*E;
  for(const k of [0,Math.floor(count/2),count]){
   const b=[...balances];b.splice(k,0,1n<<111n);const s=[...scales];s.splice(k,0,E);
   const state={balances:b.map(String),amplification:String(amp),scales:s.map(String),fee:fee.toString(),bpt_index:k};
   for(const [i0,j0] of [[0,count-1],[count-1,0]]){const i=i0>=k?i0+1:i0,j=j0>=k?j0+1:j0;
    for(const input of [0n,1n,2n,1000n*10n**BigInt(decimals[i0]),balances[i0]/3n,balances[i0]*2n,(1n<<256n)-1n])regular.push({...state,input:input.toString(),index_in:i,index_out:j,expected:await runRegular(b,amp,s,fee,input,i,j,k)});
   }
   const steps=[];let next=[...b];
   for(const [i0,j0] of [[0,count-1],[count-1,0],[0,count-1]]){const i=i0>=k?i0+1:i0,j=j0>=k?j0+1:j0,input=1000n*10n**BigInt(decimals[i0]);const expected=await runRegular(next,amp,s,fee,input,i,j,k);if(expected.reverted||expected.amount_out==='0')throw Error('composable sequence');steps.push({input:input.toString(),index_in:i,index_out:j,expected});next[i]+=input;next[j]-=BigInt(expected.amount_out);}
   sequences.push({...state,steps});
  }
  {let b=[...balances],supplyNow=supply;const steps=[];
   for(let k=0;k<6;k++){const join=k%2===0,index=k%count,input=join?1000n*10n**BigInt(decimals[index]):100n*E;
    const expected=await runBpt(b,amp,scales,fee,input,supplyNow,index,join);
    if(expected.reverted||expected.amount_out==='0')throw Error('BPT math sequence');
    steps.push({input:input.toString(),token_index:index,token_to_bpt:join,expected});
    if(join)b[index]+=input;else b[index]-=BigInt(expected.amount_out);supplyNow=BigInt(expected.next_actual_supply);
   }
   bptSequences.push({balances:balances.map(String),amplification:String(amp),scales:scales.map(String),fee:fee.toString(),actual_supply_after_fees:supply.toString(),steps});
  }
  for(const join of [true,false])for(const index of [0,count-1]){
   const inputs=join?[0n,1n,2n,1000n*10n**BigInt(decimals[index]),balances[index]/3n,balances[index]*2n]:[0n,1n,2n,E,supply/100n,supply/2n,supply,supply+1n];
   for(const input of inputs)bpt.push({balances:balances.map(String),amplification:String(amp),scales:scales.map(String),fee:fee.toString(),input:input.toString(),actual_supply_after_fees:supply.toString(),token_index:index,token_to_bpt:join,expected:await runBpt(balances,amp,scales,fee,input,supply,index,join)});
  }
 }
 verifyManifest(manifest,path.join(ref,'sources',rev));
 if(JSON.stringify(legacy)!==JSON.stringify(legacyDigest())||runtime!==runtimeDigest()||inputs.some(p=>hashes[p]!==sha(read(p))))throw Error('inputs changed');
 const text=JSON.stringify({schema_version:1,reference:{revision:rev,source_files:manifest.files.length,solc:solc.version(),flow:'Unmodified StableMath regular and BPT join/exit kernels; fee/scale test wrapper; actual ComposableStablePool not deployed; supply externally supplied after protocol fees'},regular,bpt,sequences,bpt_sequences:bptSequences},null,2)+'\n';
 for(const svc of ['integration','dex-router'])fs.writeFileSync(path.join(root,svc,'test/contracts/balancer-composable-math.json'),text);
 const report={passed:true,started_at_unix_ms:started,finished_at_unix_ms:Date.now(),revision:rev,oracle_inputs:hashes,runtime_sha256:runtime,corpus_sha256:sha(text),regular_cases:regular.length,bpt_cases:bpt.length,bpt_sequences:bptSequences.length,bpt_steps:bptSequences.reduce((n,s)=>n+s.steps.length,0),sequences:sequences.length,steps:sequences.reduce((n,s)=>n+s.steps.length,0),legacy_fixture_hashes:legacy};
 fs.writeFileSync(receiptPath,JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify({passed:true,regular:regular.length,bpt:bpt.length,sequences:sequences.length,corpus_sha256:sha(text)}));
}
main().catch(e=>{console.error(e);process.exitCode=1;}).finally(async()=>{await provider.ready;await provider.provider.disconnect();});
