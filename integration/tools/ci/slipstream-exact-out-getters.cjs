// Validate every raw field against the captured original ABI result, without RPC.
const fs = require('node:fs'), path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const {ethers} = require(path.join(root, 'integration/.runtime/stable-reference/node_modules/ethers'));
const demand = (v, why) => {if (!v) throw Error(why);};
const input = JSON.parse(fs.readFileSync(process.argv[2], 'utf8'));
const artifacts = JSON.parse(fs.readFileSync(path.join(root, 'integration/artifacts/local/core-slipstream-context-runtime-artifacts.json'), 'utf8'));
const d = input.deployment, zero = '0x'+'0'.repeat(40);
const abi = Object.fromEntries(Object.entries(artifacts.contracts).map(([k,v]) => [k, new ethers.utils.Interface(v.abi)]));
abi.voter = new ethers.utils.Interface(['function gauges(address) view returns(address)', 'function isAlive(address) view returns(bool)']);
abi.token = new ethers.utils.Interface(['function balanceOf(address) view returns(uint256)']);
const equal = (a,b) => Array.isArray(a) && Array.isArray(b) ? a.length===b.length && a.every((v,i)=>equal(v,b[i])) : String(a).toLowerCase()===String(b).toLowerCase();
let banks=0, reads=0;
function validate(raw, recorded, header) {
 const seen = new Set();
 function get(role, address, signature, expected, args=[], origin=d.sender) {
  const data=abi[role].encodeFunctionData(signature,args).toLowerCase();
  const matches=recorded.filter(r=>r.to===address && r.data===data && r.from===origin);
  demand(matches.length===1, 'SLIPSTREAM_XO_GETTER_IDENTITY:'+signature);
  const r=matches[0];demand(r.block_hash===header.hash && r.block_number===header.number && header.timestamp===input.fixed_timestamp,'SLIPSTREAM_XO_GETTER_HEADER');
  demand(!seen.has(r),'SLIPSTREAM_XO_DUPLICATE_GETTER');seen.add(r);
  const values=abi[role].decodeFunctionResult(signature,r.result);
  demand(abi[role].encodeFunctionResult(signature,values).toLowerCase()===r.result,'SLIPSTREAM_XO_GETTER_ENCODING');
  demand(equal(values.length===1 ? values[0] : values,expected),'SLIPSTREAM_XO_GETTER_VALUE:'+signature);
 }
 for (const [name,expected] of [['token0',d.token0],['token1',d.token1],['factory',d.factory],['tickSpacing',50],['fee',raw.module_state.view_swap_fee],['unstakedFee',raw.module_state.view_unstaked_fee]]) get('pool',d.pool,name+'()',expected);
 for (const name of ['slot0','liquidity','gauge','nft','factoryRegistry','feeGrowthGlobal0X128','feeGrowthGlobal1X128','rewardGrowthGlobalX128','rewardRate','rewardReserve','periodFinish','rollover','stakedLiquidity','maxLiquidityPerTick','lastUpdated','gaugeFees'])get('pool',d.pool,name+'()',raw[name]);
 for(const row of raw.tick_words)get('pool',d.pool,'tickBitmap(int16)',row.bitmap,[row.word_pos]);
 for(const row of raw.ticks)get('pool',d.pool,'ticks(int24)',row.values,[row.tick]);
 raw.observations.forEach((row,index)=>get('pool',d.pool,'observations(uint256)',row.values,[index]));
 const m=raw.module_state;
 for(const [name,expected]of [['voter',m.factory_voter],['poolImplementation',d.implementation],['factoryRegistry',d.factory_registry],['swapFeeModule',m.swap_fee_module],['unstakedFeeModule',m.unstaked_fee_module],['defaultUnstakedFee',m.factory_default_unstaked_fee]])get('factory',d.factory,name+'()',expected);
 for(const [name,value]of [['swapFeeModule',m.swap_fee_module],['unstakedFeeModule',m.unstaked_fee_module]])get('factory',d.factory,name+'()',value,[],zero);
 get('factory',d.factory,'tickSpacingToFee(int24)',m.factory_default_fee,[50]);
 get('factory',d.factory,'isPool(address)',m.factory_registered,[d.pool]);
 get('factory',d.factory,'getPool(address,address,int24)',d.pool,[d.token0,d.token1,50]);
 get('voter',d.voter,'gauges(address)',m.gauge,[d.pool]);get('voter',d.voter,'isAlive(address)',m.gauge_alive,[m.gauge]);
 get('custom_unstaked',d.unstaked_custom,'factory()',d.factory);get('custom_unstaked',d.unstaked_custom,'customFee(address)',m.unstaked_custom_fee,[d.pool]);
 demand(m.allowed_origin===d.sender,'SLIPSTREAM_XO_RECORDED_ORIGIN');
 if(m.kind==='none')demand(m.swap_fee_module===zero,'SLIPSTREAM_XO_NO_MODULE');
 else if(m.kind==='custom') {
  demand(m.swap_fee_module===d.swap_custom,'SLIPSTREAM_XO_CUSTOM_MODULE');get('custom_swap',d.swap_custom,'factory()',d.factory);get('custom_swap',d.swap_custom,'customFee(address)',m.custom_fee,[d.pool]);
 } else {
  demand(m.kind==='dynamic'&&m.swap_fee_module===d.swap_dynamic,'SLIPSTREAM_XO_DYNAMIC_MODULE');
  for(const [name,value]of [['factory',d.factory],['defaultScalingFactor',m.default_scaling_factor],['defaultFeeCap',m.default_fee_cap],['secondsAgo',m.seconds_ago]])get('dynamic_swap',d.swap_dynamic,name+'()',value);
  get('dynamic_swap',d.swap_dynamic,'dynamicFeeConfig(address)',m.dynamic_fee_config,[d.pool]);get('dynamic_swap',d.swap_dynamic,'discounted(address)',m.origin_discount,[d.sender]);
 }
 get('token',d.token0,'balanceOf(address)',raw.raw_balance0,[d.pool]);get('token',d.token1,'balanceOf(address)',raw.raw_balance1,[d.pool]);
 demand(seen.size===recorded.length,'SLIPSTREAM_XO_UNVALIDATED_GETTERS');banks++;reads+=seen.size;
}
for(const c of input.cases){validate(c.before,c.before_getter_reads,c.original_before_header);validate(c.expected.after,c.expected.after_getter_reads,c.expected.original_after_header);}
for(const s of input.sequences)for(const step of s.steps)validate(step.after,step.after_getter_reads,step.original_after_header);
console.log(JSON.stringify({passed:true,banks,reads}));
