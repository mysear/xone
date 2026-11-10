#!/usr/bin/env node
'use strict';
// Diagnostic only: walk exact hash-linked MPT nodes, keeping nonmembership
// distinct from missing data. This does not relax the acceptance verifier.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const ROOT = path.resolve(__dirname, '../../..');
const {ethers} = require(path.join(ROOT, 'integration/.runtime/stable-reference/node_modules/ethers'));
const {Trie} = require(path.join(ROOT, 'integration/.runtime/receipt-root-reference/node_modules/@ethereumjs/trie'));
const {verifySAEStateAnchor} = require('./r01_public_catalog_evm_commitments.cjs');
const {verifyPrestate} = require('./r01_public_catalog_proof.cjs');
const bytes = s => Buffer.from(s.slice(2), 'hex');
const hex = b => '0x' + Buffer.from(b).toString('hex');
const hash = ethers.utils.keccak256;
const big = s => s === '0x' ? 0n : BigInt(s);
const sha = b => crypto.createHash('sha256').update(b).digest('hex');

function traceProof(root, key, proof) {
    assert(/^0x[0-9a-f]{64}$/.test(root), 'TRACE_ROOT');
    assert(/^0x(?:[0-9a-f]{2})*$/.test(key), 'TRACE_KEY');
    assert(Array.isArray(proof) && proof.length <= 128, 'TRACE_PROOF_BUDGET');
    const pool = new Map();
    for (const node of proof) {
        assert(/^0x(?:[0-9a-f]{2})+$/.test(node), 'TRACE_NODE_BYTES');
        pool.set(hash(node), node);
    }
    const nodes = [], nibbles = key.slice(2);
    let ref = root, offset = 0;
    const done = (status, value = null) => ({root, key, status, value, nodes});
    if (root === hash('0x80')) return done('ABSENT_EMPTY_TRIE');
    for (let step = 0; step <= nibbles.length + 1; step++) {
        let encoded, inline = false;
        if (Array.isArray(ref)) { encoded = ethers.utils.RLP.encode(ref); inline = true; }
        else if (ref.length === 66) { encoded = pool.get(ref); assert(encoded, 'TRACE_MISSING_NODE:' + ref); assert.equal(hash(encoded), ref, 'TRACE_HASH_LINK'); }
        else { assert(ref !== '0x', 'TRACE_EMPTY_REF'); encoded = ref; inline = true; }
        if (inline) assert(bytes(encoded).length < 32, 'TRACE_NONCANONICAL_INLINE');
        const node = ethers.utils.RLP.decode(encoded);
        assert(Array.isArray(node) && [2, 17].includes(node.length), 'TRACE_NODE_ARITY');
        const row = {hash: hash(encoded), encoded, inline, offset};
        nodes.push(row);
        if (node.length === 17) {
            row.kind = 'branch';
            if (offset === nibbles.length) { row.branch = 16; return done(node[16] === '0x' ? 'ABSENT_EMPTY_BRANCH' : 'MEMBER', node[16] === '0x' ? null : node[16]); }
            row.branch = parseInt(nibbles[offset], 16);
            ref = node[row.branch]; offset++;
            if (ref === '0x') return done('ABSENT_EMPTY_BRANCH');
            continue;
        }
        assert(typeof node[0] === 'string' && node[0].length >= 4, 'TRACE_COMPACT_PATH');
        const packed = node[0].slice(2), flag = parseInt(packed[0], 16);
        assert(flag <= 3 && (flag % 2 || packed[1] === '0'), 'TRACE_COMPACT_FLAGS');
        const part = packed.slice(flag % 2 ? 1 : 2), leaf = flag >= 2;
        assert(leaf || part.length > 0, 'TRACE_EMPTY_EXTENSION');
        row.kind = leaf ? 'leaf' : 'extension'; row.path = part;
        let matched = 0;
        while (matched < part.length && offset + matched < nibbles.length && part[matched] === nibbles[offset + matched]) matched++;
        row.matched_nibbles = matched;
        if (matched !== part.length || (leaf && offset + matched !== nibbles.length)) return done('ABSENT_DIVERGENT_PATH');
        offset += matched;
        if (leaf) return done('MEMBER', node[1]);
        ref = node[1];
    }
    throw Error('TRACE_PATH_BUDGET');
}

async function crossCheckedTrace(root, key, proof) {
    const result = traceProof(root, key, proof);
    const standard = root === hash('0x80') && !proof.length ? null : await new Trie().verifyProof(bytes(root), bytes(key), proof.map(bytes));
    assert.equal(result.value, standard === null ? null : hex(standard), 'TRACE_INDEPENDENT_PATH_DIFFERENCE');
    return result;
}

async function auditEvidence(evidence) {
    assert.equal(evidence.chain, 'avalanche', 'DIAGNOSTIC_NATIVE_CHAIN');
    assert.equal(evidence.chain_id, '43114', 'DIAGNOSTIC_NATIVE_CHAIN_ID');
    assert.equal(evidence.expected_block_hash, evidence.block.hash, 'DIAGNOSTIC_TARGET_HASH');
    const anchor = verifySAEStateAnchor(evidence.native_headers, Number(BigInt(evidence.block.number)));
    assert.equal(anchor.block_hash, evidence.block.hash, 'DIAGNOSTIC_TARGET_HEADER');
    const accounts = [], storage_contradictions = [], seen = new Set();
    for (const deployment of evidence.deployments) {
        const address = deployment.address.toLowerCase(), p = deployment.proof;
        assert(!seen.has(address), 'DIAGNOSTIC_DUPLICATE_ACCOUNT'); seen.add(address);
        const account = await crossCheckedTrace(anchor.state_root, hash(address), p.accountProof);
        // Verify scalar/code binding independently from the storage diagnostic.
        await verifyPrestate({...evidence, chain_id: '1', block: {...evidence.block, stateRoot: anchor.state_root}, deployments: [{...deployment, proof: {...p, storageProof: []}}]}, [], {allowAbsentRPCZeroHashes: true, accountEncoding: anchor.account_encoding});
        const rpc_hashes = {codeHash: p.codeHash, storageHash: p.storageHash};
        const row = {address, account, rpc_hashes, absent_rpc_zero_sentinels: account.value === null && (p.codeHash === '0x' + '00'.repeat(32) || p.storageHash === '0x' + '00'.repeat(32)), storage: []};
        const root = account.value === null ? hash('0x80') : ethers.utils.RLP.decode(account.value)[2];
        for (const slot of p.storageProof) {
            const raw_slot = '0x' + slot.key.slice(2).padStart(64, '0').toLowerCase(), secure_key = hash(raw_slot);
            const traced = await crossCheckedTrace(root, secure_key, slot.proof);
            const committed = traced.value === null ? 0n : big(ethers.utils.RLP.decode(traced.value));
            const storage = {raw_slot, secure_key, rpc_value: slot.value, committed_value: '0x' + committed.toString(16), matches: committed === big(slot.value), path: traced};
            row.storage.push(storage);
            if (!storage.matches) storage_contradictions.push({address, storage_root: root, ...storage});
        }
        accounts.push(row);
    }
    return {schema_version: 1, kind: 'authenticated-native-proof-path-diagnostic', anchor, accounts, storage_contradictions, qualified: false, account_commitments_verified: accounts.length, genuine_absent_accounts: accounts.filter(r => r.account.value === null).length, rule: 'diagnostic cannot grant deployment qualification, choose another root, substitute a key, normalize nonzero state, or attribute the cause to a provider/codec'};
}

module.exports = {traceProof, auditEvidence};
if (require.main === module) {
    const input = path.resolve(process.argv[2]), output = path.resolve(process.argv[3]);
    const raw = fs.readFileSync(input);
    auditEvidence(JSON.parse(raw)).then(result => {
        result.input = {path: path.relative(ROOT, input), sha256: sha(raw)};
        fs.writeFileSync(output, JSON.stringify(result, null, 2) + '\n', {flag: 'wx'});
        console.log(JSON.stringify({output, account_commitments_verified: result.account_commitments_verified, genuine_absent_accounts: result.genuine_absent_accounts, storage_contradictions: result.storage_contradictions.length}));
    }).catch(e => { console.error(e.stack); process.exit(1); });
}
