'use strict';
// Independent ABI/RLP and controlled offline signatures. No provider or broadcast.
const fs = require('node:fs');
const path = require('node:path');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const root = path.resolve(__dirname, '../../..');
const modules = process.env.XONE_EVM_REFERENCE_MODULES || path.join(root, 'integration/.runtime/stable-reference/node_modules');
const { ethers } = require(path.join(modules, 'ethers'));
assert.equal(require(path.join(modules, 'ethers/package.json')).version, '5.8.0');
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
const canonical = value => Array.isArray(value) ? '[' + value.map(canonical).join(',') + ']' :
  value !== null && typeof value === 'object' ? '{' + Object.keys(value).sort().map(k => JSON.stringify(k) + ':' + canonical(value[k])).join(',') + '}' : JSON.stringify(value);
const quantity = value => BigInt(value) === 0n ? '0x' : ethers.BigNumber.from(BigInt(value).toString()).toHexString();
const without = (value, key) => { const copy = { ...value }; delete copy[key]; return copy; };
const pinned = path.join(root, 'integration/tools/reference/evm-execution');
const manifest = JSON.parse(fs.readFileSync(path.join(pinned, 'source-pins.json')));
const artifactName = 'packages/periphery/build/UniswapV2Router02.json';
const abiPath = path.join(pinned, artifactName);
const abiBytes = fs.readFileSync(abiPath);
const entry = manifest.packages.flatMap(p => p.files).find(f => f.path === artifactName);
assert(entry && sha(abiBytes) === entry.sha256, 'Original Router02 artifact pin differs');
const router = new ethers.utils.Interface(JSON.parse(abiBytes).abi);
const corpusPath = path.join(root, 'dex-router/test/contracts/v2-exact-out.json');
const corpusBytes = fs.readFileSync(corpusPath);
const corpus = JSON.parse(corpusBytes);
const originalCases = new Map(corpus.cases.map(c => [String(c.id), c]));
assert.equal(originalCases.size, 90);
// Public deterministic local-test mnemonic; this process has no RPC provider.
const wallet = ethers.Wallet.fromMnemonic('myth like bonus scare over problem client lizard pioneer submit female collect');
assert.equal(wallet.address.toLowerCase(), '0x90f8bf6a479f320ead074411a4b0e7944ea8c9c1');
const moduleSnapshot = () => Object.fromEntries(Object.keys(require.cache).filter(f => f !== __filename).sort().map(f => [f, sha(fs.readFileSync(f))]));

function check(file, exactOut) {
  const raw = fs.readFileSync(file), doc = JSON.parse(raw), bound = doc.bound;
  const wrapper = doc.preparation, p = exactOut ? wrapper.preparation : wrapper;
  assert(p.ready && !p.signed && !p.submitted && !p.independent_production_verification);
  assert.equal(sha(Buffer.from(canonical(without(p, 'preparation_hash')))), p.preparation_hash);
  if (exactOut) assert.equal(sha(Buffer.from(canonical(without(wrapper, 'exact_out_preparation_hash')))), wrapper.exact_out_preparation_hash);
  const tx = p.transaction, rpc = p.rpc_transaction;
  const bytes = ethers.utils.arrayify(tx.data);
  const intrinsic = 21000n + bytes.reduce((total, byte) => total + (byte === 0 ? 4n : 16n), 0n);
  assert.equal(BigInt(p.intrinsic_gas_lower_bound), intrinsic);
  assert(BigInt(tx.gas_limit) >= intrinsic);
  if (exactOut) {
    const original = originalCases.get(String(doc.case_id)); assert(original);
    assert.equal(p.required_input, original.expected.required_input);
    assert.equal(wrapper.target_output, original.expected.recipient_delta);
    assert.equal(wrapper.unused_quoted_input_budget, original.expected.unused_input_budget);
    assert.deepEqual(bound.plan.allocation.routes[0].tokens, original.path);
  }
  for (const field of ['chain_id', 'nonce', 'gas_limit']) assert.equal(typeof tx[field], 'string');
  const fields = [quantity(tx.chain_id), quantity(tx.nonce), quantity(tx.max_priority_fee_per_gas), quantity(tx.max_fee_per_gas),
    quantity(tx.gas_limit), tx.to, quantity(tx.value), tx.data, []];
  const unsigned = '0x02' + ethers.utils.RLP.encode(fields).slice(2);
  assert.equal(unsigned, p.unsigned_transaction);
  assert.deepEqual(ethers.utils.RLP.decode('0x' + unsigned.slice(4)), fields);
  for (const [field, expected] of Object.entries({ type: '0x2', chainId: '0x' + BigInt(tx.chain_id).toString(16),
    nonce: '0x' + BigInt(tx.nonce).toString(16), gas: '0x' + BigInt(tx.gas_limit).toString(16),
    maxFeePerGas: '0x' + BigInt(tx.max_fee_per_gas).toString(16), maxPriorityFeePerGas: '0x' + BigInt(tx.max_priority_fee_per_gas).toString(16),
    from: tx.from, to: tx.to, value: '0x0', data: tx.data })) assert.equal(rpc[field], expected, field);
  assert.deepEqual(rpc.accessList, []);
  const method = exactOut ? 'swapTokensForExactTokens' : 'swapExactTokensForTokens';
  const tokens = exactOut ? bound.plan.allocation.routes[0].tokens : bound.plan.allocation.routes[0].path.tokens;
  const amount = exactOut ? wrapper.target_output : p.required_input;
  const limit = exactOut ? wrapper.max_sell_amount : bound.plan.min_buy_amount;
  const deadline = (BigInt(bound.plan.context.deadline_ms) / 1000n).toString();
  const args = [amount, limit, tokens, bound.plan.context.recipient, deadline];
  assert.equal(router.encodeFunctionData(method, args), tx.data);
  const decoded = router.decodeFunctionData(method, tx.data);
  assert.equal(decoded[0].toString(), amount); assert.equal(decoded[1].toString(), limit);
  assert.deepEqual(decoded[2].map(s => s.toLowerCase()), tokens);
  assert.equal(decoded[3].toLowerCase(), bound.plan.context.recipient); assert.equal(decoded[4].toString(), deadline);
  const digest = ethers.utils.keccak256(unsigned), signature = wallet._signingKey().signDigest(digest);
  assert.equal(ethers.utils.recoverAddress(digest, signature).toLowerCase(), tx.from);
  const signed = '0x02' + ethers.utils.RLP.encode([...fields, quantity(signature.recoveryParam), quantity(signature.r), quantity(signature.s)]).slice(2);
  if (BigInt(tx.nonce) <= BigInt(Number.MAX_SAFE_INTEGER)) {
    const independent = { type: 2, chainId: Number(tx.chain_id), nonce: Number(tx.nonce), gasLimit: tx.gas_limit,
      maxFeePerGas: tx.max_fee_per_gas, maxPriorityFeePerGas: tx.max_priority_fee_per_gas, to: tx.to, value: tx.value, data: tx.data, accessList: [] };
    assert.equal(ethers.utils.serializeTransaction(independent), unsigned);
    const parsed = ethers.utils.parseTransaction(signed);
    assert.equal(parsed.from.toLowerCase(), tx.from); assert.equal(parsed.hash, ethers.utils.keccak256(signed));
    assert.equal(parsed.data, tx.data); assert.equal(ethers.utils.serializeTransaction(parsed), unsigned);
  }
  return { caseId: doc.case_id ?? path.basename(file), exactOut, hops: tokens.length - 1,
    wideAmount: BigInt(amount) > 2n ** 64n, wideNonce: BigInt(tx.nonce) > BigInt(Number.MAX_SAFE_INTEGER),
    documentSha256: sha(raw), preparationHash: p.preparation_hash, signingPayloadSha256: sha(Buffer.from(unsigned.slice(2), 'hex')),
    signedTransactionHash: ethers.utils.keccak256(signed), recoveredSender: tx.from, independentEncodingMatches: true, originalRouterAbiMatches: true, submitted: false };
}
function main() {
  if (process.argv[2] === '--inventory') {
    const output = process.argv[3]; assert(output);
    const signature = wallet._signingKey().signDigest(ethers.utils.keccak256('0x02'));
    ethers.utils.recoverAddress(ethers.utils.keccak256('0x02'), signature);
    const sources = moduleSnapshot(); assert(Object.keys(sources).length >= 20);
    const inputs = { [__filename]: sha(fs.readFileSync(__filename)), [abiPath]: sha(abiBytes), [corpusPath]: sha(corpusBytes), ...sources };
    fs.mkdirSync(path.dirname(output), { recursive: true });
    fs.writeFileSync(output, JSON.stringify({ passed: true, scope: 'COMPONENT_REFERENCE_INPUTS', node: process.version, ethers: ethers.version, inputs }, null, 2) + '\n');
    console.log(JSON.stringify({ passed: true, referenceFiles: Object.keys(inputs).length })); return;
  }
  const [inDir, outDir, output] = process.argv.slice(2); assert(inDir && outDir && output, 'Expected ExactIn dir, ExactOut dir and report path');
  // Load every lazily used primitive before freezing the independent runtime set.
  const signature = wallet._signingKey().signDigest(ethers.utils.keccak256('0x02'));
  ethers.utils.recoverAddress(ethers.utils.keccak256('0x02'), signature);
  const before = moduleSnapshot(); assert(Object.keys(before).length >= 20, "Independent runtime was not inventoried");
  const ins = fs.readdirSync(inDir).filter(f => f.endsWith('.json')).sort();
  const outs = fs.readdirSync(outDir).filter(f => f.endsWith('.json')).sort();
  assert.equal(ins.length, 5); assert.equal(outs.length, 90);
  const rows = [...ins.map(f => check(path.join(inDir, f), false)), ...outs.map(f => check(path.join(outDir, f), true))];
  assert.deepEqual(rows.filter(r => r.exactOut).map(r => String(r.caseId)).sort(), [...originalCases.keys()].sort());
  assert(rows.some(r => r.wideNonce)); assert(rows.filter(r => r.exactOut && r.hops > 1).length >= 30);
  assert(rows.filter(r => r.exactOut && r.wideAmount).length >= 9);
  const after = moduleSnapshot(); assert.deepEqual(after, before, 'Independent runtime changed or loaded an unrecorded dependency');
  const report = { passed: true, scope: 'COMPONENT_ORIGINAL_ABI_INDEPENDENT_RLP_OFFLINE_SIGNATURES', cases: rows.length,
    exactInCases: ins.length, exactOutCases: outs.length, rows,
    inputs: { [__filename]: sha(fs.readFileSync(__filename)), [abiPath]: sha(abiBytes), [corpusPath]: sha(corpusBytes), ...after },
    publicTransactionsSubmitted: false, production_ready: false, all_requested_features_complete: false };
  fs.mkdirSync(path.dirname(output), { recursive: true }); fs.writeFileSync(output, JSON.stringify(report, null, 2) + '\n');
  console.log(JSON.stringify({ passed: true, cases: rows.length, exactOutCases: outs.length, wideNonce: true, submitted: false }));
}
main();
