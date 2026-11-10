'use strict';
const {sha}=require('./inputs.cjs');
// All RPC reads use the captured number, then re-anchor the entire original header.
async function captureEvidence(rpc,h,addresses,runtime,mappings){
 const accounts=[],storage=[];
 for(const address of [...new Set(addresses)].sort()){
  const code=await rpc('eth_getCode',[address,h.number]),digest=sha(Buffer.from(code.slice(2),'hex'));runtime[digest]=code;
  accounts.push({address,native_balance:await rpc('eth_getBalance',[address,h.number]),nonce:await rpc('eth_getTransactionCount',[address,h.number]),code_sha256:digest,runtime_bytes:(code.length-2)/2});
  if(code!=='0x')for(let slot=0;slot<32;slot++)storage.push({address,key:'0x'+slot.toString(16),value:await rpc('eth_getStorageAt',[address,'0x'+slot.toString(16),h.number])});
 }
 for(const {address,key}of mappings)storage.push({address,key,value:await rpc('eth_getStorageAt',[address,key,h.number])});
 const header_anchor_after=await rpc('eth_getBlockByNumber',[h.number,false]);
 if(JSON.stringify(h)!==JSON.stringify(header_anchor_after))throw Error('sourceH reanchor');
 return {accounts,storage,header_anchor_after};
}
// Complete ordinary reader observations at this exact H. Reusing existing
// observations avoids inventing either ABI results or state between blocks.
async function captureServiceGetters(C, {pool,vault,factory,feeProvider,tokens,contracts}, getters) {
 const lower=s=>s.toLowerCase();
 async function read(c,method,args=[]) {
  if(!c)throw Error('missing original service contract '+method);
  const data=lower(c.interface.encodeFunctionData(method,args));
  const found=getters.find(g=>g.to===lower(c.address)&&g.data===data);
  return found?found.decoded:await C(c,method,args);
 }
 for(const method of ['getVault','getPoolId','getOwner','getProtocolFeesCollector','getPausedState',
                       'inRecoveryMode','getSwapEnabled','getMustAllowlistLPs','getJoinExitEnabled',
                       'getManagementAumFeeParams','getNormalizedWeights','getScalingFactors',
                       'getActualSupply','getSwapFeePercentage','getGradualSwapFeeUpdateParams',
                       'getGradualWeightUpdateParams','name','totalSupply'])await read(pool,method);
 for(const kind of [0,2,3])await read(pool,'getProviderFeeId',[kind]);
 await read(pool,'getProtocolFeePercentageCache',[3]);
 for(const method of ['getVault','getProtocolFeePercentagesProvider','getWeightedMath',
                       'getRecoveryModeHelper','getCreationCodeContracts','getPauseConfiguration'])await read(factory,method);
 await read(factory,'isPoolFromFactory',[pool.address]);
 const pid=(await read(pool,'getPoolId'))[0];
 for(const method of ['getPool','getPoolTokens'])await read(vault,method,[pid]);
 for(const method of ['getPausedState','getProtocolFeesCollector','WETH'])await read(vault,method);
 await read(vault,'getPoolTokenInfo',[pid,pool.address]);
 const collector=(await read(vault,'getProtocolFeesCollector'))[0];
 await read(contracts.get(lower(collector)),'vault');
 const recovery=(await read(factory,'getRecoveryModeHelper'))[0];
 await read(contracts.get(lower(recovery)),'getVault');
 await read(feeProvider,'getVault');
 const id=(await read(pool,'getProviderFeeId',[3]))[0];
 for(const method of ['getFeeTypePercentage','getFeeTypeMaximumPercentage'])await read(feeProvider,method,[id]);
 for(const who of [vault.address,(await read(pool,'getOwner'))[0],collector])await read(pool,'balanceOf',[who]);
 for(const token of tokens) {
  await read(token,'decimals');
  await read(token,'balanceOf',[vault.address]);
  await read(vault,'getPoolTokenInfo',[pid,token.address]);
  await read(pool,'getCircuitBreakerState',[token.address]);
 }
}
module.exports={captureEvidence,captureServiceGetters};
