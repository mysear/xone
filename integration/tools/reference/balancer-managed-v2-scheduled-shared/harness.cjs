'use strict';
// Provider-owned original VM. No pricing kernel/SDK or storage injection.
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),assert=require('node:assert/strict');
const I=require('./inputs.cjs'),{E,A,V,sha,originalBank,ROOT}=I;
const {ethers}=E.rr('ethers'),ganache=E.rr('ganache');
const H=require('../balancer-managed-v2-scheduled/helpers.cjs');
const L=require('../balancer-managed-v2-scheduled/layout.cjs');
const lower=s=>s.toLowerCase(),num=x=>{const n=Number(BigInt(x));assert(Number.isSafeInteger(n));return n;};
const eq=(a,b,m)=>assert.equal(V.canonical(a),V.canonical(b),m),MAX=ethers.constants.MaxUint256,Z=ethers.constants.AddressZero;
function clean(x){if(ethers.BigNumber.isBigNumber(x)||typeof x==='bigint')return x.toString();if(Array.isArray(x))return x.map(clean);if(typeof x==='string'&&x.startsWith('0x'))return lower(x);if(x&&typeof x==='object')return Object.fromEntries(Object.entries(x).map(([k,v])=>[k,clean(v)]));return x;}
function encoding(builds){
 const p=L.packedLayout(builds.ManagedPoolFactory),c=p.pool.constants,t=p.token.constants;
 return {pool_slot:9,aum_slot:10,token_mapping_slot:11,start_weight:{offset:t._START_NORM_WEIGHT_OFFSET,bits:t._NORM_WEIGHT_WIDTH},end_weight:{offset:t._END_NORM_WEIGHT_OFFSET,bits:t._NORM_WEIGHT_WIDTH},decimal_diff:{offset:t._DECIMAL_DIFF_OFFSET,bits:t._DECIMAL_DIFF_WIDTH},fee_start:{offset:c._SWAP_FEE_START_PCT_OFFSET,bits:c._SWAP_FEE_PCT_WIDTH},fee_end:{offset:c._SWAP_FEE_END_PCT_OFFSET,bits:c._SWAP_FEE_PCT_WIDTH},fee61_bit_is_value:true,fee_bit190_is_end_bit0:true,compression:'none in exact original source'};
}
class Harness {
 constructor(mode){
  assert(['mixed','active'].includes(mode));E.verifyInputs();E.nodeClosure();
  this.mode=mode;this.original=originalBank();this.R=(r,k)=>V.resolveObject(this.original.objects,r.ref,k);
  const {builds}=A.load();this.builds=builds;
  this.layout=L.deriveLayout(builds.ManagedPoolFactory,'ManagedPool');
  for(const [name,slot]of [['_poolState',9],['_aumState',10],['_tokenState',11]])assert.equal(this.layout.variables.find(v=>v.name===name).slot,slot);
  this.prefixIDs=this.original.reference.identities.map(r=>this.R(r,'identity')).filter(i=>num(this.R(this.R(i.creation_transaction,'transaction').execution_header,'header').number)<=28);
  this.oldHeaders=new Map(Object.values(this.original.objects).filter(o=>o.kind==='header').map(o=>[num(o.value.number),o.value]));
  this.db=fs.mkdtempSync(path.join(os.tmpdir(),'xone-scheduled-shared-vm-'));
  this.raw=ganache.provider({...this.original.reference.chain_options,database:{dbPath:this.db},chain:{...this.original.reference.chain_options.chain,time:new Date(this.original.reference.chain_options.chain.time)}});
  this.rpc=(m,p=[])=>this.raw.request({method:m,params:p});
  this.provider=new ethers.providers.Web3Provider(this.raw);
  this.wallets=[1,2,3,4].map(k=>new ethers.Wallet('0x'+k.toString(16).padStart(64,'0'),this.provider));
  this.roles=Object.fromEntries(['admin','manager','lp','trader'].map((name,i)=>[name,lower(this.wallets[i].address)]));
  this.contracts=new Map(this.prefixIDs.map(i=>[i.address,new ethers.Contract(i.address,this.artifact(i.artifact.build,i.artifact.name).c.abi,this.wallets[0])]));
  this.runtime={};this.newIDs=[];this.prefix=[];this.transactions=[];this.banks=[];this.clockOps=[];this.pools=[];this.tokens=[];
 }
 artifact(build,name){return A.locate(this.builds,build,name);}
 byLabel(name){const i=this.prefixIDs.find(i=>i.label===name);assert(i,'original prefix identity '+name);return this.contracts.get(i.address);}
 async header(tag='latest'){return this.rpc('eth_getBlockByNumber',[tag,false]);}
 async call(c,method,args=[],h,records){
  h=h||await this.header();const data=lower(c.interface.encodeFunctionData(method,args));
  const result=await this.rpc('eth_call',[{to:lower(c.address),data},h.number]);
  const decoded=clean(c.interface.decodeFunctionResult(method,result));
  if(records)records.push({to:lower(c.address),method,args:clean(args),data,result,decoded,block_number:h.number,block_hash:h.hash});
  return decoded;
 }
 events(receipt){return receipt.logs.map(l=>{try{const c=this.contracts.get(l.address);assert(c);const e=c.interface.parseLog(l);return {address:l.address,name:e.name,signature:e.signature,args:clean(e.args),log_index:l.logIndex};}catch{return {address:l.address,unparsed:true,log_index:l.logIndex};}});}
 async send(purpose,c,method,args=[],signer=0,creation){
  const pre=await this.header(),from=this.roles[['admin','manager','lp','trader'][signer]],to=c?lower(c.address):null,data=lower(creation||c.interface.encodeFunctionData(method,args));
  let preflight;try{preflight={result:await this.rpc('eth_call',[{from,...(to?{to}:{}),data,gas:ethers.utils.hexValue(55000000),gasPrice:'0x77359400'},pre.number])};}catch(e){preflight={error:e.message,data:typeof e.data==='string'?e.data:e.data?.result};}
  const signed=await this.wallets[signer].signTransaction({type:0,chainId:31337,nonce:num(await this.rpc('eth_getTransactionCount',[from,'latest'])),...(to?{to}:{}),data,gasLimit:55000000,gasPrice:2000000000});
  const hash=await this.rpc('eth_sendRawTransaction',[signed]),receipt=await this.rpc('eth_getTransactionReceipt',[hash]),tx=await this.rpc('eth_getTransactionByHash',[hash]),h=await this.header();
  assert.equal(receipt.status,'0x1',purpose+' failed: '+JSON.stringify(preflight));assert.equal(h.parentHash,pre.hash);assert.equal(h.timestamp,pre.timestamp,'fixed transaction clock');
  const row={id:'T'+this.transactions.length,purpose,to,method,args:clean(args),signed_transaction:signed,transaction:tx,receipt,pre_header:pre,execution_header:h,call_observation:preflight,events:this.events(receipt)};
  this.transactions.push(row);return row;
 }
 async replay(){
  eq(await this.header(),this.oldHeaders.get(0),'original genesis');let height=0;
  for(const ref of this.original.reference.transactions){
   const t=this.R(ref,'transaction'),h=this.R(t.execution_header,'header');if(num(h.number)>28)break;
   while(height+1<num(h.number)){const next=this.oldHeaders.get(height+1);await this.rpc('evm_setTime',[num(next.timestamp)*1000]);await this.rpc('evm_mine');eq(await this.header(),next,'prefix empty header');height++;}
   eq(await this.header(),this.R(t.pre_header,'header'),'prefix preheader');await this.rpc('evm_setTime',[num(h.timestamp)*1000]);
   assert.equal(await this.rpc('eth_sendRawTransaction',[t.signed_transaction]),t.transaction.hash,'original signed prefix');
   eq(await this.rpc('eth_getTransactionReceipt',[t.transaction.hash]),t.receipt,'complete original prefix receipt');eq(await this.rpc('eth_getTransactionByHash',[t.transaction.hash]),t.transaction,'complete original prefix tx');eq(await this.header(),h,'complete original prefix header');
   height=num(h.number);this.prefix.push(t);
  }
  assert.equal(this.prefix.length,28);assert.equal(height,28);
  for(const i of this.prefixIDs){const code=await this.rpc('eth_getCode',[i.address,'latest']);eq(code,this.R(i.code,'code').hex,'exact prefix runtime');this.runtime[sha(Buffer.from(code.slice(2),'hex'))]=code;}
  this.factory=this.byLabel('ManagedPoolFactory');this.vault=this.byLabel('Vault');this.feeProvider=this.byLabel('ProtocolFeePercentagesProvider');
  const parts=await this.call(this.factory,'getCreationCodeContracts');let creation='0x';
  for(const address of parts){const code=await this.rpc('eth_getCode',[address,'latest']);assert((code.length-2)/2<=24576);creation+=code.slice(2);}
  const links=Object.fromEntries(this.prefixIDs.map(i=>[i.label,i.address]));
  assert.equal(creation,A.link(this.artifact('ManagedPoolFactory','ManagedPool'),links),'original factory creation-code parts');
 }
 async identify(c,artifact,creation,args){
  const address=lower(c.address),code=await this.rpc('eth_getCode',[address,'latest']),template=artifact.c.evm.deployedBytecode;
  assert((code.length-2)/2<=24576,'normal EIP170');const digest=sha(Buffer.from(code.slice(2),'hex'));this.runtime[digest]=code;
  const vars=new Map(Object.values(this.builds[artifact.build].output.sources).flatMap(s=>s.ast.nodes.filter(n=>n.nodeType==='ContractDefinition').flatMap(n=>n.nodes)).filter(n=>n.nodeType==='VariableDeclaration').map(n=>[String(n.id),n]));
  const immutables=Object.entries(template.immutableReferences||{}).map(([id,spans])=>({ast_id:id,name:vars.get(id)?.name,type:vars.get(id)?.typeDescriptions.typeString,spans:spans.map(s=>({...s,value:'0x'+code.slice(2+s.start*2,2+(s.start+s.length)*2)}))}));
  let masked=code.slice(2),expected=template.object;const links=[];
  for(const libs of Object.values(template.linkReferences||{}))for(const [name,spans]of Object.entries(libs)){
   const target=this.prefixIDs.find(i=>i.label===name)?.address;assert(target,'original linked library');
   for(const p of spans){assert.equal(code.slice(2+p.start*2,2+(p.start+p.length)*2),target.slice(2));expected=expected.slice(0,p.start*2)+target.slice(2)+expected.slice((p.start+p.length)*2);}
   links.push({name,address:target,spans});
  }
  for(const row of immutables)for(const s of row.spans){masked=masked.slice(0,s.start*2)+'0'.repeat(s.length*2)+masked.slice((s.start+s.length)*2);expected=expected.slice(0,s.start*2)+'0'.repeat(s.length*2)+expected.slice((s.start+s.length)*2);}
  assert.equal(masked,expected,'original full compiler template except declared immutable context');
  const result={address,artifact:{build:artifact.build,source:artifact.source,name:artifact.name},creation_transaction:creation.id,constructor_arguments:clean(args),runtime_bytes:(code.length-2)/2,runtime_sha256:digest,runtime_template:template,library_links:links,immutables};
  this.newIDs.push(result);return result;
 }
 async setup(){
  const links=Object.fromEntries(this.prefixIDs.map(i=>[i.label,i.address])),a=this.artifact('Authorizer','TestToken');
  const nonce=num(await this.rpc('eth_getTransactionCount',[this.roles.admin,'latest'])),future=[0,1,2].map(i=>lower(ethers.utils.getContractAddress({from:this.roles.admin,nonce:nonce+i}))),sorted=[...future].sort();
  this.decimals=[18,6,8];
  for(let i=0;i<3;i++){
   const index=sorted.indexOf(future[i]),args=[this.roles.admin,'Shared schedule token '+index,'SHS'+index,this.decimals[index]];
   const data=new ethers.ContractFactory(a.c.abi,A.link(a,links)).getDeployTransaction(...args).data,t=await this.send('deploy-shared-token-'+index,null,'constructor',args,0,data),c=new ethers.Contract(t.receipt.contractAddress,a.c.abi,this.wallets[0]);
   this.contracts.set(lower(c.address),c);await this.identify(c,a,t,args);this.tokens.push(c);
  }
  this.tokens.sort((a,b)=>lower(a.address).localeCompare(lower(b.address)));this.tokenAddresses=this.tokens.map(t=>lower(t.address));eq(this.tokenAddresses,sorted,'sorted original underlying');
  for(const [index,name]of ['A','B'].entries()){
   const params={name:'Original shared schedule '+name,symbol:'MSS'+name,assetManagers:[Z,Z,Z]};
   const settings={tokens:this.tokenAddresses,normalizedWeights:index===0?['500000000000000000','300000000000000000','200000000000000000']:['200000000000000000','500000000000000000','300000000000000000'],swapFeePercentage:'10000000000000000',swapEnabledOnStart:true,mustAllowlistLPs:false,managementAumFeePercentage:index===0?'20000000000000000':'10000000000000000',aumFeeId:3};
   const creation=await this.send('factory-create-shared-'+name,this.factory,'create',[params,settings,this.roles.manager,ethers.constants.HashZero]);
   const poolAddress=creation.events.find(e=>e.name==='PoolCreated')?.args[0];assert(poolAddress,'actual original PoolCreated');
   const pool=new ethers.Contract(poolAddress,this.artifact('ManagedPoolFactory','ManagedPool').c.abi,this.wallets[0]);this.contracts.set(poolAddress,pool);
   const identity=await this.identify(pool,this.artifact('ManagedPoolFactory','ManagedPool'),creation,null);assert.equal(identity.runtime_bytes,24561);
   this.pools.push({name,pool:poolAddress,pool_id:(await this.call(pool,'getPoolId'))[0],params:clean(params),settings:clean(settings),creation:creation.id,initialize:null,requested_protocol:index===1&&this.mode==='mixed'?'balancer-v2-managed-aum':'balancer-v2-managed-scheduled'});
  }
  for(let i=0;i<3;i++)for(const [signer,who,amount]of [[2,this.roles.lp,20000n*10n**BigInt(this.decimals[i])],[3,this.roles.trader,10000n*10n**BigInt(this.decimals[i])]]){
   await this.send('mint-shared-'+i+'-'+signer,this.tokens[i],'mint',[who,String(amount)]);await this.send('approve-shared-'+i+'-'+signer,this.tokens[i],'approve',[this.vault.address,MAX],signer);
  }
  const initial=this.decimals.map(d=>String(10000n*10n**BigInt(d)));
  for(const p of this.pools){const tx=await this.send('initialize-shared-'+p.name,this.vault,'joinPool',[p.pool_id,this.roles.lp,this.roles.lp,{assets:[p.pool,...this.tokenAddresses],maxAmountsIn:[MAX,...initial],fromInternalBalance:false,userData:ethers.utils.defaultAbiCoder.encode(['uint256','uint256[]'],[0,initial])}],2);p.initialize=tx.id;}
 }
 mapping(token){return ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['address','uint256'],[token,11]));}
 async poolState(p,h){
  const pool=this.contracts.get(p.pool),getters=[],C=(c,m,a=[])=>this.call(c,m,a,h,getters);
  const weights=(await C(pool,'getNormalizedWeights'))[0],fee=(await C(pool,'getSwapFeePercentage'))[0],weightSchedule=await C(pool,'getGradualWeightUpdateParams'),feeSchedule=await C(pool,'getGradualSwapFeeUpdateParams'),registration=await C(this.vault,'getPoolTokens',[p.pool_id]);
  const poolWord=await this.rpc('eth_getStorageAt',[p.pool,'0x9',h.number]),aumWord=await this.rpc('eth_getStorageAt',[p.pool,'0xa',h.number]);
  const total=(await C(pool,'totalSupply'))[0],actual=(await C(pool,'getActualSupply'))[0],bpt=await C(this.vault,'getPoolTokenInfo',[p.pool_id,p.pool]),virtual=BigInt(total)-BigInt(bpt[0])-BigInt(bpt[1]);
  const tokenStates=[];
  for(let i=0;i<3;i++){
   const token=this.tokenAddresses[i],key=this.mapping(token),word=await this.rpc('eth_getStorageAt',[p.pool,key,h.number]),info=await C(this.vault,'getPoolTokenInfo',[p.pool_id,token]),wallet={};
   for(const who of [this.roles.lp,this.roles.trader,lower(this.vault.address)])wallet[who]=(await C(this.tokens[i],'balanceOf',[who]))[0];
   tokenStates.push({token,index:i,decimals:(await C(this.tokens[i],'decimals'))[0],mapping_slot:11,storage_key:key,raw_word:word,info,wallet_balances:wallet,trader_allowance:(await C(this.tokens[i],'allowance',[this.roles.trader,this.vault.address]))[0]});
  }
  const aum={params:await C(pool,'getManagementAumFeeParams'),raw_word:aumWord,total_supply:total,actual_supply:actual,virtual_supply:String(virtual),pending_aum:String(BigInt(actual)-virtual),protocol_share:(await C(pool,'getProtocolFeePercentageCache',[3]))[0],provider_percentage:(await C(this.feeProvider,'getFeeTypePercentage',[3]))[0],provider_id:(await C(pool,'getProviderFeeId',[3]))[0],bpt_info:bpt,manager_bpt:(await C(pool,'balanceOf',[this.roles.manager]))[0],collector_bpt:(await C(pool,'balanceOf',[(await C(this.vault,'getProtocolFeesCollector'))[0]]))[0]};
  await H.captureServiceGetters(C,{pool,vault:this.vault,factory:this.factory,feeProvider:this.feeProvider,tokens:this.tokens,contracts:this.contracts},getters);
  const scales=(await C(pool,'getScalingFactors'))[0],owner=(await C(pool,'getOwner'))[0],anchor=await this.header(h.number);
  return {header:h,header_anchor_after:anchor,source_h:h.number,source_clock:num(h.timestamp),getters,weights,sum_weights:String(weights.reduce((sum,w)=>sum+BigInt(w),0n)),fee,weight_schedule:weightSchedule,fee_schedule:feeSchedule,packed:{pool_word:poolWord,tokens:tokenStates.map(t=>({token:t.token,key:t.storage_key,word:t.raw_word}))},registration,token_states:tokenStates,aum,scales,owner};
 }
 async capture(label){
  const h=await this.header(),id=this.mode+'/B'+this.banks.length,pools=[];
  for(const p of this.pools)pools.push({name:p.name,state:await this.poolState(p,h)});
  const globalGetters=[],globalWallets=[];
  for(const token of this.tokens){const balances={};for(const who of [...new Set([...Object.values(this.roles),lower(this.vault.address),...this.pools.map(p=>p.pool)])])balances[who]=(await this.call(token,'balanceOf',[who],h,globalGetters))[0];globalWallets.push({token:lower(token.address),total_supply:(await this.call(token,'totalSupply',[],h,globalGetters))[0],balances});}
  const extra=this.original.reference.factory_code_storage.contracts.map(c=>c.address),mappings=this.pools.flatMap(p=>this.tokenAddresses.map(t=>({address:p.pool,key:this.mapping(t)})));
  const evidence=await H.captureEvidence(this.rpc,h,[...this.contracts.keys(),...Object.values(this.roles),...extra],this.runtime,mappings);
  eq(evidence.header_anchor_after,h,'common source H after all account/storage reads');
  for(const w of globalWallets){const cash=pools.reduce((sum,p)=>sum+BigInt(p.state.token_states.find(t=>t.token===w.token).info[0]),0n);assert(BigInt(w.balances[lower(this.vault.address)])>=cash,'full shared wallet backs all declared registrations');}
  const bank={id,namespace:this.mode,label,header:h,header_anchor_after:evidence.header_anchor_after,pools,global_wallets:globalWallets,global_wallet_getters:globalGetters,accounts:evidence.accounts,storage:evidence.storage};this.banks.push(bank);return bank;
 }
 async clock(timestamp,label){const pre=await this.header();assert(timestamp>=num(pre.timestamp)&&timestamp<1769904000);await this.rpc('evm_setTime',[timestamp*1000]);await this.rpc('evm_mine');const h=await this.header();assert.equal(num(h.timestamp),timestamp);this.clockOps.push({timestamp,label,pre_header:pre,header:h});}
 branch(){return {mode:this.mode,chain_options:this.original.reference.chain_options,roles:this.roles,factory:lower(this.factory.address),vault:lower(this.vault.address),fee_provider:lower(this.feeProvider.address),tokens:this.tokenAddresses,decimals:this.decimals,pools:this.pools,prefix_identities:this.prefixIDs,new_identities:this.newIDs,native_runtime:this.runtime,storage_layout:this.layout,encoding:encoding(this.builds),prefix_replay:{count:this.prefix.length,genesis:this.oldHeaders.get(0),transactions:this.prefix,headers:[...this.oldHeaders].filter(([n])=>n<=28).sort(([a],[b])=>a-b).map(([,h])=>h)},transactions:this.transactions,bank_ids:this.banks.map(b=>b.id),clock_operations:this.clockOps};}
 async close(){try{await this.raw.disconnect();}finally{fs.rmSync(this.db,{recursive:true,force:true});}}
}
module.exports={Harness,clean,encoding,eq,num,MAX};
