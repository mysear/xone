#!/usr/bin/env node
'use strict';
// Independent original-bytecode observations. Expected quantities are wallet or
// supply differences; no Go/Rust model or off-chain fee/swap math is used.
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const assert = require('node:assert/strict');
const ROOT = path.resolve(__dirname, '../../..');
const REF = path.join(ROOT, 'integration/tools/reference/balancer-managed-v2-aum');
const V = require(path.join(REF, 'validate.cjs'));
const BANK_SHA = '1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66';
const ADAPTER = 'balancer-v2/20230411-managed-aum-3';
const defaults = ['integration', 'chain-parser-go', 'dex-router'].map(p =>
  path.join(ROOT, p, 'test/contracts/managed-v2-aum-paused-vault.json'));
const same = (a, b, message) => assert.equal(V.canonical(a), V.canonical(b), message);
const clone = value => JSON.parse(JSON.stringify(value));
const lower = value => value.toLowerCase();
const number = value => { const n = Number(BigInt(value)); assert(Number.isSafeInteger(n)); return n; };

function argumentsFor(argv) {
  const args = argv.flatMap(a => /^--(?:input|output)=/.test(a) ? [a.slice(0, a.indexOf('=')), a.slice(a.indexOf('=')+1)] : [a]);
  if (args.length === 0) return {targets: defaults};
  if (args.length === 2 && args[0] === '--output') return {targets: [path.resolve(args[1])]};
  if (args[0] === '--check' && args.length === 1) return {check: true, targets: defaults};
  if (args[0] === '--check' && args[1] === '--input' && args.length === 3)
    return {check: true, targets: [path.resolve(args[2])]};
  throw Error('usage: [--output PATH | --check [--input PATH]]');
}

async function generate({recovery = false} = {}) {
  assert.equal(typeof recovery, 'boolean');
  const bytes = fs.readFileSync(path.join(ROOT, 'integration/test/contracts/managed-v2-aum.json'));
  assert.equal(bytes.length, 7905624, 'frozen bank byte length');
  assert.equal(V.sha(bytes), BANK_SHA, 'frozen bank SHA256');
  const bank = V.validate(JSON.parse(bytes));
  const R = (ref, kind) => V.resolveObject(bank.objects, ref.ref, kind);
  const E = require(path.join(REF, 'environment.cjs'));
  E.verifyInputs();
  same(E.nodeClosure(), bank.reference.node_closure, 'exact original runtime pins');
  const A = require(path.join(REF, 'artifacts.cjs'));
  const {builds} = A.load();
  const {ethers} = E.rr('ethers');
  const ganache = E.rr('ganache');
  // Authenticate the thin projection's original reference before reusing only
  // its identity/code/link metadata. All live public values are overwritten.
  const thin = JSON.parse(fs.readFileSync(path.join(ROOT, 'integration/test/contracts/managed-v2-aum-pools.json')));
  assert.equal(thin.original_bank.sha256, V.sha(bytes), 'thin original digest');
  assert.equal(thin.original_bank.bytes, bytes.length);
  assert.equal(thin.original_bank.filename, 'managed-v2-aum.json');
  same(thin.original_bank.reference, bank.reference, 'thin complete original reference');
  const sourceSnapshot = R(bank.cases[0].execution.before, 'snapshot');
  const originalState = sourceSnapshot.state;
  const template = clone(thin.cases[0].source_pool);
  assert.equal(template.pool_id, originalState.pool);
  const identities = bank.reference.identities.map(ref => ({...R(ref, 'identity'), ref}));
  const ids = new Map(identities.map(i => [i.address, i]));
  const labels = new Map(identities.map(i => [i.label, i]));
  const iface = new Map();
  const runtime = new Map();
  const identityProofs = new Map();
  for (const i of identities) {
    const a = A.locate(builds, i.artifact.build, i.artifact.name);
    assert.equal(a.source, i.artifact.source);
    iface.set(i.address, new ethers.utils.Interface(a.c.abi));
    const code = R(i.code, 'code').hex;
    runtime.set(i.address, code);
    const normalized = Buffer.from(code.slice(2), 'hex');
    const d = a.c.evm.deployedBytecode, encoded = d.object.split('');
    const spans = [];
    if (a.library) spans.push({start: 1, length: 20});
    for (const libs of Object.values(d.linkReferences || {}))
      for (const ss of Object.values(libs)) spans.push(...ss);
    for (const ss of Object.values(d.immutableReferences || {})) spans.push(...ss);
    for (const s of spans) { normalized.fill(0, s.start, s.start+s.length); encoded.fill('0', s.start*2, (s.start+s.length)*2); }
    assert.equal(normalized.toString('hex'), encoded.join(''), 'original runtime template');
    identityProofs.set(i.address, {code_sha256: V.sha(Buffer.from(code.slice(2), 'hex')), template_sha256: V.sha(normalized)});
  }
  const multi = template.multi_asset, m = multi.managed_v2;
  function identity(address, codeHash, templateHash) {
    const proof = identityProofs.get(address); assert(proof, 'known original identity');
    assert.equal(codeHash, proof.code_sha256, 'thin code identity');
    if (templateHash !== undefined) assert.equal(templateHash, proof.template_sha256, 'thin compiler/link template');
  }
  identity(template.pool_id, multi.code_sha256, m.pool_runtime_template_sha256);
  identity(multi.vault, multi.vault_code_sha256);
  for (const k of ['factory', 'fee_provider', 'protocol_collector']) identity(m[k], m[k+'_code_sha256']);
  for (const dep of m.dependencies) identity(dep.address, dep.code_sha256, dep.template_sha256);
  for (const asset of multi.assets) identity(asset.token, asset.code_sha256);
  assert.equal(multi.vault, originalState.vault);
  assert.equal(m.factory, labels.get('ManagedPoolFactory').address);
  assert.equal(m.fee_provider, labels.get('ProtocolFeePercentagesProvider').address);
  assert.equal(m.protocol_collector, labels.get('ProtocolFeesCollector').address);
  same(m.dependencies.map(d => [d.role, d.address]), [
    ['circuit-breaker-lib', 'CircuitBreakerLib'], ['managed-pool-add-remove-token-lib', 'ManagedPoolAddRemoveTokenLib'],
    ['managed-pool-amm-lib', 'ManagedPoolAmmLib'], ['external-weighted-math', 'ExternalWeightedMath'],
    ['recovery-mode-helper', 'RecoveryModeHelper']].map(([role, label]) => [role, labels.get(label).address]), 'original link roles');
  same(multi.assets.map(a => a.token), originalState.underlying.map(ref => R(ref, 'token_state').address), 'original token order');
  const options = bank.reference.chain_options;
  assert.equal(options.chain.hardfork, 'shanghai');
  assert.equal(options.chain.allowUnlimitedContractSize, false);
  assert.equal(options.chain.allowUnlimitedInitCodeSize, false);
  const raw = ganache.provider({...options, chain: {...options.chain, time: new Date(options.chain.time)}});
  const rpc = (method, params=[]) => raw.request({method, params});
  try {
    const provider = new ethers.providers.Web3Provider(raw);
    const wallets = [1, 4].map(k => new ethers.Wallet('0x'+k.toString(16).padStart(64, '0'), provider));
    const [admin, trader] = wallets;
    assert.equal(lower(admin.address), originalState.roles.admin);
    assert.equal(lower(trader.address), originalState.roles.trader);
    const originalHeaders = new Map(Object.values(bank.objects).filter(o => o.kind === 'header').map(o => [number(o.value.number), o.value]));
    const getHeader = async (tag='latest') => rpc('eth_getBlockByNumber', [tag, false]);
    const checkOriginalHeader = async height => {
      const h = await getHeader('0x'+height.toString(16));
      same(h, originalHeaders.get(height), 'original complete header '+height); return h;
    };
    await checkOriginalHeader(0);
    let height = 0;
    const prefix = [];
    for (const ref of bank.reference.transactions) {
      const t = R(ref, 'transaction'), h = R(t.execution_header, 'header');
      if (number(h.number) > 28) break;
      while (height+1 < number(h.number)) {
        const next = originalHeaders.get(height+1); assert(next, 'original empty block');
        await rpc('evm_setTime', [number(next.timestamp)*1000]); await rpc('evm_mine'); await checkOriginalHeader(++height);
      }
      same(await checkOriginalHeader(height), R(t.pre_header, 'header'), 'original tx source header');
      await rpc('evm_setTime', [number(h.timestamp)*1000]);
      const hash = await rpc('eth_sendRawTransaction', [t.signed_transaction]);
      assert.equal(hash, t.transaction.hash, 'original signed prefix hash');
      same(await rpc('eth_getTransactionReceipt', [hash]), t.receipt, 'original full prefix receipt');
      same(await rpc('eth_getTransactionByHash', [hash]), t.transaction, 'original full prefix transaction');
      height = number(h.number); await checkOriginalHeader(height); prefix.push(hash);
    }
    assert.equal(prefix.length, 28); assert.equal(height, 28);
    const headers = [await getHeader()], receipts = [], setupTransactions = [], setupGetters = [];
    const codeChecks = [];
    const activeIds = identities.filter(i => number(R(R(i.creation_transaction, 'transaction').execution_header, 'header').number) <= 28);
    async function observeCodes(h) {
      for (const i of activeIds) {
        const actual = await rpc('eth_getCode', [i.address, h.number]);
        assert.equal(actual, runtime.get(i.address), 'original code at fixed H '+i.label);
        assert((actual.length-2)/2 <= 24576, 'EIP170');
        codeChecks.push({address: i.address, block_tag: h.number, code_sha256: identityProofs.get(i.address).code_sha256});
      }
    }
    const keyFor = (to, method, args) => V.canonical([to, method, args]);
    async function callAt(to, method, args, h, records) {
      const abi = iface.get(to); assert(abi, 'original ABI address');
      const data = lower(abi.encodeFunctionData(method, args));
      const result = await rpc('eth_call', [{to, data}, h.number]);
      records.push({to, method, args: clone(args), data, result, block_tag: h.number});
      return abi.decodeFunctionResult(method, result);
    }
    async function snapshot(h) {
      same(await getHeader(h.number), h, 'before getter header anchor');
      const getters = [], values = new Map();
      for (const ref of sourceSnapshot.getters) {
        const g = R(ref, 'getter');
        assert.equal(lower(iface.get(g.to).encodeFunctionData(g.method, g.args)), g.data, 'original getter ABI bytes');
        values.set(keyFor(g.to, g.method, g.args), await callAt(g.to, g.method, g.args, h, getters));
      }
      const g = (to, method, args=[]) => { const v = values.get(keyFor(to, method, args)); assert(v, 'observed getter '+method); return v; };
      const pool = clone(template), v = pool.multi_asset, state = v.managed_v2;
      delete state.execution_context;
      pool.adapter_version = ADAPTER;
      pool.chain_key = 'evm:'+options.chain.chainId;
      pool.protocol = bank.profile;
      pool.token0 = v.assets[0].token; pool.token1 = v.assets[1].token;
      pool.token_behavior = 'standard'; pool.fee_denominator = 10**18;
      v.amplification = '0'; v.admin_fee = 0;
      const poolAddress = pool.pool_id, vault = v.vault;
      state.block_number = number(h.number); state.timestamp = number(h.timestamp); state.block_hash = h.hash;
      assert.equal(lower(g(poolAddress, 'getVault')[0]), vault);
      assert.equal(lower(g(poolAddress, 'getPoolId')[0]), v.pool_id);
      assert.equal(lower(g(m.factory, 'getVault')[0]), vault);
      assert(g(m.factory, 'isPoolFromFactory', [poolAddress])[0]);
      const registered = g(vault, 'getPoolTokens', [v.pool_id]);
      const registration = g(vault, 'getPool', [v.pool_id]);
      assert.equal(lower(registration[0]), poolAddress); assert.equal(Number(registration[1]), 1);
      state.bpt_index = registered[0].map(lower).indexOf(poolAddress); assert.equal(state.bpt_index, 0);
      same(registered[0].map(lower).slice(1), v.assets.map(a => a.token), 'actual registered assets');
      state.manager = lower(g(poolAddress, 'getOwner')[0]);
      state.manager_bpt_balance = g(poolAddress, 'balanceOf', [state.manager])[0].toString();
      state.protocol_collector_bpt_balance = g(poolAddress, 'balanceOf', [state.protocol_collector])[0].toString();
      const fees = g(poolAddress, 'getManagementAumFeeParams');
      state.management_aum_fee = fees[0].toString(); state.last_collection_timestamp = number(fees[1]);
      state.provider_fee_id = g(poolAddress, 'getProviderFeeId', [3])[0].toString();
      state.live_provider_aum_percentage = g(state.fee_provider, 'getFeeTypePercentage', [Number(state.provider_fee_id)])[0].toString();
      state.cached_protocol_aum_percentage = g(poolAddress, 'getProtocolFeePercentageCache', [3])[0].toString();
      for (const [to, prefix] of [[poolAddress, ''], [vault, 'vault_']]) {
        const paused = g(to, 'getPausedState');
        state[prefix+'paused'] = paused[0]; state[prefix+'pause_window_end'] = number(paused[1]); state[prefix+'buffer_period_end'] = number(paused[2]);
      }
      state.swaps_enabled = g(poolAddress, 'getSwapEnabled')[0];
      state.recovery_mode = g(poolAddress, 'inRecoveryMode')[0];
      state.allowlist_enabled = g(poolAddress, 'getMustAllowlistLPs')[0];
      state.gradual_weight_end = number(g(poolAddress, 'getGradualWeightUpdateParams')[1]);
      state.gradual_fee_end = number(g(poolAddress, 'getGradualSwapFeeUpdateParams')[1]);
      v.weights = g(poolAddress, 'getNormalizedWeights')[0].map(x => x.toString());
      const scales = g(poolAddress, 'getScalingFactors')[0];
      const rawFee = g(poolAddress, 'getSwapFeePercentage')[0].toString();
      pool.fee_numerator = Number(rawFee);
      assert.equal(BigInt(pool.fee_numerator).toString(), rawFee, 'exact public fee integer');
      state.token_info = v.assets.map((asset, index) => {
        const info = g(vault, 'getPoolTokenInfo', [v.pool_id, asset.token]);
        asset.balance = info[0].add(info[1]).toString(); asset.scaling = scales[index].toString();
        assert.equal(registered[1][index+1].toString(), asset.balance);
        const breaker = g(poolAddress, 'getCircuitBreakerState', [asset.token]);
        assert(breaker.every(x => x.toString() === '0'), 'supported inactive original breaker');
        return {cash: info[0].toString(), managed: info[1].toString(),
          vault_actual_balance: g(asset.token, 'balanceOf', [vault])[0].toString(),
          asset_manager: lower(info[3]), last_change_block: number(info[2]), circuit_breaker_enabled: false};
      });
      pool.reserve0 = v.assets[0].balance; pool.reserve1 = v.assets[1].balance;
      const bpt = g(vault, 'getPoolTokenInfo', [v.pool_id, poolAddress]);
      assert.equal(registered[1][state.bpt_index].toString(), bpt[0].add(bpt[1]).toString(), 'registered original BPT balance');
      state.total_supply = g(poolAddress, 'totalSupply')[0].toString();
      state.virtual_supply = (BigInt(state.total_supply)-BigInt(bpt[0])-BigInt(bpt[1])).toString();
      state.actual_supply = g(poolAddress, 'getActualSupply')[0].toString();
      state.pending_aum = (BigInt(state.actual_supply)-BigInt(state.virtual_supply)).toString();
      state.vault_bpt_cash = bpt[0].toString(); state.vault_bpt_managed = bpt[1].toString();
      state.vault_bpt_actual_balance = g(poolAddress, 'balanceOf', [vault])[0].toString();
      assert.equal(Object.keys(state).length, 39, 'complete public adapter 3 fields');
      await observeCodes(h);
      same(await getHeader(h.number), h, 'after getter header anchor');
      return {pool, header: h, getters};
    }
    async function appendHeader() {
      const h = await getHeader(), prev = headers[headers.length-1];
      assert.equal(number(h.number), number(prev.number)+1); assert.equal(h.parentHash, prev.hash);
      headers.push(h); return h;
    }
    async function clock(timestamp) {
      await rpc('evm_setTime', [timestamp*1000]); await rpc('evm_mine');
      const h = await appendHeader(); assert.equal(number(h.timestamp), timestamp); return h;
    }
    async function send(wallet, to, method, args, status) {
      const before = await getHeader();
      const nonce = number(await rpc('eth_getTransactionCount', [lower(wallet.address), before.number]));
      const data = lower(iface.get(to).encodeFunctionData(method, args));
      const signed = await wallet.signTransaction({type: 0, chainId: 31337, nonce, to, data, gasLimit: 10000000, gasPrice: 2000000000});
      const hash = await rpc('eth_sendRawTransaction', [signed]);
      const receipt = await rpc('eth_getTransactionReceipt', [hash]);
      const tx = await rpc('eth_getTransactionByHash', [hash]);
      assert.equal(number(receipt.status), status, 'actual signed transaction status '+method);
      const execution = await appendHeader(); assert.equal(execution.parentHash, before.hash);
      assert.equal(receipt.blockHash, execution.hash); assert.equal(tx.hash, ethers.utils.keccak256(signed));
      receipts.push(receipt);
      return {signed_transaction: signed, transaction: tx, receipt, method, args: clone(args),
        pre_header: before, execution_header: execution};
    }
    const start = number(R(bank.reference.genesis_header, 'header').timestamp);
    await clock(start+2592000);
    const vaultAddress = originalState.vault, poolAddress = originalState.pool;
    const authAddress = originalState.authorizer;
    const selector = iface.get(vaultAddress).getSighash('setPaused(bool)');
    const h = await getHeader();
    const actionId = (await callAt(vaultAddress, 'getActionId', [selector], h, setupGetters))[0];
    setupTransactions.push(await send(admin, authAddress, 'grantRole', [actionId, lower(admin.address)], 1));
    setupTransactions.push(await send(admin, vaultAddress, 'setPaused', [true], 1));
    const amount = '100000000000000000000', MAX = ethers.constants.MaxUint256.toString();
    const swapArgs = [{kind: 0, poolId: multi.pool_id, assetIn: template.token0, assetOut: template.token1, amount, userData: '0x'},
      {sender: lower(trader.address), recipient: lower(trader.address), fromInternalBalance: false, toInternalBalance: false}, 0, MAX];
    const cases = [];
    const balance = (snap, token, owner) => {
      const row = snap.getters.find(g => g.to === token && g.method === 'balanceOf' && g.args[0] === owner);
      assert(row); return BigInt(iface.get(token).decodeFunctionResult('balanceOf', row.result)[0]);
    };
    async function captureCase(name, operation, source, executionSource, status, controlMethod) {
      const tx = await send(operation === 'control' ? admin : trader, operation === 'swap' ? vaultAddress : poolAddress,
        controlMethod || (operation === 'swap' ? 'swap' : 'collectAumManagementFees'), operation === 'swap' ? swapArgs : [], status);
      const after = await snapshot(tx.execution_header);
      const beforeState = source.pool.multi_asset.managed_v2, afterState = after.pool.multi_asset.managed_v2;
      const expected = clone(after.pool), em = expected.multi_asset.managed_v2;
      for (const k of ['block_number', 'block_hash', 'timestamp']) em[k] = beforeState[k];
      em.execution_context = {block_number: number(tx.execution_header.number), timestamp: number(tx.execution_header.timestamp)};
      const params = {execution_block: em.execution_context.block_number, execution_timestamp: em.execution_context.timestamp};
      const native = {status};
      if (operation === 'swap') {
        Object.assign(params, {index_in: 0, index_out: 1, amount});
        native.actual_amount_in = (balance(source, template.token0, lower(trader.address))-balance(after, template.token0, lower(trader.address))).toString();
        native.quantity = (balance(after, template.token1, lower(trader.address))-balance(source, template.token1, lower(trader.address))).toString();
        assert.equal(native.actual_amount_in, status ? amount : '0');
        assert.equal((balance(after, template.token0, vaultAddress)-balance(source, template.token0, vaultAddress)).toString(), native.actual_amount_in);
        assert.equal((balance(source, template.token1, vaultAddress)-balance(after, template.token1, vaultAddress)).toString(), native.quantity);
        assert(status ? BigInt(native.quantity) > 0n : native.quantity === '0');
      } else {
        native.quantity = (BigInt(afterState.total_supply)-BigInt(beforeState.total_supply)).toString();
        native.observed_bpt_deltas = Object.fromEntries(['total_supply', 'manager_bpt_balance', 'protocol_collector_bpt_balance']
          .map(k => [k, (BigInt(afterState[k])-BigInt(beforeState[k])).toString()]));
        assert.equal(BigInt(native.quantity), BigInt(native.observed_bpt_deltas.manager_bpt_balance)+BigInt(native.observed_bpt_deltas.protocol_collector_bpt_balance));
        assert.equal(native.quantity, name === 'vault-paused-collection' ? '16604400166043669547' : '0',
          'original observed BPT delta '+name);
        if (operation === 'control') Object.assign(native, {
          authority: 'original-control-transaction-observation', model_execution: false});
      }
      assert.equal(afterState.paused, false);
      const observedPause = after.getters.find(g => g.to === vaultAddress && g.method === 'getPausedState');
      const effectiveVaultPaused = iface.get(vaultAddress).decodeFunctionResult('getPausedState', observedPause.result)[0];
      assert.equal(afterState.vault_paused, effectiveVaultPaused);
      if (number(tx.execution_header.timestamp) > afterState.vault_buffer_period_end) assert.equal(effectiveVaultPaused, false);
      const c = {name, operation, source_pool: source.pool, source_header: source.header, observed_pool_after: after.pool,
        expected_pool_after: expected, params, expected_native: native, before_header: tx.pre_header,
        execution_header: tx.execution_header, transaction: tx, getters_before: source.getters, getters_after: after.getters};
      if (executionSource) Object.assign(c, {execution_source_pool: executionSource.pool,
        execution_source_header: executionSource.header, execution_source_getters: executionSource.getters});
      cases.push(c); return after;
    }
    const paused = await snapshot(await getHeader());
    assert.equal(paused.pool.multi_asset.managed_v2.vault_paused, true);
    const rejected = await captureCase('vault-paused-swap', 'swap', paused, null, 0);
    const collection = await captureCase('vault-paused-collection', 'collect', rejected, null, 1);
    const collectionState = collection.pool.multi_asset.managed_v2;
    await clock(collectionState.vault_buffer_period_end+1);
    // Re-observe old H and current H independently, keeping the future-context
    // transition's canonical source bound to the real collection after-state.
    const oldSource = await snapshot(collection.header);
    same(oldSource.pool, collection.pool, 'historical collection source remains canonical');
    const executionSource = await snapshot(await getHeader());
    assert.equal(executionSource.pool.multi_asset.managed_v2.vault_paused, false);
    assert(number(executionSource.header.timestamp) < collectionState.buffer_period_end);
    const postBuffer = await captureCase('swap-after-vault-buffer', 'swap', oldSource, executionSource, 1);
    if (recovery) {
      for (const method of ['enableRecoveryMode', 'disableRecoveryMode']) {
        const selector = iface.get(poolAddress).getSighash(method);
        const actionId = (await callAt(poolAddress, 'getActionId', [selector], await getHeader(), setupGetters))[0];
        setupTransactions.push(await send(admin, authAddress, 'grantRole', [actionId, lower(admin.address)], 1));
      }
      const source = await snapshot(await getHeader());
      const normal = source.pool.multi_asset.managed_v2;
      assert.equal(normal.recovery_mode, false); assert.equal(normal.management_aum_fee, '10000000000000000');
      assert(BigInt(normal.pending_aum) > 0n, 'original normal pending fees before recovery');
      const enabled = await captureCase('enable-recovery', 'control', source, null, 1, 'enableRecoveryMode');
      const swapped = await captureCase('recovery-token-swap', 'swap', enabled, null, 1);
      // Advance only the original VM epoch clock, so zero collection and recovery
      // exit each demonstrate a real update of lastCollectionTimestamp.
      await clock(number(postBuffer.header.timestamp)+1);
      const collectionSource = await snapshot(await getHeader());
      const collected = await captureCase('recovery-zero-collection', 'collect', collectionSource, null, 1);
      await clock(number(postBuffer.header.timestamp)+2);
      const exitSource = await snapshot(await getHeader());
      const disabled = await captureCase('disable-recovery', 'control', exitSource, null, 1, 'disableRecoveryMode');
      for (const snap of [enabled, swapped, collected]) {
        const state = snap.pool.multi_asset.managed_v2;
        assert.equal(state.recovery_mode, true); assert.equal(state.management_aum_fee, '0');
      }
      for (const c of cases.slice(3)) {
        const b = c.source_pool.multi_asset.managed_v2, a = c.observed_pool_after.multi_asset.managed_v2;
        assert.equal(a.pending_aum, '0');
        for (const key of ['total_supply', 'virtual_supply', 'manager_bpt_balance', 'protocol_collector_bpt_balance',
          'vault_bpt_cash', 'vault_bpt_managed', 'vault_bpt_actual_balance']) assert.equal(a[key], b[key], c.name+' '+key);
        if (c.name === 'recovery-zero-collection' || c.name === 'disable-recovery') {
          assert(a.last_collection_timestamp > b.last_collection_timestamp);
          assert.equal(a.last_collection_timestamp, number(c.execution_header.timestamp));
        }
      }
      const exit = disabled.pool.multi_asset.managed_v2;
      assert.equal(exit.recovery_mode, false); assert.equal(exit.management_aum_fee, normal.management_aum_fee);
    }
    return {schema_version: 1, public_adapter_version: ADAPTER,
      authority: recovery ? 'independent-local-original-vault-pause-recovery-reference' : 'independent-local-original-vault-pause-reference', original_bank_sha256: BANK_SHA,
      reference: {source_script_sha256: V.sha(fs.readFileSync(__filename)), raw_prefix_transactions: 28,
        node_closure: bank.reference.node_closure, chain_options: bank.reference.chain_options,
        original_prefix_hashes: prefix, original_prefix_receipts_and_headers_matched: true,
        genesis_header: R(bank.reference.genesis_header, 'header'), setup_transactions: setupTransactions,
        setup_getters: setupGetters, code_checks: codeChecks,
        limitation: 'Independent local original-bytecode observations; no live-chain proof or production execution admission.'},
      headers, receipts, cases};
  } finally { await raw.disconnect(); }
}

async function main() {
  const args = argumentsFor(process.argv.slice(2));
  // In check mode all writes are confined to a fresh OS temporary directory.
  // No frozen metadata, output, or marker is removed or updated on any failure.
  const value = await generate(), text = JSON.stringify(value, null, 2)+'\n';
  if (args.check) {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'xone-paused-vault-check-'));
    try {
      const regenerated = path.join(dir, 'managed-v2-aum-paused-vault.json');
      fs.writeFileSync(regenerated, text);
      const expected = fs.readFileSync(regenerated);
      for (const target of args.targets) assert(fs.readFileSync(target).equals(expected), 'exact regenerated reference mismatch: '+target);
    } finally { fs.rmSync(dir, {recursive: true, force: true}); }
  } else {
    // Reject accidental writes to the frozen original bank or reference inputs.
    for (const target of args.targets) {
      assert(!target.startsWith(REF+path.sep) && !target.endsWith('/managed-v2-aum.json'), 'frozen reference output refused');
      assert(fs.statSync(path.dirname(target)).isDirectory(), 'existing output parent required');
    }
    for (const target of args.targets) fs.writeFileSync(target, text);
  }
  process.stdout.write(JSON.stringify({passed: true, mode: args.check ? 'check' : 'generate',
    cases: value.cases.length, statuses: value.cases.map(c => c.expected_native.status),
    raw_prefix_transactions: 28, branch_headers: value.headers.length, branch_receipts: value.receipts.length,
    collection_minted_bpt: value.cases[1].expected_native.quantity,
    swap_after_buffer_amount_out: value.cases[2].expected_native.quantity,
    sha256: V.sha(text), bytes: Buffer.byteLength(text), outputs: args.targets})+'\n');
}
if (require.main === module) main().catch(error => {
  process.stdout.write(JSON.stringify({passed: false, error: String(error.stack || error)})+'\n');
  process.exitCode = 1;
});

module.exports = {generate};
