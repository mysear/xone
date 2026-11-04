#!/usr/local/bin/node25.3.0
'use strict';
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const ROOT=fs.realpathSync(process.env.XONE_SOURCE_ROOT||path.resolve(__dirname,'../../..')),REF=ROOT+'/integration/tools/reference/balancer-managed-v2-aum';
const V=require(REF+'/validate.cjs'),E=require(REF+'/environment.cjs'),A=require(REF+'/artifacts.cjs');
const {ethers}=E.rr('ethers');
const lower=x=>x.toLowerCase(),num=x=>Number(BigInt(x)),same=(a,b,m)=>assert.equal(V.canonical(a),V.canonical(b),m);
const bankBytes=fs.readFileSync(ROOT+'/integration/test/contracts/managed-v2-aum.json'),bank=JSON.parse(bankBytes);
const P=require(process.env.XONE_CATALOG_PRODUCER||path.join(__dirname,'managed-v2-aum-catalog-golden.cjs'));
assert.equal(V.sha(fs.readFileSync(__filename)),V.sha(fs.readFileSync(P.sourcePath('integration/tools/ci/managed-v2-aum-catalog-check.cjs'))),'exact executing checker dependency');
const resolve=(r,k)=>V.resolveObject(bank.objects,r.ref,k),{builds}=A.load(),abis=new Map();
function validate(c){
 assert.equal(bankBytes.length,7905624);
 assert.equal(V.sha(bankBytes),'1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66');
 V.validate(bank); E.verifyInputs(); E.nodeClosure();
 same(c.original_bank.reference,bank.reference);
 assert.equal(c.original_bank.filename,'managed-v2-aum.json');
 assert.equal(c.original_bank.sha256,c.original_bank_sha256); assert.equal(c.original_bank.bytes,c.original_bank_bytes);
 const producer='integration/tools/ci/managed-v2-aum-catalog-golden.cjs';
 same(c.original_bank.tools_inputs,P.toolInputs(),'exact producer/check/manifest dependencies');
 same(c.source_preservation.before,P.preservation(),'exact immutable source inventory');
 same(c.reference_outputs,P.referenceOutputPolicy(),'mutable output classification only');
 assert.equal(c.generator.filename,producer); assert.equal(c.generator.source_root,'.');
 assert.equal(c.cases.length,8); assert.equal(Object.keys(c.banks).length,10);
 assert.equal(c.counts.raw_getters,9800); assert.equal(c.counts.collection_cases,4);

 assert.equal(c.schema_version,1);assert.equal(c.public_adapter_version,'balancer-v2/20230411-managed-aum-3');
 assert.equal(c.fixture_date,'2026-01-01T00:00:00.000Z');
 assert(!Object.hasOwn(c,'timestamp_upper_bound_exclusive'));
 assert.equal(c.actual_iso.first,new Date(num(c.banks[c.initial_bank4pools].header.timestamp)*1000).toISOString());
 assert.equal(c.actual_iso.last,new Date(num(c.banks[c.final_bank4pools].header.timestamp)*1000).toISOString());
 assert.equal(c.original_bank_sha256,V.sha(bankBytes));assert.equal(c.original_bank_bytes,bankBytes.length);
 same(c.chain_options,bank.reference.chain_options);same(c.original_reference.node_closure,bank.reference.node_closure);
 same(c.original_reference.input_closure,bank.reference.inputs);
 assert.equal(c.generator.sha256,V.sha(fs.readFileSync(process.env.XONE_CATALOG_PRODUCER||path.join(ROOT,c.generator.filename))));
 assert.equal(c.generator.actual_node_sha256,V.sha(fs.readFileSync(process.execPath)));
 same(c.source_preservation.before,c.source_preservation.after);
 for(const f of c.source_preservation.before)assert.equal(f.sha256,V.sha(fs.readFileSync(P.sourcePath(f.path))),'untouched immutable source '+f.path);
 assert.equal(c.prefix_replay.count,28);assert.equal(c.prefix_replay.complete_transaction_receipt_header_equality,true);
 same(c.prefix_replay.transactions,bank.reference.transactions.slice(0,28).map(r=>resolve(r,'transaction')),'exact original signed prefix records');
 same(c.prefix_replay.headers,[...new Map(Object.values(bank.objects).filter(o=>o.kind==='header').map(o=>[num(o.value.number),o.value]))].filter(([n])=>n<=28).sort(([a],[b])=>a-b).map(([,h])=>h));
 const ids=new Map(c.identities.map(i=>[i.address,i]));
 for(const i of c.identities){
  const a=A.locate(builds,i.artifact.build,i.artifact.name),d=a.c.evm.deployedBytecode,b=Buffer.from(i.code_hex.slice(2),'hex'),norm=Buffer.from(b),compiled=d.object.split('');
  abis.set(i.address,new ethers.utils.Interface(a.c.abi));
  assert.equal(i.runtime_bytes,b.length);assert(b.length>0&&b.length<=24576);assert.equal(i.code_sha256,V.sha(b));
  const clear=s=>{norm.fill(0,s.start,s.start+s.length);compiled.fill('0',s.start*2,(s.start+s.length)*2);};
  if(a.library){assert.equal('0x'+b.subarray(1,21).toString('hex'),i.address);clear({start:1,length:20});}
  const expectedLinks=[];
  for(const[source,libs]of Object.entries(d.linkReferences||{}))for(const[name,ss]of Object.entries(libs))for(const s of ss){const address='0x'+b.subarray(s.start,s.start+s.length).toString('hex');assert(c.identities.some(x=>x.address===address&&x.artifact.name===name),'linked original library');expectedLinks.push({source,name,...s,address});clear(s);}
  same(i.links,expectedLinks);
  same(i.immutables.map(x=>x.id).sort(),Object.keys(d.immutableReferences||{}).sort());
  for(const m of i.immutables){same(m.spans,d.immutableReferences[m.id]);for(const s of m.spans){assert.equal(m.value,'0x'+b.subarray(s.start,s.start+s.length).toString('hex'));clear(s);}}
  assert.equal(norm.toString('hex'),compiled.join(''),'original compiled template '+a.name);assert.equal(i.template_sha256,V.sha(norm));
 }
 for(const x of c.factory_creation_code_storage){const orig=bank.reference.factory_code_storage.contracts.find(y=>y.address===x.address);assert(orig);assert.equal(x.code_hex,resolve(orig.code,'code').hex);assert.equal(x.code_sha256,V.sha(Buffer.from(x.code_hex.slice(2),'hex')));}
 same(c.scenarios.map(s=>s.underlying_count),[2,3,5,50]);
 const seen=new Set();
 for(const s of c.scenarios){
  assert(!seen.has(s.pool));seen.add(s.pool);assert.equal(s.tokens.length,s.underlying_count);
  const tokens=s.tokens.map(t=>t.address);same([...tokens].sort(),tokens);for(const t of tokens){assert(!seen.has(t),'disjoint physical pools');seen.add(t);}
  const createIface=abis.get(s.creation.transaction.to),args=createIface.decodeFunctionData('create',s.creation.transaction.input);
  same(args[1].tokens.map(lower),tokens);assert.equal(args[1].aumFeeId.toString(),'3');assert.equal(args[1].managementAumFeePercentage.toString(),'10000000000000000');
  assert.equal(args[1].swapFeePercentage.toString(),'10000000000000000');
  const event=s.creation.receipt.logs.filter(l=>l.address===s.creation.transaction.to).map(l=>{try{return createIface.parseLog(l);}catch{return null;}}).find(e=>e?.name==='PoolCreated');
  assert(event);assert.equal(lower(event.args.pool),s.pool);
  assert.equal(num(s.initialization.receipt.status),1);assert.equal(s.initialization.transaction.from,lower(new ethers.Wallet('0x'+(3).toString(16).padStart(64,'0')).address));
 }
 const codeIndex=new Map(c.code_checks.map(x=>[x.block_tag+':'+x.address,x]));
 function checkedCode(address,h){const id=ids.get(address);assert(id);const check=codeIndex.get(h.number+':'+address);assert(check);assert.equal(check.block_hash,h.hash);assert.equal(check.code_sha256,id.code_sha256);return id;}
 function observe(s,h){
  const v=s.pool.multi_asset,m=v.managed_v2,p=s.pool.pool_id;
  const g=(to,method,args=[])=>{const row=s.getters.find(r=>r.to===to&&r.method===method&&V.canonical(r.args)===V.canonical(args));assert(row,method+JSON.stringify(args));return abis.get(to).decodeFunctionResult(method,row.result);};
  assert.equal(s.getter_count,s.getters.length);
  for(const row of s.getters){assert.equal(row.block_tag,h.number);assert.equal(row.block_hash,h.hash);assert.equal(row.data,lower(abis.get(row.to).encodeFunctionData(row.method,row.args)));abis.get(row.to).decodeFunctionResult(row.method,row.result);}
  const sc=c.scenarios.find(x=>x.name===s.scenario),orig=bank.scenarios.find(x=>x.name===s.scenario),origSnap=resolve(orig.source_h,'snapshot');
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
 for(const b of Object.values(c.banks)){
  assert.equal(b.pools.length,4);same(b.header,b.header_anchor_before);same(b.header,b.header_anchor_after);
  assert.equal(b.getter_count,b.pools.reduce((n,s)=>n+s.getter_count,0));
  for(const s of b.pools){same(s.header,b.header,'same actual bank H');same(s.header_anchor_before,b.header);same(s.header_anchor_after,b.header);observe(s,b.header);}
 }
 for(const t of c.transactions)assert.equal(num(t.receipt.status),1,'all new signed native transactions succeeded');
 const allTx=[...c.prefix_replay.transactions.map(t=>({...t,pre_header:resolve(t.pre_header,'header'),execution_header:resolve(t.execution_header,'header')})),...c.transactions];
 for(const tx of allTx){const p=ethers.utils.parseTransaction(tx.signed_transaction),t=tx.transaction;
  assert.equal(p.hash,t.hash);assert.equal(lower(p.from),t.from);assert.equal(p.chainId,31337);assert.equal(p.nonce,num(t.nonce));assert.equal(p.data,t.input);assert.equal(p.to?lower(p.to):null,t.to);
  assert.equal(p.gasLimit.toString(),BigInt(t.gas).toString());assert.equal(p.gasPrice.toString(),BigInt(t.gasPrice).toString());assert([0,1].includes(num(tx.receipt.status)));
  assert.equal(tx.receipt.transactionHash,p.hash);assert.equal(tx.receipt.blockHash,tx.execution_header.hash);assert.equal(tx.execution_header.parentHash,tx.pre_header.hash);
 }
 let prev=c.prefix_replay.headers.at(-1);for(const h of c.headers.slice(1)){assert.equal(num(h.number),num(prev.number)+1);assert.equal(h.parentHash,prev.hash);assert(num(h.timestamp)>=num(prev.timestamp));prev=h;}
 for(const x of c.cases){
  const matching=c.transactions.find(t=>t.transaction.hash===x.execution.transaction.hash);assert(matching);same(x.execution,matching,'case execution must equal the verified raw signed transaction');
  const src=c.banks[x.source_bank4pools],dst=c.banks[x.expected_after_bank4pools];same(x.execution.pre_header,src.header);same(x.execution.execution_header,dst.header);
  assert.equal(x.expected_native.status,num(x.execution.receipt.status));const before=src.pools.find(p=>p.scenario===x.scenario),after=dst.pools.find(p=>p.scenario===x.scenario);
  assert.equal(x.counts.source_getters,src.getter_count);assert.equal(x.counts.after_getters,dst.getter_count);
  if(x.operation==='swap'){
   const args=abis.get(x.execution.transaction.to).decodeFunctionData('swap',x.execution.transaction.input),single=args[0],owner=lower(args[1].sender);
   function bal(sn,token){const g=sn.getters.find(g=>g.to===lower(token)&&g.method==='balanceOf'&&g.args[0]===owner);assert(g);return BigInt(abis.get(lower(token)).decodeFunctionResult('balanceOf',g.result)[0]);}
   const out=(bal(after,single.assetOut)-bal(before,single.assetOut)).toString(),input=(bal(before,single.assetIn)-bal(after,single.assetIn)).toString();
   assert(BigInt(out)>0n);assert.equal(out,x.expected_native.amount_out);assert.equal(out,x.expected_native.trader_output_delta);assert.equal(input,x.expected_native.amount_in);assert.equal(input,x.expected_native.trader_input_delta);assert.equal(input,single.amount.toString());
   const event=x.execution.receipt.logs.filter(l=>l.address===x.execution.transaction.to).map(l=>{try{return abis.get(l.address).parseLog(l);}catch{return null;}}).find(e=>e?.name==='Swap');assert(event);assert.equal(event.args.amountOut.toString(),out);assert.equal(event.args.amountIn.toString(),input);
  }else{const b=before.pool.multi_asset.managed_v2,a=after.pool.multi_asset.managed_v2,e=x.expected_native;assert(BigInt(b.pending_aum)>0n);assert.equal((BigInt(a.total_supply)-BigInt(b.total_supply)).toString(),e.minted_bpt);assert(BigInt(e.minted_bpt)>0n);assert.equal((BigInt(a.manager_bpt_balance)-BigInt(b.manager_bpt_balance)).toString(),e.manager_bpt_delta);assert.equal((BigInt(a.protocol_collector_bpt_balance)-BigInt(b.protocol_collector_bpt_balance)).toString(),e.protocol_collector_bpt_delta);assert.equal(BigInt(e.minted_bpt),BigInt(e.manager_bpt_delta)+BigInt(e.protocol_collector_bpt_delta));assert.equal(a.pending_aum,'0');}
 }
 assert.equal(c.cases.filter(x=>x.operation==='swap').length,4);
 return {passed:true,banks:Object.keys(c.banks).length,cases:c.cases.length,pools:4,getters:Object.values(c.banks).reduce((n,b)=>n+b.getter_count,0),transactions:allTx.length,identities:c.identities.length};
}
module.exports={validate};
if(require.main===module){assert.equal(process.argv.length,3,'usage: managed-v2-aum-catalog-check.cjs LOCAL_RAW_PATH');const c=JSON.parse(fs.readFileSync(process.argv[2]));console.log(JSON.stringify({...validate(c),source_root:ROOT,reference_outputs_observed:P.observedReferenceOutputs()}));}
