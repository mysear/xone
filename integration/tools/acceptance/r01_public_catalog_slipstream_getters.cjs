#!/usr/bin/env node
'use strict';

const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const {loadBank, runtime} = require('./r01_public_catalog_evm_readonly.cjs');
const ROOT = path.resolve(__dirname, '../../..');
const {ethers} = require(path.join(ROOT, 'integration/.runtime/stable-reference/node_modules/ethers'));

function getter(to, signature, args = []) {
    const abi = new ethers.utils.Interface(['function ' + signature]);
    return {to, data: abi.encodeFunctionData(signature, args)};
}

async function inventory(vm, config) {
    const context = config.slipstream_context;
    const pool = config.id;
    const factory = context.expected_factory;
    const voter = context.expected_voter;
    const swap = context.swap_fee_module.address;
    const unstaked = context.unstaked_fee_module.address;
    const call = async (to, signature, args) => {
        const query = getter(to, signature, args);
        const result = await vm.view(query.to, query.data);
        assert(result.all_code_and_storage_MPT_bound, result.error);
        return result.value;
    };
    const [, tick, , , cardinality] = ethers.utils.defaultAbiCoder.decode(
        ['uint160', 'int24', 'uint16', 'uint16', 'uint16', 'bool'],
        await call(pool, 'slot0()'));
    const spacing = Number(BigInt(await call(pool, 'tickSpacing()')));
    assert(spacing > 0 && spacing < 16384, 'SLIPSTREAM_GETTER_SPACING');
    assert(cardinality > 0 && cardinality <= 4096, 'SLIPSTREAM_GETTER_OBSERVATION_COUNT');
    const gauge = ethers.utils.defaultAbiCoder.decode(
        ['address'], await call(voter, 'gauges(address)', [pool]))[0].toLowerCase();
    const tickWord = Math.floor(Math.floor(tick / spacing) / 256);
    const bitmap = BigInt(await call(pool, 'tickBitmap(int16)', [tickWord]));
    const activeTicks = Array.from({length: 256}, (_, index) => index)
        .filter(index => (bitmap >> BigInt(index)) & 1n)
        .map(index => (tickWord * 256 + index) * spacing);
    return [
        ...['token0()', 'token1()', 'factory()', 'fee()', 'tickSpacing()', 'slot0()', 'liquidity()',
            'gauge()', 'nft()', 'factoryRegistry()', 'feeGrowthGlobal0X128()', 'feeGrowthGlobal1X128()',
            'rewardGrowthGlobalX128()', 'rewardRate()', 'rewardReserve()', 'periodFinish()', 'rollover()',
            'stakedLiquidity()', 'maxLiquidityPerTick()', 'lastUpdated()', 'gaugeFees()', 'unstakedFee()']
            .map(signature => getter(pool, signature)),
        ...['voter()', 'poolImplementation()', 'factoryRegistry()', 'swapFeeModule()',
            'unstakedFeeModule()', 'defaultUnstakedFee()'].map(signature => getter(factory, signature)),
        getter(factory, 'isPool(address)', [pool]),
        getter(factory, 'getPool(address,address,int24)', [config.token0, config.token1, spacing]),
        getter(factory, 'tickSpacingToFee(int24)', [spacing]),
        ...['factory()', 'defaultScalingFactor()', 'defaultFeeCap()', 'secondsAgo()']
            .map(signature => getter(swap, signature)),
        getter(swap, 'dynamicFeeConfig(address)', [pool]),
        getter(swap, 'discounted(address)', [context.origins[0]]),
        getter(unstaked, 'factory()'),
        getter(unstaked, 'customFee(address)', [pool]),
        getter(voter, 'gauges(address)', [pool]),
        getter(voter, 'isAlive(address)', [gauge]),
        getter(config.token0, 'balanceOf(address)', [pool]),
        getter(config.token1, 'balanceOf(address)', [pool]),
        getter(pool, 'tickBitmap(int16)', [tickWord]),
        ...activeTicks.map(index => getter(pool, 'ticks(int24)', [index])),
        ...Array.from({length: cardinality}, (_, index) => getter(pool, 'observations(uint256)', [index])),
    ];
}

async function replay(input, target, shard, shards) {
    assert(Number.isInteger(shard) && Number.isInteger(shards) &&
        shards > 0 && shards <= 8 && shard >= 0 && shard < shards, 'SLIPSTREAM_GETTER_SHARDS');
    const {catalog_row: row, pool_config: config} = JSON.parse(fs.readFileSync(input));
    const source = path.join(ROOT, row.directory);
    const context = config.slipstream_context;
    assert.equal(row.id, 'base-aerodrome-slipstream', 'SLIPSTREAM_GETTER_CASE');
    assert.equal(config.protocol, 'aerodrome-slipstream-context', 'SLIPSTREAM_GETTER_PROFILE');
    assert.equal(config.id, row.pool, 'SLIPSTREAM_GETTER_POOL');
    assert.equal(context.tick_word_radius, 0, 'SLIPSTREAM_GETTER_TICK_SCOPE');
    assert.equal(context.swap_fee_module.kind, 'dynamic', 'SLIPSTREAM_GETTER_SWAP_MODULE');
    assert.equal(context.unstaked_fee_module.kind, 'custom', 'SLIPSTREAM_GETTER_UNSTAKED_MODULE');
    const bank = await loadBank(source);
    const execution = JSON.parse(fs.readFileSync(path.join(source, 'fork-execution.json')));
    assert.equal(bank.validation.height, row.anchor.height, 'SLIPSTREAM_GETTER_HEIGHT');
    assert.equal(bank.evidence.block.hash, row.anchor.hash, 'SLIPSTREAM_GETTER_HASH');
    assert.equal(bank.original.hash, row.anchor.hash, 'SLIPSTREAM_GETTER_ORIGINAL_HASH');
    assert.deepEqual(context.origins, [execution.sender], 'SLIPSTREAM_GETTER_ORIGIN');
    const vm = await runtime(bank);
    const output = fs.openSync(target, 'wx');
    try {
        const calls = await inventory(vm, config);
        for (let index = shard; index < calls.length; index += shards) {
            const query = calls[index];
            const result = await vm.view(query.to, query.data);
            assert(result.all_code_and_storage_MPT_bound, result.error);
            const {fork_reads, ...summary} = result;
            fs.writeSync(output, JSON.stringify(summary) + '\n');
        }
    } finally {
        fs.closeSync(output);
        await vm.close();
    }
}

const [input, target, shard, shards] = process.argv.slice(2);
replay(input, target, Number(shard), Number(shards)).catch(error => {
    console.error(error.stack);
    process.exitCode = 1;
});
