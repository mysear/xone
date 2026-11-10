// SPDX-License-Identifier: MIT
// Execute original recorded runtime bytes against captured actual API inputs.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../../..'),modules=path.join(root,'integration/.runtime/stable-reference/node_modules');
const ganache=require(path.join(modules,'ganache')),{ethers}=require(path.join(modules,'ethers'));
const sha=raw=>crypto.createHash('sha256').update(raw).digest('hex');
const requireValue=(ok,why)=>{if(!ok)throw Error(why);};
function read(file){const stat=fs.statSync(file);requireValue(stat.isFile()&&stat.size<=16*1024*1024,'LFJ_NATIVE_INPUT_BYTES');return fs.readFileSync(file);}
function parseJSON(raw){return JSON.parse(raw.toString('utf8'),(key,value,context)=>{
 if(typeof value!=='number')return value;
 requireValue(typeof context?.source==='string'&&/^-?(?:0|[1-9][0-9]*)$/.test(context.source),'LFJ_NATIVE_JSON_INTEGER_SOURCE');
 const integer=BigInt(context.source);
 return integer>=BigInt(Number.MIN_SAFE_INTEGER)&&integer<=BigInt(Number.MAX_SAFE_INTEGER)?Number(integer):integer;
});}
function encodeCanonical(value){
 if(typeof value==='bigint')return value.toString();
 if(Array.isArray(value))return '['+value.map(encodeCanonical).join(',')+']';
 if(value&&typeof value==='object')return '{'+Object.keys(value).sort().map(k=>JSON.stringify(k)+':'+encodeCanonical(value[k])).join(',')+'}';
 if(typeof value==='number')requireValue(Number.isSafeInteger(value),'LFJ_NATIVE_UNVERIFIED_JSON_NUMBER');
 return JSON.stringify(value);
}
const stable=value=>encodeCanonical(value).replace(/[<>&\u2028\u2029]/g,c=>({'<':'\\u003c','>':'\\u003e','&':'\\u0026','\u2028':'\\u2028','\u2029':'\\u2029'}[c]));
const hash=value=>sha(Buffer.from(stable(value)));
function equal(a,b,why){requireValue(stable(a)===stable(b),why);}
const args=process.argv.slice(2);requireValue(args.length===4,'LFJ_NATIVE_ARGUMENTS');
const referenceRaw=read(args[0]),inputRaw=read(args[1]),modelRaw=read(args[2]);
const reference=parseJSON(referenceRaw),input=parseJSON(inputRaw),model=parseJSON(modelRaw);
requireValue(reference.schema_version===1&&model.schema_version===1&&input.schema_version===1&&model.passed===true,'LFJ_NATIVE_SCHEMA');
requireValue(model.input_sha256===sha(inputRaw),'LFJ_NATIVE_ACTUAL_API_INPUT_BINDING');
requireValue(Array.isArray(model.records)&&model.records.length>0&&model.records.length<=128&&model.records.length===input.records.length,'LFJ_NATIVE_RECORD_SCOPE');
const originalPath=fs.realpathSync(path.resolve(root,reference.original_reference.path));
requireValue(originalPath.startsWith(path.join(root,'integration/artifacts/local')+path.sep)&&path.basename(originalPath).startsWith('core-lfj-v21-original-reference'),'LFJ_NATIVE_REFERENCE_LOCALITY');
requireValue(sha(read(originalPath))===reference.original_reference.sha256,'LFJ_NATIVE_ORIGINAL_REFERENCE_DIGEST');
const original=parseJSON(read(originalPath));requireValue(original.original_pair_swap_executed===true,'LFJ_NATIVE_REFERENCE_AUTHORITY');
const deployment=reference.deployment;
for(const key of ['factory','factory_code','implementation','implementation_code','token0','token0_code','token1','token1_code','pool','pool_code'])equal(deployment[key],original.deployment[key],'LFJ_NATIVE_ORIGINAL_RUNTIME_BINDING:'+key);
const pairABI=['function swap(bool,address) returns(bytes32)','function getSwapOut(uint128,bool) view returns(uint128 amountInLeft,uint128 amountOut,uint128 fee)','function totalSupply(uint256) view returns(uint256)'];
const tokenABI=['function mint(address,uint256)','function balanceOf(address) view returns(uint256)'];
const allowed=new Set([deployment.pool,'0x000000000000000000000000000000000000aacc']);
const mutable=['parameters','raw_reserve0','raw_reserve1','protocol_fees0','protocol_fees1','bins','oracle_samples'];
const word=value=>{const n=BigInt(value);requireValue(n>=0n&&n<(1n<<256n),'LFJ_NATIVE_WORD_RANGE');return '0x'+n.toString(16).padStart(64,'0');};
const packed=(x,y)=>BigInt(x)+(BigInt(y)<<128n);
const mapping=(key,index)=>ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['uint256','uint256'],[String(key),String(index)]));
(async()=>{
 const timestamp=model.records[0].snapshot.reference.block.timestamp_ms;
 requireValue(timestamp%1000===0,'LFJ_NATIVE_FIXED_CLOCK');
 const transport=ganache.provider({logging:{quiet:true},chain:{hardfork:'london',time:new Date(timestamp)},miner:{timestampIncrement:0},wallet:{deterministic:true}});
 const provider=new ethers.providers.Web3Provider(transport);
 try{
  const signer=provider.getSigner(0),sender=(await signer.getAddress()).toLowerCase();
  requireValue(sender===deployment.sender,'LFJ_NATIVE_SIGNER_BINDING');
  for(const [address,code]of [[deployment.factory,deployment.factory_code],[deployment.implementation,deployment.implementation_code],[deployment.token0,deployment.token0_code],[deployment.token1,deployment.token1_code],...[...allowed].map(id=>[id,deployment.pool_code])]){
   requireValue(/^0x[0-9a-f]{40}$/.test(address)&&/^0x(?:[0-9a-f]{2})+$/.test(code),'LFJ_NATIVE_RUNTIME_BYTES');await provider.send('evm_setAccountCode',[address,code]);
  }
  const x=new ethers.Contract(deployment.token0,tokenABI,signer),y=new ethers.Contract(deployment.token1,tokenABI,signer);
  const set=async(id,key,value)=>provider.send('evm_setAccountStorageAt',[id,typeof key==='string'?key:word(key),word(value)]);
  async function initialize(pool,source){
   requireValue(allowed.has(pool.pool_id)&&pool.protocol==='lfj-lb-v21'&&pool.adapter_version==='lfj-lb/v2.1.0-fee-oracle-1','LFJ_NATIVE_POOL_SCOPE');
   equal([pool.token0,pool.token1],[deployment.token0,deployment.token1],'LFJ_NATIVE_TOKEN_IDENTITY');
   const s=pool.lfj;for(const key of mutable)equal(s[key],source[key],'LFJ_NATIVE_SOURCE_BANK:'+key);
   requireValue(s.timestamp*1000===timestamp&&s.bin_step===source.bin_step,'LFJ_NATIVE_SOURCE_CLOCK_STEP');
   for(const [field,key]of [['code_sha256','pool_code'],['implementation_code_sha256','implementation_code'],['factory_code_sha256','factory_code'],['token0_code_sha256','token0_code'],['token1_code_sha256','token1_code']])requireValue(s[field]===sha(Buffer.from(deployment[key].slice(2),'hex')),'LFJ_NATIVE_CODE_DIGEST:'+field);
   equal([s.implementation,s.factory],[deployment.implementation,deployment.factory],'LFJ_NATIVE_DEPENDENCY_ADDRESSES');
   requireValue(s.bins.length<=32&&s.oracle_samples.length<=2,'LFJ_NATIVE_STATE_BUDGET');
   await set(pool.pool_id,3,1);await set(pool.pool_id,4,s.parameters);await set(pool.pool_id,5,packed(s.raw_reserve0,s.raw_reserve1));await set(pool.pool_id,6,packed(s.protocol_fees0,s.protocol_fees1));
   let rootBits=0n;const level1=new Map(),level2=new Map();
   for(const bin of s.bins){await set(pool.pool_id,mapping(bin.id,7),packed(bin.reserve0,bin.reserve1));await set(pool.pool_id,mapping(bin.id,1),bin.total_shares);const k2=bin.id>>>8,k1=k2>>>8;rootBits|=1n<<BigInt(k1);level1.set(k1,(level1.get(k1)||0n)|(1n<<BigInt(k2&255)));level2.set(k2,(level2.get(k2)||0n)|(1n<<BigInt(bin.id&255)));}
   await set(pool.pool_id,8,rootBits);for(const [key,bits]of level1)await set(pool.pool_id,mapping(key,9),bits);for(const [key,bits]of level2)await set(pool.pool_id,mapping(key,10),bits);
   for(const sample of s.oracle_samples)await set(pool.pool_id,11+sample.index-1,sample.data);
   await(await x.mint(pool.pool_id,s.raw_reserve0)).wait();await(await y.mint(pool.pool_id,s.raw_reserve1)).wait();
  }
  async function observe(pool){
   const parameters=BigInt(await provider.getStorageAt(pool.pool_id,4)),reserves=BigInt(await provider.getStorageAt(pool.pool_id,5)),fees=BigInt(await provider.getStorageAt(pool.pool_id,6)),mask=(1n<<128n)-1n;
   const result={parameters:String(parameters),raw_reserve0:String(reserves&mask),raw_reserve1:String(reserves>>128n),protocol_fees0:String(fees&mask),protocol_fees1:String(fees>>128n),bins:[],oracle_samples:[]};
   const pair=new ethers.Contract(pool.pool_id,pairABI,signer);
   for(const bin of pool.lfj.bins){const raw=BigInt(await provider.getStorageAt(pool.pool_id,mapping(bin.id,7)));result.bins.push({id:bin.id,reserve0:String(raw&mask),reserve1:String(raw>>128n),total_shares:(await pair.totalSupply(bin.id)).toString()});}
   for(const sample of pool.lfj.oracle_samples)result.oracle_samples.push({index:sample.index,data:String(BigInt(await provider.getStorageAt(pool.pool_id,11+sample.index-1)))});
   equal([(await x.balanceOf(pool.pool_id)).toString(),(await y.balanceOf(pool.pool_id)).toString()],[result.raw_reserve0,result.raw_reserve1],'LFJ_NATIVE_RAW_TOKEN_POST_BALANCES');
   return result;
  }
  let checkpoint=await provider.send('evm_snapshot',[]);const observations=[];let swaps=0;
  for(let index=0;index<model.records.length;index++){
   await provider.send('evm_revert',[checkpoint]);checkpoint=await provider.send('evm_snapshot',[]);
   const projected=model.records[index],captured=input.records[index],q=captured.quote,plan=captured.execution_plan;
   requireValue(projected.quote_id===q.quoteId&&projected.plan_hash===q.planHash&&projected.case_index===captured.case_index,'LFJ_NATIVE_CAPTURE_IDENTITY');
   equal(projected.snapshot,captured.snapshot,'LFJ_NATIVE_CAPTURE_SOURCE');
   requireValue(hash({context:plan.context,snapshot:plan.snapshot,allocation:plan.allocation,min_buy_amount:plan.min_buy_amount,economic_policy_hash:plan.economic_policy_hash})===plan.plan_hash&&plan.plan_hash===q.planHash,'LFJ_NATIVE_PLAN_HASH');
   const req={...captured.request,constraints:captured.request.constraints??null};
   requireValue(hash({request:req,view_id:q.quoteViewId,policy:q.economicPolicyHash,engine:'xone-router/0.1'})===q.requestHash,'LFJ_NATIVE_REQUEST_HASH');
   requireValue(req.caller===sender&&req.recipient===deployment.recipient&&req.kind==='EXACT_IN'&&req.verification==='FAST','LFJ_NATIVE_CONTEXT_SCOPE');
   requireValue(q.verification.level==='INDICATIVE'&&q.netBuyAmountEstimate===null,'LFJ_NATIVE_NO_VERIFIED_UPGRADE');
   const source=reference.cases[projected.case_index];requireValue(source&&source.pool.lfj.timestamp*1000===timestamp,'LFJ_NATIVE_SOURCE_CASE');
   for(const pool of projected.snapshot.pools)await initialize(pool,source.pool.lfj);
   const outputToken=req.buyToken===deployment.token0?x:y;
   requireValue(req.buyToken===deployment.token0||req.buyToken===deployment.token1,'LFJ_NATIVE_BUY_TOKEN');
   const initial=await outputToken.balanceOf(req.recipient);let consumed=0n;const steps=[],seen=new Set();
   requireValue(projected.steps.length===q.routes.length&&projected.steps.length>0&&projected.steps.length<=2,'LFJ_NATIVE_STEP_SCOPE');
   for(let r=0;r<projected.steps.length;r++){
    const step=projected.steps[r],route=q.routes[r];requireValue(!seen.has(step.pool_id),'LFJ_NATIVE_DUPLICATE_POOL');seen.add(step.pool_id);
    requireValue(step.amount_in===route.amount_in&&step.amount_out===route.amount_out&&route.path.edges.length===1&&route.path.edges[0].pool_id===step.pool_id&&route.path.edges[0].zero_for_one===step.zero_for_one,'LFJ_NATIVE_SELECTED_ROUTE_BINDING');
    requireValue(step.zero_for_one===(req.sellToken===deployment.token0)&&req.buyToken===(step.zero_for_one?deployment.token1:deployment.token0),'LFJ_NATIVE_DIRECTION');
    const before=await observe(step.before);for(const key of mutable)equal(before[key],step.before.lfj[key],'LFJ_NATIVE_PRESTATE:'+key);
    const pair=new ethers.Contract(step.pool_id,pairABI,signer),tokenIn=step.zero_for_one?x:y;
    const observed=await pair.getSwapOut(step.amount_in,step.zero_for_one);requireValue(observed.amountInLeft.toString()==='0'&&observed.amountOut.toString()===step.amount_out,'LFJ_NATIVE_ORIGINAL_VIEW_AMOUNT');
    const previous=await outputToken.balanceOf(req.recipient);await(await tokenIn.mint(step.pool_id,step.amount_in)).wait();
    const receipt=await(await pair.swap(step.zero_for_one,req.recipient,{gasLimit:10000000})).wait();
    const delta=(await outputToken.balanceOf(req.recipient)).sub(previous);requireValue(delta.toString()===step.amount_out,'LFJ_NATIVE_ACTUAL_TOKEN_DELTA');
    const actual=await observe(step.after);for(const key of mutable)equal(actual[key],step.after.lfj[key],'LFJ_NATIVE_COMPLETE_POSTSTATE:'+key);
    for(const key of Object.keys(step.before.lfj).filter(key=>!mutable.includes(key)))equal(step.after.lfj[key],step.before.lfj[key],'LFJ_NATIVE_MODEL_MARKER_CHANGED:'+key);
    const block=await provider.getBlock(receipt.blockNumber);requireValue(block.timestamp*1000===timestamp,'LFJ_NATIVE_REPLAY_CLOCK');
    steps.push({pool_id:step.pool_id,input:step.amount_in,recipient_delta:delta.toString(),after:actual,receipt:{transaction_hash:receipt.transactionHash,block_hash:receipt.blockHash,block_number:receipt.blockNumber,status:receipt.status,gas_used:receipt.gasUsed.toString()}});
    consumed+=BigInt(step.amount_in);swaps++;
   }
   const delta=(await outputToken.balanceOf(req.recipient)).sub(initial).toString();requireValue(consumed.toString()===req.sellAmount&&delta===q.grossBuyAmount&&delta===projected.gross_buy_amount,'LFJ_NATIVE_GROSS_AND_INPUT_CONSERVATION');
   observations.push({quote_id:q.quoteId,plan_hash:q.planHash,case_index:projected.case_index,gross_recipient_delta:delta,steps});
  }
  const report={schema_version:1,passed:true,authority:'actual captured normal API routes replayed through original recorded v2.1.0 LBPair runtime; controlled bank and real local Token transfers',reference_sha256:sha(referenceRaw),actual_api_input_sha256:sha(inputRaw),model_projection_sha256:sha(modelRaw),records:observations,native_swaps:swaps,original_runtime_recompiled_in_this_replay:false,original_runtime_source_build_reference:reference.original_reference,incoming_funding:'controlled mint to pools; no user-wallet debit/approval claim',fixed_clock_ms:timestamp,chain_fees_compared:false,production_execution_claimed:false,production_ready:false};
  const output=fs.openSync(args[3],'wx');try{fs.writeFileSync(output,JSON.stringify(report,null,2)+'\n');}finally{fs.closeSync(output);}
  console.log(JSON.stringify({passed:true,records:observations.length,native_swaps:swaps}));
 }finally{await transport.disconnect();}
})().catch(error=>{console.error(error);process.exitCode=1;});
