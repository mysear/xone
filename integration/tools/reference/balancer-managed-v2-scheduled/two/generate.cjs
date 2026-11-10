'use strict';
// Unintegrated original VM observation prototype. No local swap/weight math.
const {ROOT,REF,E,A,V,sha:inputSha,inputClosure,sourceManifest:originalSources,sourceText,scriptMetadata}=require('../inputs.cjs');const OUT=__dirname,TMP=require('node:os').tmpdir(),VM_TMP=TMP;
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const {ethers}=E.rr('ethers'),ganache=E.rr('ganache');
const BANKSHA='1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66';
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const need=(v,m)=>{if(!v)throw Error(m);};
const same=(a,b,m)=>need(V.canonical(a)===V.canonical(b),m);
const lower=s=>s.toLowerCase(),num=x=>Number(BigInt(x)),hex=n=>'0x'+BigInt(n).toString(16),clone=x=>JSON.parse(JSON.stringify(x));
function clean(x){if(ethers.BigNumber.isBigNumber(x))return x.toString();if(Array.isArray(x))return x.map(clean);if(typeof x==='string'&&x.startsWith('0x'))return lower(x);return x;}
function preservation(){const files=E.verifyInputs().files.map(f=>f.path).concat('integration/tools/reference/balancer-managed-v2-aum/file-manifest.json','integration/test/contracts/managed-v2-aum.json');return files.sort().map(p=>{const b=fs.readFileSync(path.join(ROOT,p));return {path:p,bytes:b.length,sha256:sha(b)};});}
function deriveLayout(build,name){const defs=Object.values(build.output.sources).flatMap(s=>s.ast.nodes).filter(n=>n.nodeType==='ContractDefinition'),c=defs.find(n=>n.name===name);let slot=0,offset=0;const rows=[],immutables=[];
 for(const id of [...c.linearizedBaseContracts].reverse()){const d=defs.find(n=>n.id===id);for(const v of d.nodes.filter(n=>n.nodeType==='VariableDeclaration'&&n.stateVariable)){
  const type=v.typeDescriptions.typeString;if(v.constant)continue;if(v.mutability==='immutable'){immutables.push({id:v.id,name:v.name,declaring_contract:d.name,type});continue;}
  let size;if(type==='bool')size=1;else if(/^u?int\d*$/.test(type))size=Number(type.match(/\d+/)?.[0]||256)/8;else if(/^bytes\d+$/.test(type))size=Number(type.slice(5));else if(type==='address'||type.startsWith('contract '))size=20;else if(type.startsWith('mapping(')||type==='string'||type==='bytes')size=32;else throw Error('unhandled AST layout type '+type);
  if(offset+size>32||size===32&&offset){slot++;offset=0;}rows.push({id:v.id,declaring_contract:d.name,name:v.name,type,slot,byte_offset:offset,bytes:size,authority:'derived from original linearized AST; observed storage/getter equality separately'});offset+=size;if(offset===32){slot++;offset=0;}
 }}return {variables:rows,immutables};}
async function generate(){
 const pinnedInputs=E.verifyInputs(),runtimePins=E.nodeClosure();
 const originalBytes=fs.readFileSync(ROOT+'/integration/test/contracts/managed-v2-aum.json');need(originalBytes.length===7905624&&sha(originalBytes)===BANKSHA,'original bank pins');
 const original=require('../inputs.cjs').originalBank(),R=(r,k)=>V.resolveObject(original.objects,r.ref,k),{builds,pins}=A.load();
 const before=preservation(),copied=inputClosure();
 const sourceBuild=builds.ManagedPoolFactory,sourceManifest=originalSources();

 const layout=deriveLayout(sourceBuild,'ManagedPool');for(const [name,slot]of [['_poolState',9],['_aumState',10],['_tokenState',11]])need(layout.variables.find(v=>v.name===name).slot===slot,'AST slot '+name);
 const src=R(original.cases[0].execution.before,'snapshot'),pool=src.state.pool,vault=src.state.vault,tokens=original.scenarios[0].underlying,roles=src.state.roles;
 const identities=original.reference.identities.map(r=>R(r,'identity')).filter(i=>num(R(R(i.creation_transaction,'transaction').execution_header,'header').number)<=28);
 const iface=new Map(identities.map(i=>[i.address,new ethers.utils.Interface(A.locate(builds,i.artifact.build,i.artifact.name).c.abi)]));
 const vmDb=fs.mkdtempSync(path.join(VM_TMP,'database-'));
 const raw=ganache.provider({...original.reference.chain_options,database:{dbPath:vmDb},chain:{...original.reference.chain_options.chain,time:new Date(original.reference.chain_options.chain.time)}}),rpc=(method,params=[])=>raw.request({method,params});
 const provider=new ethers.providers.Web3Provider(raw),wallets=[1,2,3,4].map(k=>new ethers.Wallet('0x'+k.toString(16).padStart(64,'0'),provider));
 const header=async(tag='latest')=>rpc('eth_getBlockByNumber',[tag,false]);
 const nativeCode={},states=[],transactions=[],cases=[],scenarios=[],clockOperations=[];
 const oldHeaders=new Map(Object.values(original.objects).filter(o=>o.kind==='header').map(o=>[num(o.value.number),o.value]));
 const prefix=[];let height=0;
 try{
 same(await header(),oldHeaders.get(0),'exact genesis');
 for(const ref of original.reference.transactions){const t=R(ref,'transaction'),h=R(t.execution_header,'header');if(num(h.number)>28)break;
  while(height+1<num(h.number)){const n=oldHeaders.get(height+1);await rpc('evm_setTime',[num(n.timestamp)*1000]);await rpc('evm_mine');same(await header(),n,'prefix empty block');height++;}
  same(await header(),R(t.pre_header,'header'),'prefix source header');await rpc('evm_setTime',[num(h.timestamp)*1000]);const hash=await rpc('eth_sendRawTransaction',[t.signed_transaction]);need(hash===t.transaction.hash,'prefix signed hash');same(await rpc('eth_getTransactionReceipt',[hash]),t.receipt,'prefix full receipt');same(await rpc('eth_getTransactionByHash',[hash]),t.transaction,'prefix full tx');same(await header(),h,'prefix full header');prefix.push(clone(t));height=num(h.number);
 }
 need(prefix.length===28&&height===28,'prefix28 count');process.stderr.write('prefix28 exact replay passed\n');
 async function getter(to,method,args,h){const data=lower(iface.get(to).encodeFunctionData(method,args)),result=await rpc('eth_call',[{to,data},h.number]);return {to,method,args:clean(args),data,result,block_number:h.number,block_hash:h.hash,decoded:clean(iface.get(to).decodeFunctionResult(method,result))};}
 const getterSpecs=src.getters.map(r=>{const g=R(r,'getter');return {to:g.to,method:g.method,args:g.args};});
 for(const method of ['getOwner','getPoolId','getScalingFactors','getNormalizedWeights','getManagementAumFeeParams'])if(!getterSpecs.some(g=>g.to===pool&&g.method===method))getterSpecs.push({to:pool,method,args:[]});
 for(const method of ['updateWeightsGradually','updateSwapFeeGradually']){
  const selector=iface.get(pool).getSighash(method);getterSpecs.push({to:pool,method:'getActionId',args:[selector]});
  const action=(await getter(pool,'getActionId',[selector],await header())).decoded[0];
  for(const who of [roles.manager,roles.admin])for(const query of ['hasRole','canPerform'])if(Object.values(iface.get(src.state.authorizer).functions).some(f=>f.name===query))getterSpecs.push({to:src.state.authorizer,method:query,args:query==='hasRole'?[action,who]:[action,who,pool]});
 }
 const accounts=[...new Set([...src.accounts.map(r=>R(r,'account').address),...identities.map(i=>i.address),...original.reference.factory_code_storage.contracts.map(c=>c.address)])].sort();
 const mappingKey=(address,slot)=>ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['address','uint256'],[address,slot]));
 const poolMappingKeys=[...tokens.flatMap(t=>[11,12].map(slot=>({key:mappingKey(t,slot),name:slot===11?'_tokenState':'_circuitBreakerState',mapping_slot:slot,token:t}))),...Object.values(roles).flatMap(a=>[0,6,13].map(slot=>({key:mappingKey(a,slot),mapping_slot:slot,account:a})))];
 async function capture(label){const h=await header(),getters=[];
  for(const g of getterSpecs)getters.push(await getter(g.to,g.method,g.args,h));
  const observedAccounts=[];
  for(const address of accounts){const code=await rpc('eth_getCode',[address,h.number]),key=sha(Buffer.from(code.slice(2),'hex'));nativeCode[key]=code;
   const id=identities.find(i=>i.address===address);if(id)need(code===R(id.code,'code').hex,'original live runtime '+id.label);
   need((code.length-2)/2<=24576,'EIP170');observedAccounts.push({address,native_balance:await rpc('eth_getBalance',[address,h.number]),nonce:await rpc('eth_getTransactionCount',[address,h.number]),code_sha256:key,runtime_bytes:(code.length-2)/2});
  }
  const storage=[];for(const address of identities.map(i=>i.address))for(let slot=0;slot<32;slot++)storage.push({address,key:hex(slot),value:await rpc('eth_getStorageAt',[address,hex(slot),h.number])});
  for(const row of poolMappingKeys)storage.push({address:pool,...row,value:await rpc('eth_getStorageAt',[pool,row.key,h.number])});
  const g=m=>getters.find(x=>x.to===pool&&x.method===m).decoded;
  const weight=g('getGradualWeightUpdateParams'),fee=g('getGradualSwapFeeUpdateParams');const word=storage.find(x=>x.address===pool&&x.key==='0x9').value;
  const s={id:'S'+states.length,label,header:h,header_anchor_after:await header(h.number),epoch_clock:num(h.timestamp),getters,accounts:observedAccounts,storage,schedule:{weights:weight,fee},current_weights:g('getNormalizedWeights')[0],current_fee:g('getSwapFeePercentage')[0],actual_supply:g('getActualSupply')[0],packed:{pool_word:word,tokens:tokens.map((t,i)=>({token:t,mapping_slot:11,storage_key:mappingKey(t,11),raw_word:storage.find(x=>x.address===pool&&x.key===mappingKey(t,11)).value,getter_start:weight[2][i],getter_end:weight[3][i]}))}};
  same(h,s.header_anchor_after,'fixed H capture');states.push(s);return s;
 }
 function decodeEvents(receipt){return receipt.logs.map(l=>{try{const e=iface.get(l.address).parseLog(l);return {address:l.address,name:e.name,signature:e.signature,args:clean(e.args),log_index:l.logIndex};}catch{return {address:l.address,unparsed:true,log_index:l.logIndex};}});}
 async function send(index,to,method,args,purpose){const before=await capture(purpose+':before'),data=lower(iface.get(to).encodeFunctionData(method,args)),from=lower(wallets[index].address);let call;
  try{call={result:await rpc('eth_call',[{to,from,data,gas:hex(55000000),gasPrice:'0x77359400'},before.header.number])};}catch(e){call={revert_data:typeof e.data==='string'?e.data:e.data?.result||'0x',message:e.message};throw Error('native management/swap preflight rejected '+purpose+': '+JSON.stringify(call));}
  const signed=await wallets[index].signTransaction({type:0,chainId:31337,nonce:num(await rpc('eth_getTransactionCount',[from,'latest'])),to,data,gasLimit:55000000,gasPrice:2000000000});
  const hash=await rpc('eth_sendRawTransaction',[signed]),receipt=await rpc('eth_getTransactionReceipt',[hash]),transaction=await rpc('eth_getTransactionByHash',[hash]),execution_header=await header();need(num(receipt.status)===1,'actual successful receipt '+purpose);need(execution_header.parentHash===before.header.hash,'native header parent');need(execution_header.timestamp===before.header.timestamp,'execution clock stable');
  const execution={purpose,to,method,args:clean(args),signed_transaction:signed,transaction,receipt,decoded_events:decodeEvents(receipt),pre_header:before.header,execution_header,call_observation:call};transactions.push(execution);
  const after=await capture(purpose+':after');process.stderr.write(purpose+' H'+num(execution_header.number)+'\n');return {before,after,execution};
 }
 async function clock(timestamp,reason){const pre_header=await header();need(timestamp>=num(pre_header.timestamp),'forward normal clock within branch');await rpc('evm_setTime',[timestamp*1000]);await rpc('evm_mine');const h=await header();need(num(h.timestamp)===timestamp&&timestamp<1769904000,'actual clock Jan2026');clockOperations.push({method:'evm_setTime + evm_mine',timestamp,reason,pre_header,header:h});}
 const initial=await capture('prefix28:initialized');same(initial.current_weights,['800000000000000000','200000000000000000'],'initial weights');need(initial.current_fee==='10000000000000000','initial1%');
 const genesisTime=num(initial.header.timestamp),start=genesisTime+60,end=start+2592000;
 const base=await rpc('evm_snapshot');const branchOperations=[{method:'evm_snapshot',id:base,label:'prefix28 base'}];
 async function swap(sc,label,dir,amount='1000000000000000000',flooring=false){const token_in=tokens[dir],token_out=tokens[1-dir];const result=await send(3,vault,'swap',[[src.state.pool_id,0,token_in,token_out,amount,'0x'],[roles.trader,false,roles.trader,false],'0',ethers.constants.MaxUint256.toString()],sc+':'+label);
  const balance=(s,t)=>BigInt(s.getters.find(g=>g.to===t&&g.method==='balanceOf'&&g.args[0]===roles.trader).result);
  const amountOut=balance(result.after,token_out)-balance(result.before,token_out);need(amountOut>=0n,'actual receive delta');
  const c={id:'VM'+(cases.length+1),scenario:sc,label,flooring_branch:flooring,kind:'GIVEN_IN',token_in,token_out,amount_in:amount,expected_output:String(amountOut),authority:'actual recipient balanceOf difference AND original Vault Swap receipt event only',before:result.before.id,after:result.after.id,execution:result.execution};cases.push(c);return c;
 }
 for(const [index,name]of ['weightsOnly','feeOnly','both'].entries()){
  if(index){need(await rpc('evm_revert',[base]),'base revert');branchOperations.push({method:'evm_revert',id:base,label:name});const replacement=await rpc('evm_snapshot');need(replacement===base,'Ganache reusable same snapshot id');branchOperations.push({method:'evm_snapshot',id:replacement,label:name+' base'});}
  const sc={name,start,end,management_transactions:[],checkpoints:[],cases:[]};scenarios.push(sc);
  if(name!=='feeOnly'){const r=await send(1,pool,'updateWeightsGradually',[start,end,tokens,['200000000000000000','800000000000000000']],name+':schedule-weights');sc.management_transactions.push({before:r.before.id,after:r.after.id,execution:r.execution});}
  if(name!=='weightsOnly'){const r=await send(1,pool,'updateSwapFeeGradually',[start,end,'10000000000000000','30000000000000000'],name+':schedule-fee');sc.management_transactions.push({before:r.before.id,after:r.after.id,execution:r.execution});}
  for(const [label,t]of [['start-1',start-1],['start',start],['start+1',start+1],['midpoint',start+1296000],['end-1',end-1],['end',end],['end+1',end+1]]){
   await clock(t,name+':'+label);const s=await capture(name+':'+label);sc.checkpoints.push({label,timestamp:t,state:s.id});
   if(name==='both'&&label==='midpoint'){
    const snap=await rpc('evm_snapshot');branchOperations.push({method:'evm_snapshot',id:snap,label:'flooring same source H'});
    const c=await swap(name,'midpoint-flooring-plus-one',0,'1000000000000000001',true);sc.cases.push(c.id);
    need(await rpc('evm_revert',[snap]),'flooring revert');branchOperations.push({method:'evm_revert',id:snap,label:'flooring revert before main sequence'});same(await header(),s.header,'flooring exact source H restored');
   }
   const directions=(label==='midpoint'||label==='end')?[0,1]:(name==='both'&&['start-1','start','start+1','end-1'].includes(label)?[label==='start'||label==='end-1'?1:0]:[]);
   for(const dir of directions){const c=await swap(name,label+':'+(dir?'reverse':'forward'),dir);sc.cases.push(c.id);}
  }
 }
 const immutableFacts=identities.map(i=>{const a=A.locate(builds,i.artifact.build,i.artifact.name),code=R(i.code,'code').hex,astVars=new Map(Object.values(builds[i.artifact.build].output.sources).flatMap(s=>s.ast.nodes.filter(n=>n.nodeType==='ContractDefinition').flatMap(n=>n.nodes)).filter(n=>n.nodeType==='VariableDeclaration').map(v=>[String(v.id),v]));return {label:i.label,address:i.address,artifact:i.artifact,runtime_sha256:sha(Buffer.from(code.slice(2),'hex')),runtime_bytes:(code.length-2)/2,immutables:Object.entries(a.c.evm.deployedBytecode.immutableReferences||{}).map(([id,spans])=>({ast_id:id,name:astVars.get(id)?.name||null,type:astVars.get(id)?.typeDescriptions.typeString||null,spans:spans.map(s=>({...s,value:'0x'+code.slice(2+s.start*2,2+(s.start+s.length)*2)}))}))};});
 same(before,preservation(),'original input hash closure before/after');
 const self_source=['generate.cjs','checker.cjs'].map(p=>({path:p,sha256:sha(fs.readFileSync(path.join(OUT,p)))}));
 const bank={self_source,status:'unintegrated-prototype',declaredfutureprofilecandidate:{name:'R06 Managed dynamic weights/fees',schema:'unknown',current_parser:'aum-3 active schedule remains rejected; domain/wire/parser unchanged'},original_bank:{path:'integration/test/contracts/managed-v2-aum.json',sha256:BANKSHA,bytes:originalBytes.length},generator:scriptMetadata(__dirname),original_inputs:{file_manifest_sha256:pinnedInputs.file_manifest_sha256,before,after:preservation(),copied,node_closure:runtimePins,source_pins:pins,compiler_jobs:original.reference.compiler_jobs,source_manifest:sourceManifest},chain_options:original.reference.chain_options,vm_temporary_storage:'owned OS temporary Ganache dbPath removed after disconnect',pool,vault,tokens,roles,start,end,initial_state:initial.id,prefix_replay:{complete_equality:true,genesis:oldHeaders.get(0),transactions:prefix,headers:[...oldHeaders].filter(([n])=>n<=28).sort(([a],[b])=>a-b).map(([,h])=>h)},identities:immutableFacts,native_code:nativeCode,storage_layout:{...layout,compiler_storageLayout_present:false,getState_public_ABI_present:false,facts:'_getPoolState/_getTokenState are internal; _poolState slot9, _aumState10, _tokenState mapping11. Immutables occupy code spans, not storage slots. getState is not in original public ABI.',coverage:'slots0..31 of all 14 prefix contracts, pool mapping keys for tokens/breakers and role BPT balances/nonces/allowlist; arbitrary Vault nested mapping and complete Ethereum trie are not enumerated',absent_original_source_names:['ManagedPoolSwapFeeStorageLib','WeightCompression'],fee_encoding:{source:'contracts/managed/ManagedPoolStorageLib.sol',start_time_offset:64,end_time_offset:96,start_fee_offset:128,end_fee_offset:190,fee_width_bits:62,compression:'none; direct unsigned integer fields',start_fee_requested:'10000000000000000',end_fee_requested:'30000000000000000',minimum_unit:'1 (18-decimal percentage fixed point)'},token_encoding:{source:'contracts/managed/ManagedPoolTokenStorageLib.sol',start_offset:0,end_offset:64,weight_width_bits:64,decimal_diff_offset:128,decimal_diff_width:5,compression:'none; direct uint64 normalized weight fields'},interpolation_authority:'original GradualValueChange Solidity execution via current-clock getters; no reconstructed interpolation used for expected outputs'},clock_operations:clockOperations,branch_operations:branchOperations,scenarios,transactions,states,cases,limitations:['No integration or schema support claimed; current parser rejects active schedules.','Original build-info lacks storageLayout: slot numbers are AST-derived and checked against raw storage and getters.','Only exact fixed original source version is covered. Named compression libraries absent in this version.','Storage coverage is explicit; arbitrary mapping keys/full trie not claimed.','Flooring is observed via plus-one-input snapshot branch; no local mathematical oracle.']};
 bank.endpoint_integer_differences=scenarios.map(sc=>{const s=states.find(s=>s.id===sc.checkpoints[0].state);return {scenario:sc.name,fee_getter_minus_requested_start:String(BigInt(s.schedule.fee[2])-10000000000000000n),fee_getter_minus_requested_end:String(BigInt(s.schedule.fee[3])-(sc.name==='weightsOnly'?10000000000000000n:30000000000000000n)),weight_getter_start_minus_requested:s.schedule.weights[2].map((v,i)=>String(BigInt(v)-[800000000000000000n,200000000000000000n][i])),weight_getter_end_minus_requested:s.schedule.weights[3].map((v,i)=>String(BigInt(v)-(sc.name==='feeOnly'?[800000000000000000n,200000000000000000n]:[200000000000000000n,800000000000000000n])[i]))};});
 const floor=cases.find(c=>c.flooring_branch),baseCase=cases.find(c=>c.scenario==='both'&&c.label==='midpoint:forward');bank.flooring_differential={branch:floor.id,base:baseCase.id,same_source_header:states.find(s=>s.id===floor.before).header.hash===states.find(s=>s.id===baseCase.before).header.hash,input_minimum_unit_difference:String(BigInt(floor.amount_in)-BigInt(baseCase.amount_in)),actual_recipient_minimum_unit_difference:String(BigInt(floor.expected_output)-BigInt(baseCase.expected_output))};need(bank.flooring_differential.same_source_header,'flooring source H');
 require('./checker.cjs').check(bank);require('../checker.cjs').verifyBranch(bank,'two',original,builds);return bank;
 }finally{await raw.disconnect();fs.rmSync(vmDb,{recursive:true,force:true});}
}
module.exports={generate};
