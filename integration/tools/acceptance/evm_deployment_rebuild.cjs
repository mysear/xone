#!/usr/bin/env node
'use strict';
// A finite source/compiler rebuild for a retained, separately proven deployment.
// No application or quote kernel participates in the runtime expectation.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict'),child=require('node:child_process');
const ROOT=path.resolve(__dirname,'../../..');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
function verifyRebuild(expected,contract,runtime){
 assert(contract&&contract.evm&&contract.evm.deployedBytecode,'compiled runtime missing');
 assert.equal('0x'+contract.evm.deployedBytecode.object.toLowerCase(),runtime.toLowerCase(),'rebuilt runtime differs from proven deployed bytes');
 const actual=JSON.parse(contract.metadata);assert.deepEqual(actual.compiler,expected.compiler,'compiler identity');
 assert.deepEqual(actual.sources,expected.sources,'full source inventory and source keccak');
 assert.deepEqual(actual.settings,expected.settings,'complete compiler settings');return true;
}
function main(){
 assert.equal(process.argv.length,5,'usage: node evm_deployment_rebuild.cjs EVIDENCE ARTIFACT OUTPUT');
 const [inputFile,artifactFile,outputFile]=process.argv.slice(2).map(p=>path.resolve(p));
 const evidenceBytes=fs.readFileSync(inputFile),artifactBytes=fs.readFileSync(artifactFile);assert(evidenceBytes.length<=32*1024*1024&&artifactBytes.length<=8*1024*1024,'input limit');
 const evidence=JSON.parse(evidenceBytes),artifact=JSON.parse(artifactBytes),metadata=JSON.parse(artifact.metadata);
 assert.equal(evidence.schema_version,1);assert.equal(evidence.deployments.length,1,'explicit single deployment scope');
 const verifierFile=path.join(ROOT,'chain-parser-go/bin/source-evidence');
 const verified=JSON.parse(child.execFileSync(verifierFile,['--verify',inputFile],{encoding:'utf8',timeout:30000,maxBuffer:1024*1024}));
 assert(fs.readFileSync(inputFile).equals(evidenceBytes),'deployment evidence changed during verification');
 assert.equal(verified.passed,true);assert.equal(verified.block_hash,evidence.expected_block_hash);assert.equal(verified.chain_id,evidence.chain_id);
 assert.equal(verified.deployments.length,1);assert.equal(verified.deployments[0].address.toLowerCase(),evidence.deployments[0].address.toLowerCase());
 assert.equal(verified.deployments[0].runtime_sha256,sha(Buffer.from(evidence.deployments[0].code.slice(2),'hex')),'verified code identity');

 const sourceRoot=path.dirname(path.dirname(artifactFile)),sources={},sourceFiles={};
 for(const relative of Object.keys(metadata.sources).sort()){
  assert(!path.isAbsolute(relative)&&!relative.split('/').includes('..'),'source path escape');const file=path.join(sourceRoot,relative);assert(fs.statSync(file).isFile());
  const bytes=fs.readFileSync(file);assert(bytes.length<=1024*1024,'source limit');sources[relative]={content:bytes.toString('utf8')};sourceFiles[relative]=sha(bytes);
 }
 const compilerRoot=path.join(ROOT,'integration/.runtime/r01-solc'),compilerFile=path.join(compilerRoot,'soljson-0.5.16.js'),compilerPin=JSON.parse(fs.readFileSync(path.join(compilerRoot,'compiler-pin.json'))),compilerBytes=fs.readFileSync(compilerFile);
 assert.equal(sha(compilerBytes),compilerPin.soljson_sha256,'compiler byte commitment');
 const wrapperFile=path.join(ROOT,'integration/.runtime/stable-reference/node_modules/solc/wrapper.js'),compiler=require(wrapperFile)(require(compilerFile));assert.equal(compiler.version(),metadata.compiler.version+'.Emscripten.clang','exact compiler build version');
 const settings={...metadata.settings,outputSelection:{'*':{'*':['evm.deployedBytecode.object','metadata']}}};delete settings.compilationTarget;
 const output=JSON.parse(compiler.compile(JSON.stringify({language:'Solidity',sources,settings})));assert(!(output.errors||[]).some(e=>e.severity==='error'),JSON.stringify(output.errors));
 const entries=Object.entries(metadata.settings.compilationTarget);assert.equal(entries.length,1);const [file,name]=entries[0];
 verifyRebuild(metadata,output.contracts[file][name],evidence.deployments[0].code);
 const report={schema_version:1,passed:true,scope:'single explicitly retained EVM deployment/source/compiler version',chain_id:evidence.chain_id,block_hash:evidence.expected_block_hash,address:evidence.deployments[0].address,artifact_sha256:sha(artifactBytes),evidence_sha256:sha(evidenceBytes),provenance_verifier_sha256:sha(fs.readFileSync(verifierFile)),provenance_verification:verified,source_files:sourceFiles,compiler:compiler.version(),compiler_sha256:sha(compilerBytes),wrapper_sha256:sha(fs.readFileSync(wrapperFile)),rebuilt_runtime_sha256:sha(Buffer.from(evidence.deployments[0].code.slice(2),'hex')),source_rebuild_verified:true,consensus_verified:false,independent_execution_and_receipt_verified:false,all_requested_versions_verified:false,production_ready:false};
 fs.writeFileSync(outputFile,JSON.stringify(report,null,2)+'\n',{flag:'wx'});process.stdout.write(JSON.stringify({passed:true,source_files:Object.keys(sourceFiles).length,report:outputFile})+'\n');
}
module.exports={verifyRebuild};if(require.main===module)main();
