// Independent ethers ABI/Keccak construction of synthetic manager RPC storage.
const fs=require('fs'),path=require('path'),crypto=require('crypto');const root=path.resolve(__dirname,'../../..');const {ethers}=require(root+'/integration/.runtime/singleton-reference/node_modules/ethers');
const ref=JSON.parse(fs.readFileSync(root+'/integration/test/contracts/singleton-quotes.json'));const address=n=>'0x'+n.toString(16).padStart(40,'0'),word=n=>ethers.utils.hexZeroPad(ethers.BigNumber.from(n.toString()).toHexString(),32);const mod=(n,b)=>(BigInt(n)+(1n<<BigInt(b)))%(1n<<BigInt(b));const hash=(key,slot)=>ethers.utils.keccak256(ethers.utils.defaultAbiCoder.encode(['bytes32','bytes32'],[key,slot]));const plus=(s,n)=>word(BigInt(s)+BigInt(n));
const pools=[];let v4={};
for(const [i,name] of ['uniswap-v4','pancake-infinity-cl'].entries()){
 const p=structuredClone(ref.cases.find(c=>c.name===`${name}/directional-fee/true/1000000`).pool);const s=p.singleton_cl,c=s.core;p.token0=address(i*10+1);p.token1=address(i*10+2);s.manager_code_sha256=crypto.createHash('sha256').update(Buffer.from([0x60,i+1,0x60,0,0x55])).digest('hex');
 const encoded=name==='uniswap-v4'?ethers.utils.defaultAbiCoder.encode(['address','address','uint24','int24','address'],[p.token0,p.token1,s.lp_fee,c.tick_spacing,s.hooks]):ethers.utils.defaultAbiCoder.encode(['address','address','address','address','uint24','bytes32'],[p.token0,p.token1,s.hooks,s.manager,s.lp_fee,s.parameters]);s.pool_id=ethers.utils.keccak256(encoded);p.pool_id=s.manager+'/'+s.pool_id;pools.push(p);
 if(name==='uniswap-v4'){
  const base=hash(s.pool_id,word(6)),slot0=BigInt(c.sqrt_price_x96)+(mod(c.tick,24)<<160n)+(BigInt(s.protocol_fee0|(s.protocol_fee1<<12))<<184n)+(BigInt(s.lp_fee)<<208n),storage={[base]:word(slot0),[plus(base,3)]:word(c.liquidity)},tickKeys=[],bitmapKeys=[];
  for(let pos=-1;pos<=1;pos++){const key=hash(word(mod(pos,256)),plus(base,5));storage[key]=word(c.tick_words.find(w=>w.word_pos===pos)?.bitmap||'0');bitmapKeys.push(key);}
  for(const t of c.ticks){const key=hash(word(mod(t.tick,256)),plus(base,4));storage[key]=word(BigInt(t.liquidity_gross)+(mod(t.liquidity_net,128)<<128n));tickKeys.push(key);}
  v4={base,storage,tick_keys:tickKeys,bitmap_keys:bitmapKeys};
 }
}
const signatures=['extsload(bytes32)','paused()','poolIdToPoolKey(bytes32)','getSlot0(bytes32)','getLiquidity(bytes32)','getPoolBitmapInfo(bytes32,int16)','getPoolTickInfo(bytes32,int24)'];const selectors=Object.fromEntries(signatures.map(s=>[s,ethers.utils.id(s).slice(0,10)]));const out={reference:'independent ethers5.8.0 ABI/Keccak mock storage; amount oracle remains pinned Pool.swap/CLPool.swap',pools,v4,selectors};fs.writeFileSync(root+'/integration/test/evm/singleton-baseline.json',JSON.stringify(out,null,2)+'\n');console.log(JSON.stringify({pools:pools.map(p=>p.pool_id),selectors}));
