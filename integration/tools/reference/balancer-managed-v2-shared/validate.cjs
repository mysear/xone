#!/usr/local/bin/node25.3.0
'use strict';
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const I=require('./inputs.cjs'),{ROOT}=I,REF=path.join(ROOT,'integration/tools/reference/balancer-managed-v2-aum');
const V=require(REF+'/validate.cjs'),E=require(REF+'/environment.cjs'),A=require(REF+'/artifacts.cjs');
const {ethers}=E.rr('ethers');
const lower=x=>x.toLowerCase(),num=x=>Number(BigInt(x)),same=(a,b,m)=>assert.equal(V.canonical(a),V.canonical(b),m);
const bankBytes=fs.readFileSync(ROOT+'/integration/test/contracts/managed-v2-aum.json'),bank=JSON.parse(bankBytes);
const resolve=(r,k)=>V.resolveObject(bank.objects,r.ref,k),{builds}=A.load();
function validate(c){
 const abis=new Map();
 I.verifyRuntime();
 same(c.original_reference.input_closure,E.verifyInputs(),'exact immutable input closure and manifest SHA');
 same(c.script_closure,I.scripts(),'exact formal script closure');
 same(c.source_preservation.before,I.preservation(),'exact source preservation inventory');
 same(c.original_reference.source_pins_sha256,bank.reference.source_pins_sha256,'original source pins');
 same(c.original_reference.compiler_jobs,bank.reference.compiler_jobs,'complete original compiler jobs');
 assert.equal(c.generator.collect_30d,true,'30d collection is mandatory');
 same(c.service_dates,['2026-01-01','2026-01-31']);assert.equal(c.catalog_date,'2026-01-31');
 assert.equal(c.generator.root,'.','portable logical repository root');
 assert.equal(c.generator.filename,'generate.cjs');assert.equal(c.generator.requested_node,'/usr/local/bin/node25.3.0');
 assert.equal(c.generator.actual_node,process.execPath);assert.equal(c.generator.requested_path_present,fs.existsSync('/usr/local/bin/node25.3.0'));
 same(c.counts,{pools:2,underlying_tokens:2,prefix_transactions:28,new_transactions:15,swap_cases:4,collection_cases:1,banks:7,raw_getters:1064,native_identities:15},'exact finite fixture counts');
 same(Object.keys(c.banks),['H38','H39','H40','H41','H42','H43','H44'],'complete common-H banks');
 assert.equal(c.initial_bank2pools,'H38');assert.equal(c.clock_bank2pools,'H43');assert.equal(c.final_bank2pools,'H44');
 assert.equal(c.transactions.length,15);assert.equal(c.cases.length,5);assert.equal(c.headers.length,17);
 same(c.cases.map(x=>[x.scenario,x.operation,x.source_bank2pools,x.expected_after_bank2pools]),[
  ['PoolA','swap','H38','H39'],['PoolB','swap','H39','H40'],['PoolA','swap','H40','H41'],['PoolB','swap','H41','H42'],['PoolA','collect','H43','H44']]);
 same(c.headers.map(h=>num(h.number)),Array.from({length:17},(_,i)=>i+28),'complete header sequence');
 same(c.transactions.map(t=>num(t.execution_header.number)),Array.from({length:16},(_,i)=>i+29).filter(n=>n!==43),'all native transactions');
 const headerByNumber=new Map(c.headers.map(h=>[h.number,h]));
 for(const [id,b] of Object.entries(c.banks)){
  assert.equal(b.id,id);assert.equal(id,'H'+num(b.header.number));same(b.header,headerByNumber.get(b.header.number),'bank header in branch');
  assert.equal(b.getter_count,152);same(b.pools.map(p=>p.scenario),['PoolA','PoolB'],'exact bank member scopes');
  const commonGetters=new Map();
  for(const p of b.pools)for(const g of p.getters){const key=V.canonical([g.to,g.method,g.args]);if(commonGetters.has(key))same(g,commonGetters.get(key),'shared getter observations at the same H');else commonGetters.set(key,g);} 
 }
 const activeOriginal=bank.reference.identities.map(r=>resolve(r,'identity')).filter(i=>num(resolve(resolve(i.creation_transaction,'transaction').execution_header,'header').number)<=28);
 same(c.identities.map(i=>i.address),[...activeOriginal.map(i=>i.address),c.scenarios[1].pool],'complete physical code identity inventory');
 for(const old of activeOriginal){const observed=c.identities.find(i=>i.address===old.address);assert.equal(observed.code_hex,resolve(old.code,'code').hex,'exact original active runtime');same(observed.artifact,old.artifact);}
 same(c.factory_creation_code_storage.map(x=>x.address),bank.reference.factory_code_storage.contracts.map(x=>x.address),'complete original Factory creation-code storage');
 assert.equal(c.code_checks.length,254);assert.equal(new Set(c.code_checks.map(x=>x.block_tag+':'+x.address)).size,254,'unique per-header code checks');
 assert.equal(c.factory_storage_step_checks.length,46,'complete per-step and bank storage checks');

 same(c.scenarios.map(s=>s.underlying_count),[2,2],'two physical two-token pools required');
 assert.equal(c.schema_version,1);assert.equal(c.public_adapter_version,'balancer-v2/20230411-managed-aum-3');
 assert.equal(c.timestamp_upper_bound_exclusive,I.CUTOFF);
 assert.equal(c.original_bank_sha256,V.sha(bankBytes));assert.equal(c.original_bank_bytes,bankBytes.length);
 same(c.chain_options,bank.reference.chain_options);same(c.original_reference.node_closure,bank.reference.node_closure);
 same(c.original_reference.input_closure,bank.reference.inputs);
 assert.equal(c.generator.sha256,V.sha(fs.readFileSync(path.join(__dirname,'generate.cjs'))));
 assert.equal(c.generator.actual_node_sha256,V.sha(fs.readFileSync(process.execPath)));
 assert.equal(process.version,'v25.3.0');assert.equal(c.generator.actual_node_sha256,'1f0504e1c9eebc309fd10aabedf3cfe1861b78871904a3b63c8ca1731d8ccbaf');
 assert.equal(c.original_bank_bytes,7905624);assert.equal(c.original_bank_sha256,'1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66');
 same(c.source_preservation.before,c.source_preservation.after);
 for(const f of c.source_preservation.before)assert.equal(f.sha256,V.sha(fs.readFileSync(path.join(ROOT,f.path))),'untouched source '+f.path);
 assert.equal(c.prefix_replay.count,28);assert.equal(c.prefix_replay.complete_transaction_receipt_header_equality,true);
 same(c.prefix_replay.transactions,bank.reference.transactions.slice(0,28).map(r=>resolve(r,'transaction')),'exact original signed prefix records');
 same(c.prefix_replay.headers,[...new Map(Object.values(bank.objects).filter(o=>o.kind==='header').map(o=>[num(o.value.number),o.value]))].filter(([n])=>n<=28).sort(([a],[b])=>a-b).map(([,h])=>h));
 const ids=new Map(c.identities.map(i=>[i.address,i]));
 for(const i of c.identities){
  const a=A.locate(builds,i.artifact.build,i.artifact.name);assert.equal(i.artifact.source,a.source);const d=a.c.evm.deployedBytecode,b=Buffer.from(i.code_hex.slice(2),'hex'),norm=Buffer.from(b),compiled=d.object.split('');
  abis.set(i.address,new ethers.utils.Interface(a.c.abi));
  assert.equal(i.runtime_bytes,b.length);assert(b.length>0&&b.length<=24576);assert.equal(i.code_sha256,V.sha(b));
  const clear=s=>{norm.fill(0,s.start,s.start+s.length);compiled.fill('0',s.start*2,(s.start+s.length)*2);};
  if(a.library){assert.equal('0x'+b.subarray(1,21).toString('hex'),i.address);clear({start:1,length:20});}
  const expectedLinks=[];
  for(const[source,libs]of Object.entries(d.linkReferences||{}))for(const[name,ss]of Object.entries(libs))for(const s of ss){const address='0x'+b.subarray(s.start,s.start+s.length).toString('hex');assert(c.identities.some(x=>x.address===address&&x.artifact.name===name),'linked original library');expectedLinks.push({source,name,...s,address});clear(s);}
  same(i.links,expectedLinks);
  same(i.immutables.map(x=>x.id).sort(),Object.keys(d.immutableReferences||{}).sort());
  for(const m of i.immutables){let declaration;function find(x){if(x&&typeof x==='object'){if(String(x.id)===m.id&&x.nodeType==='VariableDeclaration')declaration=x;for(const y of Object.values(x))find(y);}}find(builds[a.build].output.sources);assert(declaration);assert.equal(m.name,declaration.name);assert.equal(m.type,declaration.typeDescriptions.typeString);same(m.spans,d.immutableReferences[m.id]);for(const s of m.spans){assert.equal(m.value,'0x'+b.subarray(s.start,s.start+s.length).toString('hex'));clear(s);}}
  assert.equal(norm.toString('hex'),compiled.join(''),'original compiled template '+a.name);assert.equal(i.template_sha256,V.sha(norm));
 }
 for(const x of c.factory_creation_code_storage){const orig=bank.reference.factory_code_storage.contracts.find(y=>y.address===x.address);assert(orig);assert.equal(x.code_hex,resolve(orig.code,'code').hex);assert.equal(x.code_sha256,V.sha(Buffer.from(x.code_hex.slice(2),'hex')));const h=c.banks[c.initial_bank2pools].header;assert.equal(x.block_tag,h.number);assert.equal(x.block_hash,h.hash);}
 same(c.scenarios.map(s=>s.underlying_count),[2,2]);
 for(const tx of c.transactions){
  for(const i of c.identities){
   const created=c.scenarios.find(s=>s.pool===i.address)?.creation;
   if(created&&num(created.execution_header?.number||0)>num(tx.execution_header.number))continue;
   assert(c.code_checks.some(x=>x.address===i.address&&x.block_tag===tx.execution_header.number&&x.block_hash===tx.execution_header.hash&&x.code_sha256===i.code_sha256),'per-step code identity '+i.address);
  }
  for(const row of c.factory_creation_code_storage)assert(c.factory_storage_step_checks.some(x=>x.address===row.address&&x.block_tag===tx.execution_header.number&&x.code_sha256===row.code_sha256),'per-step Factory storage identity');
  const events=tx.receipt.logs.map(log=>{try{const e=abis.get(log.address).parseLog(log),args={};function val(x){return ethers.BigNumber.isBigNumber(x)?x.toString():Array.isArray(x)?x.map(val):typeof x==='string'?x.toLowerCase():x;}e.eventFragment.inputs.forEach((input,i)=>args[input.name]=val(e.args[i]));return {address:log.address,log_index:log.logIndex,name:e.name,signature:e.signature,args};}catch{return {address:log.address,log_index:log.logIndex,unparsed:true};}});
  same(tx.decoded_events,events,'decoded events equal signed transaction receipt logs');
 }
 const seen=new Set();
 for(const s of c.scenarios){
  assert(!seen.has(s.pool));seen.add(s.pool);assert.equal(s.tokens.length,s.underlying_count);
  const tokens=s.tokens.map(t=>t.address);same([...tokens].sort(),tokens);same(tokens,c.scenarios[0].tokens.map(t=>t.address),'same original underlying tokens');
  const createIface=abis.get(s.creation.transaction.to),args=createIface.decodeFunctionData('create',s.creation.transaction.input);
  assert.equal(lower(s.creation.transaction.input),lower(createIface.encodeFunctionData('create',[s.configuration.params,s.configuration.settings,s.configuration.owner,'0x'+'0'.repeat(64)])),'complete original create ABI');
  const recorded=[...c.prefix_replay.transactions,...c.transactions].find(t=>t.transaction.hash===s.creation.transaction.hash);assert(recorded);same(s.creation.transaction,recorded.transaction);same(s.creation.receipt,recorded.receipt);assert.equal(s.creation.signed_transaction,recorded.signed_transaction);
  const initRecord=[...c.prefix_replay.transactions,...c.transactions].find(t=>t.transaction.hash===s.initialization.transaction.hash);assert(initRecord);same(s.initialization.transaction,initRecord.transaction);same(s.initialization.receipt,initRecord.receipt);assert.equal(s.initialization.signed_transaction,initRecord.signed_transaction);
  const join=abis.get(s.initialization.transaction.to).decodeFunctionData('joinPool',s.initialization.transaction.input);assert.equal(lower(join[0]),s.pool_id);assert.equal(lower(join[1]),lower(join[2]));same(join[3].assets.map(lower),[s.pool,...tokens]);assert.equal(join[3].fromInternalBalance,false);const init=ethers.utils.defaultAbiCoder.decode(['uint256','uint256[]'],join[3].userData);assert.equal(init[0].toString(),'0');same(init[1].map(String),['10000000000000000000000','10000000000000000000000']);
  same(args[1].tokens.map(lower),tokens);assert.equal(args[1].aumFeeId.toString(),'3');assert.equal(args[1].managementAumFeePercentage.toString(),'10000000000000000');
  assert.equal(args[1].swapFeePercentage.toString(),'10000000000000000');
  const event=s.creation.receipt.logs.filter(l=>l.address===s.creation.transaction.to).map(l=>{try{return createIface.parseLog(l);}catch{return null;}}).find(e=>e?.name==='PoolCreated');
  assert(event);assert.equal(lower(event.args.pool),s.pool);
  assert.equal(num(s.initialization.receipt.status),1);assert.equal(s.initialization.transaction.from,lower(new ethers.Wallet('0x'+(3).toString(16).padStart(64,'0')).address));
 }
 for(const row of c.factory_storage_step_checks){const h=headerByNumber.get(row.block_tag),orig=c.factory_creation_code_storage.find(x=>x.address===row.address);assert(h&&orig);assert.equal(row.block_hash,h.hash);assert.equal(row.code_sha256,orig.code_sha256);}
 const codeIndex=new Map(c.code_checks.map(x=>[x.block_tag+':'+x.address,x]));
 for(const row of c.code_checks){const h=headerByNumber.get(row.block_tag),id=ids.get(row.address);assert(h&&id);assert.equal(row.block_hash,h.hash);assert.equal(row.code_sha256,id.code_sha256);}
 same(c.setup_getters.map(g=>[g.to,g.method,g.args]),[[c.scenarios[1].pool,'getPoolId',[]]],'complete setup getters');
 for(const row of c.setup_getters){const h=headerByNumber.get(row.block_tag);assert(h);assert.equal(row.block_hash,h.hash);assert.equal(row.data,lower(abis.get(row.to).encodeFunctionData(row.method,row.args)));assert.equal(lower(abis.get(row.to).decodeFunctionResult(row.method,row.result)[0]),c.scenarios[1].pool_id);}

 function checkedCode(address,h){const id=ids.get(address);assert(id);const check=codeIndex.get(h.number+':'+address);assert(check);assert.equal(check.block_hash,h.hash);assert.equal(check.code_sha256,id.code_sha256);return id;}
 function observe(s,h){
  const v=s.pool.multi_asset,m=v.managed_v2,p=s.pool.pool_id;
  const g=(to,method,args=[])=>{const row=s.getters.find(r=>r.to===to&&r.method===method&&V.canonical(r.args)===V.canonical(args));assert(row,method+JSON.stringify(args));return abis.get(to).decodeFunctionResult(method,row.result);};
  assert.equal(s.getter_count,76);assert.equal(s.getter_count,s.getters.length);
  const publicTemplate=JSON.parse(fs.readFileSync(path.join(ROOT,'integration/test/contracts/managed-v2-aum-pools.json'))).cases[0].source_pool;
  same(Object.keys(s.pool).sort(),Object.keys(publicTemplate).sort(),'exact public pool fields');
  for(const k of ['chain_key','protocol','fee_denominator','adapter_version','token_behavior'])same(s.pool[k],publicTemplate[k],'original public metadata '+k);
  same(Object.keys(v).sort(),Object.keys(publicTemplate.multi_asset).sort(),'exact multi asset fields');
  for(const k of ['amplification','admin_fee'])same(v[k],publicTemplate.multi_asset[k]);
  same([s.pool.token0,s.pool.token1],v.assets.map(a=>a.token),'physical token aliases');
  same(v.managed_v2.dependencies.map(d=>[d.role,d.address]),publicTemplate.multi_asset.managed_v2.dependencies.map(d=>[d.role,d.address]),'original linked dependency roles');

  for(const row of s.getters){assert.equal(row.block_tag,h.number);assert.equal(row.block_hash,h.hash);assert.equal(row.data,lower(abis.get(row.to).encodeFunctionData(row.method,row.args)));abis.get(row.to).decodeFunctionResult(row.method,row.result);}
  const sc=c.scenarios.find(x=>x.name===s.scenario),orig=bank.scenarios.find(x=>x.name===sc.original_scenario),origSnap=resolve(orig.source_h,'snapshot');
  const map=new Map([[orig.pool,p],[resolve(orig.source_h,'snapshot').state.pool_id,v.pool_id],...orig.underlying.map((a,i)=>[a,sc.tokens[i].address])]);
  function remap(x){if(typeof x==='string')return map.get(lower(x))||x;if(Array.isArray(x))return x.map(remap);return x;}
  same(s.getters.map(r=>[r.to,r.method,r.args]),origSnap.getters.map(r=>{const x=resolve(r,'getter');return [remap(x.to),x.method,remap(x.args)];}),'complete parameterized original getter list');
  const reg=g(v.vault,'getPoolTokens',[v.pool_id]),owner=lower(g(p,'getOwner')[0]),collector=lower(g(p,'getProtocolFeesCollector')[0]);
  assert.equal(lower(g(p,'getVault')[0]),v.vault);assert.equal(lower(g(p,'getPoolId')[0]),v.pool_id);
  assert.equal(lower(g(m.factory,'getVault')[0]),v.vault);assert.equal(g(m.factory,'isPoolFromFactory',[p])[0],true);
  assert.equal(lower(g(m.factory,'getProtocolFeePercentagesProvider')[0]),m.fee_provider);
  assert.equal(lower(g(m.fee_provider,'getVault')[0]),v.vault);assert.equal(lower(g(p,'getAuthorizer')[0]),resolve(bank.reference.identities[0],'identity').address);
  assert.equal(g(p,'name')[0],sc.configuration.params.name);
  const roles={'external-weighted-math':'getWeightedMath','recovery-mode-helper':'getRecoveryModeHelper'};
  for(const[role,method]of Object.entries(roles))assert.equal(lower(g(m.factory,method)[0]),m.dependencies.find(d=>d.role===role).address);
  const r=g(v.vault,'getPool',[v.pool_id]);assert.equal(lower(r[0]),p);assert.equal(Number(r[1]),1);
  same(reg[0].map(lower),[p,...v.assets.map(x=>x.token)]);
  const poolCode=checkedCode(p,h);assert.equal(v.code_sha256,poolCode.code_sha256);assert.equal(m.pool_runtime_template_sha256,poolCode.template_sha256);
  assert.equal(v.vault_code_sha256,checkedCode(v.vault,h).code_sha256);
  for(const[address,key]of [[m.factory,'factory_code_sha256'],[m.fee_provider,'fee_provider_code_sha256'],[collector,'protocol_collector_code_sha256']])assert.equal(m[key],checkedCode(address,h).code_sha256);
  for(const d of m.dependencies){const id=checkedCode(d.address,h);assert.equal(d.code_sha256,id.code_sha256);assert.equal(d.template_sha256,id.template_sha256);}
  const scales=g(p,'getScalingFactors')[0],weights=g(p,'getNormalizedWeights')[0].map(String),fee=g(p,'getManagementAumFeeParams'),total=g(p,'totalSupply')[0].toString(),bpt=g(v.vault,'getPoolTokenInfo',[v.pool_id,p]);
  const virtual=(BigInt(total)-BigInt(bpt[0])-BigInt(bpt[1])).toString(),actual=g(p,'getActualSupply')[0].toString(),pid=g(p,'getProviderFeeId',[3])[0].toString();
  const tokenInfo=v.assets.map((a,i)=>{
   const inf=g(v.vault,'getPoolTokenInfo',[v.pool_id,a.token]),balance=(BigInt(inf[0])+BigInt(inf[1])).toString();assert.equal(a.balance,balance);assert.equal(reg[1][i+1].toString(),balance);assert.equal(a.scaling,scales[i].toString());
   assert.equal(a.code_sha256,checkedCode(a.token,h).code_sha256);assert.equal(Number(g(a.token,'decimals')[0]),sc.tokens[i].decimals);
   assert(g(p,'getCircuitBreakerState',[a.token]).every(x=>x.toString()==='0'));
   return {cash:inf[0].toString(),managed:inf[1].toString(),vault_actual_balance:g(a.token,'balanceOf',[v.vault])[0].toString(),asset_manager:lower(inf[3]),last_change_block:num(inf[2]),circuit_breaker_enabled:false};
  });
  const expected={timestamp:num(h.timestamp),block_number:num(h.number),block_hash:h.hash,factory:m.factory,factory_code_sha256:checkedCode(m.factory,h).code_sha256,pool_runtime_template_sha256:poolCode.template_sha256,
   fee_provider:m.fee_provider,fee_provider_code_sha256:checkedCode(m.fee_provider,h).code_sha256,protocol_collector:collector,protocol_collector_code_sha256:checkedCode(collector,h).code_sha256,dependencies:m.dependencies,
   bpt_index:reg[0].map(lower).indexOf(p),manager:owner,manager_bpt_balance:g(p,'balanceOf',[owner])[0].toString(),protocol_collector_bpt_balance:g(p,'balanceOf',[collector])[0].toString(),
   management_aum_fee:fee[0].toString(),last_collection_timestamp:num(fee[1]),provider_fee_id:pid,live_provider_aum_percentage:g(m.fee_provider,'getFeeTypePercentage',[Number(pid)])[0].toString(),cached_protocol_aum_percentage:g(p,'getProtocolFeePercentageCache',[3])[0].toString(),
   swaps_enabled:g(p,'getSwapEnabled')[0],recovery_mode:g(p,'inRecoveryMode')[0],allowlist_enabled:g(p,'getMustAllowlistLPs')[0],gradual_weight_end:num(g(p,'getGradualWeightUpdateParams')[1]),gradual_fee_end:num(g(p,'getGradualSwapFeeUpdateParams')[1]),token_info:tokenInfo,
   total_supply:total,virtual_supply:virtual,actual_supply:actual,pending_aum:(BigInt(actual)-BigInt(virtual)).toString(),vault_bpt_cash:bpt[0].toString(),vault_bpt_managed:bpt[1].toString(),vault_bpt_actual_balance:g(p,'balanceOf',[v.vault])[0].toString()};
  for(const[to,prefix]of [[p,''],[v.vault,'vault_']]){const paused=g(to,'getPausedState');expected[prefix+'paused']=paused[0];expected[prefix+'pause_window_end']=num(paused[1]);expected[prefix+'buffer_period_end']=num(paused[2]);}
  assert.equal(Object.keys(expected).length,39);same(m,expected,'all public state39 and tokeninfo6 fields from actual getters');same(v.weights,weights);
  assert.equal(s.pool.fee_numerator,Number(g(p,'getSwapFeePercentage')[0]));assert.equal(s.pool.reserve0,v.assets[0].balance);assert.equal(s.pool.reserve1,v.assets[1].balance);
  const word=x=>ethers.utils.hexZeroPad(ethers.BigNumber.from(x).toHexString(),32).toLowerCase();
  const im={_vault:v.vault,_owner:owner,_poolId:v.pool_id,_protocolFeesCollector:collector,_protocolFeeProvider:m.fee_provider,_actionIdDisambiguator:m.factory,
   _pauseWindowEndTime:m.pause_window_end,_bufferPeriodEndTime:m.buffer_period_end,_HASHED_NAME:ethers.utils.id(sc.configuration.params.name),_HASHED_VERSION:ethers.utils.id('1'),_TYPE_HASH:ethers.utils.id('EIP712Domain(string name,string version,uint256 chainId,address verifyingContract)'),
   _weightedMath:m.dependencies.find(x=>x.role==='external-weighted-math').address,_recoveryModeHelper:m.dependencies.find(x=>x.role==='recovery-mode-helper').address,
   _feeIds:BigInt(g(p,'getProviderFeeId',[0])[0])|(BigInt(g(p,'getProviderFeeId',[2])[0])<<8n)|(BigInt(pid)<<16n)};
  for(const i of poolCode.immutables){assert(Object.hasOwn(im,i.name));assert.equal(i.value,word(im[i.name]));}
 }
 observe(c.prefix_pool_a,c.prefix_replay.headers.at(-1));same(c.prefix_pool_a.header,c.prefix_replay.headers.at(-1));
 for(const b of Object.values(c.banks)){
  assert.equal(b.pools.length,2);assert(num(b.header.timestamp)<c.timestamp_upper_bound_exclusive);same(b.header,b.header_anchor_before);same(b.header,b.header_anchor_after);
  assert.equal(b.getter_count,b.pools.reduce((n,s)=>n+s.getter_count,0));
  for(const s of b.pools){same(s.header,b.header,'same actual bank H');same(s.header_anchor_before,b.header);same(s.header_anchor_after,b.header);observe(s,b.header);}
 }
 for(const t of c.transactions)assert.equal(num(t.receipt.status),1,'all new signed native transactions succeeded');
 const allTx=[...c.prefix_replay.transactions.map(t=>({...t,pre_header:resolve(t.pre_header,'header'),execution_header:resolve(t.execution_header,'header')})),...c.transactions];
 for(const tx of allTx){const p=ethers.utils.parseTransaction(tx.signed_transaction),t=tx.transaction;
  assert.equal(p.hash,t.hash);assert.equal(lower(p.from),t.from);assert.equal(p.chainId,31337);assert.equal(p.nonce,num(t.nonce));assert.equal(p.data,t.input);assert.equal(p.to?lower(p.to):null,t.to);
  assert.equal(p.value.toString(),BigInt(t.value).toString());assert.equal(BigInt(p.r),BigInt(t.r));assert.equal(BigInt(p.s),BigInt(t.s));assert.equal(p.v,num(t.v));assert.equal(p.gasLimit.toString(),BigInt(t.gas).toString());assert.equal(p.gasPrice.toString(),BigInt(t.gasPrice).toString());assert([0,1].includes(num(tx.receipt.status)));
  assert.equal(t.blockHash,tx.execution_header.hash);assert.equal(t.blockNumber,tx.execution_header.number);
  assert.equal(tx.receipt.blockNumber,tx.execution_header.number);assert.equal(tx.receipt.from,t.from);assert.equal(tx.receipt.to,t.to);
  assert(tx.execution_header.transactions.includes(p.hash),'signed transaction included in header');
  for(const l of tx.receipt.logs){assert.equal(l.transactionHash,p.hash);assert.equal(l.blockHash,tx.execution_header.hash);assert.equal(l.blockNumber,tx.execution_header.number);}
  assert.equal(tx.receipt.transactionHash,p.hash);assert.equal(tx.receipt.blockHash,tx.execution_header.hash);assert.equal(tx.execution_header.parentHash,tx.pre_header.hash);
 }
 same(c.headers[0],c.prefix_replay.headers.at(-1),'native branch starts at prefix H28');
 for(const t of c.transactions){same(t.pre_header,headerByNumber.get(t.pre_header.number));same(t.execution_header,headerByNumber.get(t.execution_header.number));if(t.method){assert.equal(t.transaction.input,lower(abis.get(t.transaction.to).encodeFunctionData(t.method,t.args)),'recorded transaction arguments');}}
 let prev=c.prefix_replay.headers.at(-1);for(const h of c.headers.slice(1)){assert.equal(num(h.number),num(prev.number)+1);assert.equal(h.parentHash,prev.hash);assert.equal(num(h.timestamp),Date.parse((num(h.number)<43?'2026-01-01':'2026-01-31')+'T00:00:00Z')/1000,'fixed fixture clock');prev=h;}
 for(const x of c.cases){
  const matching=c.transactions.find(t=>t.transaction.hash===x.execution.transaction.hash);assert(matching);same(x.execution,matching,'case execution must equal the verified raw signed transaction');
  const src=c.banks[x.source_bank2pools],dst=c.banks[x.expected_after_bank2pools];same(x.execution.pre_header,src.header);same(x.execution.execution_header,dst.header);
  assert.equal(x.expected_native.status,num(x.execution.receipt.status));const before=src.pools.find(p=>p.scenario===x.scenario),after=dst.pools.find(p=>p.scenario===x.scenario);
  assert.equal(x.counts.source_getters,src.getter_count);assert.equal(x.counts.after_getters,dst.getter_count);
  if(x.operation==='swap'){
   const args=abis.get(x.execution.transaction.to).decodeFunctionData('swap',x.execution.transaction.input),single=args[0],owner=lower(args[1].sender);
   function bal(sn,token){const g=sn.getters.find(g=>g.to===lower(token)&&g.method==='balanceOf'&&g.args[0]===owner);assert(g);return BigInt(abis.get(lower(token)).decodeFunctionResult('balanceOf',g.result)[0]);}
   const out=(bal(after,single.assetOut)-bal(before,single.assetOut)).toString(),input=(bal(before,single.assetIn)-bal(after,single.assetIn)).toString();
   assert(BigInt(out)>0n);assert.equal(out,x.expected_native.amount_out);assert.equal(out,x.expected_native.trader_output_delta);assert.equal(input,x.expected_native.amount_in);assert.equal(input,x.expected_native.trader_input_delta);assert.equal(input,single.amount.toString());
   const event=x.execution.receipt.logs.filter(l=>l.address===x.execution.transaction.to).map(l=>{try{return abis.get(l.address).parseLog(l);}catch{return null;}}).find(e=>e?.name==='Swap');assert(event);assert.equal(event.args.amountOut.toString(),out);assert.equal(event.args.amountIn.toString(),input);
   const trader=resolve(bank.cases[0].execution.before,'snapshot').state.roles.trader;
   assert.equal(owner,trader);assert.equal(x.execution.transaction.from,trader);assert.equal(lower(args[1].recipient),trader);
   assert.equal(args[1].fromInternalBalance,false);assert.equal(args[1].toInternalBalance,false);
   assert.equal(lower(event.args.poolId),single.poolId.toLowerCase());assert.equal(lower(event.args.tokenIn),lower(single.assetIn));assert.equal(lower(event.args.tokenOut),lower(single.assetOut));
   same(x.expected_native.swap_event,{pool_id:lower(event.args.poolId),token_in:lower(event.args.tokenIn),token_out:lower(event.args.tokenOut),amount_in:input,amount_out:out},'native Swap event summary');
  }else{const b=before.pool.multi_asset.managed_v2,a=after.pool.multi_asset.managed_v2,e=x.expected_native;assert(BigInt(b.pending_aum)>0n);assert.equal((BigInt(a.total_supply)-BigInt(b.total_supply)).toString(),e.minted_bpt);assert(BigInt(e.minted_bpt)>0n);assert.equal((BigInt(a.manager_bpt_balance)-BigInt(b.manager_bpt_balance)).toString(),e.manager_bpt_delta);assert.equal((BigInt(a.protocol_collector_bpt_balance)-BigInt(b.protocol_collector_bpt_balance)).toString(),e.protocol_collector_bpt_delta);assert.equal(BigInt(e.minted_bpt),BigInt(e.manager_bpt_delta)+BigInt(e.protocol_collector_bpt_delta));assert.equal(a.pending_aum,'0');}
 }
 require('./shared_checks.cjs').validateShared(c,abis);
 assert.equal(c.cases.filter(x=>x.operation==='swap').length,4);
 I.authenticateOriginal();
 return {passed:true,prefix_pool_a_getters:c.prefix_pool_a.getter_count,banks:Object.keys(c.banks).length,cases:c.cases.length,pools:2,getters:Object.values(c.banks).reduce((n,b)=>n+b.getter_count,0),transactions:allTx.length,identities:c.identities.length};
}
module.exports={validate};
if(require.main===module){try {const args=process.argv.slice(2);assert.equal(args.length,1,'usage: validate.cjs INPUT');const input=I.inputPath(args[0]);console.log(JSON.stringify(validate(JSON.parse(fs.readFileSync(input)))));} catch(e){I.fail(e);}}
