#!/usr/bin/env node
'use strict';
// Independent original Vault bytecode replay of supported selected single hops.
// This is local reference evidence, not a production execution/build backend.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const ROOT = path.resolve(__dirname, '../../..');
const REF = path.join(ROOT, 'integration/tools/reference/balancer-managed-v2-aum');
const V = require(path.join(REF, 'validate.cjs'));
const BANK_SHA = '1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66';
let raw;
const result = {schema_version: 1, passed: false, vm_started: false, verified: false,
  production_ready: false, all_requested_features_complete: false, routes: [], native_transaction_count: 0,
  authority: 'independent-local-original-bytecode-selected-single-hop-reference',
  limitation: 'Original signed Vault swap reference at its original local clock and sender; no public executor, trusted chain proof or VERIFIED/build admission.'};
const same = (a, b, message) => assert.equal(V.canonical(a), V.canonical(b), message);
async function run() {
  const input = JSON.parse(fs.readFileSync(0, 'utf8'));
  assert.equal(input.schema_version, 1);
  assert.equal(input.original_bank_sha256, BANK_SHA, 'unqualified input bank');
  assert(Array.isArray(input.quotes) && input.quotes.length > 0 && input.quotes.length <= 16, 'bounded nonempty quote inventory');
  const bytes = fs.readFileSync(path.join(ROOT, 'integration/test/contracts/managed-v2-aum.json'));
  assert.equal(V.sha(bytes), BANK_SHA, 'original bank digest');
  assert.equal(bytes.length, 7905624, 'original bank length');
  const bank = V.validate(JSON.parse(bytes));
  const R = (ref, kind) => V.resolveObject(bank.objects, ref.ref, kind);
  const groups = new Map();
  for (const row of input.quotes) {
    const c = bank.cases.find(c => c.name === row.case);
    assert(c && c.operation === 'swap', 'unsupported native reference case');
    const before = R(c.execution.before, 'snapshot'), header = R(before.header, 'header');
    const tx = R(c.execution.transaction, 'transaction'), execution = R(tx.execution_header, 'header');
    // This batch covers both recorded two-token swaps, before future-clock tests.
    assert(['two:swap-forward', 'two:swap-reverse'].includes(c.name), 'outside current service replay scope');
    assert.equal(header.timestamp, execution.timestamp, 'swap reference clock differs');
    const s = before.state, e = c.expectation, request = row.request, q = row.quote;
    const tokens = [R(s.underlying[e.index_in], 'token_state').address, R(s.underlying[e.index_out], 'token_state').address];
    assert.equal(request.chainKey, 'evm:31337'); assert.equal(request.kind, 'EXACT_IN');
    assert.equal(request.verification, 'FAST');
    assert.equal(request.sellToken, tokens[0]); assert.equal(request.buyToken, tokens[1]);
    assert.equal(request.sellAmount, e.raw_in); assert.equal(request.caller, tx.transaction.from);
    assert.equal(request.recipient, s.roles.trader, 'native recipient binding');
    same(q.snapshotRef.block, {chain_key: 'evm:31337', number_or_slot: Number(BigInt(header.number)),
      hash: header.hash, parent_hash: header.parentHash, timestamp_ms: Number(BigInt(header.timestamp))*1000}, 'exact quote source header');
    assert.equal(q.consumedIn, e.raw_in); assert.equal(q.grossBuyAmount, e.raw_out); assert.equal(q.refund, '0');
    assert.equal(q.routes.length, 1, 'only selected unsplit single-hop reference supported');
    const route = q.routes[0]; assert.equal(route.amount_in, e.raw_in); assert.equal(route.amount_out, e.raw_out);
    same(route.path.tokens, tokens, 'native token path');
    assert.equal(route.path.amount_in, e.raw_in); assert.equal(route.path.amount_out, e.raw_out);
    assert.equal(route.path.edges.length, 1); const edge = route.path.edges[0];
    assert.equal(edge.pool_id, s.pool); assert.equal(edge.protocol, 'balancer-v2-managed-aum');
    assert.equal(edge.from_token, tokens[0]); assert.equal(edge.to, tokens[1]);
    if (!groups.has(c.name)) groups.set(c.name, {c, before, tx, tokens, rows: []});
    groups.get(c.name).rows.push(row);
  }
  const E = require(path.join(REF, 'environment.cjs'));
  const {ethers} = E.rr('ethers');
  const ganache = E.rr('ganache');
  E.nodeClosure();
  const options = bank.reference.chain_options;
  raw = ganache.provider({...options, chain: {...options.chain, time: new Date(options.chain.time)}});
  result.vm_started = true;
  const rpc = (method, params=[]) => raw.request({method, params});
  const headers = new Map(Object.values(bank.objects).filter(x => x.kind === 'header').map(x => [Number(BigInt(x.value.number)), x.value]));
  const nativeHeader = async number => {
    const actual = await rpc('eth_getBlockByNumber', ['0x'+number.toString(16), false]);
    same(actual, headers.get(number), 'original VM header '+number); return actual;
  };
  await nativeHeader(0);
  const getters = async ref => {
    const snapshot = R(ref, 'snapshot'), header = R(snapshot.header, 'header');
    await nativeHeader(Number(BigInt(header.number)));
    for (const link of snapshot.getters) {
      const g = R(link, 'getter');
      assert.equal(await rpc('eth_call', [{to:g.to, data:g.data}, header.number]), g.result, 'original getter '+g.method);
    }
    for (const link of snapshot.accounts) {
      const account = R(link, 'account');
      assert.equal(await rpc('eth_getCode', [account.address, header.number]), R(account.code, 'code').hex, 'original account code');
      assert.equal(BigInt(await rpc('eth_getBalance', [account.address, header.number])).toString(), account.native_balance, 'original account ETH');
      assert.equal(Number(BigInt(await rpc('eth_getTransactionCount', [account.address, header.number]))), account.nonce, 'original account nonce');
    }
    await nativeHeader(Number(BigInt(header.number)));
    return snapshot.getters.length;
  };
  const erc20 = new ethers.utils.Interface(['function balanceOf(address) view returns (uint256)']);
  const balance = async (token, owner) => BigInt(await rpc('eth_call', [{to:token, data:erc20.encodeFunctionData('balanceOf',[owner])}, 'latest']));
  const finalHeight = Math.max(...[...groups.values()].map(g => Number(BigInt(R(g.tx.execution_header,'header').number))));
  let height = 0;
  for (const ref of bank.reference.transactions) {
    const tx = R(ref, 'transaction'), execution = R(tx.execution_header, 'header');
    const number = Number(BigInt(execution.number)); if (number > finalHeight) break;
    while (height + 1 < number) {
      const next = headers.get(height+1); assert(next, 'missing original empty block');
      await rpc('evm_setTime', [Number(BigInt(next.timestamp))*1000]); await rpc('evm_mine'); await nativeHeader(++height);
    }
    same(await nativeHeader(height), R(tx.pre_header,'header'), 'original transaction pre-state header');
    const selected = [...groups.values()].find(g => g.c.execution.transaction.ref === ref.ref);
    let beforeIn, beforeOut, beforeCount;
    if (selected) {
      beforeCount = await getters(selected.c.execution.before);
      beforeIn = await balance(selected.tokens[0], selected.tx.transaction.from);
      beforeOut = await balance(selected.tokens[1], selected.before.state.roles.trader);
    }
    await rpc('evm_setTime', [Number(BigInt(execution.timestamp))*1000]);
    const hash = await rpc('eth_sendRawTransaction', [tx.signed_transaction]);
    assert.equal(hash, tx.transaction.hash, 'original signed transaction hash');
    same(await rpc('eth_getTransactionReceipt', [hash]), tx.receipt, 'original actual receipt/gas/logs');
    height = number; result.native_transaction_count++;
    await nativeHeader(height);
    if (selected) {
      const afterCount = await getters(selected.c.execution.after);
      const amountIn = (beforeIn - await balance(selected.tokens[0], selected.tx.transaction.from)).toString();
      const amountOut = (await balance(selected.tokens[1], selected.before.state.roles.trader) - beforeOut).toString();
      assert.equal(amountIn, selected.c.expectation.raw_in); assert.equal(amountOut, selected.c.expectation.raw_out);
      for (const row of selected.rows) result.routes.push({case:row.case, source_header:selected.before.header,
        original_transaction:ref, quoted_amount_in:row.quote.consumedIn, quoted_amount_out:row.quote.grossBuyAmount,
        actual_amount_in:amountIn, actual_amount_out:amountOut, before_getters_checked:beforeCount,
        after_getters_checked:afterCount, complete_original_poststate_matches:true, actual_receipt_sha256:V.sha(V.canonical(tx.receipt))});
    }
  }
  assert.equal(result.routes.length, input.quotes.length, 'every selected route executed');
  result.passed = true; result.selected_route_count = result.routes.length; result.original_bank_sha256 = BANK_SHA;
}
(async () => {
  try { await run(); }
  catch (error) { result.error = String(error.message || error); process.exitCode = 1; }
  finally { if (raw) { try { await raw.disconnect(); } catch (error) { result.passed=false;result.cleanup_error=String(error);process.exitCode=1; } } }
  process.stdout.write(JSON.stringify(result)+'\n');
})();
