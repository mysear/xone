'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../../..'),ref=path.join(root,'integration/tools/reference/infinity-bin-service');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
function requireThat(ok,message){if(!ok)throw Error(message);}
const base=path.join(root,'integration/.runtime/singleton-reference');
const nodeReceipt=path.join(ref,'check-node-receipt.json');
if(require.main===module)fs.writeFileSync(nodeReceipt,JSON.stringify({passed:false,state:'RUNNING'})+'\n');
const {ethers}=require(path.join(base,'node_modules/ethers'));
const json=p=>JSON.parse(fs.readFileSync(p));
const W=x=>ethers.utils.hexZeroPad(ethers.BigNumber.from(x.toString()).toHexString(),32).toLowerCase();
const add=(s,n)=>W(BigInt(s)+BigInt(n));
const map=(key,slot)=>ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['bytes32','uint256'],[W(key),slot.toString()]));
const hex=(x,bytes)=>typeof x==='string'&&new RegExp('^0x[0-9a-f]{'+(bytes*2)+'}$').test(x);
const eq=(a,b,message)=>requireThat(JSON.stringify(a)===JSON.stringify(b),message);
const dec=(x,message='unsigned decimal')=>{requireThat(typeof x==='string'&&/^(0|[1-9][0-9]*)$/.test(x),message);return BigInt(x);};
const ANCHORS={manager:'5855fc3415d02c1b7b5a6c78abe09c9fa8d75e2f1c53e4f44f87308847260b84',vault:'41e153c88b63adef8484268a24d4d5b1ed96587fc5d380022ec5feee842d088e',immutable:[1921,2767,3756,6803,7198],selectors:{'getSlot0(bytes32)':'c815641c','getBin(bytes32,uint24)':'4580c3c5','getNextNonEmptyBin(bytes32,bool,uint24)':'bf7b7286','extsload(bytes32)':'1e2eaeaf','extsload(bytes32[])':'dbd035ff','poolIdToPoolKey(bytes32)':'0e2d484a','vault()':'fbfa77cf'}};
function artifacts(){const a={};for(const n of ['BinPoolManager','Vault','Math','Currency'])a[n]=json(path.join(ref,n+'.artifact.json'));return a;}
function validateDocument(f){
 requireThat(f.schema==='infinity-bin-service/1','schema');const r=f.reference,a=artifacts(),M=f.manager,V=f.vault;
 requireThat(r.revision==='3c5ef1be1f71442d8bc60cc6fd11f9c677b714df','revision');
 requireThat(r.compiler==='0.8.26+commit.8a97fa7a.Emscripten.clang','compiler');
 requireThat(r.unlimited_contract_size===false&&r.eip170_limit===24576,'normal EIP170');
 requireThat(r.scope.full_manager_swap_settlement_verified===false&&r.scope.vault_transient_storage_execution_verified===false,'scope');
 eq(r.execution_engine,{name:'ganache',version:'7.9.2',hardfork:'shanghai',chain_id:31337},'engine');
 requireThat(hex(M,20)&&hex(V,20)&&M!==V&&BigInt(M)!==0n&&BigInt(V)!==0n,'manager/vault addresses');
 requireThat(f.tokens.length===2&&f.pools.length===2&&f.trades.length===4&&f.RPCbanks.length===5,'bounded fixture shape');
 const token=f.tokens.map(t=>t.address);requireThat(token.every(t=>hex(t,20)&&BigInt(t)>0n)&&BigInt(token[0])<BigInt(token[1]),'sorted nonzero ERC20');
 requireThat(token.every(t=>t!==M&&t!==V),'currency distinct from infrastructure');
 const bytes=c=>Buffer.from(c.slice(2),'hex');
 requireThat(sha(bytes(a.BinPoolManager.runtime_template))===ANCHORS.manager&&sha(bytes(a.Vault.runtime_template))===ANCHORS.vault,'pinned original code');
 const patches=Object.values(a.BinPoolManager.immutable_references).flat();
 eq(patches.map(x=>x.start),ANCHORS.immutable,'immutable offsets');requireThat(patches.every(x=>x.length===32),'immutable lengths');eq(r.immutable_references,a.BinPoolManager.immutable_references,'immutable reference binding');
 requireThat(r.immutable_vault===V,'immutable Vault metadata');
 for(const [sig,selector]of Object.entries(ANCHORS.selectors))requireThat(a.BinPoolManager.selectors[sig]===selector,'original selector '+sig);
 const sl=a.BinPoolManager.storage_layout;for(const [label,slot]of Object.entries({pools:'5',poolIdToPoolKey:'6',protocolFeesAccrued:'2',_paused:'1'}))requireThat(sl.storage.find(x=>x.label===label)?.slot===slot,'layout '+label);
 const state=Object.values(sl.types).find(x=>x.label==='struct BinPool.State');eq(Object.fromEntries(state.members.map(m=>[m.label,m.slot])),{slot0:'0',reserveOfBin:'1',shareOfBin:'2',positions:'3',level0:'4',level1:'5',level2:'6'},'state offsets');
 const mi=new ethers.utils.Interface(a.BinPoolManager.abi),vi=new ethers.utils.Interface(a.Vault.abi),ti=new ethers.utils.Interface(a.Currency.abi);
 function result(b,address,iface,name,args=[]){const key=iface.encodeFunctionData(name,args).toLowerCase(),raw=b.callmap[address]?.[key];requireThat(typeof raw==='string'&&/^0x([0-9a-f]{2})*$/.test(raw),'missing RPC '+name);return iface.decodeFunctionResult(name,raw);}
 function word(b,address,slot){const w=b.storage[address]?.[slot];requireThat(hex(w,32),'missing storage '+slot);return w;}
 const poolIds=new Set();
 for(const p of f.pools){
  requireThat(!poolIds.has(p.id),'duplicate pool');poolIds.add(p.id);requireThat(hex(p.id,32),'poolId');
  requireThat(Number.isInteger(p.bin_step)&&p.bin_step>=1&&p.bin_step<=100,'bin step');
  requireThat(Number.isInteger(p.lp_fee)&&p.lp_fee>=0&&p.lp_fee<=100000,'static fee');
  for(const fee of [p.protocol_fee0,p.protocol_fee1])requireThat(Number.isInteger(fee)&&fee>=0&&fee<=4000,'protocol per direction');
  requireThat(p.key.currency0===token[0]&&p.key.currency1===token[1]&&p.key.hooks===ethers.constants.AddressZero&&p.key.poolManager===M&&p.key.fee===p.lp_fee&&p.key.parameters===W(BigInt(p.bin_step)<<16n),'original static profile');
  const id=ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['tuple(address,address,address,address,uint24,bytes32)'],[[p.key.currency0,p.key.currency1,p.key.hooks,p.key.poolManager,p.key.fee,p.key.parameters]]));requireThat(id===p.id,'pool ID/key');
  requireThat(p.pool_base===map(id,5)&&p.key_base===map(id,6),'mapping base');
  requireThat(p.bin_ids.length>=7&&p.bin_ids.every((id,i)=>Number.isInteger(id)&&id>0&&id<0xffffff&&(i===0||id>p.bin_ids[i-1])),'initialized IDs');
 }
 let lastBlock=-1,lastTime=-1;
 for(const [index,b]of f.RPCbanks.entries()){
  requireThat(b.id==='bank'+index&&Number.isSafeInteger(b.block.num)&&b.block.num>lastBlock&&hex(b.block.hash,32)&&b.block.hash!==W(0)&&Number.isSafeInteger(b.block.timestamp)&&b.block.timestamp>=lastTime,'block identity/order');lastBlock=b.block.num;lastTime=b.block.timestamp;
  requireThat(b.pools.length===2,'snapshot pools');
  let normalized=b.code[M]?.slice(2);requireThat(typeof normalized==='string'&&normalized.length===23852*2,'manager runtime size');
  for(const patch of patches){requireThat(normalized.slice(patch.start*2,(patch.start+32)*2)===W(V).slice(2),'original immutable Vault');normalized=normalized.slice(0,patch.start*2)+'0'.repeat(64)+normalized.slice((patch.start+32)*2);}
  requireThat('0x'+normalized===a.BinPoolManager.runtime_template,'original manager template admission');requireThat(b.code[V]===a.Vault.runtime_template,'original Vault admission');
  requireThat(sha(bytes(b.code[M]))===r.manager_runtime_sha256&&ethers.utils.keccak256(b.code[M])===r.manager_runtime_keccak256,'manager codehash');
  requireThat(sha(bytes(b.code[V]))===r.vault_runtime_sha256&&ethers.utils.keccak256(b.code[V])===r.vault_runtime_keccak256,'vault codehash');
  requireThat(result(b,M,mi,'vault')[0].toLowerCase()===V,'original Vault getter');
  requireThat(result(b,M,mi,'paused')[0]===false&&BigInt(word(b,M,W(1)))===0n,'paused guard');
  requireThat(Number(result(b,M,mi,'maxBinStep')[0])===100&&((BigInt(word(b,M,W(3)))>>160n)&65535n)===100n,'max step getter/storage');
  requireThat(result(b,V,vi,'isAppRegistered',[M])[0]===true&&BigInt(word(b,V,map(M,5)))===1n,'registered original app');
  const protocol={},reserves={};
  for(const t of f.tokens){requireThat(b.code[t.address]===a.Currency.runtime_template,'ERC20 runtime');requireThat(Number(result(b,t.address,ti,'decimals')[0])===t.decimals&&result(b,t.address,ti,'symbol')[0]===t.symbol,'ERC20 metadata');
   reserves[t.address]=result(b,V,vi,'reservesOfApp',[M,t.address])[0].toString();protocol[t.address]=result(b,M,mi,'protocolFeesAccrued',[t.address])[0].toString();
   requireThat(dec(reserves[t.address])===BigInt(word(b,V,map(t.address,map(M,6)))),'Vault bank getter/storage');requireThat(dec(protocol[t.address])===BigInt(word(b,M,map(t.address,2))),'protocol getter/storage');
  }
  eq(b.app_reserves,reserves,'bank reserve metadata');eq(b.protocol_fees,protocol,'bank protocol metadata');
  for(const [i,p]of f.pools.entries()){
   const s=b.pools[i];requireThat(s.pool_id===p.id&&s.lp_fee===p.lp_fee&&s.protocol_fee===p.protocol_fee0+(p.protocol_fee1<<12),'pool snapshot identity');eq(s.app_reserves,reserves,'global app reserve shared by all pools');eq(s.protocol_fees,protocol,'global protocol shared by all pools');
   requireThat(b.code[p.math_harness]===a.Math.runtime_template,'library harness identity');
   const packed=BigInt(word(b,M,p.pool_base));requireThat(s.active_id===Number(packed&0xffffffn)&&s.protocol_fee===Number((packed>>24n)&0xffffffn)&&s.lp_fee===Number((packed>>48n)&0xffffffn)&&packed>>72n===0n,'packed slot0');
   eq(result(b,M,mi,'getSlot0',[p.id]).map(Number),[s.active_id,s.protocol_fee,s.lp_fee],'original slot0 getter');requireThat(result(b,M,mi,'pools',[p.id])[0]===W(packed),'original public pools getter');
   const key=result(b,M,mi,'poolIdToPoolKey',[p.id]);eq(key.slice(0,4).map(x=>x.toLowerCase()),[p.key.currency0,p.key.currency1,p.key.hooks,p.key.poolManager],'original PoolKey getter');requireThat(Number(key[4])===p.key.fee&&key[5]===p.key.parameters,'original PoolKey profile');
   const kt=Object.values(sl.types).find(x=>x.label==='struct PoolKey');for(const member of kt.members){const val=(BigInt(word(b,M,add(p.key_base,member.slot)))>>BigInt(member.offset*8))&((1n<<BigInt(Number(sl.types[member.type].numberOfBytes)*8))-1n);requireThat(val===BigInt(p.key[member.label]),'key storage/getter '+member.label);}
   eq(Object.keys(s.bins),p.bin_ids.map(String),'complete initialized bins');let rootWord=0n;const l1={},l2={};
   for(const id of p.bin_ids){const q=s.bins[id];requireThat(q.initialized===true&&dec(q.total_shares)>0n,'depleted entries retain shares');
    const x=dec(q.reserve0),y=dec(q.reserve1);requireThat(x<1n<<128n&&y<1n<<128n,'reserve128');requireThat(BigInt(word(b,M,map(id,add(p.pool_base,1))))===x+(y<<128n),'reserve storage');requireThat(BigInt(word(b,M,map(id,add(p.pool_base,2))))===dec(q.total_shares),'share storage');
    eq(result(b,M,mi,'getBin',[p.id,id]).map(String),[q.reserve0,q.reserve1,q.liquidity,q.total_shares],'original getBin');
    requireThat(dec(q.price)>0n&&dec(q.liquidity)===x*BigInt(q.price)+(y<<128n),'original liquidity/price');
    const k1=W(id>>16),k2=W(id>>8);l2[k2]=(l2[k2]||0n)|(1n<<BigInt(id&255));l1[k1]=(l1[k1]||0n)|(1n<<BigInt((id>>8)&255));rootWord|=1n<<BigInt(id>>16);
    for(const d of [true,false]){const choices=p.bin_ids.filter(k=>d?k<id:k>id),wanted=choices.length?(d?Math.max(...choices):Math.min(...choices)):(d?0xffffff:0);requireThat(q.next[String(d)]===wanted,'native tree successor');requireThat(Number(result(b,M,mi,'getNextNonEmptyBin',[p.id,d,id])[0])===q.next[String(d)],'original tree getter');}
   }
   requireThat(s.tree.level0===W(rootWord)&&word(b,M,add(p.pool_base,4))===W(rootWord),'full root tree');
   for(const [level,values,offset]of [['level1',l1,5],['level2',l2,6]]){eq(Object.keys(s.tree[level]).sort(),Object.keys(values).sort(),'complete '+level);for(const [key,val]of Object.entries(values))requireThat(s.tree[level][key]===W(val)&&word(b,M,map(key,add(p.pool_base,offset)))===W(val),'TreeMath.add '+level);}
  }
  for(const t of token){const index=token.indexOf(t),poolSum=b.pools.reduce((n,s)=>n+BigInt(Object.values(s.bins).reduce((q,z)=>q+BigInt(z[index===0?'reserve0':'reserve1']),0n)),0n);requireThat(BigInt(reserves[t])===poolSum+BigInt(protocol[t]),'Vault reserve reconciles pool + protocol');}
  const slots=Object.keys(b.storage[M]);for(const slot of slots)requireThat(result(b,M,mi,'extsload(bytes32)',[slot])[0]===b.storage[M][slot],'original extsload');eq(result(b,M,mi,'extsload(bytes32[])',[slots])[0],slots.map(slot=>b.storage[M][slot]),'original batch extsload');
  for(const [addr,balance]of Object.entries(b.native_balances))requireThat(hex(addr,20)&&hex(balance,32)&&b.code[addr]!==undefined,'native balances');
 }
 const route=[[0,true],[1,false],[1,true],[0,false]];
 for(const [i,t]of f.trades.entries()){
  const [pi,direction]=route[i],p=f.pools[pi],pre=f.RPCbanks[i],post=f.RPCbanks[i+1],before=pre.pools[pi],after=post.pools[pi],e=t.expected;
  requireThat(t.id==='trade'+i&&t.pool_id===p.id&&t.zero_for_one===direction&&t.pre_bank===pre.id&&t.post_bank===post.id,'continuous route');if(i)requireThat(t.amount_in===f.trades[i-1].expected.amount_out,'chained exact input');
  const amount=dec(t.amount_in),output=dec(e.amount_out),protocol=dec(e.protocol_amount);requireThat(amount>0n&&output>0n,'nonzero trade');requireThat(BigInt(direction?e.delta0:e.delta1)===-amount&&BigInt(direction?e.delta1:e.delta0)===output,'native deltas');
  const pf=BigInt(direction?p.protocol_fee0:p.protocol_fee1),sf=pf+BigInt(p.lp_fee)-pf*BigInt(p.lp_fee)/1000000n;requireThat(e.swap_fee===Number(sf),'original swap fee');
  eq(e.post,after,'complete native poststate');const untouched=1-pi;eq(pre.pools[untouched].bins,post.pools[untouched].bins,'untouched pool bins');requireThat(pre.pools[untouched].active_id===post.pools[untouched].active_id,'untouched active');
  const inToken=token[direction?0:1],outToken=token[direction?1:0];for(const k of token){requireThat(dec(post.app_reserves[k])===dec(pre.app_reserves[k])+(k===inToken?amount:-output),'shared app reserve transition');requireThat(dec(post.protocol_fees[k])===dec(pre.protocol_fees[k])+(k===inToken?protocol:0n),'shared protocol transition');}
  requireThat(Array.isArray(e.bin_fills)&&e.bin_fills.length>0&&e.bin_fills.length<=32,'bounded fills');let ai=0n,ao=0n,fees=0n,pfees=0n,last=before.active_id;
  const remainingBins=JSON.parse(JSON.stringify(before.bins));
  for(const [j,fill]of e.bin_fills.entries()){
   requireThat(fill.bin_id===last,'tree traversal order');const b=remainingBins[fill.bin_id];requireThat(b&&fill.price===b.price,'original fill price');eq(fill.pre,{reserve0:b.reserve0,reserve1:b.reserve1},'fill before');
   const used=dec(fill.amount_in),out=dec(fill.amount_out),fee=dec(fill.fee_amount),prot=dec(fill.protocol_amount);requireThat(prot<=fee&&fee<=used,'fee bounds');ai+=used;ao+=out;fees+=fee;pfees+=prot;
   const inField=direction?'reserve0':'reserve1',outField=direction?'reserve1':'reserve0',expectedPost={reserve0:b.reserve0,reserve1:b.reserve1};expectedPost[inField]=(BigInt(b[inField])+used-prot).toString();expectedPost[outField]=(BigInt(b[outField])-out).toString();eq(fill.post,expectedPost,'native fill reserves');b.reserve0=fill.post.reserve0;b.reserve1=fill.post.reserve1;
   if(j+1<e.bin_fills.length)last=before.bins[fill.bin_id].next[String(direction)];
  }
  requireThat(ai===amount&&ao===output&&fees===dec(e.fee_amount)&&pfees===protocol,'native fill aggregate');requireThat(after.active_id===e.bin_fills.at(-1).bin_id,'native final active');
  for(const [id,b]of Object.entries(remainingBins)){requireThat(b.reserve0===after.bins[id].reserve0&&b.reserve1===after.bins[id].reserve1&&b.total_shares===after.bins[id].total_shares,'all native post reserves/shares');}
 }
 requireThat(f.trades.some(t=>t.expected.bin_fills.some(x=>x.amount_in==='0')),'depleted traversal exercised');
 return {banks:f.RPCbanks.length,trades:f.trades.length,manager_runtime_bytes:23852,vault_runtime_bytes:9247};
}
function validateBindings(r){
 const ce=json(path.join(ref,'compile-evidence.json'));
 const pins=json(path.join(root,'integration/tools/reference/singleton-cl/source-metadata.json'));const byName=new Map(pins.files.map(s=>[s.local,s]));requireThat(r.files.length>=30,'complete source closure');
 for(const source of r.files){eq(source,byName.get(source.local),'upstream source manifest '+source.local);requireThat(sha(fs.readFileSync(path.join(base,'sources',source.local)))===source.sha256,'changed original source '+source.local);}
 for(const source of ce.sources)requireThat(r.files.some(s=>s.local===source.local&&s.sha256===source.sha256),'missing compiler dependency');
 requireThat(sha(fs.readFileSync(path.join(base,'package-lock.json')))===r.dependency_lock_sha256,'package lock');
 for(const [name,version]of [['solc','0.8.26'],['ganache','7.9.2'],['ethers','5.8.0']]){const dep=r.dependencies[name];requireThat(dep&&dep.version===version,'dependency version');const p=path.join(root,dep.package_path),bytes=fs.readFileSync(p);requireThat(sha(bytes)===dep.package_sha256&&JSON.parse(bytes).version===version,'package binding '+name);}
 requireThat(r.loaded_dependencies.length>50&&r.binary_dependencies.length>=2,'loaded dependency + binary closure');for(const dep of [...r.loaded_dependencies,...r.binary_dependencies])requireThat(sha(fs.readFileSync(path.isAbsolute(dep.path)?dep.path:path.join(root,dep.path)))===dep.sha256,'loaded dependency changed '+dep.path);
 for(const [name,desc]of Object.entries(r.artifacts)){const b=fs.readFileSync(path.join(ref,desc.file)),a=JSON.parse(b);requireThat(sha(b)===desc.sha256,'artifact binding '+name);requireThat((a.runtime_template.length-2)/2===desc.runtime_bytes&&desc.runtime_bytes<=24576,'artifact EIP170 '+name);requireThat(sha(Buffer.from(a.runtime_template.slice(2),'hex'))===desc.runtime_template_sha256&&ethers.utils.keccak256(a.runtime_template)===desc.runtime_template_keccak256,'artifact template hash '+name);}
 return true;
}
function verify(options={}){
 const receipt=options.receipt??json(path.join(ref,'execution-receipt.json'));requireThat(receipt.passed===true,'missing successful current execution receipt');
 const paths=['integration','chain-parser-go','dex-router'].map(d=>path.join(root,d,'test/contracts/infinity-bin-service.json'));
 const b=fs.readFileSync(paths[0]);for(const p of paths)requireThat(fs.readFileSync(p).equals(b),'canonical/service copy');requireThat(sha(b)===receipt.fixture_sha256,'fixture receipt digest');
 const f=JSON.parse(b),r=f.reference;requireThat(sha(fs.readFileSync(path.join(ref,'source-metadata.json')))===receipt.source_metadata_sha256,'metadata receipt digest');eq(r,json(path.join(ref,'source-metadata.json')),'fixture metadata');
 requireThat(sha(fs.readFileSync(path.join(ref,'compile-evidence.json')))===receipt.compile_evidence_sha256,'compiler evidence receipt');
 eq(r.inputs,receipt.inputs,'input closure');for(const [p,h]of Object.entries(receipt.inputs))requireThat(sha(fs.readFileSync(path.join(root,p)))===h,'changed generator input '+p);
 const ce=json(path.join(ref,'compile-evidence.json'));eq(ce.settings,r.original_settings,'compiler settings');requireThat(ce.compile_input_sha256===r.compile_input_sha256,'compile input binding');eq(ce.contracts.BinPoolManager.storage_layout,artifacts().BinPoolManager.storage_layout,'compiled manager layout');
 validateBindings(r);
 const result=validateDocument(f);requireThat(receipt.banks===result.banks&&receipt.trades===result.trades,'receipt counts');return result;
}
module.exports={verify,validateDocument,validateBindings,requireThat};
if(require.main===module)try{const result=process.argv[2]==='--semantic-document'?validateDocument(json(process.argv[3])):verify();fs.writeFileSync(nodeReceipt,JSON.stringify({passed:true,...result})+'\n');console.log(JSON.stringify(result));}catch(e){fs.writeFileSync(nodeReceipt,JSON.stringify({passed:false,error:String(e)})+'\n');console.error('infinity-bin-service check failed: '+e.stack);process.exitCode=1;}
