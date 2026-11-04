'use strict';
const fs=require('node:fs'),path=require('node:path'),cp=require('node:child_process'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../../..'),ref=path.join(root,'integration/tools/reference/infinity-bin-service'),receipt=path.join(ref,'replay-test-receipt.json');
fs.writeFileSync(receipt,JSON.stringify({passed:false,state:'RUNNING'})+'\n');
const requireThat=(x,m)=>{if(!x)throw Error(m);},sha=x=>crypto.createHash('sha256').update(x).digest('hex');
(async()=>{try{
 const f=JSON.parse(fs.readFileSync(path.join(root,'integration/test/contracts/infinity-bin-service.json'))),requests=[{action:'identity'}];
 for(const t of f.trades){const p=f.pools.find(p=>p.id===t.pool_id),b=f.RPCbanks.find(b=>b.id===t.pre_bank).pools.find(b=>b.pool_id===p.id);requests.push({action:'quote',state:{activeid:b.active_id,step:p.bin_step,lpfee:p.lp_fee,protofee:b.protocol_fee,bins:b.bins},amount:t.amount_in,zero:t.zero_for_one});}
 requests.push({action:'quote',state:{activeid:8388608,step:0,lpfee:3000,protofee:0,bins:{}},amount:'1',zero:true});
 requests.push({action:'quote',state:requests[1].state,amount:'1000000000000000000000000000000',zero:true});
 // A successful request after failures proves request isolation and cleanup.
 requests.push(requests[1]);
 const cli=path.join(__dirname,'infinity-bin-service-replay.cjs');requireThat(fs.existsSync(cli),'replay CLI not implemented');
 const child=cp.spawn(process.execPath,[cli],{stdio:['pipe','pipe','pipe']});let stdout='',stderr='';child.stdout.on('data',b=>{stdout+=b;});child.stderr.on('data',b=>{stderr+=b;});child.stdin.end(requests.map(r=>JSON.stringify(r)).join('\n')+'\n');
 const code=await new Promise((resolve,reject)=>{child.on('error',reject);child.on('close',resolve);});requireThat(code===0,'CLI exit '+code+' '+stderr);
 const replies=stdout.trim().split('\n').map(JSON.parse);requireThat(replies.length===requests.length,'one JSON reply per request');requireThat(replies[0].ok===true&&replies[0].identity.full_manager_swap_settlement_verified===false,'identity scope');
 for(let i=0;i<4;i++){const reply=replies[i+1],expected=f.trades[i].expected;requireThat(reply.ok===true,'native quote failed '+JSON.stringify(reply));for(const k of ['delta0','delta1','amount_out','swap_fee','protocol_amount'])requireThat(reply[k]===expected[k],'native replay mismatch '+i+' '+k);
  requireThat(reply.nextstate.activeid===expected.post.active_id,'active ID');for(const [id,b]of Object.entries(expected.post.bins)){const actual=reply.nextstate.bins[id];for(const k of ['reserve0','reserve1','liquidity','total_shares','price'])requireThat(actual[k]===b[k],'native post '+i+'/'+id+'/'+k);}
 }
 requireThat(replies[5].ok===false&&replies[5].native_reverted===false,'invalid profile rejected');requireThat(replies[6].ok===false&&replies[6].native_reverted===true,'native insufficient liquidity rejected');requireThat(replies[7].ok===true&&replies[7].amount_out===f.trades[0].expected.amount_out,'isolation after failure');
 const result={passed:true,native_replays:4,invalid_profile_rejected:true,native_revert_rejected:true,request_isolation:true,inputs:Object.fromEntries([cli,__filename,path.join(ref,'execution-receipt.json')].map(p=>[path.relative(root,p),sha(fs.readFileSync(p))])),fixture_sha256:sha(fs.readFileSync(path.join(root,'integration/test/contracts/infinity-bin-service.json')))};fs.writeFileSync(receipt,JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result));
 }catch(e){fs.writeFileSync(receipt,JSON.stringify({passed:false,error:String(e),stack:e.stack},null,2)+'\n');console.error(e);process.exitCode=1;}})();
