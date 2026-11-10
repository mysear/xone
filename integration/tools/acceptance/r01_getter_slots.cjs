'use strict';
const path = require('node:path');
const assert = require('node:assert/strict');
const root = path.resolve(__dirname, '../../..');
const {ethers} = require(path.join(root, 'integration/.runtime/stable-reference/node_modules/ethers'));

// The original Uniswap V3 Pool storage layout: ticks at slot 5 (four
// consecutive words per Tick.Info), tickBitmap at slot 6, slot0 and liquidity.
// These are acquisition candidates. Only the original VM and MPT verifier
// establish which values the normal reader may consume.
function slotsForWord(tick, spacing, bitmap) {
    assert(Number.isSafeInteger(tick) && tick >= -887272 && tick <= 887272, 'GETTER_TICK');
    assert(Number.isSafeInteger(spacing) && spacing > 0 && spacing <= 32767, 'GETTER_SPACING');
    assert(typeof bitmap === 'string' && /^0x[0-9a-fA-F]{1,64}$/.test(bitmap), 'GETTER_BITMAP');
    // Solidity compresses negative nonmultiples down before the arithmetic shift.
    const bits = BigInt(bitmap), word = Math.floor(Math.floor(tick / spacing) / 256);
    const slot = n => '0x' + BigInt.asUintN(256, n).toString(16).padStart(64, '0');
    const mapping = (type, key, base) => BigInt(ethers.utils.keccak256(
        ethers.utils.defaultAbiCoder.encode([type, 'uint256'], [key, base])));
    const keys = [slot(0n), slot(4n), slot(mapping('int16', word, 6))], ticks = [];
    for (let bit = 0; bit < 256; bit++) {
        if (!(bits >> BigInt(bit) & 1n)) continue;
        const initialized = (word * 256 + bit) * spacing;
        assert(initialized >= -887272 && initialized <= 887272, 'GETTER_INITIALIZED_TICK');
        ticks.push(initialized);
        const first = mapping('int24', initialized, 5);
        for (let offset = 0n; offset < 4n; offset++) keys.push(slot(first + offset));
    }
    assert(keys.length <= 1027 && new Set(keys).size === keys.length, 'GETTER_SLOT_BUDGET');
    return {word, ticks, keys};
}
module.exports = {slotsForWord};
if (require.main === module) {
    let input = '';
    process.stdin.setEncoding('utf8');
    process.stdin.on('data', text => { input += text; assert(input.length <= 4096, 'GETTER_PLAN_INPUT_BUDGET'); });
    process.stdin.on('end', () => {
        const {tick, spacing, bitmap} = JSON.parse(input);
        console.log(JSON.stringify(slotsForWord(tick, spacing, bitmap)));
    });
}
