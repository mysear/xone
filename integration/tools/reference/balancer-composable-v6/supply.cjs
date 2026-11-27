'use strict';

const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');

// Build-info files are from balancer-deployments at b55dc2b8a46bce8f27aed5a3b03f719f662f860a.
const pins = {
  ComposableStablePoolFactory: '582ca676398ccc343ea1ae9416f4835c96d405239505598420f9defc4d89228a',
  Vault: '8200d4cca5e38c218dca0242786efff3a19e7ca7de152ff27c49c6f624e997c5',
  Authorizer: 'b5b296b83c0448787030d463224211e4ca75e1e3653ecc9f568516d3b430bb78',
  ProtocolFeePercentagesProvider: 'bc47c78ca2c4b64d638386f60ddfe5f2a8fe5f4919afa60590cc7c3d6d5c1b85',
};
const [buildDir, runtimeDir, output] = process.argv.slice(2);
const { ethers } = require(path.resolve(runtimeDir, 'node_modules/ethers'));
const ganache = require(path.resolve(runtimeDir, 'node_modules/ganache'));
const solc = require(path.resolve(runtimeDir, 'node_modules/solc07'));
const builds = Object.fromEntries(Object.entries(pins).map(([name, hash]) => {
  const data = fs.readFileSync(path.join(buildDir, `${name}.json`));
  assert.equal(crypto.createHash('sha256').update(data).digest('hex'), hash);
  return [name, JSON.parse(data).output.contracts];
}));
const published = (build, name) => {
  const matches = Object.values(builds[build]).filter(contracts => contracts[name]);
  assert.equal(matches.length, 1, name);
  return matches[0][name];
};
const E = 10n ** 18n;
const DURATION = 3600;
const gas = { gasLimit: 8000000 };
const max = ethers.constants.MaxUint256;
const zero = ethers.constants.AddressZero;

async function main() {
  const chain = ganache.provider({
    chain: { hardfork: 'shanghai', time: new Date('2026-01-01T00:00:00Z') },
    wallet: { deterministic: true },
    miner: { timestampIncrement: 1 },
    logging: { quiet: true },
  });
  try {
    const provider = new ethers.providers.Web3Provider(chain);
    const signer = provider.getSigner(0);
    const owner = await signer.getAddress();
    const deploy = async (artifact, args = []) => {
      assert.deepEqual(artifact.evm.bytecode.linkReferences, {});
      const contract = await new ethers.ContractFactory(artifact.abi, artifact.evm.bytecode.object, signer)
        .deploy(...args, gas);
      await contract.deployed();
      return contract;
    };
    const send = async (contract, method, ...args) => (await contract[method](...args, gas)).wait();
    const authorizer = await deploy(published('Authorizer', 'Authorizer'), [owner]);
    const weth = await deploy(published('ProtocolFeePercentagesProvider', 'TestWETH'));
    const vault = await deploy(published('Vault', 'Vault'), [authorizer.address, weth.address, 30 * 86400, 86400]);
    const fees = await deploy(published('ProtocolFeePercentagesProvider', 'ProtocolFeePercentagesProvider'),
      [vault.address, E / 2n, E / 2n]);
    const collector = new ethers.Contract(await vault.getProtocolFeesCollector(),
      published('Vault', 'ProtocolFeesCollector').abi, signer);
    for (const [target, method] of [[collector, 'setSwapFeePercentage'], [fees, 'setFeeTypePercentage']]) {
      const id = await target.getActionId(target.interface.getSighash(method));
      await send(authorizer, 'grantRole', id, owner);
    }
    await send(collector, 'setSwapFeePercentage', E * 4n / 10n);
    await send(fees, 'setFeeTypePercentage', 2, E * 3n / 10n);
    const factory = await deploy(published('ComposableStablePoolFactory', 'ComposableStablePoolFactory'),
      [vault.address, fees.address, 'fixture-factory', 'fixture-pool']);
    const poolArtifact = published('ComposableStablePoolFactory', 'ComposableStablePool');
    const code = (await Promise.all((await factory.getCreationCodeContracts()).map(address =>
      provider.getCode(address)))).map(part => part.slice(2)).join('');
    assert.equal(code, poolArtifact.evm.bytecode.object);

    // The provider changes only the observed rate. The published pool performs all fee arithmetic.
    const helper = JSON.parse(solc.compile(JSON.stringify({
      language: 'Solidity',
      sources: { 'Rate.sol': { content: 'pragma solidity ^0.7.0; contract Rate { uint256 public getRate = 1e18; function set(uint256 value) external { getRate = value; } }' } },
      settings: { outputSelection: { '*': { '*': ['abi', 'evm.bytecode'] } } },
    })));
    const rateArtifact = helper.contracts['Rate.sol'].Rate;
    const cases = [];
    const bptSwaps = [];
    const refreshes = [];
    let rejectedJoins = 0;
    for (const [name, exempt, count, providerCount] of [
      ['no-rate', false, 2, 0], ['yield', false, 3, 2], ['exempt', true, 5, 5],
    ]) {
      const tokens = [];
      for (let index = 0; index < count; index++) {
        const token = await deploy(published('Authorizer', 'TestToken'), [owner, `Token ${index}`, `T${index}`, [6, 18, 8, 18, 12][index]]);
        tokens.push(token);
      }
      tokens.sort((a, b) => a.address.toLowerCase().localeCompare(b.address.toLowerCase()));
      const rates = [];
      for (let index = 0; index < providerCount; index++) rates.push(await deploy(rateArtifact));
      const created = await send(factory, 'create', name, 'BPT', tokens.map(t => t.address), 250,
        tokens.map((_, i) => rates[i]?.address ?? zero), tokens.map(() => DURATION), exempt,
        E / 100n, owner, ethers.utils.id(name));
      const address = created.events.find(event => event.event === 'PoolCreated').args.pool;
      const pool = new ethers.Contract(address, poolArtifact.abi, signer);
      const poolId = await pool.getPoolId();
      const { tokens: registered } = await vault.getPoolTokens(poolId);
      const bpt = (await pool.getBptIndex()).toNumber();
      assert.equal(registered[bpt], pool.address);
      const units = await Promise.all(tokens.map(async token => 10n ** BigInt(await token.decimals())));
      const initial = units.map(unit => 10000n * unit);
      initial.splice(bpt, 0, 0n);
      const maxAmountsIn = [...initial];
      maxAmountsIn[bpt] = max;
      for (let index = 0; index < count; index++) {
        await send(tokens[index], 'mint', owner, 100000n * units[index]);
        await send(tokens[index], 'approve', vault.address, max);
      }
      await send(vault, 'joinPool', poolId, owner, owner, {
        assets: registered, maxAmountsIn, fromInternalBalance: false,
        userData: ethers.utils.defaultAbiCoder.encode(['uint256', 'uint256[]'], [0, initial]),
      });

      const capture = async (step) => {
        const block = await provider.getBlock('latest');
        const at = { blockTag: block.number };
        const { balances } = await vault.getPoolTokens(poolId, at);
        const cached = await Promise.all(tokens.map(async (token, index) => {
          if (!rates[index]) return null;
          const [current, old] = await pool.getTokenRateCache(token.address, at);
          return { current: current.toString(), old: old.toString() };
        }));
        cached.splice(bpt, 0, null);
        const [amp, invariant] = await pool.getLastJoinExitData(at);
        const total = BigInt(await pool.totalSupply(at));
        const virtual = total - BigInt(balances[bpt]);
        const actual = BigInt(await pool.getActualSupply(at));
        return {
          name: `${name}/${step}`,
          block: block.number,
          balances: balances.map(String), scales: (await pool.getScalingFactors(at)).map(String),
          bpt_index: bpt, rates: cached, exempt,
          total_supply: String(total), last_join_exit_amp: String(amp), last_post_join_exit_invariant: String(invariant),
          swap_fee: String(await pool.getProtocolFeePercentageCache(0, at)),
          yield_fee: String(await pool.getProtocolFeePercentageCache(2, at)),
          virtual_supply: String(virtual), pending_protocol_bpt: String(actual - virtual), actual_supply: String(actual),
        };
      };
      cases.push(await capture('initial'));
      await send(vault, 'swap', {
        poolId, kind: 0, assetIn: tokens[0].address, assetOut: tokens[1].address,
        amount: 400n * units[0], userData: '0x',
      }, { sender: owner, recipient: owner, fromInternalBalance: false, toInternalBalance: false }, 0, max);
      cases.push(await capture('swap-fees'));
      if (rates.length) {
        for (const [step, rate] of [
          ['yield-gain', E * 115n / 100n],
          ['partial-loss', E * 9999n / 10000n],
          ['net-loss', E * 70n / 100n],
        ]) {
          for (let index = 0; index < rates.length; index++) {
            await send(rates[index], 'set', rate);
            await send(pool, 'updateTokenRateCache', tokens[index].address);
          }
          cases.push(await capture(step));
        }
      }
      if (rates.length) {
        for (let index = 0; index < rates.length; index++) {
          await send(rates[index], 'set', E * 107n / 100n);
          await send(pool, 'updateTokenRateCache', tokens[index].address);
        }
      }
      const block = await provider.getBlock('latest');
      await send(pool, 'startAmplificationParameterUpdate', 350, block.timestamp + 2 * 86400);
      await chain.request({ method: 'evm_increaseTime', params: [600] });
      await chain.request({ method: 'evm_mine', params: [] });
      const funds = { sender: owner, recipient: owner, fromInternalBalance: false, toInternalBalance: false };
      const tokenIndex = registered.indexOf(tokens[0].address);
      const walletBalances = async (at) => Promise.all([pool, tokens[0]].map(async token =>
        String(await token.balanceOf(owner, at))));
      const beforeFailure = await capture('failed-join');
      const walletsBefore = await walletBalances({ blockTag: beforeFailure.block });
      const feesBefore = await pool.balanceOf(collector.address, { blockTag: beforeFailure.block });
      const rejected = await vault.swap({
        poolId, kind: 1, assetIn: registered[tokenIndex], assetOut: registered[bpt],
        amount: 7n * E, userData: '0x',
      }, funds, 0, max, gas);
      await assert.rejects(rejected.wait(), error => error.receipt?.status === 0);
      const afterFailure = await capture('failed-join');
      assert.deepEqual(afterFailure, { ...beforeFailure, block: afterFailure.block });
      assert.deepEqual(await walletBalances({ blockTag: afterFailure.block }), walletsBefore);
      assert((await pool.balanceOf(collector.address, { blockTag: afterFailure.block })).eq(feesBefore));
      rejectedJoins++;
      for (const [step, kind, join, amount] of [
        ['join-exact-out', 1, true, 101n * E + 17n],
        ['exit-exact-out', 1, false, 51n * units[0] + 1n],
        ['join-exact-in', 0, true, 81n * units[0] + 1n],
        ['exit-exact-in', 0, false, 33n * E + 33n],
      ]) {
        const before = await capture(`${step}/before`);
        const [i, j] = join ? [tokenIndex, bpt] : [bpt, tokenIndex];
        const request = { poolId, kind, assetIn: registered[i], assetOut: registered[j], amount, userData: '0x' };
        const poolFee = await pool.getSwapFeePercentage({ blockTag: before.block });
        const receipt = await send(vault, 'swap', request, funds, kind === 0 ? 0 : max, max);
        const [amp] = await pool.getAmplificationParameter({ blockTag: receipt.blockNumber });
        const after = await capture(`${step}/after`);
        const swap = receipt.events.find(event => event.event === 'Swap').args;
        const mints = receipt.logs.filter(log => log.address === pool.address)
          .map(log => pool.interface.parseLog(log))
          .filter(log => log.name === 'Transfer' && log.args.from === zero && log.args.to === collector.address)
          .map(log => log.args.value.toString());
        const pending = before.pending_protocol_bpt;
        if (pending !== '0') assert.equal(mints.shift(), pending);
        assert(mints.length <= 1);
        bptSwaps.push({
          before, after, kind, i, j, amount: String(amount), amp: String(amp), pool_fee: String(poolFee),
          amount_in: String(swap.amountIn), amount_out: String(swap.amountOut), mint_after_swap: mints[0] ?? '0',
        });
        const traderIn = new ethers.Contract(registered[i], ['function balanceOf(address) view returns (uint256)'], signer);
        const traderOut = new ethers.Contract(registered[j], ['function balanceOf(address) view returns (uint256)'], signer);
        const atBefore = { blockTag: before.block };
        const atAfter = { blockTag: after.block };
        assert.equal(BigInt(await traderIn.balanceOf(owner, atBefore)) - BigInt(await traderIn.balanceOf(owner, atAfter)), BigInt(swap.amountIn));
        assert.equal(BigInt(await traderOut.balanceOf(owner, atAfter)) - BigInt(await traderOut.balanceOf(owner, atBefore)), BigInt(swap.amountOut));
      }
      if (rates.length) {
        const cacheState = async (tag) => Promise.all(registered.map(async address => {
          const index = tokens.findIndex(token => token.address === address);
          if (!rates[index]) return null;
          const [current, old, duration, expires] = await pool.getTokenRateCache(address, { blockTag: tag });
          return { current: String(current), old: String(old), duration: duration.toNumber(), expires: expires.toNumber() };
        }));
        const providerRates = registered.map(() => null);
        const decimals = registered.map(address => {
          const index = tokens.findIndex(token => token.address === address);
          return String(index === -1 ? E : E * E / units[index]);
        });
        for (let index = 0; index < rates.length; index++) {
          const rate = E * BigInt(101 + index * 3) / 100n;
          providerRates[registered.indexOf(tokens[index].address)] = String(rate);
          await send(rates[index], 'set', rate);
        }
        const request = {
          poolId, kind: 0, assetIn: tokens[0].address, assetOut: tokens[1].address,
          amount: 37n * units[0], userData: '0x',
        };
        for (const step of ['before-expiry', 'after-expiry']) {
          if (step === 'after-expiry') {
            await chain.request({ method: 'evm_increaseTime', params: [DURATION] });
            await chain.request({ method: 'evm_mine', params: [] });
          }
          const before = await cacheState('latest');
          const failed = await vault.swap(request, funds, max, max, gas);
          await assert.rejects(failed.wait(), error => error.receipt?.status === 0);
          assert.deepEqual(await cacheState('latest'), before);
          const receipt = await send(vault, 'swap', request, funds, 0, max);
          const block = await provider.getBlock(receipt.blockNumber);
          const tag = { blockTag: block.number };
          const after = await cacheState(block.number);
          const called = before.flatMap((cache, index) => cache && block.timestamp > cache.expires ? [index] : []);
          assert.equal(called.length, step === 'after-expiry' ? rates.length : 0);
          for (const index of called) assert.equal(after[index].expires, block.timestamp + before[index].duration);
          refreshes.push({
            name: `${name}/${step}`, before, after, bpt, decimals, called,
            timestamp: block.timestamp, provider_rates: providerRates, scales: (await pool.getScalingFactors(tag)).map(String),
          });
        }
      }
    }
    fs.writeFileSync(output, JSON.stringify({ builds: pins, cases, bpt_swaps: bptSwaps, rejected_joins: rejectedJoins, refreshes }, null, 2) + '\n');
  } finally {
    await chain.disconnect();
  }
}

main().catch(error => {
  console.error(error);
  process.exitCode = 1;
});
