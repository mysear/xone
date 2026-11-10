'use strict';
// Original source qualification only; price values come from real VM wallets.
const assert=require('node:assert/strict');
const I=require('./inputs.cjs'),{E,A,V,sha}=I,{ethers}=E.rr('ethers');
const Native=require('../balancer-managed-v2-scheduled/checker.cjs');
const Shared=require('../balancer-managed-v2-scheduled-shared/checker.cjs');
const {eq,clean}=require('./runtime.cjs');
function lookup(id,table){const t=table.get(id);assert(t,'actual signed transaction '+id);return t;}
function read(state,to,method,args=[]){const same=a=>V.canonical(a.map(x=>typeof x==='number'?String(x):x));const rows=state.getters.filter(g=>g.to===to&&g.method===method&&same(g.args)===same(args));assert(rows.length,'original getter '+method);return rows[0].decoded;}
function validateIdentity(i,b,txs,builds){
 const a=A.locate(builds,i.artifact.build,i.artifact.name);assert.equal(a.source,i.artifact.source);
 assert(['TestToken','ManagedPool'].includes(a.name),'only original shared tokens and pools');
 assert.equal(a.build,a.name==='TestToken'?'Authorizer':'ManagedPoolFactory');
 const code=b.native_runtime[i.runtime_sha256];assert(typeof code==='string'&&/^0x(?:[0-9a-f]{2})*$/.test(code));assert.equal(sha(Buffer.from(code.slice(2),'hex')),i.runtime_sha256);
 assert.equal((code.length-2)/2,i.runtime_bytes);assert(i.runtime_bytes<=24576,'normal EIP170');
 eq(i.runtime_template,a.c.evm.deployedBytecode,'exact original compiled template');
 const vars=new Map(Object.values(builds[a.build].output.sources).flatMap(s=>s.ast.nodes.filter(n=>n.nodeType==='ContractDefinition').flatMap(n=>n.nodes)).filter(n=>n.nodeType==='VariableDeclaration').map(v=>[String(v.id),v]));
 const wantedImmutables=Object.entries(a.c.evm.deployedBytecode.immutableReferences||{}).map(([id,spans])=>({ast_id:id,name:vars.get(id).name,type:vars.get(id).typeDescriptions.typeString,spans:spans.map(s=>({...s,value:'0x'+code.slice(2+s.start*2,2+(s.start+s.length)*2)}))}));
 eq(i.immutables,wantedImmutables,'immutable interpretation from original AST and bytes');
 let masked=code.slice(2),expected=a.c.evm.deployedBytecode.object;const links=[];
 for(const libs of Object.values(a.c.evm.deployedBytecode.linkReferences||{}))for(const [name,spans]of Object.entries(libs)){
  const address=b.prefix_identities.find(x=>x.label===name)?.address;assert(address,'pinned original library');links.push({name,address,spans});
  for(const s of spans){assert.equal(code.slice(2+s.start*2,2+(s.start+s.length)*2),address.slice(2));expected=expected.slice(0,s.start*2)+address.slice(2)+expected.slice((s.start+s.length)*2);}
 }
 eq(i.library_links,links,'complete original library links');
 for(const row of wantedImmutables)for(const s of row.spans){masked=masked.slice(0,s.start*2)+'0'.repeat(s.length*2)+masked.slice((s.start+s.length)*2);expected=expected.slice(0,s.start*2)+'0'.repeat(s.length*2)+expected.slice((s.start+s.length)*2);}
 assert.equal(masked,expected,'original runtime outside declared immutable contexts');
 const tx=lookup(i.creation_transaction,txs);assert.equal(tx.receipt.status,'0x1');
 if(a.name==='TestToken'){
  assert.equal(tx.transaction.to,null);assert.equal(tx.receipt.contractAddress,i.address);
  assert.equal(ethers.utils.getContractAddress({from:tx.transaction.from,nonce:Number(BigInt(tx.transaction.nonce))}).toLowerCase(),i.address);
  eq(i.constructor_arguments,tx.args,'actual token constructor args');
  const addresses=Object.fromEntries(b.prefix_identities.map(x=>[x.label,x.address]));
  const deploy=new ethers.ContractFactory(a.c.abi,A.link(a,addresses)).getDeployTransaction(...tx.args).data.toLowerCase();
  assert.equal(tx.transaction.input,deploy,'original token initcode plus constructor');
 }else assert.equal(i.runtime_bytes,24561,'published ManagedPool runtime');
}
function bptTransfers(t,pool,abi){
 return t.receipt.logs.filter(l=>l.address===pool).flatMap(l=>{const event=abi.parseLog(l);return event.name==='Transfer'?[{from:event.args[0].toLowerCase(),to:event.args[1].toLowerCase(),amount:BigInt(event.args[2].toString())}]:[];});
}
function applyBptTransfers(before,total,events){
 const balances=new Map(Object.entries(before).map(([a,n])=>[a,BigInt(n)])),zero=ethers.constants.AddressZero;
 for(const e of events){
  assert(balances.has(e.to),'every actual BPT recipient retained');
  if(e.from===zero){total+=e.amount;balances.set(e.to,balances.get(e.to)+e.amount);}
  else{
   assert(e.to!==zero,'BPT reference swaps do not burn supply');assert(balances.has(e.from),'every actual BPT sender retained');assert(balances.get(e.from)>=e.amount,'actual event BPT funds');
   balances.set(e.from,balances.get(e.from)-e.amount);balances.set(e.to,balances.get(e.to)+e.amount);
  }
 }
 return {balances:Object.fromEntries([...balances].map(([a,n])=>[a,String(n)])),total};
}
function validateInitialBptLocks(initial,b,txs,abis){
 const locked=new Map(),zero=ethers.constants.AddressZero;
 for(const p of b.pools){
  const wallet=initial.bpt_wallets.find(w=>w.token===p.pool),state=initial.pools.find(q=>q.name===p.name).state;
  const empty=Object.fromEntries(Object.keys(wallet.balances).map(w=>[w,'0']));
  const events=b.transactions.filter(t=>BigInt(t.execution_header.number)<=BigInt(initial.header.number)).flatMap(t=>bptTransfers(t,p.pool,abis.get(p.pool)));
  const actual=applyBptTransfers(empty,0n,events);eq(actual.balances,wallet.balances,'initial BPT holder balances derive from original mint/transfer receipts');assert.equal(actual.total,BigInt(state.aum.total_supply));
  const lock=events.filter(e=>e.from===zero&&e.to===zero).reduce((n,e)=>n+e.amount,0n);assert.equal(lock,1000000n,'original NewBasePool minimum BPT lock');locked.set(p.pool,lock);
  const init=lookup(p.initialize,txs);assert.equal(init.method,'joinPool');assert.equal(init.transaction.to,b.vault);assert.equal(init.transaction.from,b.roles.lp);assert.equal(init.args[0],p.pool_id);eq(init.args.slice(1,3),[b.roles.lp,b.roles.lp]);eq(init.args[3].assets,[p.pool,...b.tokens]);
 }
 return locked;
}
function validateAumRaw(raw,params,recovery){
 assert(typeof raw==='string'&&/^0x[0-9a-f]*$/.test(raw)&&raw.length<=66,'original AUM raw word');
 const word=BigInt(raw==='0x'?'0x0':raw),fee=word&((1n<<60n)-1n),last=(word>>60n)&0xffffffffn;
 assert.equal(word>>92n,0n,'original AUM unused bits remain zero');
 assert(fee<=950000000000000000n,'original AUM annual fee bound');
 assert.equal(String(last),params[1],'AUM raw collection timestamp/getter');
 assert.equal(recovery?'0':String(fee),params[0],'AUM raw/effective annual fee getter');
}
function validateBankBindings(bank,b,initial,locked){
 const field=(w,offset,bits)=>(w>>BigInt(offset))&((1n<<BigInt(bits))-1n),one=10n**18n;
 const lerp=(start,end,a,z,time)=>{let progress;if(time>=end)progress=one;else if(time<=start)progress=0n;else progress=(time-start)*one/(end-start);return a<=z?a+(z-a)*progress/one:a-(a-z)*progress/one;};
 for(const p of b.pools){
  const s=bank.pools.find(q=>q.name===p.name).state,first=initial.pools.find(q=>q.name===p.name).state,time=BigInt(bank.header.timestamp);
  const aumRaw=bank.storage.find(r=>r.address===p.pool&&r.key==='0xa');assert(aumRaw);assert.equal(s.aum.raw_word,aumRaw.value,'AUM metadata/raw source');
  validateAumRaw(aumRaw.value,read(s,p.pool,'getManagementAumFeeParams'),read(s,p.pool,'inRecoveryMode')[0]);
  for(const g of s.getters.filter(g=>g.to===p.pool&&['getOwner','getVault','getPoolId','getProtocolFeesCollector','getProviderFeeId','name','getPausedState'].includes(g.method))){
   const original=first.getters.find(x=>x.to===g.to&&x.data===g.data);assert(original,'original immutable getter recorded');assert.equal(g.result,original.result,'every-bank getter remains bound to original constructor/runtime '+g.method);
  }
  const raw=bank.storage.find(r=>r.address===p.pool&&r.key==='0x9');assert(raw);assert.equal(s.packed.pool_word,raw.value,'every bank packed pool metadata equals actual slot9');
  const word=BigInt(raw.value),ws=field(word,0,32),we=field(word,32,32),fs=field(word,64,32),fe=field(word,96,32),fee0=field(word,128,62),fee1=field(word,190,62);
  eq(s.weight_schedule.slice(0,2),[String(ws),String(we)]);eq(s.fee_schedule,[String(fs),String(fe),String(fee0),String(fee1)]);
  assert.equal(s.fee,String(lerp(fs,fe,fee0,fee1,time)),'every bank fee view follows original raw clock');
  for(const [bit,method]of [[252,'getSwapEnabled'],[253,'getMustAllowlistLPs'],[254,'inRecoveryMode'],[255,'getJoinExitEnabled']])assert.equal(read(s,p.pool,method)[0],field(word,bit,1)!==0n,'actual raw flag/getter '+method);
  let targetTotal=0n;
  for(let index=0;index<3;index++){
   const t=s.packed.tokens[index],token=b.tokens[index],key=ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['address','uint256'],[token,11]));assert.equal(t.token,token);assert.equal(t.key,key);
   const row=bank.storage.find(r=>r.address===p.pool&&r.key===key);assert(row);assert.equal(t.word,row.value,'every bank actual token word');
   const rawToken=BigInt(t.word),a=field(rawToken,0,64),z=field(rawToken,64,64),difference=field(rawToken,128,5);assert.equal(rawToken>>133n,0n,'actual token reserved bits zero');
   assert.equal(s.weight_schedule[2][index],String(a));assert.equal(s.weight_schedule[3][index],String(z));assert.equal(s.weights[index],String(lerp(ws,we,a,z,time)),'every bank current selected weight matches raw schedule');
   assert.equal(difference,18n-BigInt(s.token_states[index].decimals));assert.equal(s.scales[index],String(10n**(18n+difference)));targetTotal+=z;
  }
  assert.equal(targetTotal,one,'original caller end-weight array sum; computed starts not normalized');
  const wallet=bank.bpt_wallets.find(w=>w.token===p.pool);assert.equal(BigInt(wallet.balances[ethers.constants.AddressZero]),locked.get(p.pool),'original zero-holder minimum lock persists in every bank');
 }
}
// Reuse the same full source bindings for additional selected-plan after banks.
// `initial` must come from a separately qualified original branch.
function validateBoundBank(bank,b,initial,abis){
 const runtime=new Map(initial.accounts.map(a=>[a.address,a.code_sha256]));
 for(const account of bank.accounts)assert.equal(account.code_sha256,runtime.get(account.address),'selected after retains original runtime identity');
 const ids=[...b.prefix_identities,...b.new_identities];Shared.validateBank(bank,b,ids,abis);validateExtra(bank,b);
 const locked=new Map(b.pools.map(p=>[p.pool,BigInt(initial.bpt_wallets.find(w=>w.token===p.pool).balances[ethers.constants.AddressZero])]));
 validateBankBindings(bank,b,initial,locked);
}
function wallet(bank,asset,holder){const row=bank.bpt_wallets.find(w=>w.token===asset)||bank.global_wallets.find(w=>w.token===asset);assert(row&&Object.hasOwn(row.balances,holder),'retained actual wallet');return BigInt(row.balances[holder]);}
function poolView(state){const reads=new Map();for(const g of state.getters)reads.set(g.to+g.data,g.result);return [...reads].sort(([a],[b])=>a.localeCompare(b));}
function nativeAccounts(before,after,t){
 const from=t.transaction.from,miner=t.execution_header.miner,fee=BigInt(t.receipt.gasUsed)*BigInt(t.receipt.effectiveGasPrice),tip=BigInt(t.receipt.gasUsed)*(BigInt(t.receipt.effectiveGasPrice)-BigInt(t.execution_header.baseFeePerGas));
 for(const a of before.accounts){const z=after.accounts.find(x=>x.address===a.address);assert(z);assert.equal(z.code_sha256,a.code_sha256);assert.equal(BigInt(z.nonce),BigInt(a.nonce)+(a.address===from?1n:0n));const delta=(a.address===from?-fee:0n)+(a.address===miner?tip:0n);assert.equal(BigInt(z.native_balance)-BigInt(a.native_balance),delta,'native gas, coinbase and signed nonce');}
}
function validateExtra(bank,b){
 assert.equal(bank.bpt_wallets.length,2);assert.equal(bank.actor_allowlists.length,2);assert.equal(bank.control_storage.length,2*(4+3));
 const collector=read(bank.pools[0].state,b.vault,'getProtocolFeesCollector')[0],holders=[...new Set([...Object.values(b.roles),collector,ethers.constants.AddressZero,b.vault,...b.pools.map(p=>p.pool)])];
 assert.equal(b.control_mapping_slots._circuitBreakerState,12);assert.equal(b.control_mapping_slots._allowedAddresses,13);
 for(const p of b.pools){
  const s=bank.pools.find(x=>x.name===p.name).state,w=bank.bpt_wallets.find(w=>w.pool_name===p.name),allow=bank.actor_allowlists.find(w=>w.pool_name===p.name);assert(w&&allow);assert.equal(w.token,p.pool);eq(Object.keys(w.balances),holders);
  let total=0n;for(const who of holders){assert.equal(w.balances[who],read(s,p.pool,'balanceOf',[who])[0]);total+=BigInt(w.balances[who]);}assert.equal(total,BigInt(s.aum.total_supply),'all fixture BPT holders conserve supply');
  eq(Object.keys(w.allowances),Object.values(b.roles));eq(Object.keys(allow.members),Object.values(b.roles));assert.equal(allow.enabled,read(s,p.pool,'getMustAllowlistLPs')[0]);
  for(const who of Object.values(b.roles)){
   assert.equal(w.allowances[who],read(s,p.pool,'allowance',[who,b.vault])[0]);assert.equal(allow.members[who],read(s,p.pool,'isAddressOnAllowlist',[who])[0]);
   const row=bank.control_storage.find(r=>r.address===p.pool&&r.kind==='allowlist'&&r.member===who);assert(row);assert.equal(row.slot,13);
   const key=ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['address','uint256'],[who,13]));assert.equal(row.key,key);assert.equal(row.value,bank.storage.find(r=>r.address===p.pool&&r.key===key).value);
   const value=BigInt(row.value==='0x'?'0x0':row.value);assert(value===0n||value===1n);assert.equal(allow.members[who],value===1n,'original allowlist mapping and getter');
  }
  for(let index=0;index<3;index++){
   const token=b.tokens[index],row=bank.control_storage.find(r=>r.address===p.pool&&r.kind==='circuit'&&r.token===token);assert(row);assert.equal(row.slot,12);
   const key=ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['address','uint256'],[token,12]));assert.equal(row.key,key);assert.equal(row.value,bank.storage.find(r=>r.address===p.pool&&r.key===key).value);
   const word=BigInt(row.value==='0x'?'0x0':row.value),field=(offset,bits)=>(word>>BigInt(offset))&((1n<<BigInt(bits))-1n),g=read(s,p.pool,'getCircuitBreakerState',[token]);
   const factor=BigInt(s.scales[index]);eq(g.slice(0,4),[String(field(96,96)*factor/10n**18n),String(field(32,64)),String(field(0,16)<<48n),String(field(16,16)<<48n)],'original circuit packed fields, not a pricing oracle');
   if(word===0n)eq(g,Array(6).fill('0'),'disabled breaker has all known zero fields');
  }
 }
}
function validateSwap(c,b,banks,txs,abis){
 const before=banks.get(c.before),after=banks.get(c.after),p=b.pools.find(p=>p.name===c.pool_name),t=lookup(c.transaction,txs);assert(before&&after&&p);assert.equal(before.namespace,b.mode);assert.equal(after.namespace,b.mode);eq(before.header,t.pre_header);eq(after.header,t.execution_header);
 assert.equal(t.method,'swap');assert.equal(t.transaction.to,b.vault);assert.equal(t.transaction.from,c.actor);assert(Object.values(b.roles).includes(c.actor));
 const [swap,funds,limit,deadline]=t.args;eq(swap,{poolId:p.pool_id,kind:0,assetIn:c.token_in,assetOut:c.token_out,amount:c.amount_in,userData:'0x'});eq(funds,{sender:c.actor,recipient:c.actor,fromInternalBalance:false,toInternalBalance:false});assert.equal(String(limit),'0');assert.equal(String(deadline),ethers.constants.MaxUint256.toString());
 const operation=c.token_out===p.pool?'bpt_join':c.token_in===p.pool?'bpt_exit':'token_swap';assert.equal(c.operation,operation);assert(c.token_in!==c.token_out);assert([p.pool,...b.tokens].includes(c.token_in)&&[p.pool,...b.tokens].includes(c.token_out));
 const status=Number(BigInt(t.receipt.status)),e=c.expected_native;assert.equal(e.status,status);assert([0,1].includes(status));nativeAccounts(before,after,t);
 const x=before.pools.find(x=>x.name===p.name).state,y=after.pools.find(x=>x.name===p.name).state;
 const input=wallet(before,c.token_in,c.actor)-wallet(after,c.token_in,c.actor),output=wallet(after,c.token_out,c.actor)-wallet(before,c.token_out,c.actor);
 assert.equal(e.actual_amount_in,String(input));assert.equal(e.actual_amount_out,String(output));
 const minted=BigInt(y.aum.total_supply)-BigInt(x.aum.total_supply),manager=BigInt(y.aum.manager_bpt)-BigInt(x.aum.manager_bpt),collector=BigInt(y.aum.collector_bpt)-BigInt(x.aum.collector_bpt);
 eq([e.aum_minted_bpt,e.manager_bpt_delta,e.collector_bpt_delta],[String(minted),String(manager),String(collector)]);
 if(status===0){
  assert.equal(input,0n);assert.equal(output,0n);eq(e.swap_event,null);eq(t.receipt.logs,[]);
  eq(before.storage,after.storage,'reverted original operation restores all observed raw storage');eq(before.global_wallets,after.global_wallets);eq(before.bpt_wallets,after.bpt_wallets);eq(before.actor_allowlists,after.actor_allowlists);
  for(let i=0;i<2;i++){eq(poolView(before.pools[i].state),poolView(after.pools[i].state),'reverted source preserves every original getter value');const omit=state=>Object.fromEntries(Object.entries(state).filter(([key])=>!['header','header_anchor_after','source_h','source_clock','getters'].includes(key)));eq(omit(before.pools[i].state),omit(after.pools[i].state),'reverted source preserves all Pool metadata and packed words');}
  return;
 }
 assert(BigInt(c.amount_in)>0n);assert.equal(input,BigInt(c.amount_in));assert(output>=0n);
 const logs=t.receipt.logs.filter(l=>l.address===b.vault).map(l=>{try{return abis.get(b.vault).parseLog(l);}catch{return null;}}).filter(e=>e?.name==='Swap');assert.equal(logs.length,1);const event=logs[0].args;
 eq(e.swap_event,{pool_id:event.poolId.toLowerCase(),token_in:event.tokenIn.toLowerCase(),token_out:event.tokenOut.toLowerCase(),amount_in:event.amountIn.toString(),amount_out:event.amountOut.toString()});
 eq(e.swap_event,{pool_id:p.pool_id,token_in:c.token_in,token_out:c.token_out,amount_in:String(input),amount_out:String(output)});
 const isBpt=operation!=='token_swap';assert.equal(minted,isBpt?BigInt(x.aum.pending_aum):0n,'only original AUM is minted');assert.equal(minted,manager+collector);
 if(isBpt){assert.equal(y.aum.pending_aum,'0');assert.equal(y.aum.params[0],x.aum.params[0]);assert.equal(BigInt(y.aum.params[1]),BigInt(after.header.timestamp)&0xffffffffn);}
 else eq(y.aum,x.aum,'underlying regular swap does not collect AUM');
 const bptDelta=operation==='bpt_join'?-output:operation==='bpt_exit'?input:0n;
 assert.equal(BigInt(y.aum.bpt_info[0])-BigInt(x.aum.bpt_info[0]),bptDelta,'own registered BPT cash transfer');assert.equal(y.aum.bpt_info[1],x.aum.bpt_info[1]);
 assert.equal(BigInt(y.aum.virtual_supply)-BigInt(x.aum.virtual_supply),minted-bptDelta,'virtual supply changes by AUM mint minus own BPT registration');
 if(isBpt)assert.equal(BigInt(y.aum.actual_supply),BigInt(x.aum.actual_supply)-bptDelta);
 for(let index=0;index<3;index++){
  const token=b.tokens[index],delta=token===c.token_in?input:token===c.token_out?-output:0n;
  assert.equal(BigInt(y.token_states[index].info[0])-BigInt(x.token_states[index].info[0]),delta);assert.equal(y.token_states[index].info[1],x.token_states[index].info[1]);assert.equal(y.token_states[index].info[3],x.token_states[index].info[3]);
  const w=before.global_wallets[index],z=after.global_wallets[index];assert.equal(w.token,token);assert.equal(z.total_supply,w.total_supply);
  for(const who of Object.keys(w.balances))assert.equal(BigInt(z.balances[who])-BigInt(w.balances[who]),who===b.vault?delta:who===c.actor?-delta:0n,'all observed underlying wallets');
 }
 const collectorAddress=read(x,b.vault,'getProtocolFeesCollector')[0],wb=before.bpt_wallets.find(w=>w.token===p.pool),wa=after.bpt_wallets.find(w=>w.token===p.pool);
 for(const who of Object.keys(wb.balances)){
  const delta=(who===b.vault?bptDelta:0n)+(who===c.actor?-bptDelta:0n)+(who===b.roles.manager?manager:0n)+(who===collectorAddress?collector:0n);
  assert.equal(BigInt(wa.balances[who])-BigInt(wb.balances[who]),delta,'all BPT holders distinguish mint and Vault transfer');
 }
 eq(wa.allowances,wb.allowances,'fixture BPT infinite allowances remain infinite');
 for(const key of ['weights','sum_weights','fee','weight_schedule','fee_schedule','packed','scales','owner'])eq(y[key],x[key],'BPT swap preserves original readonly pool view '+key);
 const peer=b.pools.find(q=>q.name!==p.name),px=before.pools.find(q=>q.name===peer.name).state,py=after.pools.find(q=>q.name===peer.name).state;
 for(const key of ['weights','sum_weights','fee','weight_schedule','fee_schedule','packed','scales','owner','registration','aum'])eq(py[key],px[key],'peer physical pool accounting/view preserved '+key);
 eq(py.token_states.map(t=>t.info),px.token_states.map(t=>t.info),'peer registered token balances do not follow global wallet');eq(before.bpt_wallets.find(w=>w.token===peer.pool),after.bpt_wallets.find(w=>w.token===peer.pool),'peer BPT state unchanged');
 const flags=BigInt(x.packed.pool_word),swapEnabled=Boolean((flags>>252n)&1n),joinExit=Boolean((flags>>255n)&1n),allowlisted=Boolean((flags>>253n)&1n);
 assert(swapEnabled,'successful swap obeys original flag');if(isBpt)assert(joinExit,'successful BPT swap obeys join/exit flag');
 if(operation==='bpt_join'&&allowlisted)assert(before.actor_allowlists.find(a=>a.pool_name===p.name).members[c.actor],'original caller allowlist applies to BPT join');
}
function checkObservations(raw){
 assert.equal(raw.schema_version,1);assert.equal(raw.profile,'balancer-v2-managed-bpt-original-native');assert.equal(raw.authority,'independent-local-original-managed-v2-bpt');assert.equal(raw.production_ready,false);assert.equal(raw.all_requested_features_complete,false);
 eq(Object.keys(raw.branches),['settled','scheduled','controls','limits']);eq(raw.inputs.original,I.originalClosure?I.originalClosure():require('../balancer-managed-v2-scheduled-shared/inputs.cjs').closure(),'inherited original authority');
 const original=I.originalBank(),{builds,pins}=A.load(),source=raw.original_reference;
 eq(source.source_manifest,I.sourceManifest());eq(source.source_pins,pins);eq(source.compiler_jobs,original.reference.compiler_jobs);assert.equal(source.source_pins_sha256,original.reference.source_pins_sha256);eq(source.provenance,require('../balancer-managed-v2-scheduled/generate.cjs').provenance());
 const banks=new Map(raw.banks.map(b=>[b.id,b]));assert.equal(banks.size,raw.banks.length);assert.equal(new Set(raw.cases.map(c=>c.id)).size,raw.cases.length);
 for(const [namespace,b]of Object.entries(raw.branches)){
  assert.equal(b.mode,namespace);assert.equal(b.namespace,namespace);eq(b.chain_options,original.reference.chain_options);assert.equal(b.chain_options.chain.allowUnlimitedContractSize,false);assert.equal(b.chain_options.chain.allowUnlimitedInitCodeSize,false);
  const rows=raw.banks.filter(b=>b.namespace===namespace);eq(b.bank_ids,rows.map(b=>b.id));assert(rows.length>1);
  const txs=new Map(b.transactions.map(t=>[t.id,t]));assert.equal(txs.size,b.transactions.length);
  const ids=[...b.prefix_identities,...b.new_identities];assert.equal(new Set(ids.map(i=>i.address)).size,ids.length);eq(b.new_identities.map(i=>i.address).sort(),[...b.tokens,...b.pools.map(p=>p.pool)].sort());
  const abis=new Map(ids.map(i=>[i.address,new ethers.utils.Interface(A.locate(builds,i.artifact.build,i.artifact.name).c.abi)]));
  for(const t of b.transactions)Native.transaction(t,abis);for(const i of b.new_identities)validateIdentity(i,b,txs,builds);
  // Constructor/immutable/original-prefix proofs from the initial zero-breaker
  // bank remain fixed by exact runtime hashes through every later control bank.
  const authority={...raw,inputs:{original:raw.inputs.original.original}};
  for(const p of b.pools){const context=Shared.poolContext(authority,b,p,original);context.states=context.states.slice(0,1);Native.verifyBranch(context,namespace+'/'+p.name,original,builds);}
  const locked=validateInitialBptLocks(rows[0],b,txs,abis);
  const runtimeBindings=new Map(rows[0].accounts.map(a=>[a.address,a.code_sha256]));
  for(const bank of rows){
   for(const a of bank.accounts)assert.equal(a.code_sha256,runtimeBindings.get(a.address),'runtime fixed through all native controls');
   Shared.validateBank(bank,b,ids,abis);validateExtra(bank,b);validateBankBindings(bank,b,rows[0],locked);
  }
  const cases=raw.cases.filter(c=>c.namespace===namespace),controls=raw.controls.filter(c=>c.namespace===namespace),swaps=b.transactions.filter(t=>t.method==='swap');
  assert.equal(new Set(cases.map(c=>c.transaction)).size,cases.length,'unique native BPT execution refs');eq(cases.map(c=>c.transaction).sort(),swaps.map(t=>t.id).sort(),'all original swap attempts represented once');
  for(let index=0;index<cases.length;index++){const c=cases[index];assert.equal(c.id,namespace+'/C'+index);validateSwap(c,b,banks,txs,abis);}
  const eventByAfter=new Map();for(const c of [...cases,...controls]){assert(!eventByAfter.has(c.after));eventByAfter.set(c.after,c);}
  const clocks=new Map(b.clock_operations.map(c=>[c.header.hash,c]));assert.equal(clocks.size,b.clock_operations.length);
  let seen=0,clockSeen=0;
  for(let index=1;index<rows.length;index++){
   const before=rows[index-1],after=rows[index];assert.equal(after.header.parentHash,before.header.hash);assert.equal(BigInt(after.header.number),BigInt(before.header.number)+1n);
   const event=eventByAfter.get(after.id);
   if(event){assert.equal(event.before,before.id,'closed control/swap bank timeline');const t=lookup(event.transaction,txs);eq(before.header,t.pre_header);eq(after.header,t.execution_header);if(!Object.hasOwn(event,'operation')){assert.equal(t.transaction.from,b.roles.manager);assert.equal(t.transaction.to,b.pools.find(p=>p.name===event.pool_name).pool);assert.equal(t.method,event.method);eq(t.args,event.args);assert.equal(t.receipt.status,'0x1');nativeAccounts(before,after,t);}seen++;}
   else{const clock=clocks.get(after.header.hash);assert(clock,'all native bank changes require original control/swap/clock');eq(clock.pre_header,before.header);eq(clock.header,after.header);assert.equal(BigInt(after.header.timestamp),BigInt(clock.timestamp));eq(after.header.transactions,[]);clockSeen++;}
  }
  for(let index=1;index<rows.length;index++){
   const before=rows[index-1],after=rows[index],event=eventByAfter.get(after.id),tx=event?lookup(event.transaction,txs):null;
   for(const p of b.pools){
    const x=before.bpt_wallets.find(w=>w.token===p.pool),y=after.bpt_wallets.find(w=>w.token===p.pool),state=before.pools.find(q=>q.name===p.name).state,afterState=after.pools.find(q=>q.name===p.name).state;
    const applied=applyBptTransfers(x.balances,BigInt(state.aum.total_supply),tx?bptTransfers(tx,p.pool,abis.get(p.pool)):[]);
    eq(applied.balances,y.balances,'every BPT holder transition matches original Transfer events exactly');assert.equal(applied.total,BigInt(afterState.aum.total_supply),'original mint supply matches Transfer event totals');
   }
  }
  assert.equal(seen,cases.length+controls.length);assert.equal(clockSeen,b.clock_operations.length);
  const initial=BigInt(rows[0].header.number),used=new Set([...cases,...controls].map(c=>c.transaction));eq(b.transactions.filter(t=>BigInt(t.pre_header.number)>=initial).map(t=>t.id).sort(),[...used].sort(),'no hidden post-initialization operation');
 }
 const coverage={namespaces:Object.keys(raw.branches),banks:raw.banks.length,swap_attempts:raw.cases.length,successful_swaps:raw.cases.filter(c=>c.expected_native.status===1).length,reverted_swaps:raw.cases.filter(c=>c.expected_native.status===0).length,BPT_join_attempts:raw.cases.filter(c=>c.operation==='bpt_join').length,BPT_exit_attempts:raw.cases.filter(c=>c.operation==='bpt_exit').length,clock_checkpoints:raw.checkpoints.length};eq(raw.coverage,coverage);
 assert(raw.cases.some(c=>c.label==='minimum-18-join'&&c.expected_native.status===1&&c.expected_native.actual_amount_out==='0'),'original zero-output minimum must remain observed');
 return coverage;
}
function check(raw){eq(raw.inputs,I.closure(),'complete BPT source authority');return checkObservations(raw);}
module.exports={check,checkObservations,validateSwap,validateExtra,validateBoundBank,validateAumRaw};
