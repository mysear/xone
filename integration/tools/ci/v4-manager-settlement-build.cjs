'use strict';

const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const fs = require('node:fs');
const path = require('node:path');

const root = path.resolve(__dirname, '../../..');
const runtime = path.join(root, 'integration/.runtime/singleton-reference');
const references = path.join(root, 'integration/tools/reference');
const manifest = require(path.join(references, 'singleton-cl/source-metadata.json'));
const hashes = new Map(manifest.files.map(file => [file.local, file.sha256]));
const solc = require(path.join(runtime, 'node_modules/solc'));
const {compile: compileInfinityBin} = require('./infinity-bin-service-compile.cjs');
assert.match(solc.version(), /^0\.8\.26\+/);

function readPinned(name) {
    assert(hashes.has(name), `Unpinned Solidity import ${name}`);
    const bytes = fs.readFileSync(path.join(runtime, 'sources', name));
    const digest = crypto.createHash('sha256').update(bytes).digest('hex');
    assert.equal(digest, hashes.get(name), name);
    return bytes.toString('utf8');
}

const manager = 'v4/src/PoolManager.sol';
const harness = 'V4ManagerSettlement.sol';
const executor = 'V4SingleSwap.sol';
const infinityHarness = 'InfinityManagerSettlement.sol';
const infinityExecutor = 'InfinitySingleSwap.sol';
const vault = 'infinity/src/Vault.sol';
const clManager = 'infinity/src/pool-cl/CLPoolManager.sol';
const binManager = 'infinity/src/pool-bin/BinPoolManager.sol';
const input = {
    language: 'Solidity',
    sources: {
        [manager]: {content: readPinned(manager)},
        [harness]: {content: fs.readFileSync(path.join(references, 'singleton-advanced', harness), 'utf8')},
        [executor]: {content: fs.readFileSync(path.join(root, 'dex-router/evm', executor), 'utf8')},
        [infinityHarness]: {content: fs.readFileSync(path.join(references, 'singleton-advanced', infinityHarness), 'utf8')},
        [infinityExecutor]: {content: fs.readFileSync(path.join(root, 'dex-router/evm', infinityExecutor), 'utf8')},
        [vault]: {content: readPinned(vault)},
        [clManager]: {content: readPinned(clManager)},
    },
    settings: {
        optimizer: {enabled: true, runs: 200},
        viaIR: true,
        evmVersion: 'cancun',
        metadata: {bytecodeHash: 'none', appendCBOR: false},
        remappings: ['solmate/=v4/lib/solmate/', '@openzeppelin/=infinity/lib/openzeppelin-contracts/'],
        outputSelection: {'*': {'*': ['evm.bytecode.object', 'evm.deployedBytecode']}},
    },
};
const build = JSON.parse(solc.compile(JSON.stringify(input), {
    import: name => ({contents: readPinned(name)}),
}));
const errors = (build.errors || []).filter(error => error.severity === 'error');
assert.equal(errors.length, 0, errors.map(error => error.formattedMessage).join('\n'));
const code = (source, name) => build.contracts[source][name].evm.bytecode.object;
const output = process.argv[2];
assert(output, 'Usage: node v4-manager-settlement-build.cjs <output.json>');
const hookRuntime = build.contracts[harness].SettlementHook.evm.deployedBytecode;
assert.equal(Object.keys(hookRuntime.immutableReferences).length, 1);
const [hookManagerReferences] = Object.values(hookRuntime.immutableReferences);
const executorRuntime = build.contracts[executor].V4SingleSwap.evm.deployedBytecode;
assert.equal(Object.keys(executorRuntime.immutableReferences).length, 1);
const [executorManagerReferences] = Object.values(executorRuntime.immutableReferences);
const {out: binBuild} = compileInfinityBin();
fs.writeFileSync(output, JSON.stringify({
    manager: code(manager, 'PoolManager'),
    harness: code(harness, 'V4ManagerSettlement'),
    hook: code(harness, 'SettlementHook'),
    executor: code(executor, 'V4SingleSwap'),
    executorRuntime: executorRuntime.object,
    executorManagerReferences,
    hookRuntime: hookRuntime.object,
    hookManagerReferences,
    infinity: {
        vault: code(vault, 'Vault'),
        vaultRuntime: build.contracts[vault].Vault.evm.deployedBytecode.object,
        clManager: code(clManager, 'CLPoolManager'),
        binManager: binBuild.contracts[binManager].BinPoolManager.evm.bytecode.object,
        binVault: binBuild.contracts[vault].Vault.evm.bytecode.object,
        harness: code(infinityHarness, 'InfinityManagerSettlement'),
        executor: code(infinityExecutor, 'InfinitySingleSwap'),
        executorRuntime: build.contracts[infinityExecutor].InfinitySingleSwap.evm.deployedBytecode,
        hookRuntime: build.contracts[infinityHarness].InfinitySettlementHook.evm.deployedBytecode,
    },
}) + '\n');
