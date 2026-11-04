// Independent EthereumJS MPT/RLP reference. Never imports the Go parser.
const path = require('path');
const fs = require('fs');
const runtime = path.resolve(__dirname, '../../../.runtime/receipt-root-reference/node_modules');
const { Trie } = require(path.join(runtime, '@ethereumjs/trie'));
const { RLP } = require(path.join(runtime, '@ethereumjs/rlp'));
const { keccak256 } = require(path.join(runtime, 'ethereum-cryptography/keccak'));
const raw = x => Buffer.from(x.slice(2), 'hex');
const hex = b => '0x' + Buffer.from(b).toString('hex');
function integer(x) {
  let s = BigInt(x).toString(16);
  if (s === '0') return Buffer.alloc(0);
  return Buffer.from(s.length % 2 ? '0' + s : s, 'hex');
}
function bloom(logs) {
  const b = Buffer.alloc(256);
  for (const l of logs) for (const value of [l.address, ...l.topics]) {
    const digest = keccak256(raw(value));
    for (let i=0;i<6;i+=2) {
      const bit = ((digest[i] << 8) | digest[i+1]) & 2047;
      b[255 - (bit >> 3)] |= 1 << (bit & 7);
    }
  }
  return b;
}
async function seal(receipts) {
  const trie = new Trie({ useKeyHashing: false });
  const normalized=[];
  for (let i=0;i<receipts.length;i++) {
    const r={type:'0x0', status:'0x1', cumulativeGasUsed:'0x'+((i+1)*21000).toString(16), ...receipts[i]};
    if (r.root) delete r.status;
    r.logsBloom=hex(bloom(r.logs));
    const logs=r.logs.map(l=>[raw(l.address),l.topics.map(raw),raw(l.data)]);
    const value=RLP.encode([r.root ? raw(r.root) : integer(r.status), integer(r.cumulativeGasUsed), raw(r.logsBloom), logs]);
    const type=Number(BigInt(r.type));
    await trie.put(RLP.encode(integer(i)), type===0 ? value : Buffer.concat([Buffer.from([type]),value]));
    normalized.push(r);
  }
  return {root:hex(trie.root()),receipts:normalized};
}
async function main() {
  const input=JSON.parse(fs.readFileSync(0,'utf8'));
  if (Array.isArray(input.cases)) {
    const cases=[];
    for (const c of input.cases) cases.push({...c,...await seal(c.receipts)});
    console.log(JSON.stringify({reference:'@ethereumjs/trie@6.2.1 + @ethereumjs/rlp@5.0.2',cases}));
  } else console.log(JSON.stringify(await seal(input.receipts)));
}
main().catch(e=>{console.error(e);process.exit(1)});
