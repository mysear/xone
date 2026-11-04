// Independent official SDK oracle; requires the isolated pinned package setup.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../../..');
const modules=path.join(root,'integration/.runtime/v3-sdk-oracle/node_modules');
const {Pool,TickMath,TickListDataProvider}=require(path.join(modules,'@uniswap/v3-sdk'));
const {Token,CurrencyAmount}=require(path.join(modules,'@uniswap/sdk-core'));
const base=JSON.parse(fs.readFileSync(path.join(root,'integration/test/contracts/v3-state.json')));
const token0=new Token(31337,base.token0,18),token1=new Token(31337,base.token1,18);
const clone=x=>JSON.parse(JSON.stringify(x));
function makePool({fee=3000,spacing=60,tick=0,boundary=false,gap=false,liquidity=1000000000n,maxSteps=512,multipliers=[-10,-2,2,10]}={}){
 const p=clone(base);p.fee_numerator=fee;const s=p.v3;
 s.tick_spacing=spacing;s.tick=tick-(boundary?1:0);s.sqrt_price_x96=TickMath.getSqrtRatioAtTick(tick).toString();s.max_steps=maxSteps;
 const nets=gap?[liquidity,-liquidity,2n*liquidity,-2n*liquidity]:[liquidity,2n*liquidity,-2n*liquidity,-liquidity];
 s.ticks=multipliers.map((n,i)=>({tick:n*spacing,liquidity_gross:(nets[i]<0n?-nets[i]:nets[i]).toString(),liquidity_net:nets[i].toString()}));
 s.liquidity=s.ticks.filter(t=>t.tick<=s.tick).reduce((l,t)=>l+BigInt(t.liquidity_net),0n).toString();
 const current=Math.floor(Math.floor(s.tick/spacing)/256);let lo=Math.min(-1,current),hi=Math.max(0,current);
 s.tick_words=[];for(let w=lo;w<=hi;w++){
  let bits=0n;for(const t of s.ticks){let compressed=t.tick/spacing;if(Math.floor(compressed/256)===w)bits|=1n<<BigInt((compressed%256+256)%256);}
  s.tick_words.push({word_pos:w,bitmap:bits.toString()});
 }
 return p;
}
async function oracle(pool,input,zero){
 let count=0;const s=pool.v3;const sdk=new TickListDataProvider(s.ticks.map(t=>({index:t.tick,liquidityGross:t.liquidity_gross,liquidityNet:t.liquidity_net})),s.tick_spacing);
 const provider={getTick:(t)=>sdk.getTick(t),nextInitializedTickWithinOneWord:(tick,lte,spacing)=>{
  if(++count>s.max_steps)throw Error('V3_MAX_STEPS');
  const compressed=Math.floor(tick/spacing)+(lte?0:1),word=Math.floor(compressed/256);
  if(!s.tick_words.some(w=>w.word_pos===word))throw Error('V3_TICK_RANGE_INCOMPLETE');
  return sdk.nextInitializedTickWithinOneWord(tick,lte,spacing);
 }};
 const p=new Pool(token0,token1,pool.fee_numerator,s.sqrt_price_x96,s.liquidity,s.tick,provider);
 Object.defineProperty(p,'tickSpacing',{value:s.tick_spacing});
 try{const [output,next]=await p.getOutputAmount(CurrencyAmount.fromRawAmount(zero?token0:token1,input));return {amount_out:output.quotient.toString(),consumed_in:input,remaining_in:'0',sqrt_price_x96:next.sqrtRatioX96.toString(),tick:next.tickCurrent,liquidity:next.liquidity.toString(),steps:count};}
 catch(e){return {error:e.message};}
}
(async()=>{
 const specs=[{}, {boundary:true,tick:120}, {tick:-120,boundary:true}, {tick:-60}, {tick:59}, {gap:true}, {fee:0}, {fee:500,spacing:10}, {fee:10000,spacing:200}, {fee:100,spacing:1}, {liquidity:1234567890123456789012345n}, {maxSteps:1}, {tick:800000,spacing:16383,liquidity:100000000000000000000000000000000000n,multipliers:[-54,-50,50,54]}];
 const cases=[];
 for(let i=0;i<specs.length;i++)for(const zero of [true,false])for(const amount of ['1','2','10','999','10000','1000000','10000000','20000000','30000000','1000000000000000000']){
  const p=makePool(specs[i]);cases.push({name:`case-${i}-${zero}-${amount}`,pool:p,input:amount,zero_for_one:zero,expected:await oracle(p,amount,zero)});
 }
 // Reverse a previously simulated swap on the same physical pool.
 for(const zero of [true,false]){
  const p=makePool(),first=await oracle(p,'20000000',zero);
  if(first.error)throw Error('invalid sequential fixture');
  p.v3.sqrt_price_x96=first.sqrt_price_x96;p.v3.tick=first.tick;p.v3.liquidity=first.liquidity;
  cases.push({name:`sequential-reverse-${zero}`,pool:p,input:'1000000',zero_for_one:!zero,expected:await oracle(p,'1000000',!zero)});
 }
 const overflow=makePool({tick:800000,spacing:16383,liquidity:100000000000000000000000000000000000n,multipliers:[-54,-50,50,54]});
 const overflowInput='10000000000000000000000000000000000';
 const adjusted=BigInt(overflowInput)*997000n/1000000n;
 if((adjusted*BigInt(overflow.v3.sqrt_price_x96)).toString(2).length<=256)throw Error('overflow fixture does not reach fallback');
 const overflowResult=await oracle(overflow,overflowInput,true);if(overflowResult.error)throw Error(overflowResult.error);
 cases.push({name:'uint256-product-overflow-sqrt-fallback',pool:overflow,input:overflowInput,zero_for_one:true,expected:overflowResult});
 const minimum=JSON.parse(fs.readFileSync(path.join(root,'integration/test/contracts/v3-minimum-state.json')));
 cases.push({name:'minimum-initialized-price-upward-gap',pool:minimum,input:'10',zero_for_one:false,expected:await oracle(minimum,'10',false)});
 const allocationCases=[];
 for(const reverse of [false,true]){
  const v3=makePool(),front=(id,reserve)=>({chain_key:v3.chain_key,pool_id:id,protocol:'uniswap-v2',token0:'0x'+'05'.repeat(20),token1:v3.token0,reserve0:reserve,reserve1:reserve,fee_numerator:3,fee_denominator:1000,adapter_version:'uniswap-v2/1',token_behavior:'standard'});
  const fronts=[front('front-1','1000000'),front('front-2','2000000')];
  const sequence=[{id:'front-1',amount:'4001'},{id:'front-2',amount:'6002'}];if(reverse)sequence.reverse();
  const initial=clone(v3);let sum=0n;const routes=[];
  for(const step of sequence){
   const f=fronts.find(p=>p.pool_id===step.id),input=BigInt(step.amount),effective=input*997n,mid=effective*BigInt(f.reserve1)/(BigInt(f.reserve0)*1000n+effective);
   const out=await oracle(v3,mid.toString(),true);if(out.error)throw Error(out.error);sum+=BigInt(out.amount_out);
   routes.push({pool_ids:[step.id,v3.pool_id],amount_in:step.amount,amount_out:out.amount_out});
   v3.v3.sqrt_price_x96=out.sqrt_price_x96;v3.v3.tick=out.tick;v3.v3.liquidity=out.liquidity;
  }
  allocationCases.push({name:'shared-v3-'+reverse,pools:[initial,...fronts],routes,sell_amount:'10003',gross_buy_amount:sum.toString()});
 }
 const small=makePool({liquidity:100n});
 const dust=await oracle(small,'1',true),smallFour=await oracle(small,'4',true),smallWhole=await oracle(small,'8',true);
 if(dust.amount_out!=='0'||smallFour.error||!smallWhole.error)throw Error('discovery regression fixture changed');
 const discoveryRegressions=[{name:'dust-probe-can-grow-to-valid-allocation',pool:small,whole_input:'8',whole_result:smallWhole,first_probe:dust,per_pool_input:'4',per_pool_output:smallFour.amount_out,gross_buy_amount:(2n*BigInt(smallFour.amount_out)).toString()},
 {name:'retain-full-order-fallback',input:'100000000',v2_reserve0:'1000000000',v2_reserve1:'1000000000',v2_output:(100000000n*997n*1000000000n/(1000000000n*1000n+100000000n*997n)).toString()}];
 const pkg=require(path.join(modules,'@uniswap/v3-sdk/package.json'));
 const result={schema_version:1,reference:{name:'@uniswap/v3-sdk',version:pkg.version,integrity:'sha512-sbmSA1O+Ct960r66Ie/c1rOmVadpwRu8nQ79pGICv0pZJdnFIQ/SReG3F+iC2C2UNNjNP6aC2WDUggXrjyrgnA==',scope:'Pool/getOutputAmount + TickListDataProvider + SwapMath, bounded proven words/steps'},cases,allocation_cases:allocationCases,discovery_regressions:discoveryRegressions};
 const text=JSON.stringify(result,null,2)+'\n';
 for(const name of ['integration','dex-router'])fs.writeFileSync(path.join(root,name,'test/contracts/v3-quotes.json'),text);
 const report={cases:cases.length,successes:cases.filter(c=>!c.expected.error).length,reference:result.reference,sha256:crypto.createHash('sha256').update(text).digest('hex')};
 fs.writeFileSync(path.join(root,'integration/artifacts/local/v3-sdk-golden-generation.json'),JSON.stringify(report,null,2)+'\n');console.log(JSON.stringify(report));
})().catch(e=>{console.error(e);process.exit(1);});
