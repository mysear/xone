'use strict';

const assert = require('node:assert/strict');
const cp = require('node:child_process');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');

const [compiler, modules, output] = process.argv.slice(2);
const { ethers } = require(path.resolve(modules, 'ethers'));
const ganache = require(path.resolve(modules, 'ganache'));
const solc = require(path.resolve(modules, 'solc'));
const pin = require('./source-pin.json');
const root = path.resolve(__dirname, '../../../..');
const source = path.join(root, pin.local);
const sha = (bytes) => crypto.createHash('sha256').update(bytes).digest('hex');
const codeHash = (value) => sha(Buffer.from(value.slice(2), 'hex'));
const base = '0xbebc44782c7db0a1a60cb6fe97d0b483032ff1c7';
const lp = '0x6c3f90f043a72fa612cbac8115ee7e52bde6e490';
const coins = [
  '0x6b175474e89094c44da98b954eedeac495271d0f',
  '0xa0b86991c6218b36c1d19d4a2e9eb0ce3606eb48',
  '0xdac17f958d2ee523a2206206994597c13d831ec7',
];
const baseCode = 'eb09f8e03f107e5355ad85ec428609a1d8cb47bc30509c80455a2c8a108cac0d';
const lpCode = 'bb95e5787fa843002128fbe09575a115700650f4bca6efb51e70da316497438f';
const blockNumber = 21057317;
const blockHash = '0xe629de9301b3e54aeded60fdfa5978a0df85a7297d350d7edb5a76e19529dd80';
const holder = '0x28c6c06298d514db089934071355e5743bf21d60';
const gas = { gasLimit: 8000000 };
const unit = 10n ** 18n;
const fee = 4000000;
const baseAbi = [
  'function coins(uint256) view returns (address)',
  'function balances(uint256) view returns (uint256)',
  'function admin_balances(uint256) view returns (uint256)',
  'function A() view returns (uint256)',
  'function fee() view returns (uint256)',
  'function admin_fee() view returns (uint256)',
  'function get_virtual_price() view returns (uint256)',
  'function add_liquidity(uint256[3],uint256)',
];

async function main() {
  assert.equal(sha(fs.readFileSync(source)), pin.sha256);
  assert.match(cp.execFileSync(compiler, ['--version'], { encoding: 'utf8' }).trim(), /^0\.3\.1(?:\+|$)/);
  const [bytecode, runtime, abi] = cp.execFileSync(compiler, ['-f', 'bytecode,bytecode_runtime,abi', source], {
    encoding: 'utf8',
  }).trim().split('\n');
  assert.equal(codeHash(runtime), pin.runtime_sha256);
  assert.match(solc.version(), /^0\.8\.19\+/);
  const mock = JSON.parse(solc.compile(JSON.stringify({
    language: 'Solidity',
    sources: { 'Fixtures.sol': { content: fs.readFileSync(path.join(__dirname, 'Fixtures.sol'), 'utf8') } },
    settings: { outputSelection: { '*': { '*': ['abi', 'evm.bytecode.object'] } } },
  })));
  assert.ok(!mock.errors?.some((error) => error.severity === 'error'), JSON.stringify(mock.errors));
  const fixtures = mock.contracts['Fixtures.sol'];
  const archiveUrl = process.env.XONE_CURVE_META_ARCHIVE_RPC || 'https://ethereum.drpc.org';
  const archive = new ethers.providers.StaticJsonRpcProvider(archiveUrl, 1);
  assert.equal((await archive.getBlock(blockNumber)).hash, blockHash);
  const chain = ganache.provider({
    fork: { url: archiveUrl, blockNumber },
    chain: { chainId: 1, hardfork: 'shanghai', time: new Date(1730036375000) },
    wallet: { deterministic: true, unlockedAccounts: [holder] },
    miner: { timestampIncrement: 0 },
    logging: { quiet: true },
  });
  try {
    const provider = new ethers.providers.Web3Provider(chain);
    provider.pollingInterval = 10;
    assert.equal(codeHash(await provider.getCode(base, blockNumber)), baseCode);
    assert.equal(codeHash(await provider.getCode(lp, blockNumber)), lpCode);
    const signer = provider.getSigner();
    const owner = await signer.getAddress();
    const deploy = async (abi, code) => {
      const contract = await new ethers.ContractFactory(abi, code, signer).deploy(gas);
      await contract.deployed();
      return contract;
    };
    const send = async (tx) => (await tx).wait();
    const stable = new ethers.Contract(base, baseAbi, signer);
    const tokenAbi = fixtures.BookkeepingERC20.abi;
    const baseCoins = coins.map((address) => new ethers.Contract(address, tokenAbi, signer));
    const lpToken = new ethers.Contract(lp, tokenAbi, signer);
    const usdc = baseCoins[1];
    await send(usdc.connect(provider.getSigner(holder)).transfer(owner, 10000000000n, gas));
    await send(usdc.approve(base, ethers.constants.MaxUint256, gas));
    await send(stable.add_liquidity([0, 1000000000n, 0], 0, gas));
    const token = await deploy(tokenAbi, fixtures.BookkeepingERC20.evm.bytecode.object);
    await send(token.configure(18, gas));
    await send(token.mint(owner, 2000n * unit, gas));
    const metaAbi = JSON.parse(abi).map(({ gas, ...entry }) => entry);
    const implementation = await deploy(metaAbi, bytecode);
    const factory = await deploy(fixtures.CloneFactory.abi, fixtures.CloneFactory.evm.bytecode.object);
    const init = implementation.interface.encodeFunctionData('initialize', ['Fixture', 'TEST', token.address, unit, 100, fee]);
    const receipt = await send(factory.create(implementation.address, init, gas));
    const address = receipt.events.find((event) => event.event === 'Created').args.pool.toLowerCase();
    const meta = new ethers.Contract(address, metaAbi, signer);
    for (const asset of [token, lpToken, ...baseCoins]) {
      await send(asset.approve(address, ethers.constants.MaxUint256, gas));
    }
    await send(meta['add_liquidity(uint256[2],uint256)']([500n * unit, 500n * unit], 0, gas));

    const codes = {};
    for (const target of [address, base, lp, token.address, implementation.address]) {
      codes[target.toLowerCase()] = await provider.getCode(target);
    }
    const config = {
      id: address, protocol: 'curve-metausd-direct', token0: token.address.toLowerCase(), token1: lp,
      fee_numerator: fee, fee_denominator: 1e10,
      multi_asset: {
        expected_code_sha256: codeHash(codes[address]),
        expected_implementation: implementation.address.toLowerCase(),
        expected_implementation_code_sha256: pin.runtime_sha256,
        expected_base_pool_code_sha256: baseCode,
        enable_underlying: true,
        tokens: [token.address.toLowerCase(), lp].map((token) => ({ token, expected_code_sha256: codeHash(codes[token]) })),
        token_policy: 'standard-base-lp',
      },
    };
    const snapshots = [];
    const capture = async () => {
      const tag = ethers.utils.hexValue(await provider.getBlockNumber());
      const header = await chain.request({ method: 'eth_getBlockByNumber', params: [tag, false] });
      const calls = {};
      const record = async (contract, fn, args = []) => {
        const to = contract.address.toLowerCase();
        const data = contract.interface.encodeFunctionData(fn, args);
        const value = await chain.request({ method: 'eth_call', params: [{ to, data }, tag] });
        calls[to + data] = value;
        return String(contract.interface.decodeFunctionResult(fn, value)[0]);
      };
      const storage = {};
      for (const [target, slot] of [[address, '0xb'], [base, '0x5'], [base, '0xf']]) {
        storage[target + slot] = await chain.request({ method: 'eth_getStorageAt', params: [target, slot, tag] });
        storage[target + slot] = ethers.utils.hexZeroPad(storage[target + slot], 32);
      }
      const observed = { meta_balances: [], meta_admin: [], stored: [], base_balances: [], base_admin: [] };
      observed.virtual_price = await record(stable, 'get_virtual_price');
      observed.supply = await record(lpToken, 'totalSupply');
      for (const fn of ['A', 'fee', 'admin_fee']) await record(stable, fn);
      for (const fn of ['A_precise', 'fee', 'admin_fee']) await record(meta, fn);
      for (let i = 0; i < 3; i++) {
        assert.equal((await record(stable, 'coins', [i])).toLowerCase(), coins[i]);
        observed.base_balances.push(await record(stable, 'balances', [i]));
        observed.base_admin.push(await record(stable, 'admin_balances', [i]));
      }
      for (const [i, asset] of [token, lpToken].entries()) {
        await record(meta, 'coins', [i]);
        await record(asset, 'decimals');
        observed.meta_balances.push(await record(meta, 'balances', [i]));
        observed.meta_admin.push(await record(meta, 'admin_balances', [i]));
        observed.stored.push(await record(asset, 'balanceOf', [address]));
      }
      snapshots.push({ header, calls, storage, observed });
    };
    await capture();
    const swaps = [];
    const underlying = [token, ...baseCoins];
    for (const [i, j, amount] of [[2, 0, 100000017n], [0, 3, 12n * unit], [3, 1, 7000000n]]) {
      const [inBefore, outBefore] = await Promise.all([i, j].map((k) => underlying[k].balanceOf(owner)));
      await send(meta['exchange_underlying(int128,int128,uint256,uint256)'](i, j, amount, 0, gas));
      const [inAfter, outAfter] = await Promise.all([i, j].map((k) => underlying[k].balanceOf(owner)));
      assert.equal(String(inBefore.sub(inAfter)), String(amount));
      assert.ok(outAfter.gt(outBefore));
      swaps.push({ sell: underlying[i].address.toLowerCase(), buy: underlying[j].address.toLowerCase(),
        amount_in: String(amount), amount_out: String(outAfter.sub(outBefore)) });
      await capture();
    }
    fs.writeFileSync(output, JSON.stringify({ config, codes, snapshots, swaps }) + '\n');
  } finally {
    await chain.disconnect();
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
