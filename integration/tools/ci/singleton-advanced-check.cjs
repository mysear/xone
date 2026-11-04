'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const root=path.resolve(__dirname,'../../..'),ref=path.join(root,'integration/tools/reference/singleton-advanced');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const pins=JSON.parse(fs.readFileSync(path.join(ref,'source-metadata.json')));
const manifest=fs.readFileSync(path.join(ref,'../singleton-cl/source-metadata.json'));
assert.equal(sha(manifest),pins.upstream_manifest_sha256);
for(const f of pins.harnesses)assert.equal(sha(fs.readFileSync(path.join(ref,f.name))),f.sha256,f.name);
for(const [file,hash,bases]of [
 ['singleton-advanced-quotes.json',pins.fixture_sha256,['integration','dex-router','chain-parser-go','chain-parser-rust']],
 ['singleton-advanced-bin-quotes.json',pins.bin_fixture_sha256,['integration','dex-router']]]){
 const bytes=fs.readFileSync(path.join(root,'integration/test/contracts',file));assert.equal(sha(bytes),hash,file);
 for(const base of bases)assert.deepEqual(bytes,fs.readFileSync(path.join(root,base,'test/contracts',file)));
 const fixture=JSON.parse(bytes),cases=fixture.cases;
 assert.equal(new Set(cases.map(c=>c.name)).size,cases.length);
 if(file==='singleton-advanced-quotes.json'){
  assert.equal(cases.length,240);assert.equal(fixture.reference.capabilities.length,8);
  for(const protocol of ['uniswap-v4','pancake-infinity-cl']){
   const group=cases.filter(c=>c.snapshot.protocol===protocol);
   assert.ok(group.some(c=>c.snapshot.key_fee===0x800000&&c.snapshot.snapshot.lp_fee===0));
   assert.ok(group.some(c=>c.snapshot.currency0==='0x'+'00'.repeat(20)));
   assert.ok(group.some(c=>c.expected.tick>=60));assert.ok(group.some(c=>c.expected.tick<=-60));
   assert.ok(group.some(c=>c.expected.swap_fee===1000000&&c.expected.amount_out==='0'));
   for(const c of group){assert.equal(BigInt(c.expected.consumed_in)+BigInt(c.expected.remaining_in),BigInt(c.input));assert.equal(c.expected.remaining_in,'0')}
  }
 }else {assert.equal(cases.length,180);assert.ok(cases.some(c=>c.expected.reverted));assert.ok(cases.some(c=>!c.expected.reverted&&c.expected.protocol_amount!=='0'))}
}
assert.deepEqual(fs.readFileSync(path.join(root,'dex-router/crates/domain/src/singleton.rs')),fs.readFileSync(path.join(root,'chain-parser-rust/crates/domain/src/singleton.rs')));
console.log('advanced pins, frozen fixtures, coverage, mirrors: PASS (240 CL + 180 Bin + 8 hook capability rejections)');
