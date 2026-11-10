'use strict';
const assert=require('node:assert/strict'),fs=require('node:fs');
const {ROOT,E,A,V,sha,closure,sourceManifest}=require('./inputs.cjs');
const {ethers}=E.rr('ethers');
const {index,provenance,PROFILE}=require('./generate.cjs');
const eq=(a,b,m)=>assert.equal(V.canonical(a),V.canonical(b),m),lower=s=>s.toLowerCase();
const field=(w,o,n)=>(BigInt(w)>>BigInt(o))&((1n<<BigInt(n))-1n);
function clean(x){if(ethers.BigNumber.isBigNumber(x))return x.toString();if(Array.isArray(x))return x.map(clean);if(typeof x==='string'&&x.startsWith('0x'))return lower(x);return x;}
function header(h){
 const keys=['parentHash','sha3Uncles','miner','stateRoot','transactionsRoot','receiptsRoot','logsBloom','difficulty','number','gasLimit','gasUsed','timestamp','extraData','mixHash','nonce','baseFeePerGas','withdrawalsRoot'].filter(k=>h[k]!==undefined),numeric=new Set(['difficulty','number','gasLimit','gasUsed','timestamp','baseFeePerGas']);
 const q=x=>{const n=BigInt(x),h=n.toString(16);return n===0n?'0x':'0x'+(h.length%2?'0':'')+h;};
 assert.equal(ethers.utils.keccak256(ethers.utils.RLP.encode(keys.map(k=>numeric.has(k)?q(h[k]):h[k]))),h.hash,'original header hash');
 assert(BigInt(h.timestamp)>=1767225600n&&BigInt(h.timestamp)<1769904000n,'Jan2026 clock');
}
function transaction(t,abis){
 const p=ethers.utils.parseTransaction(t.signed_transaction),tx=t.transaction,r=t.receipt,h=t.execution_header,pre=t.pre_header;
 header(h);header(pre);
 assert.equal(ethers.utils.keccak256(t.signed_transaction),p.hash,'signed hash');
 eq([p.hash,p.from.toLowerCase(),(p.to||'').toLowerCase(),p.nonce,p.chainId,p.gasLimit.toString(),p.gasPrice.toString(),p.value.toString(),p.data.toLowerCase()],
 [tx.hash,tx.from,(tx.to||''),Number(BigInt(tx.nonce)),31337,BigInt(tx.gas).toString(),BigInt(tx.gasPrice).toString(),BigInt(tx.value).toString(),tx.input],'signed transaction complete fields');
 assert.equal(r.transactionHash,p.hash);assert.equal(r.from,tx.from);assert.equal(r.to,tx.to);assert.equal(r.blockHash,h.hash);assert.equal(tx.blockHash,h.hash);assert.equal(r.blockNumber,h.number);assert.equal(tx.blockNumber,h.number);
 assert.equal(h.parentHash,pre.hash);assert.equal(BigInt(h.number),BigInt(pre.number)+1n);assert.equal(h.timestamp,pre.timestamp);eq(h.transactions,[p.hash],'header transaction membership');
 assert(['0x0','0x1'].includes(r.status));
 if(tx.to){const abi=abis.get(tx.to);assert(abi,'known transaction original ABI');assert.equal(lower(abi.encodeFunctionData(t.method,t.args)),tx.input,'canonical signed method args');}
 for(const l of r.logs){assert.equal(l.blockHash,h.hash);assert.equal(l.blockNumber,h.number);assert.equal(l.transactionHash,p.hash);assert.equal(l.transactionIndex,r.transactionIndex);}
 return p;
}
// Resolve immutable names from the original compiler AST rather than trusting
// analyst-supplied immutable labels/spans. Reuse layouts across state records.
const immutableLayouts=new WeakMap();
function immutableWords(builds,contract,code){
 const buildName=contract==='Vault'?'Vault':'ManagedPoolFactory',build=builds[buildName];let cache=immutableLayouts.get(build);
 if(!cache){cache=new Map();immutableLayouts.set(build,cache);}
 let layout=cache.get(contract);
 if(!layout){
  const variables=new Map(Object.values(build.output.sources).flatMap(s=>s.ast.nodes.filter(n=>n.nodeType==='ContractDefinition').flatMap(n=>n.nodes)).filter(n=>n.nodeType==='VariableDeclaration').map(n=>[String(n.id),n.name]));
  const template=A.locate(builds,buildName,contract).c.evm.deployedBytecode;
  layout=Object.entries(template.immutableReferences||{}).map(([id,spans])=>{assert(variables.has(id),'original immutable AST name');return {name:variables.get(id),spans};});cache.set(contract,layout);
 }
 assert(typeof code==='string'&&/^0x(?:[0-9a-f]{2})*$/.test(code),'canonical original runtime bytes');
 const words=new Map();
 for(const row of layout)for(const span of row.spans){assert.equal(span.length,32,'original immutable word width');assert(2+(span.start+span.length)*2<=code.length,'original immutable span');const word='0x'+code.slice(2+span.start*2,2+(span.start+span.length)*2);if(words.has(row.name))assert.equal(words.get(row.name),word,'duplicate original immutable context');words.set(row.name,word);}
 return words;
}
// Qualification must retain every ordinary source dependency, including
// factory bindings that the focused schedule prototypes did not consume.
function verifyServiceSource(b,two,s,prefixIDs,original,builds){
 const known=new Map(prefixIDs.map(i=>[i.label,i.address]));
 const pool=two?b.pool:b.configuration.pool,vault=two?b.vault:b.configuration.vault;
 const tokens=two?b.tokens:b.configuration.tokens,roles=two?b.roles:b.configuration.roles;
 const factory=known.get('ManagedPoolFactory'),provider=known.get('ProtocolFeePercentagesProvider');
 const collector=known.get('ProtocolFeesCollector'),recovery=known.get('RecoveryModeHelper');
 const normalized=args=>args.map(x=>typeof x==='number'?String(x):x);
 function read(to,method,args=[]){
  const rows=s.getters.filter(g=>g.to===to&&g.method===method&&V.canonical(normalized(g.args))===V.canonical(normalized(args)));
  assert(rows.length,'required original service getter '+method);return rows[0].decoded;
 }
 const one=(to,method,args=[])=>{const v=read(to,method,args);assert.equal(v.length,1,'service getter arity');return v[0];};
 assert.equal(one(pool,'getVault'),vault,'original Pool Vault');
 assert.equal(one(pool,'getOwner'),roles.manager,'original manager');
 assert.equal(one(pool,'getProtocolFeesCollector'),collector,'original collector');
 const pid=one(pool,'getPoolId');
 eq(read(vault,'getPool',[pid]),[pool,1],'original Vault specialization');
 for(const method of ['getPausedState','inRecoveryMode','getSwapEnabled','getMustAllowlistLPs','getJoinExitEnabled',
   'getManagementAumFeeParams','getNormalizedWeights','getScalingFactors','getActualSupply','getSwapFeePercentage',
   'getGradualSwapFeeUpdateParams','getGradualWeightUpdateParams','name','totalSupply'])read(pool,method);
 for(const kind of [0,2])assert.equal(one(pool,'getProviderFeeId',[kind]),String(kind),'original constructor fixed swap/yield provider fee IDs');
 read(pool,'getProviderFeeId',[3]);
 read(pool,'getProtocolFeePercentageCache',[3]);
 for(const address of [factory,provider,recovery])assert.equal(one(address,'getVault'),vault,'dependency Vault binding');
 assert.equal(one(collector,'vault'),vault,'collector Vault binding');
 assert.equal(one(factory,'getProtocolFeePercentagesProvider'),provider,'factory provider binding');
 assert.equal(one(factory,'getWeightedMath'),known.get('ExternalWeightedMath'),'factory math binding');
 assert.equal(one(factory,'getRecoveryModeHelper'),recovery,'factory recovery binding');
 assert.equal(one(factory,'isPoolFromFactory',[pool]),true,'factory membership');
 const originalGetter=original.reference.factory_code_storage.getters.map(r=>V.resolveObject(original.objects,r.ref,'getter')).find(g=>g.method==='getCreationCodeContracts');
 assert(originalGetter,'pinned original factory split-code getter');
 const factoryABI=new ethers.utils.Interface(A.locate(builds,'ManagedPoolFactory','ManagedPoolFactory').c.abi);
 eq(read(factory,'getCreationCodeContracts'),clean(factoryABI.decodeFunctionResult('getCreationCodeContracts',originalGetter.result)),'factory original creation-code addresses');
 read(vault,'getPoolTokens',[pid]);read(vault,'getPoolTokenInfo',[pid,pool]);
 read(vault,'getPausedState');assert.equal(one(vault,'WETH'),known.get('WETH'),'Vault original WETH binding');
 assert.equal(one(vault,'getProtocolFeesCollector'),collector,'Vault collector binding');
 const id=one(pool,'getProviderFeeId',[3]);
 read(provider,'getFeeTypePercentage',[id]);read(provider,'getFeeTypeMaximumPercentage',[id]);
 for(const who of [vault,roles.manager,collector])read(pool,'balanceOf',[who]);
 // Code, getters and constructor/factory contexts must agree at every H.
 const runtime=two?b.native_code:b.native_runtime;
 const sourceCode=address=>{const account=s.accounts.find(a=>a.address===address);assert(account,'immutable source account');const code=runtime[account.code_sha256];assert(code!==undefined,'immutable source runtime');return code;};
 const uint=value=>ethers.utils.hexZeroPad(ethers.BigNumber.from(value).toHexString(),32).toLowerCase();
 const factoryWords=immutableWords(builds,'ManagedPoolFactory',sourceCode(factory));
 const end=BigInt(factoryWords.get('_poolsPauseWindowEndTime')),duration=BigInt(factoryWords.get('_bufferPeriodDuration')),now=BigInt(s.header.timestamp);
 eq(read(factory,'getPauseConfiguration'),[String(end>now?end-now:0n),String(end>now?duration:0n)],'original factory remaining pause context');
 const paused=read(pool,'getPausedState'),poolEnd=BigInt(paused[1]),poolBuffer=BigInt(paused[2]);
 if(end>now){assert.equal(poolEnd,end,'factory-bound pool pause end');assert.equal(poolBuffer,end+duration,'factory-bound pool buffer end');}else assert(poolEnd<=now&&poolBuffer>=poolEnd,'closed factory pool pause context');
 const vaultPaused=read(vault,'getPausedState'),vaultWords=immutableWords(builds,'Vault',sourceCode(vault));
 assert.equal(vaultWords.get('_pauseWindowEndTime'),uint(vaultPaused[1]),'Vault original pause immutable');
 assert.equal(vaultWords.get('_bufferPeriodEndTime'),uint(vaultPaused[2]),'Vault original buffer immutable');
 assert.equal(vaultWords.get('_weth'),uint(one(vault,'WETH')),'Vault original WETH immutable');
 let feeIDs=0n;for(const [kind,shift]of [[0,0n],[2,8n],[3,16n]]){const value=BigInt(one(pool,'getProviderFeeId',[kind]));assert(value>=0n&&value<=255n,'original provider fee id width');feeIDs|=value<<shift;}
 const name=one(pool,'name');if(!two){assert.equal(name,b.configuration.params.name,'signed factory pool name');assert.equal(String(id),String(b.configuration.settings.aumFeeId),'signed factory AUM fee id');assert.equal(pid,b.configuration.pool_id,'original registered pool id');}
 const bindings={_vault:uint(vault),_owner:uint(roles.manager),_poolId:pid,_protocolFeesCollector:uint(collector),
  _protocolFeeProvider:uint(provider),_feeIds:uint(feeIDs.toString()),_actionIdDisambiguator:uint(factory),
  _pauseWindowEndTime:uint(paused[1]),_bufferPeriodEndTime:uint(paused[2]),_HASHED_NAME:ethers.utils.id(name),
  _HASHED_VERSION:ethers.utils.id('1'),_TYPE_HASH:ethers.utils.id('EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)'),
  _weightedMath:uint(known.get('ExternalWeightedMath')),_recoveryModeHelper:uint(recovery)};
 for(const [name,word]of immutableWords(builds,'ManagedPool',sourceCode(pool))){assert(Object.hasOwn(bindings,name),'unbound original Pool immutable '+name);assert.equal(word,bindings[name],'original Pool immutable/getter/constructor binding '+name);}
 for(const token of tokens){read(token,'decimals');read(token,'balanceOf',[vault]);
  read(vault,'getPoolTokenInfo',[pid,token]);eq(read(pool,'getCircuitBreakerState',[token]),['0','0','0','0','0','0'],'disabled original breaker');}
}
function verifyBranch(b,namespace,original,builds){
 const two=namespace==='two',pool=two?b.pool:b.configuration.pool,tokens=two?b.tokens:b.configuration.tokens,roles=two?b.roles:b.configuration.roles;
 const ids=two?b.identities:[...b.prefix_identities,...b.new_identities],abis=new Map(ids.map(i=>[i.address,new ethers.utils.Interface(A.locate(builds,i.artifact.build,i.artifact.name).c.abi)]));
 assert.equal(new Set(ids.map(i=>i.address)).size,ids.length,'native identities may not alias or shadow pinned addresses');
 if(!two)eq(b.new_identities.map(i=>i.address).sort(),[pool,...tokens].sort(),'only original created Pool and token identities may extend prefix');
 const poolABI=new ethers.utils.Interface(A.locate(builds,'ManagedPoolFactory','ManagedPool').c.abi);abis.set(pool,poolABI);
 eq(b.chain_options,original.reference.chain_options,'complete original chain options');
 const R0=(r,k)=>V.resolveObject(original.objects,r.ref,k),prefixIDs=original.reference.identities.map(r=>R0(r,'identity')).filter(i=>BigInt(R0(R0(i.creation_transaction,'transaction').execution_header,'header').number)<=28n);
 const originalHeaders=Object.values(original.objects).filter(o=>o.kind==='header'&&BigInt(o.value.number)<=28n).map(o=>o.value).sort((a,c)=>Number(BigInt(a.number)-BigInt(c.number)));
 eq(b.prefix_replay.headers,originalHeaders,'complete original prefix header inventory');eq(b.prefix_replay.genesis,originalHeaders[0],'original prefix genesis');
 if(two){
  eq(b.original_bank,{path:'integration/test/contracts/managed-v2-aum.json',sha256:sha(fs.readFileSync(ROOT+'/integration/test/contracts/managed-v2-aum.json')),bytes:7905624},'complete original bank metadata');
  eq(b.self_source,['generate.cjs','checker.cjs'].map(p=>({path:p,sha256:sha(fs.readFileSync(__dirname+'/two/'+p))})),'complete two source inventory');
  eq(b.identities.map(i=>[i.label,i.address,i.artifact]),prefixIDs.map(i=>[i.label,i.address,i.artifact]),'original identities');
  for(const i of b.identities){const originalID=prefixIDs.find(j=>j.address===i.address),code=R0(originalID.code,'code').hex;assert.equal(i.runtime_sha256,sha(Buffer.from(code.slice(2),'hex')));assert.equal(i.runtime_bytes,(code.length-2)/2);assert.equal(b.native_code[i.runtime_sha256],code,'exact original runtime bytes');}
 }else eq(b.prefix_identities,prefixIDs,'complete original prefix identities');
 const source=two?b.original_inputs:b.inputs;
 eq(source.source_manifest,sourceManifest(),'complete fixed source inventory');
 const pins=A.load().pins;
 if(two){
  const expected=[...E.verifyInputs().files.map(f=>f.path),'integration/tools/reference/balancer-managed-v2-aum/file-manifest.json','integration/test/contracts/managed-v2-aum.json'].sort().map(p=>{const bytes=fs.readFileSync(ROOT+'/'+p);return {path:p,bytes:bytes.length,sha256:sha(bytes)};});
  eq(source.before,expected,'original immutable before closure');eq(source.after,expected,'original immutable after closure');eq(source.copied,require('./inputs.cjs').inputClosure(),'original runtime file closure');eq(source.node_closure,E.nodeClosure(),'original Node closure');eq(source.file_manifest_sha256,E.verifyInputs().file_manifest_sha256,'original manifest SHA');eq(source.compiler_jobs,original.reference.compiler_jobs,'original compiler jobs');
 }else{eq(source.input_pins,E.verifyInputs(),'branch original input pins');eq(source.node_pins,E.nodeClosure(),'branch original Node pins');eq(source.original_compiler_jobs,original.reference.compiler_jobs,'branch compiler jobs');}
 eq(source.source_pins,pins,'original source pins');
 const astLayout=require('./layout.cjs').deriveLayout(builds.ManagedPoolFactory,'ManagedPool'),packedLayout=require('./layout.cjs').packedLayout(builds.ManagedPoolFactory);
 eq(b.storage_layout.variables,astLayout.variables,'original AST layout');eq(b.storage_layout.immutables,astLayout.immutables,'original immutable AST layout');
 if(!two){
  eq(b.storage_layout,astLayout,'complete layout metadata');
  const c=packedLayout.pool.constants,t=packedLayout.token.constants;
  eq(b.encoding,{pool_slot:9,aum_slot:10,token_mapping_slot:11,start_weight:{offset:t._START_NORM_WEIGHT_OFFSET,bits:t._NORM_WEIGHT_WIDTH},end_weight:{offset:t._END_NORM_WEIGHT_OFFSET,bits:t._NORM_WEIGHT_WIDTH},decimal_diff:{offset:t._DECIMAL_DIFF_OFFSET,bits:t._DECIMAL_DIFF_WIDTH},fee_start:{offset:c._SWAP_FEE_START_PCT_OFFSET,bits:c._SWAP_FEE_PCT_WIDTH},fee_end:{offset:c._SWAP_FEE_END_PCT_OFFSET,bits:c._SWAP_FEE_PCT_WIDTH},fee61_bit_is_value:true,fee_bit190_is_end_bit0:true,compression:'none in exact original source'},'encoding from original input source and AST');
  const config=b.configuration,create=b.transactions.find(t=>t.id===config.creation),factoryABI=abis.get(config.factory),decoded=factoryABI.decodeFunctionData('create',create.transaction.input);
  assert.equal(lower(factoryABI.encodeFunctionData('create',[config.params,config.settings,roles.manager,decoded[3]])),create.transaction.input,'full original factory params/settings/owner');
 }else{assert.equal(b.storage_layout.compiler_storageLayout_present,false);assert.equal(b.storage_layout.getState_public_ABI_present,false);}

 const runtime=two?b.native_code:b.native_runtime;
 for(const [digest,code]of Object.entries(runtime)){assert(typeof code==='string'&&/^0x(?:[0-9a-f]{2})*$/.test(code),'canonical runtime hex before hashing');assert.equal(sha(Buffer.from(code.slice(2),'hex')),digest,'actual sourceCode digest');}
 const authenticatedRuntime=new Map(prefixIDs.map(i=>[i.address,sha(Buffer.from(R0(i.code,'code').hex.slice(2),'hex'))]));
 for(const i of b.new_identities||[])authenticatedRuntime.set(i.address,i.runtime_sha256);
 for(const part of original.reference.factory_code_storage.contracts)authenticatedRuntime.set(part.address,sha(Buffer.from(R0(part.code,'code').hex.slice(2),'hex')));
 const states=new Map();
 const requiredAccounts=[...new Set([...ids.map(i=>i.address),...Object.values(roles),...original.reference.factory_code_storage.contracts.map(c=>c.address)])].sort();
 for(const s of b.states){
  eq(s.accounts.map(a=>a.address),requiredAccounts,'complete observed account set');
  assert(!states.has(s.id),'state ID duplicate within namespace');states.set(s.id,s);header(s.header);eq(s.header_anchor_after,s.header,'full blockTag source reanchor');
  if(!two)assert.equal(s.source_h,s.header.number,'sourceH');
  const weights=two?s.schedule.weights:s.weight_schedule,fees=two?s.schedule.fee:s.fee_schedule;
  const getterKeys=new Map();
  for(const g of s.getters){
   const abi=abis.get(g.to);assert(abi,'known getter original ABI');assert(!['getState'].includes(g.method),'no fabricated getState');
   const key=V.canonical([g.to,g.method,g.args]);if(getterKeys.has(key))eq(g,getterKeys.get(key),'repeated getter same sourceH and bytes');getterKeys.set(key,g);
   assert.equal(lower(abi.encodeFunctionData(g.method,g.args)),g.data,'canonical getter method/args');const decoded=clean(abi.decodeFunctionResult(g.method,g.result));eq(decoded,g.decoded,'canonical getter result');
   assert.equal(lower(abi.encodeFunctionResult(g.method,decoded)),g.result,'getter result canonical bytes');
   assert.equal(g.block_number,s.header.number,'getter source blockTag');assert.equal(g.block_hash,s.header.hash,'getter source hash');
  }
  verifyServiceSource(b,two,s,prefixIDs,original,builds);
  const get=(method)=>{const g=s.getters.find(g=>g.to===pool&&g.method===method);assert(g,'required original pool getter '+method);return g.decoded;};
  eq(get('getNormalizedWeights')[0],two?s.current_weights:s.weights,'current weight original getter');eq(get('getSwapFeePercentage')[0],two?s.current_fee:s.fee,'current fee original getter');
  if(two){
   const pid=get('getPoolId')[0],read=(method,args)=>{const g=s.getters.find(g=>g.to===b.vault&&g.method===method&&V.canonical(g.args)===V.canonical(args));assert(g,'required original Vault getter '+method);return g.decoded;},registration=read('getPoolTokens',[pid]);
   eq([...registration[0]].sort(),[pool,...tokens].sort(),'original Vault registration membership');
   for(const token of tokens){const info=read('getPoolTokenInfo',[pid,token]),i=registration[0].indexOf(token);assert.equal(BigInt(registration[1][i]),BigInt(info[0])+BigInt(info[1]),'Vault raw registration cash+managed');}
  }
  const pw=s.storage.find(x=>x.address===pool&&x.key==='0x9');assert(pw,'originStorage poolslot9');eq(pw.value,s.packed.pool_word,'poolword originStorage');
  for(const [bit,method]of [[252,'getSwapEnabled'],[253,'getMustAllowlistLPs'],[254,'inRecoveryMode'],[255,'getJoinExitEnabled']]){
   const g=s.getters.find(g=>g.to===pool&&g.method===method);assert(g,'original flag getter '+method);assert.equal(Boolean(field(pw.value,bit,1)),g.decoded[0],'packed flags252..255');
  }
  for(let i=0;i<tokens.length;i++){
   const key=ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['address','uint256'],[tokens[i],11])),row=s.storage.find(x=>x.address===pool&&x.key===key);assert(row,'originStorage tokenmap11');
   const packed=two?s.packed.tokens[i].raw_word:s.packed.tokens[i].word;assert.equal(row.value,packed,'tokenWord originStorage');
   assert.equal(field(packed,133,123),0n,'unused reserved token bits133..255');
   eq([String(field(packed,0,64)),String(field(packed,64,64))],[weights[2][i],weights[3][i]],'weight64 endpoints');
   const dg=s.getters.find(g=>g.to===tokens[i]&&g.method==='decimals');assert(dg,'original decimals getter');assert.equal(Number(field(packed,128,5)),18-dg.decoded[0],'decimal5');
  }
  eq([String(field(pw.value,0,32)),String(field(pw.value,32,32))],weights.slice(0,2),'weight clock fields');eq([64,96,128,190].map((o,i)=>String(field(pw.value,o,i<2?32:62))),fees,'direct fee62 fields');
  for(const a of s.accounts){assert.equal(a.code_sha256,authenticatedRuntime.get(a.address)||sha(Buffer.alloc(0)),'observed account runtime bound to authenticated original identity');const code=runtime[a.code_sha256];assert(code!==undefined,'state sourceCode reference');assert.equal((code.length-2)/2,a.runtime_bytes);if(a.address===pool)assert.equal(a.runtime_bytes,24561,'normal EIP170 24561');}
 }
 if(two){
  const resolveExecution=t=>{const stored=b.transactions.find(x=>x.transaction.hash===t.transaction.hash&&x.execution_header.hash===t.execution_header.hash&&x.purpose===t.purpose);assert(stored,'case/management transaction occurrence');eq(t,stored,'full transaction reference across atomic records');return stored;};
  const read=(s,to,method,args)=>{const g=s.getters.find(g=>g.to===to&&g.method===method&&V.canonical(g.args)===V.canonical(args));assert(g,'swap state raw getter '+method);return g.decoded;};
  for(const c of b.cases){
   const t=resolveExecution(c.execution),before=states.get(c.before),after=states.get(c.after);eq(before.header,t.pre_header,'swap source header');eq(after.header,t.execution_header,'swap after header');
   const pid=read(before,pool,'getPoolId',[])[0],input=BigInt(c.amount_in),output=BigInt(c.expected_output);
   for(const token of tokens){
    const delta=token===c.token_in?input:-output,a=read(before,b.vault,'getPoolTokenInfo',[pid,token]),z=read(after,b.vault,'getPoolTokenInfo',[pid,token]);
    assert.equal(BigInt(z[0])-BigInt(a[0]),delta,'original Vault cash swap delta');eq(z[1],a[1],'swap managed balance unchanged');eq(z[3],a[3],'swap asset manager unchanged');
    assert.equal(BigInt(read(after,token,'balanceOf',[b.vault])[0])-BigInt(read(before,token,'balanceOf',[b.vault])[0]),delta,'original Vault ERC20 wallet swap delta');
    eq(read(after,token,'totalSupply',[]),read(before,token,'totalSupply',[]),'swap underlying total supply unchanged');
    for(const g of before.getters.filter(g=>g.to===token&&g.method==='balanceOf')){
     const holder=g.args[0],expected=holder===b.vault?delta:holder===roles.trader?-delta:0n;
     assert.equal(BigInt(read(after,token,'balanceOf',g.args)[0])-BigInt(g.decoded[0]),expected,'all observed underlying wallets close across swap');
    }
   }
  }
  for(const sc of b.scenarios)for(const m of sc.management_transactions){const t=resolveExecution(m.execution);eq(states.get(m.before).header,t.pre_header,'manager source header');eq(states.get(m.after).header,t.execution_header,'manager after header');}
 }
 const txs=b.transactions;
 const unique=new Map();
 for(const t of txs){
  const p=transaction(t,abis);const identity=t.execution_header.hash+'/'+p.hash;if(unique.has(identity))for(const k of ['signed_transaction','transaction','receipt','pre_header','execution_header'])eq(t[k],unique.get(identity)[k],'repeated snapshot signed tx '+k);unique.set(identity,t);
  const before=b.states.find(s=>s.header.hash===t.pre_header.hash),after=b.states.find(s=>s.header.hash===t.execution_header.hash);
  if(before&&after){
   const a=before.accounts.find(a=>a.address===lower(p.from)),z=after.accounts.find(a=>a.address===lower(p.from));assert(a&&z,'sender account closure');
   assert.equal(BigInt(a.nonce),BigInt(p.nonce),'source signed nonce');assert.equal(BigInt(z.nonce),BigInt(a.nonce)+1n,'after sender nonce');
   const fee=BigInt(t.receipt.gasUsed)*BigInt(t.receipt.effectiveGasPrice),tip=BigInt(t.receipt.gasUsed)*(BigInt(t.receipt.effectiveGasPrice)-BigInt(t.execution_header.baseFeePerGas)),miner=t.execution_header.miner;
   assert.equal(BigInt(a.native_balance)-BigInt(z.native_balance),fee+BigInt(p.value.toString())-(a.address===miner?tip:0n),'sender native gas/value/coinbase delta');
   for(const acct of before.accounts){const next=after.accounts.find(a=>a.address===acct.address);assert(next,'account set stable');assert.equal(acct.code_sha256,next.code_sha256,'runtime unchanged after call');if(acct.address!==lower(p.from)){assert.equal(acct.nonce,next.nonce,'other account nonce');assert.equal(BigInt(next.native_balance)-BigInt(acct.native_balance),acct.address===miner?tip:0n,'other native balance including coinbase tip');}}
   if(t.method==='updateWeightsGradually'&&t.receipt.status==='0x1')eq(two?before.current_weights:before.weights,(two?after.schedule.weights:after.weight_schedule)[2],'management source getter == computed start');
   if(t.receipt.status==='0x0'){eq(before.storage,after.storage,'failed caller target complete observed storage unchanged');for(const acct of before.accounts)if(acct.address!==lower(p.from)&&acct.address!==miner)eq(acct,after.accounts.find(a=>a.address===acct.address),'failed target other accounts unchanged');}
  }
 }
 // Prefix signed bytes/receipts/headers are pinned to the original bank by value, not a verified flag.
 const R=(r,k)=>V.resolveObject(original.objects,r.ref,k);
 eq(b.prefix_replay.transactions,original.reference.transactions.map(r=>R(r,'transaction')).filter(t=>BigInt(R(t.execution_header,'header').number)<=28n),'exact signed prefix28 records');
 for(const t of b.prefix_replay.transactions){header(R(t.pre_header,'header'));header(R(t.execution_header,'header'));}
 return {states:states.size};
}
function check(b){
 assert.equal(b.schema_version,1);assert.equal(b.authority,'independent-local-original-managed-schedule');assert.equal(b.source_public_profile,PROFILE);
 eq(b.inputs,closure(),'full producer/checker/manifest/Node inputs');eq(b.provenance,provenance(),'original license provenance and public support scope');eq(Object.keys(b.branches),['two','three','reschedule'],'branch namespaces');
 const originalBytes=fs.readFileSync(ROOT+'/integration/test/contracts/managed-v2-aum.json');assert.equal(sha(originalBytes),b.inputs.original_bank.sha256);assert.equal(originalBytes.length,7905624);const original=require('./inputs.cjs').originalBank(),{builds}=A.load();
 require('./two/checker.cjs').check(b.branches.two);require('./three/checker.cjs').check(b.branches.three);require('./reschedule/checker.cjs').check(b.branches.reschedule,b.branches.three);
 for(const [name,v]of Object.entries(b.branches))verifyBranch(v,name,original,builds);
 const i=index(b.branches);for(const k of ['cases','checkpoints','management'])eq(b[k],i[k],'complete namespaced '+k);
 assert.equal(b.cases.length,53);assert.equal(new Set(b.cases.map(c=>c.id)).size,53);assert.equal(b.checkpoints.length,41);
 return {swaps:53,checkpoints:b.checkpoints.length,states:Object.fromEntries(Object.entries(b.branches).map(([n,v])=>[n,v.states.length])),management:Object.fromEntries(['two','three','reschedule'].map(n=>[n,b.management.filter(m=>m.namespace===n).map(m=>m.status)])),authority:b.authority,source_public_profile:b.source_public_profile};
}
module.exports={check,verifyBranch,header,transaction};
