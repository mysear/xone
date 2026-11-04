'use strict';
// Independent observer validator. It never imports the generator or computes a Router quote.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),zlib=require('node:zlib');
const assert=require('node:assert/strict');
const ROOT=path.resolve(__dirname,'../../../..');
const {createRequire}=require('node:module');
const rr=createRequire(path.join(ROOT,'integration/.runtime/stable-reference/package.json'));
const {ethers}=rr('ethers');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const codeSha=x=>sha(Buffer.from(x.slice(2),'hex'));
const word=x=>'0x'+BigInt(x).toString(16).padStart(64,'0');
const mapping=(key,slot)=>ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['address','uint256'],[key,slot]));
const PROFILE='uniswap-v2-complete', ADAPTER='uniswap-v2/core-1.0.1-full-swap-1';
const PAIR='8b5db55fa9ab3b9527508d4abe0b39eb588bf310270c8e04b3f38214e8ba63b4';
const FACTORY='3abc53f12a9cb8ae37ebfada9efc261c1ab4c2759d161e341a49bf67df3f8321';
const INVENTORY=[
 'sync-0-to-1','sync-1-to-0',
 'donate0-0-to-1','donate0-1-to-0','donate1-0-to-1','donate1-1-to-0','donate-both-0-to-1','donate-both-1-to-0',
 'donation-sequence-forward-1','donation-sequence-forward-2','donation-sequence-reverse-1','donation-sequence-reverse-2',
 'after-sync-0-to-1','after-sync-1-to-0','after-skim-0-to-1','after-skim-1-to-0',
 'uint32-before-wrap','uint32-after-wrap','cumulative0-wrap','cumulative1-wrap',
 'reserve-overflow-0','reserve-overflow-1','pair-k-revert-0','pair-k-revert-1'];
const SEQUENCES=[['donation-sequence-forward-1','donation-sequence-forward-2','donation-sequence-reverse-1','donation-sequence-reverse-2'],['after-sync-0-to-1','after-sync-1-to-0'],['after-skim-0-to-1','after-skim-1-to-0'],['uint32-before-wrap','uint32-after-wrap']];
const decimal=x=>assert(typeof x==='string'&&/^(0|[1-9][0-9]*)$/.test(x),'decimal amount');
const uint=(x,bits)=>{decimal(x);assert(BigInt(x)<(1n<<BigInt(bits)),'amount width');};
const number=x=>assert(Number.isSafeInteger(x)&&x>=0,'lossless u64 number');
const hash=x=>assert(/^0x[0-9a-f]{64}$/.test(x),'lowercase hash');
const eq=(a,b,m)=>assert.deepEqual(a,b,m);
function headerHash(b){
 const fields=['parentHash','sha3Uncles','miner','stateRoot','transactionsRoot','receiptsRoot','logsBloom','difficulty','number','gasLimit','gasUsed','timestamp','extraData','mixHash','nonce','baseFeePerGas','withdrawalsRoot'];
 const numeric=new Set(['difficulty','number','gasLimit','gasUsed','timestamp','baseFeePerGas']);
 const quantity=v=>{const n=BigInt(v),s=n.toString(16);return n===0n?'0x':'0x'+(s.length%2?'0':'')+s;};
 eq(ethers.utils.keccak256(ethers.utils.RLP.encode(fields.map(k=>numeric.has(k)?quantity(b[k]):b[k]))),b.hash,'raw Shanghai header hash including PREVRANDAO');
}
function traceAccesses(trace,t,identities){
 const initial=t.transaction.to||t.receipt.contractAddress,contexts=new Map([[trace.structLogs[0].depth,initial]]),accesses=new Map();let last;
 for(const log of trace.structLogs){if(last&&log.depth>last.depth){eq(log.depth,last.depth+1);let context;
  if(last.op==='CREATE'||last.op==='CREATE2'){const id=identities.find(v=>v.creation_transaction_hash===t.receipt.transactionHash);assert(id);context=id.address;}
  else {assert(['CALL','STATICCALL','DELEGATECALL','CALLCODE'].includes(last.op));context=['DELEGATECALL','CALLCODE'].includes(last.op)?contexts.get(last.depth):'0x'+last.stack.at(-2).slice(-40);}
  contexts.set(log.depth,context);
 }const address=contexts.get(log.depth);assert(address);if(log.op==='SLOAD'||log.op==='SSTORE'){const slot=word('0x'+log.stack.at(-1)),key=address+'/'+slot;if(!accesses.has(key))accesses.set(key,{address,slot,operations:new Set()});accesses.get(key).operations.add(log.op);}last=log;}
 return [...accesses.values()].sort((a,b)=>(a.address+a.slot).localeCompare(b.address+b.slot)).map(a=>({...a,operations:[...a.operations].sort()}));
}

const sameSlots=(accounts,a,s)=>{const account=accounts.find(x=>x.address===a);assert(account,`account omitted ${a}`);const v=account.storage.find(x=>x.slot===word(s));assert(v,`slot omitted ${a}/${s}`);return BigInt(v.value);};
function pool(p,anchor,accounts,getters) {
 eq(p.protocol,PROFILE);eq(p.adapter_version,ADAPTER);eq(p.token_behavior,'standard');eq(p.fee_numerator,3);eq(p.fee_denominator,1000);
 uint(p.reserve0,112);uint(p.reserve1,112);eq(p.v2.code_sha256,PAIR);
 const s=p.v2.swap;assert(s,'complete swap state required');
 eq(Object.keys(s).sort(),['raw_balance0','raw_balance1','block_timestamp_last','price0_cumulative_last','price1_cumulative_last','unlocked','factory_code_sha256','bank_block_number','bank_block_hash','bank_unix_timestamp'].sort());
 uint(s.raw_balance0,256);uint(s.raw_balance1,256);uint(s.price0_cumulative_last,256);uint(s.price1_cumulative_last,256);
 number(s.block_timestamp_last);assert(s.block_timestamp_last<2**32);eq(s.unlocked,true);eq(s.factory_code_sha256,FACTORY);
 number(s.bank_block_number);number(s.bank_unix_timestamp);hash(s.bank_block_hash);
 eq([s.bank_block_number,s.bank_block_hash,s.bank_unix_timestamp],[anchor.block_number,anchor.block_hash,anchor.timestamp],'source H bank retained');
 eq(sameSlots(accounts,p.pool_id,8),BigInt(p.reserve0)+(BigInt(p.reserve1)<<112n)+(BigInt(s.block_timestamp_last)<<224n));
 eq(sameSlots(accounts,p.pool_id,9),BigInt(s.price0_cumulative_last));eq(sameSlots(accounts,p.pool_id,10),BigInt(s.price1_cumulative_last));eq(sameSlots(accounts,p.pool_id,12),1n);
 eq(sameSlots(accounts,p.token0,mapping(p.pool_id,1)),BigInt(s.raw_balance0));eq(sameSlots(accounts,p.token1,mapping(p.pool_id,1)),BigInt(s.raw_balance1));
 eq(codeSha(accounts.find(a=>a.address===p.pool_id).code),PAIR);eq(codeSha(accounts.find(a=>a.address===p.v2.factory).code),FACTORY);
 eq(getters.pool,p,'getter Pool observation');hash(getters.block_hash);
 eq(getters.calls.length,10,'all fixed-bank dependencies required');
 eq(getters.calls.map(g=>[g.to,g.method,g.args]),[
 [p.pool_id,'token0',[]],[p.pool_id,'token1',[]],[p.pool_id,'factory',[]],[p.pool_id,'getReserves',[]],
 [p.token0,'balanceOf',[p.pool_id]],[p.token1,'balanceOf',[p.pool_id]],[p.pool_id,'price0CumulativeLast',[]],[p.pool_id,'price1CumulativeLast',[]],
 [p.v2.factory,'getPair',[p.token0,p.token1]],[p.v2.factory,'getPair',[p.token1,p.token0]]],'complete getter identities');
 eq(getters.lock_observation,{address:p.pool_id,slot:word(12),value:word(1),block_hash:getters.block_hash,block_number:getters.block_number},'same-bank lock slot');

 const tokenAbi=new ethers.utils.Interface(['function balanceOf(address) view returns(uint256)']);
 const pairAbi=new ethers.utils.Interface(['function getReserves() view returns(uint112,uint112,uint32)','function token0() view returns(address)','function token1() view returns(address)','function factory() view returns(address)','function price0CumulativeLast() view returns(uint256)','function price1CumulativeLast() view returns(uint256)']);
 const factoryAbi=new ethers.utils.Interface(['function getPair(address,address) view returns(address)']);
 for(const g of getters.calls){eq(g.block_hash,getters.block_hash);eq(g.block_number,getters.block_number);assert(/^0x[0-9a-f]*$/.test(g.result));
   const abi=g.method==='balanceOf'?tokenAbi:g.method==='getPair'?factoryAbi:pairAbi;
   eq(abi.encodeFunctionData(g.method,g.args).toLowerCase(),g.data);
   const r=abi.decodeFunctionResult(g.method,g.result);
   let expected;
   if(g.method==='balanceOf')expected=[g.to===p.token0?s.raw_balance0:s.raw_balance1];
   else if(g.method==='getReserves')expected=[p.reserve0,p.reserve1,String(s.block_timestamp_last)];
   else if(g.method==='getPair')expected=[p.pool_id];
   else if(g.method==='factory')expected=[p.v2.factory];
   else if(g.method==='token0'||g.method==='token1')expected=[p[g.method]];
   else expected=[g.method==='price0CumulativeLast'?s.price0_cumulative_last:s.price1_cumulative_last];
   eq(r.map(v=>v.toString().toLowerCase()),expected);
 }
}
function validate(x,{traceDir=path.join(__dirname,'traces')}={}) {
 eq(x.schema_version,1);eq(x.profile,PROFILE);eq(x.adapter_version,ADAPTER);eq(x.authority,'original-local-node-artifact-replay-not-live');
 eq(x.reference.case_inventory,INVENTORY);eq(x.cases.map(c=>c.name),INVENTORY);eq(x.reference.case_count,24);eq(x.reference.success_count,20);eq(x.reference.revert_count,4);
 eq(x.sequences.map(s=>s.cases),SEQUENCES);eq(x.reference.sequence_count,4);
 const pinsPath=path.join(ROOT,'integration/tools/reference/evm-execution/source-pins.json');
 eq(x.source_pins,JSON.parse(fs.readFileSync(pinsPath)));eq(x.reference.source_pins_sha256,sha(fs.readFileSync(pinsPath)));
 eq(x.source_pins.packages.flatMap(p=>p.files).length,93);
 eq(x.reference.loaded_dependencies,JSON.parse(fs.readFileSync(path.join(__dirname,'runtime-pins.json'))),'complete pinned dependency inventory');
 for(const p of x.source_pins.packages)for(const f of p.files)eq(sha(fs.readFileSync(path.join(ROOT,'integration/tools/reference/evm-execution',f.path))),f.sha256,'official source pin');
 for(const list of [x.reference.scripts,x.reference.loaded_dependencies]){assert(list.length>0);const seen=new Set();for(const f of list){assert(!seen.has(f.path));seen.add(f.path);assert(!path.isAbsolute(f.path)&&!f.path.split('/').includes('..'));eq(sha(fs.readFileSync(path.join(ROOT,f.path))),f.sha256,'source/runtime closure');}}
 const required=['integration/tools/reference/v2-complete/generate.cjs','integration/tools/reference/v2-complete/validate.cjs','integration/tools/reference/v2-complete/README.md','integration/tools/reference/v2-complete/runtime-pins.json','integration/tools/ci/v2-complete-golden.cjs','integration/tools/ci/v2-complete-check.py','integration/tools/ci/v2-complete-guards.py','integration/.runtime/stable-reference/package.json','integration/.runtime/stable-reference/package-lock.json'];
 required.push('integration/tools/reference/evm-execution/source-pins.json','integration/tools/reference/evm-execution/core-registry.json','integration/tools/reference/evm-execution/periphery-registry.json','integration/tools/reference/evm-execution/lib-registry.json');
 eq(x.reference.scripts.map(s=>s.path).sort(),required.sort(),'complete script and package metadata inventory');
 eq(x.reference.node.version,process.version);eq(sha(fs.readFileSync(process.execPath)),x.reference.node.sha256);eq(x.reference.node.executable,process.execPath);
 const opts=x.reference.chain_options;eq(opts.chain.allowUnlimitedContractSize,false);eq(opts.chain.allowUnlimitedInitCodeSize,false);eq(opts.chain.hardfork,'shanghai');eq(opts.chain.time,'2026-10-01T00:00:00.000Z');eq(opts.wallet.seed,'t33-v2-complete-official');
 const txs=x.reference.transactions;assert(txs.length>24);eq(x.reference.transaction_count,txs.length);
 const byHash=new Map(),lastAccounts=new Map();let prev=x.reference.genesis_header;
 headerHash(prev);eq(x.reference.runtime_identities.length,12,'official creation identity inventory');
 eq(x.reference.runtime_identities.map(i=>path.basename(i.artifact,'.json')).sort(),['UniswapV2Factory','WETH9','UniswapV2Router02',...Array(6).fill('ERC20'),...Array(3).fill('UniswapV2Pair')].sort());
 for(const t of txs){assert(!byHash.has(t.receipt.transactionHash));byHash.set(t.receipt.transactionHash,t);const a=t.preblock_header,b=t.execution_block_header;
   eq(a,prev,'complete transaction chain');headerHash(b);eq(b.parentHash,a.hash);eq(BigInt(b.number),BigInt(a.number)+1n);eq(b.transactions,[t.receipt.transactionHash]);hash(a.mixHash);hash(b.mixHash);assert(b.mixHash!=='0x'+'0'.repeat(64),'raw PREVRANDAO');
   eq(t.receipt.blockHash,b.hash);eq(t.transaction.blockHash,b.hash);eq(t.transaction.hash,t.receipt.transactionHash);eq(t.transaction.input,t.calldata);
   const signed=ethers.utils.parseTransaction(t.signed_transaction);eq(signed.hash,t.transaction.hash);eq(signed.from.toLowerCase(),t.transaction.from);eq(signed.data,t.calldata);eq(signed.nonce,Number(BigInt(t.transaction.nonce)));eq(signed.chainId,31337);
   eq(t.post_accounts.map(a=>a.address),t.accounts.map(a=>a.address));assert(t.accounts.length>=5);eq(new Set(t.accounts.map(a=>a.address)).size,t.accounts.length,'unique declared actors');for(const a of t.accounts)assert(/^0x[0-9a-f]{40}$/.test(a.address),'canonical actor address');
   for(let i=0;i<t.accounts.length;i++){const pre=t.accounts[i],post=t.post_accounts[i];eq(Object.keys(pre).sort(),['address','exists','balance','nonce','code','storage'].sort());
    for(const a of [pre,post]){decimal(a.balance);number(a.nonce);assert(typeof a.exists==='boolean');assert(/^0x[0-9a-f]*$/.test(a.code));assert((a.code.length-2)/2<=24576);const seen=new Set();for(const s of a.storage){hash(s.slot);hash(s.value);assert(!seen.has(s.slot));seen.add(s.slot);}}
    eq(pre.storage.map(s=>s.slot),post.storage.map(s=>s.slot));
    const last=lastAccounts.get(pre.address);
    if(last){for(const k of ['exists','balance','nonce','code'])eq(pre[k],last[k],'inter-transaction account continuity '+k);for(const st of pre.storage)if(last.slots.has(st.slot))eq(st.value,last.slots.get(st.slot),'inter-transaction storage continuity');}
    const merged=last?.slots||new Map();for(const st of post.storage)merged.set(st.slot,st.value);lastAccounts.set(pre.address,{...post,slots:merged});
    if(pre.address!==t.transaction.from&&pre.exists&&pre.code==='0x')eq(post.nonce,pre.nonce,'non-sender EOA nonce stable');
    if(!pre.exists&&post.code!=='0x')eq(post.nonce,1,'created official contract nonce');

   }
   const senderPre=t.accounts.find(a=>a.address===t.transaction.from),senderPost=t.post_accounts.find(a=>a.address===t.transaction.from);assert(senderPre&&senderPost);eq(senderPost.nonce,senderPre.nonce+1);
   const traceBytes=fs.readFileSync(path.join(traceDir,t.trace.file));eq(t.trace.file,t.trace.sha256+'.json.gz');eq(sha(traceBytes),t.trace.gzip_sha256);const traceText=zlib.gunzipSync(traceBytes);eq(sha(traceText),t.trace.sha256);const trace=JSON.parse(traceText);eq(trace.structLogs.length,t.trace.steps);assert(trace.structLogs.length>0);eq(traceAccesses(trace,t,x.reference.runtime_identities),t.storage_accesses,'all raw trace storage accesses retained');
   const reverting=t.purpose.startsWith('reserve-overflow-')||t.purpose.startsWith('pair-k-revert-');eq(Number(BigInt(t.receipt.status)),reverting?0:1,'setup/case receipt status');
   eq(trace.structLogs.at(-1).op==='REVERT',reverting,'raw trace receipt agreement');
   eq(t.receipt.cumulativeGasUsed,t.receipt.gasUsed);eq(BigInt(t.receipt.gasUsed),BigInt(b.gasUsed));eq(t.receipt.logsBloom,b.logsBloom);eq(t.receipt.transactionIndex,'0x0');eq(t.transaction.transactionIndex,'0x0');
   for(const log of t.receipt.logs){eq(log.blockHash,b.hash);eq(log.blockNumber,b.number);eq(log.transactionHash,t.receipt.transactionHash);eq(log.transactionIndex,'0x0');eq(log.removed,false);}
   if(reverting){assert(t.call_observation.data.startsWith('0x08c379a0'));eq(ethers.utils.defaultAbiCoder.decode(['string'],'0x'+t.call_observation.data.slice(10))[0],t.trace.revert_reason,'actual EVM revert bytes');}

   for(const a of t.storage_accesses)assert(t.accounts.some(v=>v.address===a.address&&v.storage.some(s=>s.slot===a.slot)),'accessed slot omitted');
   for(const g of [t.pre_getters,t.post_getters])for(const observation of g){hash(observation.block_hash);const block=g===t.pre_getters?a:b;eq(observation.block_hash,block.hash);eq(observation.block_number,Number(BigInt(block.number)));pool(observation.pool,{block_number:Number(BigInt(a.number)),block_hash:a.hash,timestamp:Number(BigInt(a.timestamp))},g===t.pre_getters?t.accounts:t.post_accounts,observation);}
   prev=b;
 }
 for(const id of x.reference.runtime_identities){
  const t=byHash.get(id.creation_transaction_hash);assert(t);const a=JSON.parse(fs.readFileSync(path.join(ROOT,id.artifact)));
  const creation='0x'+a.evm.bytecode.object.replace(/^0x/,''),template='0x'+a.evm.deployedBytecode.object.replace(/^0x/,'');
  eq(id.creation_code_sha256,codeSha(creation));eq(id.runtime_template_sha256,codeSha(template));
  const before=t.accounts.find(a=>a.address===id.address),after=t.post_accounts.find(a=>a.address===id.address);assert(before&&after);eq(before.exists,false);eq(before.code,'0x');eq(after.exists,true);eq(after.nonce,1);eq(codeSha(after.code),id.runtime_code_sha256);
  const actual=Buffer.from(after.code.slice(2),'hex'),expected=Buffer.from(template.slice(2),'hex');eq(actual.length,expected.length);assert(actual.length<=24576);
  eq(id.immutable_references.map(r=>[r.id,r.references]),Object.entries(a.evm.deployedBytecode.immutableReferences||{}));
  for(const r of id.immutable_references)for(const v of r.references){eq(actual.subarray(v.start,v.start+v.length).toString('hex'),word(r.value).slice(2));expected.copy(actual,v.start,v.start,v.start+v.length);}
  eq('0x'+actual.toString('hex'),template,'official published runtime template');
  if(!id.artifact.endsWith('/UniswapV2Pair.json')){assert(t.calldata.startsWith(creation),'official constructor creation bytes');eq(t.receipt.contractAddress,id.address);}
  else {const g=t.post_getters.find(g=>g.pool.pool_id===id.address);assert(g);const p=g.pool;eq(ethers.utils.getCreate2Address(p.v2.factory,ethers.utils.keccak256(ethers.utils.solidityPack(['address','address'],[p.token0,p.token1])),ethers.utils.keccak256(creation)).toLowerCase(),id.address);}
 }
 const cases=new Map();
 for(const c of x.cases){cases.set(c.name,c);const t=byHash.get(c.expected.tx_hash);assert(t,'case original transaction');const a=c.state_anchor,e=c.expected,b=t.execution_block_header;
  eq([a.block_number,a.block_hash,a.timestamp],[Number(BigInt(t.preblock_header.number)),t.preblock_header.hash,Number(BigInt(t.preblock_header.timestamp))]);
  eq(c.state_anchor.genesis_hash,x.reference.genesis_header.hash);
  eq(c.execution_environment,{chain_id:31337,fork:'shanghai',block_number:Number(BigInt(b.number)),block_hash:b.hash,parent_hash:b.parentHash,timestamp:Number(BigInt(b.timestamp)),coinbase:b.miner.toLowerCase(),gas_limit:Number(BigInt(b.gasLimit)),base_fee:BigInt(b.baseFeePerGas).toString(),prevrandao:b.mixHash});
  eq(c.transaction_context,{nonce:Number(BigInt(t.transaction.nonce)),gas_limit:Number(BigInt(t.transaction.gas)),gas_price:BigInt(t.transaction.gasPrice).toString(),chain_id:31337});
  eq(c.execution_timestamp,Number(BigInt(b.timestamp)));eq(c.execution_environment.prevrandao,b.mixHash);eq(c.execution_environment.block_hash,b.hash);eq(c.execution_environment.timestamp,c.execution_timestamp);number(c.execution_timestamp);
  eq(c.accounts,t.accounts);eq(e.post_accounts,t.post_accounts);eq(c.evidence,t);eq(c.transaction.data,t.calldata);eq(c.transaction.from,t.transaction.from);eq(c.transaction.to,t.transaction.to);
  pool(c.pool,a,c.accounts,c.evidence.pre_getters.find(g=>g.pool.pool_id===c.pool.pool_id));
  pool(e.pool_after,a,e.post_accounts,c.evidence.post_getters.find(g=>g.pool.pool_id===c.pool.pool_id));
  eq(c.evidence.pre_getters.find(g=>g.pool.pool_id===c.pool.pool_id).block_hash,a.block_hash);eq(c.evidence.post_getters.find(g=>g.pool.pool_id===c.pool.pool_id).block_hash,b.hash);
  const reverse=c.direction==='token1-to-token0';eq(c.token_in,reverse?c.pool.token1:c.pool.token0);eq(c.token_out,reverse?c.pool.token0:c.pool.token1);
  decimal(c.amount_in);decimal(e.amount_out);eq(e.receiptstatus,Number(BigInt(t.receipt.status)));eq(e.status,e.receiptstatus?'success':'revert');eq(e.gas_used,BigInt(t.receipt.gasUsed).toString());
  const bal=(accounts,token,who)=>sameSlots(accounts,token,mapping(who,1));
  eq(BigInt(e.amount_out),bal(e.post_accounts,c.token_out,c.recipient)-bal(c.accounts,c.token_out,c.recipient));
  eq(BigInt(e.sender_input_consumed),bal(c.accounts,c.token_in,c.transaction.from)-bal(e.post_accounts,c.token_in,c.transaction.from));
  const p=c.pool,s=p.v2.swap,z=e.pool_after,after=z.v2.swap;
  if(e.status==='success'){
   eq(e.sender_input_consumed,c.amount_in);assert(e.swap_event);const ev=e.swap_event;
   eq(ev.amount0_out,reverse?e.amount_out:'0');eq(ev.amount1_out,reverse?'0':e.amount_out);
   const in0=BigInt(s.raw_balance0)+(!reverse?BigInt(c.amount_in):0n)-BigInt(p.reserve0),in1=BigInt(s.raw_balance1)+(reverse?BigInt(c.amount_in):0n)-BigInt(p.reserve1);
   eq(ev.amount0_in,in0.toString());eq(ev.amount1_in,in1.toString());
   eq(BigInt(after.raw_balance0),BigInt(s.raw_balance0)+(!reverse?BigInt(c.amount_in):-BigInt(e.amount_out)));
   eq(BigInt(after.raw_balance1),BigInt(s.raw_balance1)+(reverse?BigInt(c.amount_in):-BigInt(e.amount_out)));
   eq(z.reserve0,after.raw_balance0);eq(z.reserve1,after.raw_balance1);
   assert((BigInt(z.reserve0)*1000n-in0*3n)*(BigInt(z.reserve1)*1000n-in1*3n)>=BigInt(p.reserve0)*BigInt(p.reserve1)*1000000n,'Pair K condition');
   const dt=(BigInt(c.execution_timestamp)-BigInt(s.block_timestamp_last))&((1n<<32n)-1n),mask=(1n<<256n)-1n;
   eq(after.block_timestamp_last,c.execution_timestamp%2**32);
   eq(after.price0_cumulative_last,((BigInt(s.price0_cumulative_last)+((BigInt(p.reserve1)<<112n)/BigInt(p.reserve0))*dt)&mask).toString());
   eq(after.price1_cumulative_last,((BigInt(s.price1_cumulative_last)+((BigInt(p.reserve0)<<112n)/BigInt(p.reserve1))*dt)&mask).toString());
   assert(t.storage_accesses.some(v=>v.address===p.pool_id&&v.slot===word(12)&&v.operations.includes('SSTORE')),'actual lock writes');
  } else {eq(z,p,'revert rolls back entire Pair');eq(e.amount_out,'0');eq(e.sender_input_consumed,'0');eq(e.swap_event,null);assert(t.trace.revert_reason.includes(c.name.startsWith('reserve-overflow')?'OVERFLOW':'K'),'actual Pair revert reason');}
  if(c.name.startsWith('donate0-')){assert(BigInt(s.raw_balance0)>BigInt(p.reserve0));eq(s.raw_balance1,p.reserve1);}
  if(c.name.startsWith('donate1-')){assert(BigInt(s.raw_balance1)>BigInt(p.reserve1));eq(s.raw_balance0,p.reserve0);}
  if(c.name.startsWith('donate-both-')){assert(BigInt(s.raw_balance0)>BigInt(p.reserve0));assert(BigInt(s.raw_balance1)>BigInt(p.reserve1));}
  if(c.name.startsWith('sync-')||c.name.startsWith('after-')){eq(s.raw_balance0,p.reserve0);eq(s.raw_balance1,p.reserve1);}
  if(c.name.startsWith('reserve-overflow'))assert(BigInt(s.raw_balance0)>=(1n<<112n)||BigInt(s.raw_balance1)>=(1n<<112n));
  if(c.name.startsWith('cumulative')){const key=c.name==='cumulative0-wrap'?'price0_cumulative_last':'price1_cumulative_last';assert(BigInt(after[key])<BigInt(s[key]),'observed cumulative wrap');}
  if(c.service_eligible){eq(c.execution_timestamp,a.timestamp);assert(c.execution_timestamp<2**32,'future wrap banks excluded from headclock');}
 }
 for(const sequence of x.sequences){
  const initial=cases.get(sequence.cases[0]);eq(sequence.source_anchor,initial.state_anchor);eq(sequence.pool,initial.pool);eq(sequence.steps.length,sequence.cases.length);
  const derived=pool=>{const p=structuredClone(pool),a=sequence.source_anchor;Object.assign(p.v2.swap,{bank_block_number:a.block_number,bank_block_hash:a.block_hash,bank_unix_timestamp:a.timestamp});return p;};
  for(let i=0;i<sequence.cases.length;i++){const c=cases.get(sequence.cases[i]),expected=structuredClone(c.expected);expected.pool_after=derived(c.expected.pool_after);eq(sequence.steps[i],{case:c.name,execution_timestamp:c.execution_timestamp,token_in:c.token_in,token_out:c.token_out,amount_in:c.amount_in,pool:derived(c.pool),expected});if(i){eq(sequence.steps[i-1].expected.pool_after,sequence.steps[i].pool,'shared sequence full transition without source reanchor');eq(c.state_anchor.block_hash,cases.get(sequence.cases[i-1]).execution_environment.block_hash);}}
 }
 return x;
}
module.exports={validate,INVENTORY,SEQUENCES,ROOT,sha,codeSha,word,mapping,PROFILE,ADAPTER,PAIR,FACTORY};
