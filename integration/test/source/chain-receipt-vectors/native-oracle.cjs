// Independent native header/body/SAE/state oracle. Uses only the pinned JS
// cryptographic/RLP/MPT packages, never imports Go or the parser implementation.
const fs=require('fs'),path=require('path');
const runtime=path.resolve(__dirname,'../../../.runtime/receipt-root-reference/node_modules');
const {RLP}=require(path.join(runtime,'@ethereumjs/rlp'));
const {Trie}=require(path.join(runtime,'@ethereumjs/trie'));
const {keccak256}=require(path.join(runtime,'ethereum-cryptography/keccak'));
const raw=s=>Buffer.from(s.slice(2),'hex'),hex=b=>'0x'+Buffer.from(b).toString('hex');
function int(v){let s=BigInt(v).toString(16);return s==='0'?Buffer.alloc(0):Buffer.from(s.length%2?'0'+s:s,'hex')}
function headerHash(b,key){
 const h=[raw(b.parentHash),raw(b.sha3Uncles),raw(b.miner),raw(b.stateRoot),raw(b.transactionsRoot),raw(b.receiptsRoot),raw(b.logsBloom),int(b.difficulty),int(b.number),int(b.gasLimit),int(b.gasUsed),int(b.timestamp),raw(b.extraData),raw(b.mixHash),raw(b.nonce)];
 let optional;
 if(key==='evm:43114'){
  h.push(raw(b.extDataHash));optional=[['baseFeePerGas',1],['extDataGasUsed',1],['blockGasCost',1],['blobGasUsed',1],['excessBlobGas',1],['parentBeaconBlockRoot',0],['timestampMilliseconds',1],['minDelayExcess',1],['targetExponent',1],['minPriceExponent',1],['settledHeight',1],['settledGasUnix',1],['settledGasNumerator',1],['settledExcess',1]];
 }else optional=[['baseFeePerGas',1],['withdrawalsRoot',0],['blobGasUsed',1],['excessBlobGas',1],['parentBeaconBlockRoot',0],['requestsHash',0]];
 let last=-1;for(let i=0;i<optional.length;i++)if(b[optional[i][0]]!==undefined&&b[optional[i][0]]!==null)last=i;
 for(let i=0;i<=last;i++){const [name,isInt]=optional[i],v=b[name];h.push(v===undefined||v===null?Buffer.alloc(0):isInt?int(v):raw(v))}
 return hex(keccak256(RLP.encode(h)));
}
function signedJSON(tx){
 const kind=Number(BigInt(tx.type||'0x0'));let fields;
 switch(kind){
 case 0:fields=[int(tx.nonce),int(tx.gasPrice),int(tx.gas),tx.to?raw(tx.to):Buffer.alloc(0),int(tx.value),raw(tx.input),int(tx.v),int(tx.r),int(tx.s)];break;
 case 1:fields=[int(tx.chainId),int(tx.nonce),int(tx.gasPrice),int(tx.gas),tx.to?raw(tx.to):Buffer.alloc(0),int(tx.value),raw(tx.input),tx.accessList.map(x=>[raw(x.address),x.storageKeys.map(raw)]),int(tx.yParity||tx.v),int(tx.r),int(tx.s)];break;
 case 2:fields=[int(tx.chainId),int(tx.nonce),int(tx.maxPriorityFeePerGas),int(tx.maxFeePerGas),int(tx.gas),tx.to?raw(tx.to):Buffer.alloc(0),int(tx.value),raw(tx.input),tx.accessList.map(x=>[raw(x.address),x.storageKeys.map(raw)]),int(tx.yParity||tx.v),int(tx.r),int(tx.s)];break;
 case 0x6a:fields=[int(tx.chainId),raw(tx.input)];break;
 case 0x7e:fields=[raw(tx.sourceHash),raw(tx.from),tx.to?raw(tx.to):Buffer.alloc(0),int(tx.mint||'0x0'),int(tx.value),int(tx.gas),int(tx.isSystemTx?1:0),raw(tx.input)];break;
 default:throw Error('Oracle native type not enumerated: '+tx.type);
 }
 const payload=RLP.encode(fields);return kind===0?payload:Buffer.concat([Buffer.from([kind]),payload]);
}
async function txRoot(txs){const trie=new Trie({useKeyHashing:false});let identities=true;for(let i=0;i<txs.length;i++){const value=signedJSON(txs[i]);identities&&=hex(keccak256(value))===txs[i].hash;await trie.put(RLP.encode(int(i)),value)}return {root:hex(trie.root()),identities}}
async function receiptsRoot(window){const trie=new Trie({useKeyHashing:false});let index=0;for(const block of window)for(const r of [...block.receipts].sort((a,b)=>Number(BigInt(a.transactionIndex)-BigInt(b.transactionIndex)))){
 const fields=[r.root?raw(r.root):int(r.status),int(r.cumulativeGasUsed),raw(r.logsBloom),r.logs.map(l=>[raw(l.address),l.topics.map(raw),raw(l.data)])];
 if(r.type==='0x7e'&&r.depositReceiptVersion!==undefined)fields.push(int(r.depositNonce),int(r.depositReceiptVersion));
 const value=RLP.encode(fields),kind=Number(BigInt(r.type||'0x0'));await trie.put(RLP.encode(int(index++)),kind===0?value:Buffer.concat([Buffer.from([kind]),value]));
 }return {root:hex(trie.root()),receipts:index}}
function read(file){return JSON.parse(fs.readFileSync(path.join(__dirname,file)))}
async function main(){
 const headers=[],bodies=[],settlements=[],accounts=[];
 for(const name of ['arbitrum','avalanche','avalanche-current','polygon-synthetic']){
  const v=read(name+'/vector.json');const computed=headerHash(v.block,v.chain_key);headers.push({name,expected:v.block.hash,computed,match:computed===v.block.hash});
  const full=read(name+'/full-block.json').response.result;let txs=full.transactions;
  if(name==='polygon-synthetic')txs=txs.slice(0,-1);
  const result=await txRoot(txs);bodies.push({name,transactions:txs.length,tx_identities_match:result.identities,expected_root:full.transactionsRoot,computed_root:result.root,match:result.root===full.transactionsRoot});
 }
 for(const file of ['settlement.json','settlement-multi.json','settlement-fresh.json']){
  const s=read('avalanche-current/'+file),end=s.headers.at(-1),parent=s.headers.at(-2),startHeight=BigInt(parent.settledHeight)+1n,endHeight=BigInt(end.settledHeight);
  const root=await receiptsRoot(s.window);let chain=true;for(let i=0;i<s.headers.length;i++){const b=s.headers[i];chain&&=headerHash(b,'evm:43114')===b.hash;if(i)chain&&=s.headers[i-1].hash===b.parentHash&&BigInt(b.number)===BigInt(s.headers[i-1].number)+1n}
  settlements.push({file,target_height:s.target_height,settling_height:Number(BigInt(end.number)),window_start:Number(startHeight),window_end:Number(endHeight),receipts:root.receipts,header_chain_match:chain,expected_root:end.receiptsRoot,computed_root:root.root,match:root.root===end.receiptsRoot,state_height:Number(endHeight),state_root:end.stateRoot});
  if(s.state_proof){
   const p=s.state_proof,proof=p.accountProof.map(raw),key=keccak256(raw(p.address));
   const verified=await new Trie({useKeyHashing:false}).verifyProof(raw(end.stateRoot),key,proof),account=RLP.decode(verified);
   const matches=BigInt('0x'+(hex(account[0]).slice(2)||'0'))===BigInt(p.nonce)&&BigInt('0x'+(hex(account[1]).slice(2)||'0'))===BigInt(p.balance)&&hex(account[2])===p.storageHash&&hex(account[3])===p.codeHash&&account.length===5;
   accounts.push({file,address:p.address,state_height:Number(endHeight),state_root:end.stateRoot,proof_nodes:proof.length,account_fields:account.length,held_xchain_asset:hex(account[4]),match:matches});
  }
 }
 const result={reference:'independent @ethereumjs/trie@6.2.1 + RLP@5.0.2 + keccak',headers,bodies,settlements,accounts};
 fs.writeFileSync(path.join(__dirname,'native-oracle-results.json'),JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify(result,null,2));
 if([...headers,...bodies,...settlements,...accounts].some(x=>!x.match)||settlements.some(x=>!x.header_chain_match)||bodies.some(x=>!x.tx_identities_match))process.exitCode=1;
}
main().catch(e=>{console.error(e);process.exitCode=1});
