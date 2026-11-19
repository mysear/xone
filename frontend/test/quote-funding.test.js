import test from 'node:test';
import assert from 'node:assert/strict';
import {quoteFundingRows} from '../src/model.js';

function sample() {
  const binding = '2'.repeat(64);
  return {planHash: 'selected', chainKey: 'evm:1337', quality: {costRanking: {
    selectedPlanHash: 'selected', costAdjustmentApplied: true,
    selectedExecution: {bindingHash: binding, authority: 'PROVIDED_LOCAL_STATE', verification: {success: true}, transaction: {from: 'payer'}}
  }}, costs: {bindingHash: binding, selectedPlanHash: 'selected', chainKey: 'evm:1337', nativeUnit: 'EVM_NATIVE_ATOMS',
    chainFeeCompleteness: 'LOCAL_EVM_ONLY', payerNetNativeDebit: '9', nativeFunding: {bindingHash: binding, payer: 'payer',
      wrappedPrincipalNative: '99999999999999999999', totalNetDebitNative: '100000000000000000008',
      minimumOpeningBalanceNative: '100000000000000000123', sourceBalanceNative: '200000000000000000000'}}};
}

test('selected quote displays principal, network fee and reserve as exact wei', () => {
  const q = sample();
  const rows = Object.fromEntries(quoteFundingRows(q));
  assert.equal(rows['包装本金'], '99999999999999999999 wei');
  assert.equal(rows['付款人网络费（本地仿真）'], '9 wei');
  assert.equal(rows['执行所需最低起始余额'], '100000000000000000123 wei');
  q.quality.costRanking.costAdjustmentApplied = false;
  assert.equal(Object.fromEntries(quoteFundingRows(q))['报价排序依据'], '按毛额比较');
  assert.deepEqual(quoteFundingRows({}), []);
});

test('stale binding, impossible funding or lossy JSON numbers withhold amounts', () => {
  for (const mutate of [q => { q.planHash = 'old'; }, q => { q.costs.nativeFunding.bindingHash = 'foreign'; },
    q => { q.costs.nativeFunding.totalNetDebitNative = '4'; }, q => { q.costs.payerNetNativeDebit = 9; }]) {
    const q = sample(); mutate(q);
    assert.deepEqual(quoteFundingRows(q), [['资金估算', '所选路线的费用证据不一致']]);
  }
});
