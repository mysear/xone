#!/usr/bin/env node
'use strict';
// Original-runtime eth_call source. Every fork state read is authenticated by
// the retained MPT; missing nodes fail closed. No public network exists here.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const ROOT=path.resolve(__dirname,'../../..');
const {ethers}=require(path.join(ROOT,'integration/.runtime/stable-reference/node_modules/ethers'));
const {Trie}=require(path.join(ROOT,'integration/.runtime/receipt-root-reference/node_modules/@ethereumjs/trie'));
const ganache=require(path.join(ROOT,'integration/.runtime/stable-reference/node_modules/ganache'));
const {verifyEvidence}=require('./r01_public_catalog_evm_commitments.cjs');
const {verifyPrestate}=require('./r01_public_catalog_proof.cjs');
const {traceProof}=require('./r01_public_catalog_evm_trie.cjs');
const load=p=>JSON.parse(fs.readFileSync(p)),bytes=s=>Buffer.from(s.slice(2),'hex'),hex=b=>'0x'+Buffer.from(b).toString('hex'),sha=b=>crypto.createHash('sha256').update(b).digest('hex'),hash=ethers.utils.keccak256;
const ref=p=>({path:path.relative(ROOT,p),sha256:sha(fs.readFileSync(p))});
const readRef=r=>{assert(typeof r.path==='string'&&!path.isAbsolute(r.path)&&!r.path.split('/').includes('..'),'READONLY_REF_PATH');const b=fs.readFileSync(path.join(ROOT,r.path));assert.equal(sha(b),r.sha256,'READONLY_SOURCE_HASH');return JSON.parse(b);};
const uint=h=>h==='0x'?0n:BigInt(h),word=h=>'0x'+uint(h).toString(16).padStart(64,'0');
const resolvePath=(root,key,nodes)=>traceProof(root,key,nodes);

async function loadBank(directory,supplements=[]){
 directory=path.resolve(directory);const ep=path.join(directory,fs.existsSync(path.join(directory,'prestate-evidence-v2.json'))?'prestate-evidence-v2.json':'prestate-evidence.json'),evidence=load(ep);
 assert(['base','optimism','polygon','arbitrum'].includes(evidence.chain),'READONLY_SIX_CASE_CHAIN');
 const validation=await verifyEvidence(evidence,[]),anchor=load(path.join(directory,'anchor.json')),original=readRef(anchor.header).response.result;
 const bank={directory,evidence,validation,anchor,original,accounts:new Map(),accountNodes:new Set(),headers:new Map(),ledger:[],misses:[],stateCache:new Map()};
 const keepHeader=h=>{if(h&&h.hash&&h.number){bank.headers.set(h.number,h);bank.headers.set(h.hash,h);}};keepHeader(original);
 for(const r of load(path.join(directory,'rpc-index.json')).records){const row=readRef(r);if(['eth_getBlockByNumber','eth_getBlockByHash'].includes(row.method)&&row.response?.result)keepHeader(row.response.result);}
 for(const r of [anchor.genesis,anchor.witness].filter(Boolean)){const row=readRef(r);if(row.response?.result)keepHeader(row.response.result);}
 function add(deployment,provenance){
  const a=deployment.address.toLowerCase(),p=deployment.proof;for(const node of p.accountProof)bank.accountNodes.add(node);
  const accountPath=resolvePath(original.stateRoot,hash(a),p.accountProof),exists=accountPath.value!==null,effectiveStorageRoot=exists?ethers.utils.RLP.decode(accountPath.value)[2]:hash('0x80');
  if(!bank.accounts.has(a))bank.accounts.set(a,{...deployment,exists,effectiveStorageRoot,nodes:new Set(),sources:[]});
  const account=bank.accounts.get(a);assert.equal(account.code,deployment.code,'SUPPLEMENT_CODE_CHANGED');assert.equal(account.proof.storageHash,p.storageHash,'SUPPLEMENT_STORAGE_ROOT_CHANGED');
  for(const slot of p.storageProof)for(const node of slot.proof)account.nodes.add(node);account.sources.push(provenance);
 }
 for(const d of evidence.deployments)add(d,ref(ep));
 for(const supplement of supplements){
  const wire=load(path.resolve(supplement));assert.equal(wire.method,'eth_getProof','SUPPLEMENT_METHOD');assert.equal(wire.params[2],original.number,'SUPPLEMENT_FIXED_H');const p=wire.response?.result;assert(p,'SUPPLEMENT_RESULT');assert.equal(p.address.toLowerCase(),wire.params[0].toLowerCase(),'SUPPLEMENT_ADDRESS');
  const prior=bank.accounts.get(p.address.toLowerCase());assert(prior,'SUPPLEMENT_UNKNOWN_RUNTIME');const deployment={address:p.address.toLowerCase(),code:prior.code,proof:p};
  await verifyPrestate({...evidence,chain_id:'1',deployments:[deployment]},[],{allowAbsentRPCZeroHashes:true});add(deployment,ref(path.resolve(supplement)));
 }
 function account(address){
  address=address.toLowerCase();if(bank.accounts.has(address))return bank.accounts.get(address);
  const traced=resolvePath(original.stateRoot,hash(address),[...bank.accountNodes]);
  assert.equal(traced.value,null,'READONLY_ACCOUNT_RUNTIME_NOT_CAPTURED:'+address);
  const p={address,nonce:'0x0',balance:'0x0',codeHash:hash('0x'),storageHash:hash('0x80'),accountProof:traced.nodes.filter(n=>!n.inline).map(n=>n.encoded),storageProof:[]};const absent={address,code:'0x',proof:p,exists:false,effectiveStorageRoot:hash('0x80'),nodes:new Set(),sources:[ref(ep)],derived_absence:traced};bank.accounts.set(address,absent);return absent;
 }
 bank.storage=async(address,key)=>{
  address=address.toLowerCase();key=word(key);const id=address+':'+key;if(bank.stateCache.has(id))return bank.stateCache.get(id);
  let ac;try{ac=account(address);}catch(e){bank.misses.push({kind:'account',address,error:e.message});throw e;}
  const root=ac.effectiveStorageRoot||(ac.derived_absence?hash('0x80'):ac.proof.storageHash);
  let traced,standard;try{
   if(root===hash('0x80')){traced=resolvePath(root,hash(key),[]);standard=null;}
   else{if(!ac.trie)ac.trie=await Trie.createFromProof([...ac.nodes].map(bytes),{root:bytes(root),useKeyHashing:false},true);standard=await ac.trie.get(bytes(hash(key)),true);const selected=await ac.trie.createProof(bytes(hash(key)));traced=resolvePath(root,hash(key),selected.map(hex));}
  }catch(e){const miss={kind:'storage',address,key,root,error:e.message};bank.misses.push(miss);throw Error('READONLY_MPT_STORAGE_MISSING:'+JSON.stringify(miss));}
  assert.equal(traced.value,standard===null?null:hex(standard),'READONLY_PATH_CROSSCHECK');
  const v=traced.value===null?'0x':ethers.utils.RLP.decode(traced.value);assert(typeof v==='string','READONLY_STORAGE_RLP');assert(v==='0x'||!v.slice(2).startsWith('00'),'READONLY_STORAGE_LEADING_ZERO');
  const result={address,key,root,value:word(v),quantity:'0x'+uint(v).toString(16),proof_nodes:traced.nodes.filter(n=>!n.inline).map(n=>n.encoded),path:traced.status,source_records:ac.sources,authority:ac.exists===false?'MPT authenticated account absence implies canonical empty storage':'MPT storage path under authenticated fixed-H account leaf'};
  bank.stateCache.set(id,result);bank.ledger.push(result);return result;
 };
 bank.request=async({method,params=[]})=>{
  if(method==='eth_chainId')return '0x'+BigInt(evidence.chain_id).toString(16);
  if(method==='net_version')return evidence.chain_id;
  if(method==='eth_blockNumber')return original.number;
  if(['eth_getBlockByNumber','eth_getBlockByHash'].includes(method)){
   const key=['latest','safe','finalized'].includes(params[0])?original.number:params[0]==='earliest'?'0x0':params[0];const h=bank.headers.get(key);assert(h,'READONLY_HEADER_NOT_CAPTURED:'+key);
   return {...h,transactions:[]}; // Declared local fork initialization view, never normal parser's original body.
  }
  assert(['eth_getStorageAt','eth_getCode','eth_getBalance','eth_getTransactionCount'].includes(method),'READ_ONLY_FORK_METHOD:'+method);
  const at=method==='eth_getStorageAt'?params[2]:params[1];assert(at===original.number||at==='latest'||(typeof at==='object'&&at.blockHash===original.hash&&at.requireCanonical===true),'READONLY_FIXED_H');
  if(method==='eth_getStorageAt')return (await bank.storage(params[0],params[1])).value;
  let a;try{a=account(params[0]);}catch(e){bank.misses.push({kind:'account',address:params[0],error:e.message});throw e;}
  const value=method==='eth_getCode'?a.code:method==='eth_getBalance'?a.proof.balance:a.proof.nonce;
  const codeObservation=method==='eth_getCode'&&value!=='0x'?{value_sha256:sha(bytes(value)),value_keccak256:hash(value),value_bytes:bytes(value).length,code_source:ref(ep),code_json_pointer:'/deployments/'+evidence.deployments.findIndex(d=>d.address.toLowerCase()===params[0].toLowerCase())+'/code'}:{value};bank.ledger.push({method,address:params[0].toLowerCase(),...codeObservation,source_records:a.sources,authority:'MPT account/code fixed-H commitment; full original bytes retained once at code_source'});return value;
 };
 return bank;
}

async function runtime(bank){
 const ex=load(path.join(bank.directory,'fork-execution.json')),sender=ex.sender;
 const raw=ganache.provider({fork:{provider:{request:q=>bank.request(q)},blockNumber:bank.validation.height,disableCache:true},wallet:{totalAccounts:0},chain:{chainId:Number(bank.evidence.chain_id),networkId:Number(bank.evidence.chain_id),hardfork:'shanghai',time:new Date(BigInt(bank.original.timestamp)*1000n>0n?Number(BigInt(bank.original.timestamp)*1000n):0)},miner:{blockGasLimit:30000000,coinbase:'0x000000000000000000000000000000000000cafe'},logging:{quiet:true}});
 await raw.request({method:'eth_chainId',params:[]});
 let current=null;
 raw.on('ganache:vm:tx:step',event=>{
  if(!current)return;const s=event.data,op=s.opcode.name,address=hex(s.address),code_address=hex(s.codeAddress);
  current.steps++;current.code_addresses.add(code_address);
  if(current.pending_environment){current.pending_environment.value='0x'+s.stack.at(-1).toString(16);current.pending_environment=null;}
  if(current.pending){const p=current.pending;p.value='0x'+s.stack.at(-1).toString(16).padStart(64,'0');current.storage.push(p);current.pending=null;}
  if(op==='SLOAD')current.pending={address,key:'0x'+s.stack.at(-1).toString(16).padStart(64,'0'),code_address};
  if(['SSTORE','TSTORE','CREATE','CREATE2','SELFDESTRUCT','LOG0','LOG1','LOG2','LOG3','LOG4'].includes(op))current.forbidden.push({op,address,pc:s.pc.toString()});
  if(['TIMESTAMP','NUMBER','COINBASE','BASEFEE','PREVRANDAO','DIFFICULTY','BLOCKHASH','CHAINID','ORIGIN','CALLER','GASPRICE','GASLIMIT'].includes(op)){const item={op,address,pc:s.pc.toString()};current.environment.push(item);current.pending_environment=item;}
 });
 const view=async(to,data)=>{
  const start=bank.ledger.length,beforeMiss=bank.misses.length;current={steps:0,storage:[],code_addresses:new Set(),forbidden:[],environment:[],pending:null,pending_environment:null};let value,error;
  const request={from:sender,to,data,gas:'0x989680',gasPrice:'0x0'};
  try{value=await raw.request({method:'eth_call',params:[request,bank.original.number]});assert(current.steps>0,'READONLY_VM_STEP_CAPTURE_UNAVAILABLE');assert.equal(current.forbidden.length,0,'READONLY_MUTATING_GETTER');
   const environmentExpected={TIMESTAMP:bank.original.timestamp,NUMBER:bank.original.number,CHAINID:'0x'+BigInt(bank.evidence.chain_id).toString(16),BASEFEE:bank.original.baseFeePerGas,ORIGIN:sender,GASPRICE:'0x0'};
   for(const item of current.environment){if(environmentExpected[item.op]!==undefined){item.expected=environmentExpected[item.op];assert.equal(BigInt(item.value),BigInt(item.expected),'READONLY_GETTER_ENVIRONMENT:'+item.op);}assert(item.op!=='BLOCKHASH','READONLY_GETTER_BLOCKHASH_UNBOUND');}
   for(const s of current.storage){const p=await bank.storage(s.address,s.key);assert.equal(s.value,p.value,'READONLY_SLOAD_MPT_DIFFERENCE');}
   for(const a of current.code_addresses)await bank.request({method:'eth_getCode',params:[a,bank.original.number]});
  }catch(e){error=e.message;}
  const report={to,data,request,block_tag:bank.original.number,value:value??null,error:error??null,steps:current.steps,sloads:current.storage,code_addresses:[...current.code_addresses],forbidden:current.forbidden,environment:current.environment,misses:bank.misses.slice(beforeMiss),fork_reads:bank.ledger.slice(start),all_code_and_storage_MPT_bound:!error,source_kind:'original-runtime-read-only-local-VM-execution; derived result, not a public eth_call wire response'};
  current=null;return report;
 };
 return {raw,view,close:()=>raw.disconnect()};
}

async function probe(inputFile,out){
 const input=load(inputFile),row=input.row;assert(['base-uniswap-v3','base-aerodrome-v2','base-aerodrome-slipstream','optimism-uniswap-v3','polygon-uniswap-v3','arbitrum-uniswap-v3'].includes(row.id),'BOUNDED_SIX_CASE_SCOPE');
 for(const r of Object.values(row.inputs))if(r&&typeof r==='object'&&r.path&&r.sha256)readRef(r);
 const bank=await loadBank(path.join(ROOT,row.directory),(input.supplements||[]).map(r=>{readRef(r);return path.join(ROOT,r.path);}));assert.equal(bank.validation.height,row.anchor.height,'READONLY_ROW_H');assert.equal(bank.evidence.block.hash,row.anchor.hash,'READONLY_ROW_HASH');assert.equal(bank.evidence.chain_id,String(row.chain_id),'READONLY_ROW_CHAIN');assert.equal(sha(bytes(bank.accounts.get(row.pool).code)),row.pool_runtime_sha256,'READONLY_ROW_RUNTIME');assert(bank.accounts.has(row.factory)&&sha(bytes(bank.accounts.get(row.factory).code))===row.factory_runtime_sha256,'READONLY_ROW_FACTORY_RUNTIME');
 if(row.implementation_address){const m=/^0x363d3d373d3d3d363d73([0-9a-f]{40})5af43d82803e903d91602b57fd5bf3$/.exec(bank.accounts.get(row.pool).code);assert(m&&'0x'+m[1]===row.implementation_address,'READONLY_PROXY_TARGET');assert(bank.accounts.has(row.implementation_address)&&sha(bytes(bank.accounts.get(row.implementation_address).code))===row.implementation_runtime_sha256,'READONLY_ROW_IMPLEMENTATION_RUNTIME');}
 const vm=await runtime(bank),calls=[];let failure,admissionRejection;
 const call=async(a,d)=>{const r=await vm.view(a,d);calls.push(r);if(r.error)throw Error(r.error);return r.value;};
 try{
  const pool=row.pool;
  for(const d of ['0x0dfe1681','0xd21220a7','0xc45a0155'])await call(pool,d);
  if(row.protocol==='aerodrome-v2'){
   const meta=await call(pool,'0x392f37e9'),stable=BigInt('0x'+meta.slice(2+4*64,2+5*64));await call(row.factory,'0xb187bd26');await call(row.factory,'0xcc56b2c5'+pool.slice(2).padStart(64,'0')+stable.toString(16).padStart(64,'0'));
  }else{
   for(const d of ['0xddca3f43','0xd0c93a7c','0x3850c7bd','0x1a686502'])await call(pool,d);
   if(row.protocol==='aerodrome-slipstream'){const swap=await call(row.factory,'0x23c43a51'),unstaked=await call(row.factory,'0x7693bc11');if(BigInt(swap)!==0n||BigInt(unstaked)!==0n)throw Object.assign(Error('SLIPSTREAM_FEE_MODULE_UNSUPPORTED'),{admission:true});}
   const get=d=>calls.find(c=>c.to===pool&&c.data===d).value;
   const space=Number(BigInt(get('0xd0c93a7c'))),slot0=get('0x3850c7bd');let tick=BigInt('0x'+slot0.slice(66,130));if(tick>>255n)tick-=1n<<256n;
   const center=Math.floor(Math.floor(Number(tick)/space)/256),arg=n=>(BigInt(n)+(n<0?1n<<256n:0n)).toString(16).padStart(64,'0');
   const bitmap=BigInt(await call(pool,'0x5339c296'+arg(center)));
   for(let bit=0;bit<256;bit++)if(bitmap>>BigInt(bit)&1n)await call(pool,'0xf30dba93'+arg((center*256+bit)*space));
  }
 }catch(e){if(e.admission)admissionRejection=e.message;else failure=e.message;}finally{await vm.close();}
 const result={schema_version:4,trace_format:'deduplicated-code-ledger/v1',id:row.id,anchor:row.anchor,input:ref(path.resolve(inputFile)),source_prestate:ref(path.join(bank.directory,fs.existsSync(path.join(bank.directory,'prestate-evidence-v2.json'))?'prestate-evidence-v2.json':'prestate-evidence.json')),passed:!failure,error:failure??null,normal_admission_rejection:admissionRejection??null,calls,authenticated_reads:bank.ledger,missing:bank.misses,public_network_requests:0,public_writes:0,local_hardfork:'shanghai',caller:load(path.join(bank.directory,'fork-execution.json')).sender,caller_policy:'existing public EOA, authenticated original balance/nonce; local eth_call only; no original state override',normal_parser_typed_baseline_injected:false};
 fs.writeFileSync(out,JSON.stringify(result,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify({id:row.id,passed:result.passed,calls:calls.length,error:result.error,missing:result.missing},null,2));return result;
}
module.exports={loadBank,resolvePath,runtime,probe};
if(require.main===module){probe(path.resolve(process.argv[2]),path.resolve(process.argv[3])).catch(e=>{console.error(e.stack);process.exit(1)});}
