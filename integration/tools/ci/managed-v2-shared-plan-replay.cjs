#!/usr/bin/env node
'use strict';
// Selected API plans against the original shared Vault. No pricing model or
// public execution admission: each invocation restores one exact source in a VM.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const ROOT = path.resolve(__dirname, '../../..');
const REF = path.join(ROOT, 'integration/tools/reference/balancer-managed-v2-aum');
const SHARED = path.join(ROOT, 'integration/tools/reference/balancer-managed-v2-shared');
const V = require(path.join(REF, 'validate.cjs'));
const BANK_SHA = '1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66';
const MAX = (1n << 256n) - 1n;
const same = (a, b, label) => assert.equal(V.canonical(a), V.canonical(b), label);
const number = x => { const n = Number(BigInt(x)); assert(Number.isSafeInteger(n), 'safe header integer'); return n; };
const address = x => { assert(typeof x === 'string' && /^0x[0-9a-fA-F]{40}$/.test(x), 'valid address'); return x.toLowerCase(); };
const decimal = (x, label, zero = false) => {
  assert(typeof x === 'string' && /^(0|[1-9][0-9]*)$/.test(x), 'canonical decimal '+label);
  assert(x.length <= 78, 'uint256 '+label);
  const n = BigInt(x); assert(n <= MAX && (zero || n > 0n), 'positive uint256 '+label); return n;
};
const hashBlock = h => ({chain_key:'evm:31337', number_or_slot:number(h.number), hash:h.hash,
  parent_hash:h.parentHash, timestamp_ms:number(h.timestamp)*1000});
let raw;
const result = {schema_version:1, passed:false, vm_started:false, verified:false,
  production_ready:false, all_requested_features_complete:false,
  authority:'independent-local-original-bytecode-selected-shared-vault-plan-reference',
  limitation:'One quote per fresh original VM; source clock retained across subsequent blocks. Local ERC20 execution evidence only; no live-chain execution gas-cost proof or public VERIFIED/build admission.',
  routes:[], transactions:[], native_transaction_count:0, prefix_transaction_count:0, plan_transaction_count:0};

function indicative(value) {
  if (!value || typeof value !== 'object') return;
  for (const [key, v] of Object.entries(value)) {
    if (['verified', 'production_ready', 'all_requested_features_complete'].includes(key)) assert.equal(v, false, 'verification remains false');
    if (key === 'verification') {
      assert(v && typeof v === 'object' && !Array.isArray(v), 'quote verification object');
      if (Object.hasOwn(v, 'success')) assert.equal(v.success, false, 'indicative verification success');
      if (Object.hasOwn(v, 'level')) assert.equal(v.level, 'INDICATIVE', 'indicative verification level');
    }
    indicative(v);
  }
}

function selectedPlan(input, shared, trader, ethers) {
  assert(!Object.hasOwn(input, 'quotes'), 'exactly one request/quote per fresh VM');
  const r = input.request, q = input.quote;
  assert(r && q && typeof r === 'object' && typeof q === 'object', 'request and complete API quote required');
  assert.equal(r.chainKey, 'evm:31337'); assert.equal(r.kind, 'EXACT_IN'); assert.equal(r.verification, 'FAST');
  assert.equal(address(r.caller), trader, 'original caller key4 identity');
  assert(ethers.utils.isAddress(r.caller), 'valid caller');
  const recipient = address(r.recipient);
  assert(ethers.utils.isAddress(r.recipient) && recipient !== '0x'+'0'.repeat(40), 'valid nonzero recipient');
  const sell = address(r.sellToken), buy = address(r.buyToken);
  assert.notEqual(sell, buy, 'distinct sell/buy tokens');
  const amount = decimal(r.sellAmount, 'request sellAmount');
  assert.equal(q.consumedIn, r.sellAmount, 'request/consumed input');
  decimal(q.consumedIn, 'consumedIn'); decimal(q.grossBuyAmount, 'grossBuyAmount');
  assert.equal(q.refund, '0', 'zero refund');
  if (Object.hasOwn(q, 'sellAmount')) assert.equal(q.sellAmount, r.sellAmount, 'quote sellAmount');
  if (Object.hasOwn(q, 'chainKey')) assert.equal(q.chainKey, r.chainKey, 'quote chainKey');
  if (Object.hasOwn(q, 'minBuyAmount')) assert(decimal(q.minBuyAmount, 'minBuyAmount', true) <= BigInt(q.grossBuyAmount), 'minimum <= gross');
  indicative(q);
  const sources = Object.values(shared.banks).filter(b => V.canonical(hashBlock(b.header)) === V.canonical(q.snapshotRef?.block));
  assert.equal(sources.length, 1, 'exact shared source header number/hash/parent/time/chain');
  const source = sources[0];
  const pools = new Map();
  for (const sn of source.pools) {
    const p = sn.pool, multi = p.multi_asset;
    assert.equal(p.chain_key, r.chainKey); assert.equal(p.protocol, 'balancer-v2-managed-aum');
    const physical = address(p.pool_id), scenario = shared.scenarios.find(s => s.pool === physical);
    assert(scenario, 'declared physical pool'); assert.equal(multi.pool_id, scenario.pool_id);
    same(multi.assets.map(a => address(a.token)), scenario.tokens.map(t => t.address), 'declared underlying tokens');
    assert(!multi.assets.some(a => address(a.token) === physical), 'no BPT path assets');
    assert(!pools.has(physical), 'unique source pool'); pools.set(physical, p);
  }
  assert(Array.isArray(q.routes) && q.routes.length > 0 && q.routes.length <= 8, '1..8 selected routes');
  let inputTotal = 0n, outputTotal = 0n, edgeTotal = 0;
  const routes = q.routes.map(route => {
    inputTotal += decimal(route.amount_in, 'route input'); outputTotal += decimal(route.amount_out, 'route output');
    assert(inputTotal <= MAX && outputTotal <= MAX, 'aggregate uint256');
    const p = route.path;
    assert(p && Array.isArray(p.tokens) && Array.isArray(p.edges), 'path tokens/edges required');
    assert(p.edges.length > 0 && p.edges.length <= 4, '1..4 edges per path');
    edgeTotal += p.edges.length; assert(edgeTotal <= 24, 'at most 24 edges');
    assert.equal(p.tokens.length, p.edges.length+1, 'continuous path length');
    const tokens = p.tokens.map(address);
    assert.equal(tokens[0], sell, 'path sell token'); assert.equal(tokens.at(-1), buy, 'path buy token');
    assert.equal(p.amount_in, route.amount_in, 'path/route input'); assert.equal(p.amount_out, route.amount_out, 'path/route output');
    const edges = p.edges.map((e, i) => {
      assert.equal(e.protocol, 'balancer-v2-managed-aum', 'declared Managed2 protocol');
      const pool = pools.get(address(e.pool_id)); assert(pool, 'declared source pool');
      assert.equal(address(e.from_token), tokens[i], 'edge from/token continuity');
      assert.equal(address(e.to), tokens[i+1], 'edge to/token continuity');
      assert.notEqual(tokens[i], tokens[i+1], 'distinct step tokens');
      for (const token of tokens.slice(i, i+2)) assert(pool.multi_asset.assets.some(a => a.token === token), 'ordinary declared underlying only');
      return {pool, assetIn:tokens[i], assetOut:tokens[i+1]};
    });
    return {quoted:route, edges};
  });
  assert.equal(inputTotal, amount, 'route amount sum = request/consumedIn');
  assert.equal(outputTotal.toString(), q.grossBuyAmount, 'route output sum = gross');
  return {source, routes, caller:trader, recipient, sell, buy};
}

async function run() {
  const input = JSON.parse(fs.readFileSync(0, 'utf8'));
  assert.equal(input.schema_version, 1, 'schema1');
  assert.equal(input.original_bank_sha256, BANK_SHA, 'original bank input SHA');
  const oldBytes = fs.readFileSync(path.join(ROOT, 'integration/test/contracts/managed-v2-aum.json'));
  assert.equal(V.sha(oldBytes), BANK_SHA, 'immutable original bank SHA'); assert.equal(oldBytes.length, 7905624);
  const sharedPath = process.env.XONE_SHARED_REFERENCE || path.join(ROOT, 'integration/test/contracts/managed-v2-shared-vault.json');
  assert(path.isAbsolute(sharedPath), 'absolute shared reference path required');
  const st = fs.lstatSync(sharedPath); assert(st.isFile() && !st.isSymbolicLink() && st.nlink === 1, 'regular shared raw required');
  assert(typeof input.shared_reference_sha256 === 'string' && /^[a-f0-9]{64}$/.test(input.shared_reference_sha256), 'full shared raw SHA256 required');
  const sharedBytes = fs.readFileSync(sharedPath);
  assert.equal(V.sha(sharedBytes), input.shared_reference_sha256, 'exact shared raw bytes SHA256');
  const bank = JSON.parse(oldBytes), shared = JSON.parse(sharedBytes);
  const R = (ref, kind) => V.resolveObject(bank.objects, ref.ref, kind);
  const E = require(path.join(REF, 'environment.cjs'));
  const {ethers} = E.rr('ethers');
  const trader = R(bank.cases[0].execution.before, 'snapshot').state.roles.trader;
  const key = '0x'+'4'.padStart(64, '0');
  const signer = new ethers.Wallet(key);
  assert.equal(address(signer.address), trader, 'original key4 trader');
  const plan = selectedPlan(input, shared, trader, ethers);
  // Raw SHA binding alone does not authenticate content: run both complete native
  // validators, including compiler, immutable source, script and Node closures.
  V.validate(bank);
  const S = require(path.join(SHARED, 'validate.cjs'));
  const validation = S.validate(shared); assert.equal(validation.passed, true, 'formal shared raw validation');
  E.nodeClosure();
  const options = bank.reference.chain_options;
  same(shared.chain_options, options, 'original chain options');
  assert.equal(options.chain.chainId, 31337); assert.equal(options.chain.hardfork, 'shanghai');
  assert.equal(options.chain.allowUnlimitedContractSize, false); assert.equal(options.chain.allowUnlimitedInitCodeSize, false);
  assert.equal(options.miner.blockGasLimit, 60000000); assert.equal(options.miner.timestampIncrement, 0);
  const ganache = E.rr('ganache');
  raw = ganache.provider({...options, chain:{...options.chain, time:new Date(options.chain.time)}});
  result.vm_started = true;
  result.original_bank_sha256 = BANK_SHA; result.shared_reference_sha256 = V.sha(sharedBytes);
  const rpc = (method, params=[]) => raw.request({method, params});
  const head = () => rpc('eth_getBlockByNumber', ['latest', false]);
  const oldHeaders = new Map(Object.values(bank.objects).filter(o => o.kind === 'header').map(o => [number(o.value.number), o.value]));
  const headers = new Map([...oldHeaders].filter(([n]) => n <= 28));
  for (const h of shared.headers) {
    if (headers.has(number(h.number))) same(headers.get(number(h.number)), h, 'prefix/shared header overlap');
    headers.set(number(h.number), h);
  }
  const nativeHeader = async n => {
    const actual = await rpc('eth_getBlockByNumber', ['0x'+n.toString(16), false]);
    assert(headers.has(n), 'recorded header exists'); same(actual, headers.get(n), 'complete native header H'+n); return actual;
  };
  const evidence = (phase, signed, transaction, receipt, header) => {
    const e = {phase, signed_transaction:signed, transaction, receipt,
      transaction_sha256:V.sha(V.canonical(transaction)), receipt_sha256:V.sha(V.canonical(receipt)),
      header_sha256:V.sha(V.canonical(header)), header};
    result.transactions.push(e); return e;
  };
  let height = 0; await nativeHeader(0);
  const originals = bank.reference.transactions.map(ref => {
    const t = R(ref, 'transaction'); return {...t, pre_header:R(t.pre_header, 'header'), execution_header:R(t.execution_header, 'header')};
  }).filter(t => number(t.execution_header.number) <= 28);
  assert.equal(originals.length, 28, 'complete original prefix28');
  const target = number(plan.source.header.number);
  const replay = [...originals, ...shared.transactions.filter(t => number(t.execution_header.number) <= target)];
  for (const tx of replay) {
    const execution = tx.execution_header, next = number(execution.number);
    assert(next > height && next <= target, 'ordered native history');
    while (height+1 < next) {
      const empty = headers.get(height+1); assert(empty, 'original empty header');
      await rpc('evm_setTime', [number(empty.timestamp)*1000]); await rpc('evm_mine'); await nativeHeader(++height);
    }
    same(await nativeHeader(height), tx.pre_header, 'complete original pre-header');
    await rpc('evm_setTime', [number(execution.timestamp)*1000]);
    const hash = await rpc('eth_sendRawTransaction', [tx.signed_transaction]);
    assert.equal(hash, tx.transaction.hash, 'original signed transaction hash');
    const receipt = await rpc('eth_getTransactionReceipt', [hash]), transaction = await rpc('eth_getTransactionByHash', [hash]);
    same(transaction, tx.transaction, 'complete original transaction'); same(receipt, tx.receipt, 'complete original receipt');
    height = next; const actualHeader = await nativeHeader(height);
    evidence(height <= 28 ? 'original-prefix' : 'shared-history', tx.signed_transaction, transaction, receipt, actualHeader);
    result.native_transaction_count++; if (height <= 28) result.prefix_transaction_count++;
  }
  while (height < target) {
    const h = headers.get(height+1); assert(h, 'recorded empty source block');
    await rpc('evm_setTime', [number(h.timestamp)*1000]); await rpc('evm_mine'); await nativeHeader(++height);
  }
  same(await nativeHeader(height), plan.source.header, 'exact requested source');
  const erc20 = new ethers.utils.Interface(['function balanceOf(address) view returns (uint256)',
    'function allowance(address,address) view returns (uint256)']);
  const vault = new ethers.utils.Interface([
    'function swap((bytes32 poolId,uint8 kind,address assetIn,address assetOut,uint256 amount,bytes userData) singleSwap,(address sender,bool fromInternalBalance,address recipient,bool toInternalBalance) funds,uint256 limit,uint256 deadline) returns (uint256)',
    'event Swap(bytes32 indexed poolId,address indexed tokenIn,address indexed tokenOut,uint256 amountIn,uint256 amountOut)']);
  const balance = async (token, owner) => BigInt(await rpc('eth_call', [{to:token, data:erc20.encodeFunctionData('balanceOf', [owner])}, 'latest']));
  const wallets = async (tokenIn, tokenOut, recipient) => ({sender_sell:(await balance(tokenIn, trader)).toString(),
    sender_buy:(await balance(tokenOut, trader)).toString(), recipient_buy:(await balance(tokenOut, recipient)).toString()});
  const observe = async original => {
    const h = await head(), cache = new Map(), pools = [];
    for (const sn of plan.source.pools) {
      const getters = [];
      for (const g of sn.getters) {
        if (original) { assert.equal(g.block_tag, h.number); assert.equal(g.block_hash, h.hash); }
        const id = address(g.to)+':'+g.data;
        if (!cache.has(id)) cache.set(id, await rpc('eth_call', [{to:g.to, data:g.data}, h.number]));
        const actual = cache.get(id);
        if (original) assert.equal(actual, g.result, 'original source getter '+sn.scenario+':'+g.method);
        getters.push({to:g.to, data:g.data, method:g.method, args:g.args, result:actual,
          block_tag:h.number, block_hash:h.hash});
      }
      pools.push({scenario:sn.scenario, pool:sn.pool.pool_id, getters});
    }
    const codes = [];
    for (const i of shared.identities) {
      const code = await rpc('eth_getCode', [i.address, h.number]);
      assert.equal(code, i.code_hex, 'original physical runtime '+i.address);
      const digest = V.sha(Buffer.from(code.slice(2), 'hex'));
      assert.equal(digest, i.code_sha256, 'runtime SHA'); codes.push({address:i.address, code_sha256:digest});
    }
    for (const i of shared.factory_creation_code_storage) {
      assert.equal(await rpc('eth_getCode', [i.address, h.number]), i.code_hex, 'original Factory code storage');
    }
    for (const sn of plan.source.pools) {
      const p = sn.pool, m = p.multi_asset;
      for (const [addr, digest] of [[p.pool_id, m.code_sha256], [m.vault, m.vault_code_sha256], ...m.assets.map(a => [a.token, a.code_sha256])]) {
        assert(codes.some(c => c.address === addr && c.code_sha256 === digest), 'static pool/runtime identity');
      }
    }
    same(await head(), h, 'getter header anchor');
    return {header:h, header_sha256:V.sha(V.canonical(h)), pools, code_checks:codes, unique_getter_calls:cache.size};
  };
  let current = await observe(true);
  result.source = {bank:plan.source.id, block:hashBlock(plan.source.header), state:current,
    getter_records_checked:current.pools.reduce((n, p) => n+p.getters.length, 0), unique_getter_calls:current.unique_getter_calls};
  const aggregateBefore = await wallets(plan.sell, plan.buy, plan.recipient);
  let sumIn = 0n, sumOut = 0n;
  for (const [routeIndex, selected] of plan.routes.entries()) {
    const routeBefore = current, routeWalletsBefore = await wallets(plan.sell, plan.buy, plan.recipient);
    let stepAmount = BigInt(selected.quoted.amount_in);
    const row = {index:routeIndex, quoted_amount_in:selected.quoted.amount_in, quoted_amount_out:selected.quoted.amount_out,
      before:routeBefore, steps:[]};
    result.routes.push(row);
    for (const [edgeIndex, edge] of selected.edges.entries()) {
      const recipient = edgeIndex === selected.edges.length-1 ? plan.recipient : trader;
      const vaultAddress = edge.pool.multi_asset.vault;
      const beforeWallets = await wallets(edge.assetIn, edge.assetOut, recipient), before = current;
      assert(BigInt(beforeWallets.sender_sell) >= stepAmount, 'actual trader balance sufficient');
      const allowance = BigInt(await rpc('eth_call', [{to:edge.assetIn,
        data:erc20.encodeFunctionData('allowance', [trader, vaultAddress])}, 'latest']));
      assert(allowance >= stepAmount, 'original allowance insufficient; approval forbidden');
      const data = vault.encodeFunctionData('swap', [
        {poolId:edge.pool.multi_asset.pool_id, kind:0, assetIn:edge.assetIn, assetOut:edge.assetOut, amount:stepAmount.toString(), userData:'0x'},
        {sender:trader, fromInternalBalance:false, recipient, toInternalBalance:false}, 0, MAX.toString()]);
      const nonce = number(await rpc('eth_getTransactionCount', [trader, 'latest']));
      const signed = await signer.signTransaction({type:0, chainId:31337, nonce, to:vaultAddress, data,
        gasLimit:3000000, gasPrice:2000000000});
      await rpc('evm_setTime', [number(plan.source.header.timestamp)*1000]);
      const hash = await rpc('eth_sendRawTransaction', [signed]);
      assert.equal(hash, ethers.utils.keccak256(signed), 'actual signed swap hash');
      const receipt = await rpc('eth_getTransactionReceipt', [hash]), transaction = await rpc('eth_getTransactionByHash', [hash]), h = await head();
      const txEvidence = evidence('selected-plan', signed, transaction, receipt, h); result.plan_transaction_count++;
      assert.equal(transaction.from, trader, 'actual transaction caller'); assert.equal(transaction.type, '0x0');
      assert.equal(transaction.gasPrice, '0x77359400'); assert(number(transaction.gas) <= 10000000);
      assert.equal(receipt.status, '0x1', 'actual Vault swap receipt success');
      assert.equal(receipt.blockHash, h.hash); assert.equal(transaction.blockHash, h.hash);
      assert.equal(h.parentHash, before.header.hash); assert.equal(number(h.number), number(before.header.number)+1);
      assert.equal(h.timestamp, plan.source.header.timestamp, 'source clock preserved');
      const afterWallets = await wallets(edge.assetIn, edge.assetOut, recipient);
      const actualIn = BigInt(beforeWallets.sender_sell)-BigInt(afterWallets.sender_sell);
      const actualOut = BigInt(afterWallets.recipient_buy)-BigInt(beforeWallets.recipient_buy);
      assert.equal(actualIn, stepAmount, 'actual sender debit'); assert(actualOut > 0n, 'actual recipient credit');
      const events = receipt.logs.filter(l => l.address === vaultAddress).flatMap(l => {
        try { const e = vault.parseLog(l); return e.name === 'Swap' ? [e.args] : []; } catch { return []; }
      });
      assert.equal(events.length, 1, 'one actual Vault Swap event');
      const event = events[0]; assert.equal(event.poolId.toLowerCase(), edge.pool.multi_asset.pool_id);
      assert.equal(address(event.tokenIn), edge.assetIn); assert.equal(address(event.tokenOut), edge.assetOut);
      assert.equal(event.amountIn.toString(), actualIn.toString()); assert.equal(event.amountOut.toString(), actualOut.toString());
      current = await observe(false);
      row.steps.push({pool:edge.pool.pool_id, pool_id:edge.pool.multi_asset.pool_id, asset_in:edge.assetIn, asset_out:edge.assetOut,
        sender:trader, recipient, original_allowance:allowance.toString(), actual_amount_in:actualIn.toString(), actual_amount_out:actualOut.toString(),
        wallets_before:beforeWallets, wallets_after:afterWallets, before, after:current, transaction:txEvidence});
      stepAmount = actualOut; // Actual ERC20 credit alone funds the next original swap.
    }
    const routeWalletsAfter = await wallets(plan.sell, plan.buy, plan.recipient);
    const actualIn = BigInt(routeWalletsBefore.sender_sell)-BigInt(routeWalletsAfter.sender_sell);
    const actualOut = BigInt(routeWalletsAfter.recipient_buy)-BigInt(routeWalletsBefore.recipient_buy);
    Object.assign(row, {actual_amount_in:actualIn.toString(), actual_amount_out:actualOut.toString(), after:current,
      wallets_before:routeWalletsBefore, wallets_after:routeWalletsAfter});
    result.final_state = current;
    assert.equal(actualIn.toString(), selected.quoted.amount_in, 'actual route input');
    assert.equal(actualOut.toString(), selected.quoted.amount_out, 'actual route output');
    sumIn += actualIn; sumOut += actualOut;
  }
  const aggregateAfter = await wallets(plan.sell, plan.buy, plan.recipient);
  result.wallets_before = aggregateBefore; result.wallets_after = aggregateAfter;
  result.actual_amount_in = (BigInt(aggregateBefore.sender_sell)-BigInt(aggregateAfter.sender_sell)).toString();
  result.actual_amount_out = (BigInt(aggregateAfter.recipient_buy)-BigInt(aggregateBefore.recipient_buy)).toString();
  assert.equal(sumIn.toString(), input.request.sellAmount, 'actual aggregate sell input');
  assert.equal(sumOut.toString(), input.quote.grossBuyAmount, 'actual aggregate gross output');
  assert.equal(result.actual_amount_in, sumIn.toString()); assert.equal(result.actual_amount_out, sumOut.toString());
  result.selected_route_count = result.routes.length;
  result.final_state = await observe(false);
  same(result.final_state, current, 'final complete getters remain actual VM state');
  result.passed = true;
}

(async () => {
  try { await run(); }
  catch (error) { result.error = String(error.message || error); process.exitCode = 1; }
  finally {
    if (raw) try { await raw.disconnect(); } catch (error) { result.passed=false; result.cleanup_error=String(error); process.exitCode=1; }
  }
  process.stdout.write(JSON.stringify(result)+'\n');
})();
