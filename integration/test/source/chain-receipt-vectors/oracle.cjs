// Independent EthereumJS RLP/MPT oracle. Does not import the Go implementation.
// Encoding branches are transcribed from the fixed upstream pins in sources/.
const fs=require('fs'), path=require('path');
const runtime=path.resolve(__dirname,'../../../.runtime/receipt-root-reference/node_modules');
const {Trie}=require(path.join(runtime,'@ethereumjs/trie'));
const {RLP}=require(path.join(runtime,'@ethereumjs/rlp'));
const {keccak256}=require(path.join(runtime,'ethereum-cryptography/keccak'));
const raw=s=>Buffer.from(s.slice(2),'hex'), hex=b=>'0x'+Buffer.from(b).toString('hex');
function integer(v) {let s=BigInt(v).toString(16);return s==='0'?Buffer.alloc(0):Buffer.from(s.length%2?'0'+s:s,'hex')}
function bloom(logs) {const b=Buffer.alloc(256);for(const l of logs)for(const v of [l.address,...l.topics]){const d=keccak256(raw(v));for(let i=0;i<6;i+=2){const bit=((d[i]<<8)|d[i+1])&2047;b[255-(bit>>3)]|=1<<(bit&7)}}return hex(b)}
async function seal(key,receipts) {
 const trie=new Trie({useKeyHashing:false});const encodings=[];
 for(let i=0;i<receipts.length;i++) {
  const r=receipts[i], kind=Number(BigInt(r.type||'0x0'));
  const fields=[r.root?raw(r.root):integer(r.status),integer(r.cumulativeGasUsed),raw(r.logsBloom),r.logs.map(l=>[raw(l.address),l.topics.map(raw),raw(l.data)])];
  if((key==='evm:10'||key==='evm:8453')&&kind===126&&r.depositReceiptVersion!==undefined)fields.push(integer(r.depositNonce),integer(r.depositReceiptVersion));
  const payload=RLP.encode(fields);
  const encoded=kind===0||(key==='evm:42161'&&kind===120)?payload:Buffer.concat([Buffer.from([kind]),payload]);
  const index=r.transactionIndex===undefined?i:BigInt(r.transactionIndex);
  await trie.put(RLP.encode(integer(index)),encoded);encodings.push(hex(encoded));
 }
 return {root:hex(trie.root()),encodings};
}
async function main(){
 const cases=[];
 const receipt=(type,overrides={})=>({type,status:'0x1',cumulativeGasUsed:'0x5208',gasUsed:'0x5208',logs:[],logsBloom:bloom([]),...overrides});
 for(const kind of [0x64,0x65,0x66,0x68,0x69,0x6a,0x78])cases.push({name:'nitro-'+kind.toString(16),chain_key:'evm:42161',receipts:[receipt('0x'+kind.toString(16))]});
 for(const [name,fields]of Object.entries({'pre-regolith':{},'regolith-zero':{depositNonce:'0x0'},'regolith':{depositNonce:'0x23'},'canyon-zero':{depositNonce:'0x0',depositReceiptVersion:'0x1'},'canyon':{depositNonce:'0x23',depositReceiptVersion:'0x1'}}))cases.push({name:'op-'+name,chain_key:'evm:10',receipts:[receipt('0x7e',fields)]});
 for(const [key,kind]of [['evm:56','0x0'],['evm:42161','0x6a'],['evm:10','0x7e'],['evm:137','0x7f']])cases.push({name:'zero-gas-'+key,chain_key:key,receipts:[receipt(kind,{cumulativeGasUsed:'0x0',gasUsed:'0x0'}),receipt('0x2',{cumulativeGasUsed:'0x5208'})]});
 cases.push({name:'bsc-equal-cumulative',chain_key:'evm:56',receipts:[receipt('0x0'),receipt('0x0',{gasUsed:'0x0'})]});
 for(const c of cases)Object.assign(c,await seal(c.chain_key,c.receipts));
 fs.writeFileSync(path.join(__dirname,'encoding-vectors.json'),JSON.stringify({reference:'@ethereumjs/trie@6.2.1 + @ethereumjs/rlp@5.0.2',cases},null,2)+'\n');
 const results=[]; const bodies=[];
 for(const name of fs.readdirSync(__dirname).sort()){
  const file=path.join(__dirname,name,'vector.json');if(!fs.existsSync(file))continue;
  const v=JSON.parse(fs.readFileSync(file));const result=await seal(v.chain_key,v.receipts);
  const bodyFile=path.join(__dirname,name,'body.json');
  if(fs.existsSync(bodyFile) && ["bsc","optimism-regolith"].includes(name)) {
   const source=JSON.parse(fs.readFileSync(bodyFile));const txTrie=new Trie({useKeyHashing:false});let hashesMatch=true;
   const rawByHash=new Map(source.raw_transactions.map(record=>[record.request.params[0],record.response.result]));
   for(let i=0;i<v.block.transactions.length;i++){const hash=v.block.transactions[i],bytes=raw(rawByHash.get(hash));hashesMatch&&=hex(keccak256(bytes))===hash;await txTrie.put(RLP.encode(integer(i)),bytes)}
   const b=v.block;
   const header=[raw(b.parentHash),raw(b.sha3Uncles),raw(b.miner),raw(b.stateRoot),raw(b.transactionsRoot),raw(b.receiptsRoot),raw(b.logsBloom),integer(b.difficulty),integer(b.number),integer(b.gasLimit),integer(b.gasUsed),integer(b.timestamp),raw(b.extraData),raw(b.mixHash),raw(b.nonce)];
   for(const [field,isInt]of [['baseFeePerGas',true],['withdrawalsRoot',false],['blobGasUsed',true],['excessBlobGas',true],['parentBeaconBlockRoot',false],['requestsHash',false]])if(b[field]!==undefined&&b[field]!==null)header.push(isInt?integer(b[field]):raw(b[field]));
   const headerHash=hex(keccak256(RLP.encode(header))),txRoot=hex(txTrie.root());
   bodies.push({name,transactions:v.block.transactions.length,tx_hashes_match:hashesMatch,expected_transactions_root:b.transactionsRoot,computed_transactions_root:txRoot,transactions_root_match:txRoot===b.transactionsRoot,expected_header_hash:b.hash,computed_header_hash:headerHash,header_hash_match:headerHash===b.hash});
  }
  let bor;
  if(v.expected_verification_error==='RECEIPT_BOR_SYNTHETIC_UNCOMMITTED'){
   const height=Buffer.alloc(8);height.writeBigUInt64BE(BigInt(v.block.number));const synthetic=hex(keccak256(Buffer.concat([Buffer.from('matic-bor-receipt-'),height,raw(v.block.hash)])));
   const without=await seal(v.chain_key,v.receipts.slice(0,-1));bor={derived_synthetic_hash:synthetic,synthetic_identity_match:synthetic===v.block.transactions.at(-1)&&synthetic===v.receipts.at(-1).transactionHash,root_without_synthetic:without.root,root_excludes_synthetic:without.root===v.block.receiptsRoot};
  }
  results.push({bor,name,chain_key:v.chain_key,block_number:v.block.number,transactions:v.block.transactions.length,receipts:v.receipts.length,expected_root:v.block.receiptsRoot,computed_root:result.root,match:result.root===v.block.receiptsRoot,expected_match:v.expected_verification_error===undefined,expectation_met:(result.root===v.block.receiptsRoot)===(v.expected_verification_error===undefined)});
 }
 fs.writeFileSync(path.join(__dirname,'body-oracle-results.json'),JSON.stringify(bodies,null,2)+'\n');
 fs.writeFileSync(path.join(__dirname,'oracle-results.json'),JSON.stringify(results,null,2)+'\n');
 console.log(JSON.stringify({receipts:results,bodies},null,2));if(results.some(r=>!r.expectation_met||(r.bor&&(!r.bor.synthetic_identity_match||!r.bor.root_excludes_synthetic)))||bodies.some(r=>!r.tx_hashes_match||!r.transactions_root_match||!r.header_hash_match))process.exitCode=1;
}
main().catch(e=>{console.error(e);process.exitCode=1});
