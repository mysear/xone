#!/usr/bin/env node
'use strict';
// Independent original-bytecode pause + Recovery branch, using the same
// authenticated runtime and signed 28-transaction prefix as the pause producer.
const fs = require('node:fs');
const path = require('node:path');
const os = require('node:os');
const assert = require('node:assert/strict');
const crypto = require('node:crypto');
const {generate} = require('./managed-v2-aum-paused-vault-golden.cjs');
const ROOT = path.resolve(__dirname, '../../..');
const REF = path.join(ROOT, 'integration/tools/reference/balancer-managed-v2-aum');
const HELPER = path.join(__dirname, 'managed-v2-aum-paused-vault-golden.cjs');
const defaults = ['integration', 'chain-parser-go', 'dex-router'].map(p =>
  path.join(ROOT, p, 'test/contracts/managed-v2-aum-recovery.json'));
const sha = bytes => crypto.createHash('sha256').update(bytes).digest('hex');
function argumentsFor(argv) {
  const args = argv.flatMap(a => /^--(?:input|output)=/.test(a) ? [a.slice(0, a.indexOf('=')), a.slice(a.indexOf('=')+1)] : [a]);
  if (args.length === 0) return {targets: defaults};
  if (args.length === 2 && args[0] === '--output') return {targets: [path.resolve(args[1])]};
  if (args.length === 1 && args[0] === '--check') return {check: true, targets: defaults};
  if (args.length === 3 && args[0] === '--check' && args[1] === '--input')
    return {check: true, targets: [path.resolve(args[2])]};
  throw Error('usage: [--output PATH | --check [--input PATH]]');
}
function checkOutput(target) {
  const parent = fs.realpathSync(path.dirname(target));
  const actual = path.join(parent, path.basename(target));
  const temporary = actual.startsWith(fs.realpathSync(os.tmpdir())+path.sep);
  assert((defaults.includes(actual) || temporary) && !actual.startsWith(REF+path.sep) &&
    !actual.endsWith('/managed-v2-aum.json'), 'output refused: '+target);
  assert(!fs.existsSync(target) || !fs.lstatSync(target).isSymbolicLink(), 'output refused: symlink '+target);
  if (fs.existsSync(target)) assert.equal(fs.statSync(target).nlink, 1, 'output refused: hardlink '+target);
}
async function main() {
  const args = argumentsFor(process.argv.slice(2));
  if (!args.check) for (const target of args.targets) checkOutput(target);
  const value = await generate({recovery: true});
  value.reference.wrapper_sha256 = sha(fs.readFileSync(__filename));
  value.reference.script_dependencies = [HELPER, __filename].map(p => ({
    path: path.relative(ROOT, p), sha256: sha(fs.readFileSync(p))}));
  const text = JSON.stringify(value, null, 2)+'\n';
  if (args.check) {
    const dir = fs.mkdtempSync(path.join(os.tmpdir(), 'xone-recovery-check-'));
    try {
      const regenerated = path.join(dir, 'managed-v2-aum-recovery.json');
      fs.writeFileSync(regenerated, text);
      const expected = fs.readFileSync(regenerated);
      for (const target of args.targets)
        assert(fs.readFileSync(target).equals(expected), 'exact regenerated reference mismatch: '+target);
    } finally { fs.rmSync(dir, {recursive: true, force: true}); }
  } else {
    for (const target of args.targets) fs.writeFileSync(target, text);
  }
  process.stdout.write(JSON.stringify({passed: true, mode: args.check ? 'check' : 'generate',
    cases: value.cases.length, statuses: value.cases.map(c => c.expected_native.status),
    raw_prefix_transactions: value.reference.raw_prefix_transactions,
    branch_headers: value.headers.length, branch_receipts: value.receipts.length,
    recovery_swap_amount_out: value.cases[4].expected_native.quantity,
    recovery_collection_minted_bpt: value.cases[5].expected_native.quantity,
    sha256: sha(text), bytes: Buffer.byteLength(text), outputs: args.targets})+'\n');
}
if (require.main === module) main().catch(error => {
  process.stdout.write(JSON.stringify({passed: false, error: String(error.stack || error)})+'\n');
  process.exitCode = 1;
});
