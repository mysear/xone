#!/usr/bin/env node
'use strict';
// Offline exact compiler-input verifier for recovered deployed metadata.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const ROOT=path.resolve(__dirname,'../../..');const sha=x=>crypto.createHash('sha256').update(x).digest('hex');
const {bindImmutables,compileStandardHeap}=require('./r01_public_catalog_sources.cjs'),wrapper=require(path.join(ROOT,'integration/.runtime/stable-reference/node_modules/solc/wrapper.js'));
function blob(ref){const p=fs.realpathSync(path.resolve(ROOT,ref.path));assert(!path.isAbsolute(ref.path)&&!ref.path.split('/').includes('..')&&p.startsWith(ROOT+path.sep),'EXACT_SOURCE_PATH');const b=fs.readFileSync(p);assert.equal(sha(b),ref.sha256,'EXACT_SOURCE_BLOB_HASH');return {path:p,bytes:b};}
function verifyEntry(entry){
 const input=JSON.parse(blob(entry.input).bytes),expected=JSON.parse(blob(entry.artifact).bytes),metadata=JSON.parse(expected.metadata),observed=JSON.parse(blob(entry.runtime_observation).bytes);
 assert.equal(observed.method,'eth_getCode','EXACT_RUNTIME_METHOD');assert(!observed.response.error,'EXACT_RUNTIME_RPC_ERROR');const runtime=observed.response.result;assert(/^0x[0-9a-fA-F]+$/.test(runtime)&&runtime.length%2===0,'EXACT_RUNTIME_HEX');assert.equal(sha(Buffer.from(runtime.slice(2),'hex')),entry.runtime_sha256,'EXACT_RUNTIME_BINDING');
 const compilerBlob=blob(entry.compiler.blob),list=JSON.parse(blob(entry.compiler.official_list).bytes),build=list.builds.find(b=>b.longVersion===entry.compiler.version);
 assert(build&&build.sha256.replace(/^0x/,'')===entry.compiler.blob.sha256,'EXACT_OFFICIAL_COMPILER_PIN');
 const compiler=wrapper(require(compilerBlob.path));assert.equal(compiler.version(),entry.compiler.version+'.Emscripten.clang','EXACT_COMPILER_VERSION');assert.equal(metadata.compiler.version,entry.compiler.version,'EXACT_METADATA_COMPILER');
 const targets=Object.entries(metadata.settings.compilationTarget);assert.equal(targets.length,1);const [file,name]=targets[0];assert(input.sources[file],'EXACT_SOURCE_TARGET');
 const selected=structuredClone(input);selected.settings.outputSelection={[file]:{[name]:['metadata','abi','evm.deployedBytecode','storageLayout']}};
 const text=JSON.stringify(selected),output=JSON.parse(entry.compiler.version.startsWith('0.7.6+')?compileStandardHeap(require(compilerBlob.path),text):compiler.compile(text));assert(!(output.errors||[]).some(e=>e.severity==='error'),JSON.stringify(output.errors));
 const contract=output.contracts[file][name];assert.equal(contract.metadata,expected.metadata,'EXACT_METADATA_BYTES');assert.equal(contract.evm.deployedBytecode.object,expected.evm.deployedBytecode.object,'EXACT_COMPILED_TEMPLATE');assert.deepEqual(contract.evm.deployedBytecode.immutableReferences,expected.evm.deployedBytecode.immutableReferences,'EXACT_IMMUTABLE_REFERENCES');
 const linked=bindImmutables(contract.evm.deployedBytecode.object,runtime,contract.evm.deployedBytecode.immutableReferences);
 const pointers=[];
 for(const dependency of entry.implementation_dependencies||[]){
  const getter=JSON.parse(blob(dependency.getter_observation).bytes);assert.equal(getter.method,'eth_call');assert.equal(getter.params[0].data,dependency.getter_selector,'IMPLEMENTATION_GETTER_SELECTOR');assert(!getter.response.error,'IMPLEMENTATION_GETTER_ERROR');assert.equal(getter.params[0].to,observed.params[0],'IMPLEMENTATION_GETTER_FACTORY');assert.equal(getter.params[1],observed.params[1],'IMPLEMENTATION_GETTER_H');
  const address='0x'+getter.response.result.slice(-40).toLowerCase();assert.equal(address,dependency.address.toLowerCase(),'IMPLEMENTATION_GETTER_TARGET');
  const code=JSON.parse(blob(dependency.observation).bytes);assert.equal(code.method,'eth_getCode','IMPLEMENTATION_CODE_METHOD');assert(code.response.result.length>2&&!code.response.error,'IMPLEMENTATION_EMPTY_CODE');assert.equal(code.params[0],address,'IMPLEMENTATION_CODE_ADDRESS');assert.equal(code.params[1],observed.params[1],'IMPLEMENTATION_CODE_H');assert.equal(sha(Buffer.from(code.response.result.slice(2),'hex')),dependency.sha256,'IMPLEMENTATION_CODE_HASH');
  const immutable=Object.values(linked.bindings).some(b=>b.value.toLowerCase()==='0x'+address.slice(2).padStart(64,'0'));
  pointers.push({address,runtime_sha256:dependency.sha256,immutable_target:immutable,getter:dependency.getter_selector});
 }
 return {id:entry.id,source_rebuild_verified:true,compiler:compiler.version(),compiler_sha256:entry.compiler.blob.sha256,runtime_sha256:entry.runtime_sha256,metadata_exact:true,source_files:Object.fromEntries(Object.entries(input.sources).map(([name,s])=>[name,sha(s.content)])),settings:input.settings,metadata:JSON.parse(contract.metadata),immutable_bindings:linked.bindings,implementation_targets:pointers,
  deployment_mode:pointers.length&&pointers.every(p=>p.immutable_target)?'direct-factory-with-immutable-clone-target':'source-rebuilt-runtime-with-separately-observed-implementation-getter',storage_layout:contract.storageLayout,consensus_verified:false};
}
function verify(manifest){assert(manifest.entries?.length,'EMPTY_EXACT_SOURCE_MANIFEST');assert.equal(new Set(manifest.entries.map(e=>e.id)).size,manifest.entries.length,'DUPLICATE_EXACT_SOURCE_ENTRY');return {schema_version:1,entries:manifest.entries.map(entry=>{try{return verifyEntry(entry)}catch(error){return {id:entry.id,source_rebuild_verified:false,error:error.message}}})};}
module.exports={verifyEntry,verify};
if(require.main===module){const result=verify(JSON.parse(fs.readFileSync(process.argv[2])));if(process.argv[3])fs.writeFileSync(process.argv[3],JSON.stringify(result,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify(result,null,2));process.exitCode=result.entries.every(r=>r.source_rebuild_verified)?0:1;}
