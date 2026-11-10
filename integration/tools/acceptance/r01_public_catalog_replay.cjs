#!/usr/bin/env node
'use strict';
// Re-run original published bytecode in an owned in-process node. No public
// transaction is submitted. The local bank is deliberately not historical state.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict'),cp=require('node:child_process');
const ROOT=path.resolve(__dirname,'../../..');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
const json=x=>JSON.stringify(x,null,2)+'\n';
function blob(root,ref){
 assert(ref&&typeof ref.path==='string','BLOB_REF');const file=path.resolve(root,ref.path);
 assert(!path.isAbsolute(ref.path)&&!ref.path.split('/').includes('..')&&file.startsWith(root+path.sep),'BLOB_PATH');
 const bytes=fs.readFileSync(file);assert.equal(sha(bytes),ref.sha256,'BLOB_HASH '+ref.path);return {file,bytes};
}
function validateExecution(fixture,runtime,checkEnvironment=true){
 assert(fixture.authority==='original-local-node-artifact-replay-not-live','EXECUTION_AUTHORITY');
 const validate=require('../reference/evm-execution/validate.cjs').validate;
 if(checkEnvironment)validate(fixture);
 assert.deepEqual(fixture.cases.map(c=>c.name),require('../reference/evm-execution/generate.cjs').CASES.map(c=>c.name),'EXECUTION_EXACT_CASE_INVENTORY');
 assert.equal(fixture.cases.length,10,'EXECUTION_INVENTORY');
 const identities=fixture.reference.runtime_identities.filter(i=>i.artifact==='packages/core/build/UniswapV2Pair.json');
 assert(identities.length>=1&&identities.every(i=>i.runtime_code_sha256===runtime),'EXECUTION_DEPLOYED_RUNTIME');
 const cases=fixture.cases.map(c=>{
  assert.equal(c.pool.chain_key,'evm:31337','EXECUTION_LOCAL_CHAIN');
  assert.equal(c.pool.v2.code_sha256,runtime,'EXECUTION_PAIR_RUNTIME');
  const pair=c.accounts.find(a=>a.address===c.pool.pool_id);assert(pair,'EXECUTION_PAIR_ACCOUNT');
  assert.equal(sha(Buffer.from(pair.code.slice(2),'hex')),runtime,'EXECUTION_ACCOUNT_CODE');
  const delta=BigInt(c.expected.recipient_buy_after)-BigInt(c.expected.recipient_buy_before);
  assert.equal(delta.toString(),c.expected.amount_out,'EXECUTION_RECIPIENT_DELTA');
  assert.equal(c.evidence.receipt.transactionHash,c.expected.tx_hash,'EXECUTION_RECEIPT_HASH');
  assert.equal(c.evidence.transaction.hash,c.expected.tx_hash,'EXECUTION_TRANSACTION_HASH');
  assert.equal(BigInt(c.evidence.receipt.status),c.expected.status==='success'?1n:0n,'EXECUTION_RECEIPT_STATUS');
  assert(c.evidence.signed_transaction.startsWith('0x'),'EXECUTION_SIGNED_ENVELOPE');
  assert.equal(c.evidence.execution_scope,'observed-local-next-block-environment-not-live-canonical','EXECUTION_SCOPE');
  return {name:c.name,status:c.expected.status,recipient_before:c.expected.recipient_buy_before,
   recipient_after:c.expected.recipient_buy_after,recipient_delta:delta.toString(),input_consumed:c.expected.sender_input_consumed,
   transaction_hash:c.expected.tx_hash,gas_used:c.expected.gas_used};
 });
 assert(cases.some(c=>c.status==='success'&&BigInt(c.recipient_delta)>0n),'EXECUTION_SUCCESS_MISSING');
 assert(cases.some(c=>c.status==='revert'&&c.recipient_delta==='0'),'EXECUTION_REVERT_MISSING');
 return {scope:'original-runtime-local-bank-not-original-chain-history',chain_key:'evm:31337',original_chain_historical_execution:false,
   runtime_sha256:runtime,case_count:cases.length,success_count:cases.filter(c=>c.status==='success').length,
   revert_count:cases.filter(c=>c.status==='revert').length,cases};
}
async function replay(root,row,mode,outdir){
 root=path.resolve(root);assert.equal(root,ROOT,'REPLAY_WORKSPACE');
 assert.equal(row.chain_key,'evm:1','REPLAY_CHAIN');
 const inputs=row.rebuild.inputs;
 const evidence=blob(root,inputs.evidence),artifact=blob(root,inputs.artifact),compiler=blob(root,inputs.compiler),pin=JSON.parse(blob(root,inputs.compiler_pin).bytes);
 assert.equal(sha(compiler.bytes),pin.soljson_sha256,'COMPILER_PIN');
 const expected=JSON.parse(artifact.bytes),metadata=JSON.parse(expected.metadata);
 assert.equal(metadata.compiler.version,'0.5.16+commit.9c3226ce','COMPILER_BUILD');
 assert.deepEqual(Object.keys(inputs.source_files).sort(),Object.keys(metadata.sources).sort(),'COMPILER_SOURCE_INVENTORY');
 for(const ref of Object.values(inputs.source_files))blob(root,ref);
 const ev=JSON.parse(evidence.bytes);assert.equal(ev.chain_id,'1','PROOF_CHAIN');assert.equal(ev.expected_block_hash,row.anchor.hash,'PROOF_HASH');
 assert.equal(BigInt(ev.block.number),BigInt(row.anchor.height),'PROOF_HEIGHT');assert.equal(ev.deployments.length,1,'PROOF_SCOPE');
 assert.equal(ev.deployments[0].address.toLowerCase(),row.address.toLowerCase(),'PROOF_ADDRESS');
 assert.equal(sha(Buffer.from(ev.deployments[0].code.slice(2),'hex')),row.runtime.sha256,'PROOF_RUNTIME');
 const verifierFile=path.join(root,'chain-parser-go/bin/source-evidence');
 const proof=JSON.parse(cp.execFileSync(verifierFile,['--verify',evidence.file],{encoding:'utf8',timeout:30000,maxBuffer:1024*1024}));
 assert.equal(proof.passed,true,'PROOF_VERIFICATION');
 const wrapperFile=path.join(root,'integration/.runtime/stable-reference/node_modules/solc/wrapper.js');
 const compilerModule=require(wrapperFile)(require(compiler.file));
 assert.equal(compilerModule.version(),metadata.compiler.version+'.Emscripten.clang','COMPILER_BUILD');
 const sources={},sourceFiles={};
 for(const [name,ref] of Object.entries(inputs.source_files)){
  const source=blob(root,ref);assert(source.bytes.length<=1024*1024,'SOURCE_LIMIT');
  sources[name]={content:source.bytes.toString('utf8')};sourceFiles[name]=ref.sha256;
 }
 const settings={...metadata.settings,outputSelection:{'*':{'*':['evm.deployedBytecode.object','metadata']}}};delete settings.compilationTarget;
 const compiled=JSON.parse(compilerModule.compile(JSON.stringify({language:'Solidity',sources,settings})));
 assert(!(compiled.errors||[]).some(e=>e.severity==='error'),JSON.stringify(compiled.errors));
 const targets=Object.entries(metadata.settings.compilationTarget);assert.equal(targets.length,1,'COMPILER_TARGET');
 const [targetFile,targetName]=targets[0];
 require('./evm_deployment_rebuild.cjs').verifyRebuild(metadata,compiled.contracts[targetFile][targetName],ev.deployments[0].code);
 const build={schema_version:1,passed:true,chain_id:ev.chain_id,block_hash:ev.expected_block_hash,address:ev.deployments[0].address,
  artifact_sha256:inputs.artifact.sha256,evidence_sha256:inputs.evidence.sha256,provenance_verifier_sha256:sha(fs.readFileSync(verifierFile)),
  provenance_verification:proof,source_files:sourceFiles,compiler:compilerModule.version(),compiler_sha256:sha(compiler.bytes),
  wrapper_sha256:sha(fs.readFileSync(wrapperFile)),rebuilt_runtime_sha256:row.runtime.sha256,source_rebuild_verified:true,
  consensus_verified:false,independent_execution_and_receipt_verified:false,all_requested_versions_verified:false,production_ready:false};
 const generator=require('../reference/evm-execution/generate.cjs');
 const fixture=await generator.generate();const execution=validateExecution(fixture,row.runtime.sha256);
 const result={schema_version:1,passed:true,chain_key:row.chain_key,address:row.address.toLowerCase(),height:row.anchor.height,
  block_hash:row.anchor.hash,runtime_sha256:row.runtime.sha256,source_rebuild_verified:true,consensus_verified:false,
  execution_scope:execution.scope,original_chain_historical_execution:false,execution};
 if(mode==='generate'){
  assert(outdir&&path.resolve(outdir).startsWith(path.join(root,'integration/test/source/deployment-catalog')+path.sep),'OWNED_OUTPUT');
  fs.mkdirSync(outdir,{recursive:true});
  for(const [name,data] of Object.entries({'proof-report.json':proof,'rebuild-report.json':build,'execution-report.json':execution,'execution-fixture.json':fixture,'replay-report.json':result}))fs.writeFileSync(path.join(outdir,name),json(data),{flag:'wx'});
 }else{
  const oldProof=JSON.parse(blob(root,row.proof.report).bytes);assert.deepEqual(oldProof,proof,'PROOF_REPORT');
  const oldBuild=JSON.parse(blob(root,row.rebuild.report).bytes);
  for(const key of ['chain_id','block_hash','address','artifact_sha256','evidence_sha256','source_files','compiler','compiler_sha256','rebuilt_runtime_sha256','source_rebuild_verified'])assert.deepEqual(oldBuild[key],build[key],'REBUILD_REPORT '+key);
  const oldFixture=JSON.parse(blob(root,row.execution.fixture).bytes);const oldExecution=validateExecution(oldFixture,row.runtime.sha256,false);
  // Retained host executable paths are historical metadata. Compare the complete
  // deterministic bank cases rather than requiring an identical installation path.
  assert.deepEqual(oldFixture.cases,fixture.cases,'FULL_CASE_REPLAY_RESULT');
  for(const key of ['chain_options','genesis_header','setup_transactions','runtime_identities','pair_init_code_keccak256'])assert.deepEqual(oldFixture.reference[key],fixture.reference[key],'BANK_REPLAY '+key);
  assert.deepEqual(JSON.parse(blob(root,row.execution.report).bytes),oldExecution,'EXECUTION_REPORT');
  assert.deepEqual(oldExecution,execution,'EXECUTION_REPLAY_RESULT');
 }
 return result;
}
async function main(){const [mode,root,rowjson,outdir]=process.argv.slice(2);assert(['generate','verify'].includes(mode),'MODE');const result=await replay(root,JSON.parse(rowjson),mode,outdir);console.log(JSON.stringify(result));}
module.exports={validateExecution,replay};
if(require.main===module)main().catch(e=>{console.error(e.stack);process.exit(1);});
