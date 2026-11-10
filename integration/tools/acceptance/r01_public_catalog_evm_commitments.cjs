#!/usr/bin/env node
'use strict';
// Secure account/storage tries are shared by these execution chains. Header
// encoding and the height whose state a header commits remain separate facts.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const ROOT=path.resolve(__dirname,'../../..'),{ethers}=require(path.join(ROOT,'integration/.runtime/stable-reference/node_modules/ethers')),{verifyPrestate}=require('./r01_public_catalog_proof.cjs');
const CHAINS={base:'8453',optimism:'10',bsc:'56',arbitrum:'42161',polygon:'137',avalanche:'43114'},sha=x=>crypto.createHash('sha256').update(x).digest('hex');
function quantity(x){assert(/^0x[0-9a-f]+$/.test(x),'HEADER_QUANTITY');const n=BigInt(x);assert.equal('0x'+n.toString(16),x,'HEADER_CANONICAL_QUANTITY');if(!n)return '0x';let s=n.toString(16);return '0x'+(s.length%2?'0':'')+s;}
function verifyHeader(chain,header){
 assert(CHAINS[chain],'HEADER_CHAIN_UNSUPPORTED');
 // This mirrors the existing Go native ordinary/Nitro header profile only.
 const names=['parentHash','sha3Uncles','miner','stateRoot','transactionsRoot','receiptsRoot','logsBloom','difficulty','number','gasLimit','gasUsed','timestamp','extraData','mixHash','nonce'];const ints=new Set(['difficulty','number','gasLimit','gasUsed','timestamp','baseFeePerGas','blobGasUsed','excessBlobGas']);
 let values=names.map(name=>{assert(header[name]!==undefined,'HEADER_FIELD:'+name);return ints.has(name)?quantity(header[name]):header[name];});
 const coreth=['baseFeePerGas','extDataGasUsed','blockGasCost','blobGasUsed','excessBlobGas','parentBeaconBlockRoot','timestampMilliseconds','minDelayExcess','targetExponent','minPriceExponent','settledHeight','settledGasUnix','settledGasNumerator','settledExcess'];
 const tail=chain==='avalanche'?coreth:['baseFeePerGas','withdrawalsRoot','blobGasUsed','excessBlobGas','parentBeaconBlockRoot','requestsHash'];let last=-1;for(let i=0;i<tail.length;i++)if(header[tail[i]]!==null&&header[tail[i]]!==undefined)last=i;
 if(chain==='avalanche'){assert(!header.withdrawalsRoot&&!header.requestsHash,'CORETH_FORBIDDEN_ETHEREUM_TAIL');assert(/^0x[0-9a-f]{64}$/.test(header.extDataHash),'CORETH_EXT_DATA_HASH');values.push(header.extDataHash);}
 for(let i=0;i<=last;i++){const name=tail[i],present=header[name]!==null&&header[name]!==undefined;if(chain!=='avalanche')assert(present,'HEADER_OPTIONAL_GAP:'+name);else{const time=BigInt(header.timestamp);if((time>=1763568000n&&i>=6&&i<=7)||(time>=1790089200n&&i>=8))assert(present,'CORETH_ACTIVATION_FIELD:'+name);if(name==='timestampMilliseconds'&&present)assert.equal(BigInt(header[name])/1000n,time,'CORETH_MILLISECOND_TIME');}values.push(!present?'0x':['withdrawalsRoot','parentBeaconBlockRoot','requestsHash'].includes(name)?header[name]:quantity(header[name]));}
 const encoded=ethers.utils.RLP.encode(values),hash=ethers.utils.keccak256(encoded);assert.equal(hash,header.hash,'CHAIN_NATIVE_HEADER_HASH');
 return {passed:true,chain,chain_id:CHAINS[chain],height:Number(BigInt(header.number)),hash,state_root:header.stateRoot,transactions_root:header.transactionsRoot,header_profile:chain==='avalanche'?'coreth-header-extdata-granite-sae-rlp/v1':chain==='arbitrum'?'arbitrum-nitro-header-rlp/v1':'ethereum-execution-header-rlp/v1',native_body_transactions_verified:false,consensus_verified:false,authority:'independent exact RLP header hash check; full native body/receipt replay remains a separate Go obligation'};
}
async function verifyBody(chain,header){
 const report=verifyHeader(chain,header),{Trie}=require(path.join(ROOT,'integration/.runtime/receipt-root-reference/node_modules/@ethereumjs/trie')),{serializeTransaction}=require('./r01_public_catalog_fork.cjs'),trie=new Trie(),encoded=[];let native=0;
 for(let i=0;i<header.transactions.length;i++){
  const tx=header.transactions[i],type=Number(BigInt(tx.type||'0x0'));assert.equal(tx.blockHash,header.hash,'NATIVE_TRANSACTION_BLOCK');assert.equal(tx.blockNumber,header.number,'NATIVE_TRANSACTION_HEIGHT');assert.equal(BigInt(tx.transactionIndex),BigInt(i),'NATIVE_TRANSACTION_ORDER');let bytes;
  if(type===0x7e){assert(['base','optimism'].includes(chain),'NATIVE_OP_CHAIN');bytes='0x7e'+ethers.utils.RLP.encode([tx.sourceHash,tx.from,tx.to||'0x',quantity(tx.mint||'0x0'),quantity(tx.value),quantity(tx.gas),tx.isSystemTx?'0x01':'0x',tx.input]).slice(2);native++;}
  else if(type===0x6a){assert.equal(chain,'arbitrum','NATIVE_NITRO_CHAIN');assert.equal(BigInt(tx.chainId),BigInt(CHAINS[chain]),'NITRO_TRANSACTION_CHAIN');bytes='0x6a'+ethers.utils.RLP.encode([quantity(tx.chainId),tx.input]).slice(2);native++;}
  else {if(type>0)assert.equal(BigInt(tx.chainId),BigInt(CHAINS[chain]),'NATIVE_TRANSACTION_CHAIN');else if(BigInt(tx.v)>=35n)assert.equal((BigInt(tx.v)-35n)/2n,BigInt(CHAINS[chain]),'LEGACY_TRANSACTION_CHAIN');bytes=serializeTransaction(tx);}
  assert.equal(ethers.utils.keccak256(bytes),tx.hash,'NATIVE_TRANSACTION_SIGNED_HASH');encoded.push(bytes);await trie.put(Buffer.from(ethers.utils.RLP.encode(i?quantity('0x'+i.toString(16)):'0x').slice(2),'hex'),Buffer.from(bytes.slice(2),'hex'));
 }
 assert.equal('0x'+Buffer.from(trie.root()).toString('hex'),header.transactionsRoot,'CHAIN_NATIVE_TRANSACTION_ROOT');
 return {...report,native_body_transactions_verified:true,transactions:encoded.length,native_transactions:native,raw_transaction_bytes_sha256:sha(JSON.stringify(encoded)),authority:'exact native deposit/Nitro or signed EVM transaction bytes -> individual hash -> ordered transaction MPT -> independently hashed native header; no original-block replay claim'};
}
function verifySAEStateAnchor(headers,height){
 assert(Array.isArray(headers)&&headers.length>=2&&headers.length<=64,'SAE_HEADER_WINDOW_BUDGET');let target,settling;const byHeight=new Map();
 for(const h of headers){verifyHeader('avalanche',h);const n=Number(BigInt(h.number));assert(!byHeight.has(n),'SAE_DUPLICATE_HEADER');byHeight.set(n,h);if(n===height)target=h;if(Number(BigInt(h.settledHeight))===height)settling=h;}
 assert(target&&settling,'SAE_EXACT_TARGET_SETTLEMENT_REQUIRED');for(let n=height+1;n<=Number(BigInt(settling.number));n++){const h=byHeight.get(n),parent=byHeight.get(n-1);assert(h&&parent&&h.parentHash===parent.hash,'SAE_NATIVE_PARENT_GAP');}
 return {height,block_hash:target.hash,state_root:settling.stateRoot,settling_height:Number(BigInt(settling.number)),settling_hash:settling.hash,account_encoding:'coreth-account-held-xchain-rlp/v1',storage_key_encoding:'coreth-partitioned-storage/v1',authority:'exact settledHeight and contiguous independently hashed Coreth native successor headers'};
}
async function verifyEvidence(evidence,records){
 assert.equal(evidence.chain_id,CHAINS[evidence.chain],'MPT_CHAIN_ID');assert.equal(evidence.block.hash,evidence.expected_block_hash,'MPT_HEADER_ANCHOR');assert.equal(evidence.missing_proofs.length,0,'PUBLIC_PRESTATE_PROOFS_INCOMPLETE');
 const header=await verifyBody(evidence.chain,evidence.block);
 const stateAnchor=evidence.chain==='avalanche'?verifySAEStateAnchor(evidence.native_headers,Number(BigInt(evidence.block.number))):null;
 const proof=await verifyPrestate({...evidence,chain_id:'1',block:stateAnchor?{...evidence.block,stateRoot:stateAnchor.state_root}:evidence.block},records,{allowAbsentRPCZeroHashes:true,accountEncoding:stateAnchor?.account_encoding,storageKeyEncoding:stateAnchor?'coreth-partitioned-storage/v1':'ethereum-storage/v1'}); // only the chain-independent secure MPT primitive is reused
 return {passed:true,execution_state_anchor:stateAnchor,chain:evidence.chain,chain_id:evidence.chain_id,header,accounts:proof.accounts,storage_slots:proof.storage_slots,absent_accounts:proof.absent_accounts,public_prestate_reads_bound:proof.public_prestate_reads_bound,state_root:proof.state_root,height:proof.height,native_body_transactions_verified:true,full_native_body_receipt_verified:false,authority:'secure MPT against independently hashed explicit native header root; not a generic Ethereum body/receipt verifier',consensus_verified:false};
}
module.exports={verifyHeader,verifyBody,verifySAEStateAnchor,verifyEvidence};
if(require.main===module){const dir=path.resolve(process.argv[2]),e=JSON.parse(fs.readFileSync(path.join(dir,process.argv[4]||'prestate-evidence.json'))),index=JSON.parse(fs.readFileSync(path.join(dir,'rpc-index.json')));const records=index.records.map(ref=>{const bytes=fs.readFileSync(path.join(ROOT,ref.path));assert.equal(sha(bytes),ref.sha256,'TRANSCRIPT_HASH');return JSON.parse(bytes)});verifyEvidence(e,records).then(result=>{if(process.argv[3])fs.writeFileSync(process.argv[3],JSON.stringify(result,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify(result,null,2))}).catch(error=>{console.error(error.stack);process.exit(1)});}
