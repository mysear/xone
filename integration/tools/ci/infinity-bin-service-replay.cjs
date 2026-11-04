'use strict';
// Independent JSON Lines original-Solidity reference. Stdout contains only JSON replies.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),readline=require('node:readline');
const root=path.resolve(__dirname,'../../..'),ref=path.join(root,'integration/tools/reference/infinity-bin-service');
const receiptPath=path.join(ref,'replay-receipt.json');
fs.writeFileSync(receiptPath,JSON.stringify({passed:false,state:'RUNNING'})+'\n');
const sha=x=>crypto.createHash('sha256').update(x).digest('hex');
function requireThat(ok,message){if(!ok)throw Error(message);}
let raw,provider,math,snapshot,ethers,reference,artifact,identity,completed=0,requests=0,rejected=0;
function integer(value,min,max,name){requireThat(Number.isSafeInteger(value)&&value>=min&&value<=max,name);return value;}
function uint(value,max,name){requireThat(typeof value==='string'&&/^(0|[1-9][0-9]*)$/.test(value),name+' must be a decimal string');const n=BigInt(value);requireThat(n<=max,name+' range');return value;}
function sourceState(input){
 requireThat(input&&typeof input==='object'&&!Array.isArray(input),'state object');
 const activeid=integer(input.activeid??input.active_id,1,0xffffff-1,'activeid');
 const step=integer(input.step??input.bin_step,1,100,'bin step');
 const lpfee=integer(input.lpfee??input.lp_fee,0,100000,'static LP fee');
 const protofee=integer(input.protofee??input.protocol_fee,0,0xffffff,'packed protocol fee');
 requireThat((protofee&4095)<=4000&&(protofee>>12)<=4000,'protocol fee direction <=4000');
 requireThat(input.bins&&typeof input.bins==='object','initialized bins');
 const values=Array.isArray(input.bins)?input.bins.map(b=>[b.id,b]):Object.entries(input.bins);
 requireThat(values.length>0&&values.length<=2048,'1..2048 initialized bins');
 const rows=values.map(([id,b])=>({id:integer(Number(id),1,0xffffff-1,'bin ID'),reserve0:uint(b.reserve0,(1n<<128n)-1n,'reserve0'),reserve1:uint(b.reserve1,(1n<<128n)-1n,'reserve1'),shares:uint(b.shares??b.total_shares,(1n<<256n)-1n,'initialized shares')})).sort((a,b)=>a.id-b.id);
 for(let i=0;i<rows.length;i++){requireThat(BigInt(rows[i].shares)>0n,'initialized shares nonzero');requireThat(i===0||rows[i].id>rows[i-1].id,'unique initialized bins');}
 return {activeid,step,lpfee,protofee,rows};
}
async function init(){
 // Full frozen source/compiler/binary/artifact closure is verified before Ganache or artifact use.
 const {verify}=require('./infinity-bin-service-check.cjs');verify();
 reference=JSON.parse(fs.readFileSync(path.join(ref,'source-metadata.json')));
 artifact=JSON.parse(fs.readFileSync(path.join(ref,'Math.artifact.json')));
 const base=path.join(root,'integration/.runtime/singleton-reference/node_modules');
 const ganache=require(path.join(base,'ganache'));({ethers}=require(path.join(base,'ethers')));
 const bound=new Map(reference.loaded_dependencies.map(d=>[path.join(root,d.path),d.sha256]));
 for(const filename of Object.keys(require.cache).filter(p=>p.startsWith(base+path.sep)))requireThat(bound.get(filename)===sha(fs.readFileSync(filename)),'unbound loaded runtime dependency '+filename);
 requireThat(sha(Buffer.from(artifact.runtime_template.slice(2),'hex'))===reference.artifacts.Math.runtime_template_sha256,'frozen Math template');
 requireThat((artifact.runtime_template.length-2)/2<=24576,'Math EIP170');
 raw=ganache.provider({logging:{quiet:true},chain:{hardfork:'shanghai',chainId:31337,allowUnlimitedContractSize:false,time:new Date('2026-11-02T00:00:00Z')},wallet:{deterministic:true},miner:{blockGasLimit:100000000,timestampIncrement:1}});
 provider=new ethers.providers.Web3Provider(raw);provider.pollingInterval=10;
 math=await new ethers.ContractFactory(artifact.abi,artifact.creation,provider.getSigner()).deploy({gasLimit:18000000});await math.deployed();
 const code=(await provider.getCode(math.address)).toLowerCase();requireThat(code===artifact.runtime_template,'deployed frozen original Math runtime');
 snapshot=await provider.send('evm_snapshot',[]);
 identity={schema:'infinity-bin-service-replay/1',revision:reference.revision,compiler:reference.compiler,engine:reference.execution_engine,unlimited_contract_size:false,math_runtime_bytes:(code.length-2)/2,math_runtime_sha256:sha(Buffer.from(code.slice(2),'hex')),math_runtime_keccak256:ethers.utils.keccak256(code),manager_runtime_template_sha256:reference.artifacts.BinPoolManager.runtime_template_sha256,vault_runtime_template_sha256:reference.artifacts.Vault.runtime_template_sha256,full_manager_swap_settlement_verified:false,scope:'original BinPool.swap/BinHelper/TreeMath; caller supplies snapshot, no Manager/Vault settlement or application fee math',source_metadata_sha256:sha(fs.readFileSync(path.join(ref,'source-metadata.json'))),execution_receipt_sha256:sha(fs.readFileSync(path.join(ref,'execution-receipt.json'))),artifact_sha256:sha(fs.readFileSync(path.join(ref,'Math.artifact.json'))),cli_sha256:sha(fs.readFileSync(__filename))};
}
async function cleanSeed(s){
 requireThat(await provider.send('evm_revert',[snapshot]),'request snapshot restore');snapshot=await provider.send('evm_snapshot',[]);
 await (await math.seed(s.activeid,s.step,s.lpfee,s.protofee,s.rows.map(b=>[b.id,b.reserve0,b.reserve1,b.shares]),{gasLimit:80000000})).wait();
}
async function state(s){
 const packed=await math.slot0(),activeid=Number(BigInt(packed)&0xffffffn),bins={},tree={level0:null,level1:{},level2:{}};
 const W=n=>ethers.utils.hexZeroPad(ethers.BigNumber.from(n).toHexString(),32).toLowerCase();
 for(const row of s.rows){const b=await math.bin(row.id),t=await math.tree(row.id);requireThat(t[3],'initialized tree member');tree.level0=t[0];tree.level1[W(row.id>>16)]=t[1];tree.level2[W(row.id>>8)]=t[2];
  bins[row.id]={reserve0:b[0].toString(),reserve1:b[1].toString(),liquidity:b[2].toString(),total_shares:b[3].toString(),shares:b[3].toString(),price:(await math.price(row.id)).toString(),initialized:true,next:{true:Number(await math.next(true,row.id)),false:Number(await math.next(false,row.id))}};
 }
 return {activeid,step:s.step,lpfee:s.lpfee,protofee:s.protofee,active_id:activeid,bin_step:s.step,lp_fee:s.lpfee,protocol_fee:s.protofee,slot0:packed,bins,tree};
}
function revertData(error){const seen=new Set();function walk(e){if(!e||typeof e!=='object'||seen.has(e))return;seen.add(e);if(typeof e.result==='string'&&/^0x[0-9a-f]*$/i.test(e.result))return e.result;if(typeof e.data==='string'&&/^0x[0-9a-f]*$/i.test(e.data)&&e.data!=='0x')return e.data;for(const key of ['error','data','cause']){const r=walk(e[key]);if(r)return r;}}return walk(error)||'0x';}
async function quote(request){
 const s=sourceState(request.state);const amount=uint(request.amount??request.amount_in,(1n<<127n)-1n,'amount');requireThat(BigInt(amount)>0n,'positive exact input');const zero=request.zero??request.zero_for_one;requireThat(typeof zero==='boolean','zero direction boolean');
 await cleanSeed(s);
 let result;
 try{result=await math.callStatic.swap(amount,zero,{gasLimit:80000000});}catch(error){return {ok:false,native_reverted:true,revert_data:revertData(error),error:'original BinPool.swap reverted',nextstate:await state(s),post:await state(s)};}
 // The transaction proves poststate, independently of the nonpersisting static result.
 await (await math.swap(amount,zero,{gasLimit:80000000})).wait();const nextstate=await state(s);
 return {ok:true,native_reverted:false,amount_in:amount,amount_out:(zero?result.delta1:result.delta0).toString(),delta0:result.delta0.toString(),delta1:result.delta1.toString(),swap_fee:Number(result.swapFee),protocol_amount:result.protocolAmount.toString(),active_id:nextstate.activeid,nextstate,post:nextstate};
}
async function main(){
 await init();const lines=readline.createInterface({input:process.stdin,crlfDelay:Infinity});
 for await(const line of lines){if(!line.trim())continue;requests++;let response;try{const request=JSON.parse(line),action=typeof request==='string'?request:(request.action??request.op);if(action==='identity')response={ok:true,identity};else if(action==='quote')response=await quote(request);else throw Error('action must be identity or quote');if(response.ok)completed++;else rejected++;}catch(error){rejected++;response={ok:false,native_reverted:false,error:String(error)};}process.stdout.write(JSON.stringify(response)+'\n');}
 return {passed:true,requests,completed,rejected,identity};
}
(async()=>{let receipt;try{receipt=await main();}catch(error){receipt={passed:false,error:String(error),stack:error.stack};console.error('infinity-bin-service replay failed: '+error.stack);process.exitCode=1;}finally{if(raw)try{await raw.disconnect();}catch(error){receipt={passed:false,error:'provider cleanup: '+error};process.exitCode=1;}fs.writeFileSync(receiptPath,JSON.stringify(receipt,null,2)+'\n');}})();
