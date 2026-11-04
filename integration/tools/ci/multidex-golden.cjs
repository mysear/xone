// Independent pinned SDK and pair-invariant oracle. Never imports xone math.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../../..'),modules=path.join(root,'integration/.runtime/multidex-oracle/node_modules');
const sdk3=require(path.join(modules,'@pancakeswap/v3-sdk')),sdk2=require(path.join(modules,'@pancakeswap/v2-sdk')),core=require(path.join(modules,'@pancakeswap/swap-sdk-core'));
const clone=x=>JSON.parse(JSON.stringify(x));
const token0=new core.Token(56,'0x'+'01'.repeat(20),18,'A'),token1=new core.Token(56,'0x'+'02'.repeat(20),18,'B');
const identity={factory:'0x'+'04'.repeat(20),code_sha256:'aa'.repeat(32)};
function pairPool(protocol,r0,r1){const [fee,den]=protocol==='pancake-v1'?[2,1000]:protocol==='pancake-v2'?[25,10000]:[3,1000];return {chain_key:'evm:56',pool_id:'0x'+'03'.repeat(20),protocol,token0:token0.address,token1:token1.address,reserve0:r0,reserve1:r1,fee_numerator:fee,fee_denominator:den,adapter_version:protocol+'/core-1',token_behavior:'standard',v2:clone(identity)};}
// Find maximum output satisfying the official pair's post-swap invariant.
// This uses no division rearrangement from the quote implementation.
function invariantQuote(pool,input,zero){const rin=BigInt(zero?pool.reserve0:pool.reserve1),rout=BigInt(zero?pool.reserve1:pool.reserve0),amount=BigInt(input),d=BigInt(pool.fee_denominator),f=BigInt(pool.fee_numerator),adjusted=(rin+amount)*d-amount*f,target=rin*rout*d*d;
 let low=0n,high=rout;while(low+1n<high){const mid=(low+high)/2n;if(adjusted*(rout-mid)*d>=target)low=mid;else high=mid;}
 return low;
}
async function concentratedOracle(pool,input,zero){let steps=0;const s=pool.v3,ticks=new sdk3.TickListDataProvider(s.ticks.map(t=>({index:t.tick,liquidityGross:BigInt(t.liquidity_gross),liquidityNet:BigInt(t.liquidity_net)})),s.tick_spacing);
 const provider={getTick:t=>ticks.getTick(t),nextInitializedTickWithinOneWord:(tick,lte,spacing)=>{if(++steps>s.max_steps)throw Error('V3_MAX_STEPS');const word=Math.floor((Math.floor(tick/spacing)+(lte?0:1))/256);if(!s.tick_words.some(w=>w.word_pos===word))throw Error('V3_TICK_RANGE_INCOMPLETE');return ticks.nextInitializedTickWithinOneWord(tick,lte,spacing);}};
 const sdk=new sdk3.Pool(token0,token1,pool.fee_numerator,BigInt(s.sqrt_price_x96),BigInt(s.liquidity),s.tick,provider);Object.defineProperty(sdk,'tickSpacing',{value:s.tick_spacing});
 try{const [out,next]=await sdk.getOutputAmount(core.CurrencyAmount.fromRawAmount(zero?token0:token1,BigInt(input)));return {amount_out:out.quotient.toString(),sqrt_price_x96:next.sqrtRatioX96.toString(),tick:next.tickCurrent,liquidity:next.liquidity.toString(),steps};}catch(e){return {error:e.message};}
}
(async()=>{const cases=[];
 for(const protocol of ['pancake-v1','pancake-v2','sushiswap-v2'])for(const [r0,r1]of [['1000000','1000000'],['1001','900001'],['5192296858534827628530496300000','519229685853482762853049630000']])for(const zero of [true,false])for(const input of ['1','2','10','10000','1000000','18446744073709551615']){
  const p=pairPool(protocol,r0,r1);let expected;const rin=BigInt(zero?r0:r1);
  if(rin+BigInt(input)>=(1n<<112n)){expected={error:'V2_RESERVE_OVERFLOW'};}else{let out;
   if(protocol==='pancake-v2'){try{const pair=new sdk2.Pair(core.CurrencyAmount.fromRawAmount(token0,BigInt(r0)),core.CurrencyAmount.fromRawAmount(token1,BigInt(r1)));out=pair.getOutputAmount(core.CurrencyAmount.fromRawAmount(zero?token0:token1,BigInt(input)))[0].quotient;}catch(e){expected={error:'INSUFFICIENT_OUTPUT'};}}
   else out=invariantQuote(p,input,zero);
   if(out!==undefined)expected=out===0n?{error:'INSUFFICIENT_OUTPUT'}:{amount_out:out.toString()};
  }cases.push({name:protocol+'-'+cases.length,pool:p,input,zero_for_one:zero,expected});
 }
 const v3=JSON.parse(fs.readFileSync(path.join(root,'integration/test/contracts/v3-quotes.json')));
 for(const original of v3.cases){const c=clone(original);c.name='pancake-v3-'+c.name;c.pool.chain_key='evm:56';c.pool.protocol='pancake-v3';c.pool.adapter_version='pancake-v3/core-1';c.pool.v3.fee_protocol=209718400;c.expected=await concentratedOracle(c.pool,c.input,c.zero_for_one);cases.push(c);}
 const result={schema_version:1,reference:{pancake_v3_sdk:'3.10.3',pancake_v2_sdk:'1.2.3',pancake_v1_core:'3b214306770e86bc3a64e67c2b5bdb566b4e94a7',sushiswap_v2_core:'a349882c918c752d0363e4c6e02dedcbc48b734e',scope:'official Pancake SDK + independent binary-search pair invariant'},cases};
 const text=JSON.stringify(result,null,2)+'\n';for(const project of ['integration','dex-router'])fs.writeFileSync(path.join(root,project,'test/contracts/dex-versions.json'),text);
 const report={cases:cases.length,successes:cases.filter(c=>!c.expected.error).length,reference:result.reference,sha256:crypto.createHash('sha256').update(text).digest('hex')};fs.writeFileSync(path.join(root,'integration/artifacts/local/multidex-golden-generation.json'),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify(report));
})().catch(e=>{console.error(e);process.exit(1)});
