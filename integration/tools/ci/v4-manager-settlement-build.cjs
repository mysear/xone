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
const input = {
    language: 'Solidity',
    sources: {
        [manager]: {content: readPinned(manager)},
        [harness]: {content: fs.readFileSync(path.join(references, 'singleton-advanced', harness), 'utf8')},
    },
    settings: {
        optimizer: {enabled: true, runs: 200},
        viaIR: true,
        evmVersion: 'cancun',
        metadata: {bytecodeHash: 'none', appendCBOR: false},
        remappings: ['solmate/=v4/lib/solmate/'],
        outputSelection: {'*': {'*': ['evm.bytecode.object']}},
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
fs.writeFileSync(output, JSON.stringify({
    manager: code(manager, 'PoolManager'),
    harness: code(harness, 'V4ManagerSettlement'),
    hook: code(harness, 'SettlementHook'),
}) + '\n');
