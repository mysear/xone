// Contract of the generated independent reference fixture, no service imports.
const assert = require('node:assert/strict');
const fs = require('node:fs');
const path = require('node:path');
const root = path.resolve(__dirname, '../../..');
const {ethers} = require(path.join(root, 'integration/.runtime/singleton-reference/node_modules/ethers'));
const first = fs.readFileSync(path.join(root, 'integration/test/contracts/singleton-quotes.json'));
assert.deepEqual(first, fs.readFileSync(path.join(root, 'dex-router/test/contracts/singleton-quotes.json')));
const fixture = JSON.parse(first);
assert.ok(fixture.cases.length >= 100);
const protocols = ['uniswap-v4', 'pancake-infinity-cl'];
for (const protocol of protocols) {
  const cases = fixture.cases.filter(c => c.pool.protocol === protocol);
  assert.ok(cases.length >= 50);
  for (const direction of [true, false]) {
    const directed = cases.filter(c => c.zero_for_one === direction);
    assert.ok(directed.some(c => !c.expected.reverted && c.reference.full_input_consumed));
    assert.ok(directed.some(c => c.reference.partial));
    assert.ok(directed.some(c => c.reference.reverted));
    assert.ok(directed.some(c => c.tags.includes('empty-word') && !c.expected.reverted && Math.abs(c.expected.tick) > 256*c.pool.singleton_cl.core.tick_spacing));
    assert.ok(directed.some(c => c.tags.includes('crossing-tick') && !c.expected.reverted));
    assert.ok(directed.some(c => c.tags.includes('zero-liquidity')));
    assert.ok(directed.some(c => c.tags.includes('sequential')));
    const allocation = [0,1,2].map(i => {
      const name = `${protocol}/allocation/${direction}/${i}`;
      const entry = cases.find(c => c.name === name);
      assert.ok(entry, 'missing same-direction allocation case: ' + name);
      assert.equal(entry.zero_for_one, direction);
      assert.ok(entry.tags.includes('allocation'));
      assert.ok(!entry.expected.reverted);
      assert.equal(entry.reference.consumed_in, entry.input);
      return entry;
    });
    assert.deepEqual(allocation.map(c=>c.input), ['20000000','1000000','3000000']);
    assert.deepEqual(allocation[0].pool, cases.find(c=>c.name===`${protocol}/sequential/${direction}/0`).pool);
    for(let i=1;i<allocation.length;i++) {
      const previous = allocation[i-1], current = allocation[i];
      const continued = JSON.parse(JSON.stringify(previous.pool));
      Object.assign(continued.singleton_cl.core, previous.expected.next_state);
      assert.deepEqual(current.pool, continued);
    }
    const sequence = fixture.reference.allocation_sequences.find(s=>s.protocol===protocol && s.zero_for_one===direction);
    assert.ok(sequence);
    assert.deepEqual(sequence.cases, allocation.map(c=>c.name));
    assert.deepEqual(sequence.inputs, allocation.map(c=>c.input));
    assert.equal(sequence.same_direction, true);
  }
  assert.ok(cases.some(c => BigInt(c.input) === (1n << 127n) && !c.expected.reverted));
  assert.ok(cases.some(c => c.pool.singleton_cl.lp_fee === 0));
  assert.ok(cases.some(c => c.pool.singleton_cl.lp_fee === 3000));
  assert.ok(cases.some(c => c.pool.singleton_cl.lp_fee === 999999));
  const max = protocol === 'uniswap-v4' ? 1000 : 4000;
  assert.ok(cases.some(c => c.pool.singleton_cl.protocol_fee0 === max));
  assert.ok(cases.some(c => c.pool.singleton_cl.protocol_fee1 === max));
}
assert.equal(fixture.reference.allocation_sequences.length, 4);
assert.equal(new Set(fixture.cases.map(c=>c.name)).size, fixture.cases.length);
for (const c of fixture.cases) {
  const p = c.pool, s = p.singleton_cl, core = s.core;
  assert.equal(p.chain_key, 'evm:31337');
  assert.equal(p.pool_id, `${s.manager}/${s.pool_id}`);
  assert.match(s.manager_code_sha256, /^[a-f0-9]{64}$/);
  assert.match(s.pool_id, /^0x[a-f0-9]{64}$/);
  assert.ok(p.token0 < p.token1 && BigInt(p.token0) > 0n);
  assert.equal(p.fee_numerator, s.lp_fee);
  assert.ok(s.lp_fee < 1000000);
  assert.equal(p.fee_denominator, 1000000);
  assert.equal(s.hooks, '0x' + '00'.repeat(20));
  assert.equal(p.reserve0, '0'); assert.equal(p.reserve1, '0');
  assert.equal(p.token_behavior, 'standard');
  const encoding = p.protocol === 'uniswap-v4'
    ? ethers.utils.defaultAbiCoder.encode(['address','address','uint24','int24','address'],[p.token0,p.token1,s.lp_fee,core.tick_spacing,s.hooks])
    : ethers.utils.defaultAbiCoder.encode(['address','address','address','address','uint24','bytes32'],[p.token0,p.token1,s.hooks,s.manager,s.lp_fee,s.parameters]);
  assert.equal(s.pool_id, ethers.utils.keccak256(encoding));
  assert.equal(core.liquidity, core.ticks.filter(t=>t.tick<=core.tick).reduce((n,t)=>n+BigInt(t.liquidity_net),0n).toString());
  for(let i=1;i<core.tick_words.length;i++) assert.equal(core.tick_words[i].word_pos,core.tick_words[i-1].word_pos+1);
  assert.equal(s.parameters, p.protocol === 'uniswap-v4' ? '' : '0x' + (BigInt(core.tick_spacing) << 16n).toString(16).padStart(64, '0'));
  if (!c.reference.reverted) {
    assert.equal(BigInt(c.reference.consumed_in) + BigInt(c.reference.remaining_in), BigInt(c.input));
    assert.equal(c.reference.partial, BigInt(c.reference.remaining_in) !== 0n);
    assert.equal(c.reference.full_input_consumed, !c.reference.partial);
    if (!c.expected.reverted) {
      assert.ok(c.reference.full_input_consumed);
      assert.ok(BigInt(c.expected.amount_out) > 0n);
      assert.equal(c.expected.consumed_in, c.input);
      assert.equal(c.expected.remaining_in, '0');
      assert.deepEqual(c.expected.next_state, {sqrt_price_x96:c.expected.sqrt_price_x96,tick:c.expected.tick,liquidity:c.expected.liquidity});
      for (const field of ['amount_out', 'sqrt_price_x96', 'tick', 'liquidity']) assert.equal(c.expected[field], c.reference[field]);
    }
    assert.deepEqual(c.reference.next_state, {sqrt_price_x96:c.reference.sqrt_price_x96,tick:c.reference.tick,liquidity:c.reference.liquidity});
    if (c.reference.partial || c.reference.amount_out === '0' || c.reference.swap_fee >= 1000000) assert.equal(c.expected.reverted, true);
  }
  for (const t of core.ticks) {
    const compressed = t.tick / core.tick_spacing;
    const word = core.tick_words.find(w => w.word_pos === Math.floor(compressed / 256));
    assert.ok(word && (BigInt(word.bitmap) & (1n << BigInt((compressed % 256 + 256) % 256))));
  }
}
console.log(JSON.stringify({fixture_cases: fixture.cases.length, allocation_sequences: fixture.reference.allocation_sequences.length, mirrored: true, schema_and_coverage: 'passed'}));
