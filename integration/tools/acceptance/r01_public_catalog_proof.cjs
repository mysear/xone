#!/usr/bin/env node
'use strict';
// Verify all public prestate: contracts, EOAs, absent accounts and storage zeroes.
// The existing Go verifier independently checks the Ethereum header/transaction
// root and nonempty contract accounts. This verifier adds the EOA/absence boundary.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
const ROOT=path.resolve(__dirname,'../../..');
const {ethers}=require(path.join(ROOT,'integration/.runtime/stable-reference/node_modules/ethers'));
const {Trie}=require(path.join(ROOT,'integration/.runtime/receipt-root-reference/node_modules/@ethereumjs/trie'));
const bytes=h=>Buffer.from(h.replace(/^0x/,''),'hex'),hex=b=>'0x'+Buffer.from(b).toString('hex'),big=x=>x==='0x'||x===undefined?0n:BigInt(x);
function storageTrieKey(key,encoding='ethereum-storage/v1'){
 assert(typeof key==='string'&&/^0x[0-9a-fA-F]{1,64}$/.test(key),'STORAGE_KEY_UINT256');
 assert(['ethereum-storage/v1','coreth-partitioned-storage/v1'].includes(encoding),'STORAGE_KEY_ENCODING_UNSUPPORTED');
 const padded='0x'+key.slice(2).padStart(64,'0').toLowerCase();
 if(encoding==='ethereum-storage/v1')return padded;
 return '0x'+(parseInt(padded.slice(2,4),16)&0xfe).toString(16).padStart(2,'0')+padded.slice(4);
}
async function verifyPrestate(evidence,records=[],options={}){
 assert.equal(evidence.chain_id,'1','ETHEREUM_PRESTATE_CHAIN');
 assert(options.accountEncoding===undefined||['ethereum-account-rlp/v1','coreth-account-held-xchain-rlp/v1'].includes(options.accountEncoding),'ACCOUNT_ENCODING_UNSUPPORTED');
 assert(options.storageKeyEncoding===undefined||['ethereum-storage/v1','coreth-partitioned-storage/v1'].includes(options.storageKeyEncoding),'STORAGE_KEY_ENCODING_UNSUPPORTED');
 assert(options.storageKeyEncoding!=='coreth-partitioned-storage/v1'||options.accountEncoding==='coreth-account-held-xchain-rlp/v1','STORAGE_KEY_NATIVE_ACCOUNT_PROFILE');
 const stateRoot=evidence.block.stateRoot,trie=new Trie(),accounts=new Map();let slots=0,absent=0;const nativeFields={};
 for(const deployment of evidence.deployments){
  const address=deployment.address.toLowerCase(),p=deployment.proof;for(const field of ['balance','nonce'])assert(typeof p[field]==='string'&&/^0x(?:0|[1-9a-f][0-9a-f]*)$/.test(p[field]),'REQUIRED_ACCOUNT_QUANTITY:'+field);for(const field of ['codeHash','storageHash'])assert(typeof p[field]==='string'&&/^0x[0-9a-f]{64}$/.test(p[field]),'REQUIRED_ACCOUNT_HASH:'+field);assert(!accounts.has(address),'DUPLICATE_PRESTATE_ACCOUNT');assert.equal(p.address.toLowerCase(),address,'PRESTATE_PROOF_ADDRESS');
  const value=await trie.verifyProof(bytes(stateRoot),bytes(ethers.utils.keccak256(address)),p.accountProof.map(bytes));
  const actual=value?ethers.utils.RLP.decode(hex(value)):null;
  if(!actual){
   absent++;assert.equal(big(p.nonce),0n,'ABSENT_ACCOUNT_NONCE');assert.equal(big(p.balance),0n,'ABSENT_ACCOUNT_BALANCE');assert.equal(deployment.code,'0x','ABSENT_ACCOUNT_CODE');
   const zero='0x'+'00'.repeat(32),sentinel=options.allowAbsentRPCZeroHashes===true;assert(p.codeHash===ethers.utils.keccak256('0x')||(sentinel&&p.codeHash===zero),'ABSENT_CODE_HASH');assert(p.storageHash===ethers.utils.keccak256('0x80')||(sentinel&&p.storageHash===zero),'ABSENT_STORAGE_ROOT');
  }else{
   const coreth=options.accountEncoding==='coreth-account-held-xchain-rlp/v1';assert.equal(actual.length,coreth?5:4,'ACCOUNT_RLP_FIELDS');if(coreth){assert(['0x','0x01'].includes(actual[4]),'CORETH_ACCOUNT_BOOL_ENCODING');nativeFields[address]={held_x_chain_asset:actual[4]==='0x01'};}assert.equal(big(actual[0]),big(p.nonce),'PRESTATE_NONCE');assert.equal(big(actual[1]),big(p.balance),'PRESTATE_BALANCE');
   assert.equal(actual[2],p.storageHash,'PRESTATE_STORAGE_ROOT');assert.equal(actual[3],p.codeHash,'PRESTATE_CODE_HASH');assert.equal(ethers.utils.keccak256(deployment.code),p.codeHash,'PRESTATE_OBSERVED_CODE');
  }
  const storage=new Map();
  for(const slot of p.storageProof){
   assert(typeof slot.key==='string'&&/^0x[0-9a-fA-F]{1,64}$/.test(slot.key),'STORAGE_KEY_UINT256');assert(typeof slot.value==='string'&&/^0x(?:0|[1-9a-f][0-9a-f]*)$/.test(slot.value),'REQUIRED_STORAGE_QUANTITY');
   const key='0x'+slot.key.replace(/^0x/,'').padStart(64,'0').toLowerCase();assert(!storage.has(key),'DUPLICATE_PRESTATE_SLOT');
   const storageRoot=actual?p.storageHash:ethers.utils.keccak256('0x80');
   const empty=storageRoot===ethers.utils.keccak256('0x80')&&slot.proof.length===0;
   const v=empty?null:await trie.verifyProof(bytes(storageRoot),bytes(ethers.utils.keccak256(key)),slot.proof.map(bytes));
   const decoded=v?ethers.utils.RLP.decode(hex(v)):'0x';assert.equal(big(decoded),big(slot.value),'PRESTATE_STORAGE_VALUE');storage.set(key,big(slot.value));slots++;
  }
  accounts.set(address,{deployment,storage,emptyStorage:!actual||p.storageHash===ethers.utils.keccak256('0x80')});
 }
 let bound=0;
 for(const r of records){
  if(!r.response||!('result' in r.response))continue;
  const method=r.method,p=r.params,account=accounts.get((p[0]||'').toString().toLowerCase());
  if(['eth_getCode','eth_getBalance','eth_getTransactionCount','eth_getStorageAt'].includes(method)){
   assert(account,'PUBLIC_READ_ACCOUNT_NOT_PROVEN '+p[0]);const value=r.response.result,proof=account.deployment.proof;
   if(method==='eth_getCode')assert.equal(value.toLowerCase(),account.deployment.code.toLowerCase(),'PUBLIC_CODE_PROOF_DIFFERENCE');
   if(method==='eth_getBalance')assert.equal(big(value),big(proof.balance),'PUBLIC_BALANCE_PROOF_DIFFERENCE');
   if(method==='eth_getTransactionCount')assert.equal(big(value),big(proof.nonce),'PUBLIC_NONCE_PROOF_DIFFERENCE');
   if(method==='eth_getStorageAt'){const key=storageTrieKey(p[1],options.storageKeyEncoding);if(account.storage.has(key))assert.equal(big(value),account.storage.get(key),'PUBLIC_STORAGE_PROOF_DIFFERENCE');else{assert(account.emptyStorage,'PUBLIC_STORAGE_READ_NOT_PROVEN');assert.equal(big(value),0n,'PROVEN_EMPTY_STORAGE_NONZERO_READ');}}
   bound++;
  }
  assert(method!=='eth_call','PUBLIC_CALL_RESULT_HAS_NO_DIRECT_MPT_AUTHORITY');
 }
 return {...(options.accountEncoding==='coreth-account-held-xchain-rlp/v1'?{native_account_fields:nativeFields,account_encoding:options.accountEncoding}:{}),storage_key_encoding:options.storageKeyEncoding||'ethereum-storage/v1',passed:true,chain_id:'1',height:Number(BigInt(evidence.block.number)),block_hash:evidence.expected_block_hash,state_root:stateRoot,accounts:accounts.size,absent_accounts:absent,storage_slots:slots,public_prestate_reads_bound:bound,authority:'MPT-account-and-storage-proofs-against-explicit-Ethereum-stateRoot; header and tx-root verified independently by Go',consensus_verified:false};
}
module.exports={verifyPrestate,storageTrieKey};
if(require.main===module){const [evidenceFile,indexFile,out]=process.argv.slice(2),e=JSON.parse(fs.readFileSync(evidenceFile)),index=JSON.parse(fs.readFileSync(indexFile));const records=index.records.map(ref=>{const b=fs.readFileSync(path.join(ROOT,ref.path));assert.equal(require('node:crypto').createHash('sha256').update(b).digest('hex'),ref.sha256,'PRESTATE_TRANSCRIPT_HASH');return JSON.parse(b)});verifyPrestate(e,records).then(result=>{fs.writeFileSync(out,JSON.stringify(result,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify(result))}).catch(error=>{console.error(error.stack);process.exit(1)});}
