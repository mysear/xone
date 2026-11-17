import test from 'node:test';
import assert from 'node:assert/strict';
import {positiveInteger,boundedInteger,routeView,snapshotView,escapeHtml,protocolBindingsMatch,singlePreparationPool,evmPreparationPools,evmPreparedSimulationAvailable,evmPreparedEvidenceMatches,evmFeeRequest} from '../src/model.js';
test('raw amounts retain precision above Number.MAX_SAFE_INTEGER and reject decimal/exponent/overflow',()=>{
  assert.equal(positiveInteger('9007199254740993123456','数量'),'9007199254740993123456');assert.equal(positiveInteger('000123','数量'),'123');
  for(const v of ['0','-1','1e18','0.1','NaN','Infinity',' 10','1'.repeat(1000),(1n<<256n).toString()])assert.throws(()=>positiveInteger(v,'数量'));
});
test('hop and slippage limits are bounded integers',()=>{assert.equal(boundedInteger('4',1,4,'跳数'),4);for(const v of ['5','0','1.5','1e1'])assert.throws(()=>boundedInteger(v,1,4,'跳数'));});
test('ExactIn path and ExactOut snake_case routes preserve protocol/pool/amount',()=>{
  const path={edges:[{protocol:'meteora-dlmm-orders',pool_id:'pool1'}],tokens:['A','B']};
  assert.deepEqual(routeView({path,amount_in:'699',amount_out:'1000'}),routeView({...path,amount_in:'699',amount_out:'1000',target_output:'999'}));assert.deepEqual(routeView({path,amount_in:'699',amount_out:'1000'}).pools,['pool1']);
});
test('snapshot uses the public snake_case block identity and markup is escaped',()=>{assert.equal(snapshotView({snapshotRef:{block:{chain_key:'solana:mainnet-beta',number_or_slot:300000000}}}),'solana:mainnet-beta / 300000000');assert.equal(escapeHtml('<script a="1">&'), '&lt;script a=&quot;1&quot;&gt;&amp;');});

test('protocol bindings reject changed capability views even when the parent view is unchanged',()=>{
 const q={protocolViewIds:{'proto@adapter':'old'}};
 const chain={viewId:'same',protocolViews:[{protocol:'proto',adapterVersion:'adapter',viewId:'old',ready:true}]};
 assert.equal(protocolBindingsMatch(q,chain),true);
 assert.equal(protocolBindingsMatch(q,{...chain,protocolViews:[{...chain.protocolViews[0],viewId:'new'}]}),false);
 assert.equal(protocolBindingsMatch(q,{...chain,protocolViews:[{...chain.protocolViews[0],ready:false}]}),false);
 assert.equal(protocolBindingsMatch(q,{...chain,protocolViews:[]}),false);
 assert.equal(protocolBindingsMatch({},chain),true);
});

test('preparation uses the exact singleton route and protocol, never a supported edge inside a split or multi-hop',()=>{
 const pool={poolId:'p',protocol:'pump-amm-classic-wsol',available:true,execution:{preparation:true}};
 const chain={pools:[pool]};const edge={pool_id:'p',protocol:pool.protocol};
 assert.equal(singlePreparationPool({routes:[{edges:[edge]}]},chain),pool);
 assert.equal(singlePreparationPool({routes:[{path:{edges:[edge]}}]},chain),pool);
 for(const quote of [{routes:[]},{routes:[{edges:[edge]},{edges:[edge]}]},{routes:[{edges:[edge,{pool_id:'other',protocol:'other'}]}]},{routes:[{edges:[{...edge,protocol:'other'}]}]}])assert.equal(singlePreparationPool(quote,chain),null);
 assert.equal(singlePreparationPool({routes:[{edges:[edge]}]},{pools:[{...pool,available:false}]}),null);
});


test('EVM preparation requires every exact route pool and keeps fee budgets in strings',()=>{
 const edge={pool_id:'a',protocol:'uniswap-v2-complete'};const pool={poolId:'a',protocol:edge.protocol,available:true,execution:{evmPreparation:true}};
 const chain={chainKey:'evm:1',pools:[pool]};const quote={chainKey:'evm:1',routes:[{edges:[edge]}]};
 assert.deepEqual(evmPreparationPools(quote,chain),[pool]);
 assert.deepEqual(evmPreparationPools({...quote,routes:[{edges:[edge,edge]}]},chain),[]);
 assert.deepEqual(evmPreparationPools({...quote,chainKey:'evm:2'},chain),[]);
 assert.deepEqual(evmPreparationPools(quote,{...chain,pools:[{...pool,execution:{}}]}),[]);
 const second={...pool,poolId:'b'};const multi={...quote,routes:[{edges:[edge,{...edge,pool_id:'b'}]}]};
 assert.deepEqual(evmPreparationPools(multi,{...chain,pools:[pool,second]}),[pool,second]);
 assert.deepEqual(evmPreparationPools({...multi,kind:'EXACT_OUT'},{...chain,pools:[pool,second]}),[pool,second]);
 assert.deepEqual(evmFeeRequest('500000','3000000000','1000000000','1500000000000000'),{gas_limit:'500000',max_fee_per_gas:'3000000000',max_priority_fee_per_gas:'1000000000',max_network_fee:'1500000000000000'});
 assert.deepEqual(evmFeeRequest('21000','0','0','0'),{gas_limit:'21000',max_fee_per_gas:'0',max_priority_fee_per_gas:'0',max_network_fee:'0'});
 for(const args of [['20999','0','0','0'],['21000','1','2','42000'],['21000','2','1','1'],['21000','1e10','0','1000000000000']]) assert.throws(()=>evmFeeRequest(...args));
});

test('EVM independent simulation requires every selected pool runtime and retains authority boundaries',()=>{
 const pool={poolId:'pool',protocol:'uniswap-v2-complete',available:true,execution:{evmPreparation:true,evmIndependentLocalSimulation:true}};
 const q={chainKey:'evm:1',kind:'EXACT_IN',quoteId:'q',planHash:'plan',quoteViewId:'view',grossBuyAmount:'9',routes:[{edges:[{pool_id:'pool',protocol:pool.protocol}]}]};
 const chain={chainKey:'evm:1',pools:[pool]};
 assert.equal(evmPreparedSimulationAvailable(q,chain),true);
 assert.equal(evmPreparedSimulationAvailable(q,{...chain,pools:[{...pool,execution:{evmPreparation:true}}]}),false);
 const hash='ab'.repeat(32);
 for(const kind of ['EXACT_IN','EXACT_OUT']){
  const quote={...q,kind};
  const inner={preparation_hash:'component'};
  const p={ready:true,quoteViewId:'view',preparationHash:'api',rpcPreparation:{rpc_preparation_hash:'rpc'},requiredInput:'3',
    component:kind==='EXACT_OUT'?{preparation:inner,exact_out_preparation_hash:'outer'}:inner};
  const simulation={verified:false,matches_plan:true,actual_input_consumed:'3',actual_buy_amount:'9'};
  const evidence={quoteKind:kind,quoteId:'q',planHash:'plan',quoteViewId:'view',apiPreparationHash:'api',rpcPreparationHash:'rpc',
    verification:{success:true},verified:false,authority:'PROVIDED_LOCAL_STATE',fees:{chainFeeCompleteness:'LOCAL_EVM_ONLY'},
    preparedExecution:{preparation_hash:'component',unsigned_transaction_hash:hash,exact_out_preparation_hash:'outer',simulation}};
  assert.equal(evmPreparedEvidenceMatches(quote,p,evidence,hash),true);
  for(const field of ['quoteId','quoteViewId','apiPreparationHash','rpcPreparationHash']){
   assert.equal(evmPreparedEvidenceMatches(quote,p,{...evidence,[field]:'other'},hash),false);
  }
  assert.equal(evmPreparedEvidenceMatches(quote,p,evidence,'00'.repeat(32)),false);
  assert.equal(evmPreparedEvidenceMatches(quote,p,{...evidence,verified:true},hash),false);
  assert.equal(evmPreparedEvidenceMatches(quote,p,{...evidence,authority:'PUBLIC_VERIFIED'},hash),false);
  assert.equal(evmPreparedEvidenceMatches(quote,p,{...evidence,preparedExecution:{...evidence.preparedExecution,
    simulation:{...simulation,actual_buy_amount:'10'}}},hash),false);
 }
});
