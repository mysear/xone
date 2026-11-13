import test from 'node:test';
import assert from 'node:assert/strict';
import {positiveInteger,boundedInteger,routeView,snapshotView,escapeHtml,protocolBindingsMatch} from '../src/model.js';
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
