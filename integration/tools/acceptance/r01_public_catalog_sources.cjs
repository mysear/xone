#!/usr/bin/env node
'use strict';
// Compile public Standard JSON inputs. No metadata stripping, guessed immutable
// replacement, or configuration digest can satisfy deployed byte equivalence.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
function blob(root,ref){const p=path.resolve(root,ref.path);assert(!path.isAbsolute(ref.path)&&!ref.path.split('/').includes('..')&&p.startsWith(root+path.sep),'BLOB_PATH');const b=fs.readFileSync(p);assert.equal(sha(b),ref.sha256,'BLOB_HASH');return {path:p,bytes:b};}
function compareRuntime(compiled,observed){
 const a=compiled.replace(/^0x/,''),b=observed.replace(/^0x/,'');
 let first=-1;for(let i=0;i<Math.min(a.length,b.length);i+=2)if(a.slice(i,i+2)!==b.slice(i,i+2)){first=i/2;break;}
 assert(a===b,'SOURCE_RUNTIME_MISMATCH compiled_sha256='+sha(Buffer.from(a,'hex'))+' observed_sha256='+sha(Buffer.from(b,'hex'))+' first_difference_byte='+first);
 return true;
}
function bindImmutables(compiled,observed,references){
 const code=Buffer.from(compiled.replace(/^0x/,''),'hex'),actual=Buffer.from(observed.replace(/^0x/,''),'hex'),bindings={};
 assert.equal(code.length,actual.length,'SOURCE_RUNTIME_LENGTH');
 const occupied=new Set();
 for(const [id,refs] of Object.entries(references||{})){
  assert(refs.length,'IMMUTABLE_REFERENCES');let value;
  for(const ref of refs){
   assert(Number.isInteger(ref.start)&&Number.isInteger(ref.length)&&ref.length>0&&ref.start>=0&&ref.start+ref.length<=code.length,'IMMUTABLE_RANGE');
   for(let i=ref.start;i<ref.start+ref.length;i++){assert(!occupied.has(i),'IMMUTABLE_OVERLAP');occupied.add(i);assert.equal(code[i],0,'IMMUTABLE_NOT_ZERO_PLACEHOLDER');}
   const current=actual.subarray(ref.start,ref.start+ref.length).toString('hex');
   if(value===undefined)value=current;else assert.equal(value,current,'IMMUTABLE_VALUE_INCONSISTENT');
   actual.copy(code,ref.start,ref.start,ref.start+ref.length);
  }
  bindings[id]={value:'0x'+value,references:refs,authority:'compiler-declared-slots-linked-to-observed-runtime-not-creation-input-proof'};
 }
 compareRuntime(code.toString('hex'),actual.toString('hex'));return {code:code.toString('hex'),bindings};
}
function compileStandardHeap(soljson,input){
 // cwrap's 'string' marshalling puts multi-megabyte JSON on the small legacy
 // Emscripten stack. Use Solidity's own heap API; preserve all input bytes.
 assert(Buffer.byteLength(input,'utf8')<=8*1024*1024,'STANDARD_JSON_INPUT_LIMIT');
 const length=soljson.lengthBytesUTF8(input)+1,pointer=soljson._solidity_alloc(length);
 assert(pointer,'SOLIDITY_INPUT_ALLOCATION');
 try{soljson.stringToUTF8(input,pointer,length);const output=soljson._solidity_compile(pointer,0,0);assert(output,'SOLIDITY_OUTPUT');return soljson.UTF8ToString(output);}
 finally{soljson._solidity_reset();}
}
function compile(root,manifest){
 const wrapperPath=path.join(root,'integration/.runtime/stable-reference/node_modules/solc/wrapper.js');
 const wrapper=require(wrapperPath),compilers=new Map(),cache=new Map();
 return manifest.entries.map(entry=>{
  const result={id:entry.id,source_rebuild_verified:false,runtime_sha256:entry.runtime_sha256,consensus_verified:false};
  try{
   const source=JSON.parse(blob(root,entry.source.blob).bytes),compilerBlob=blob(root,entry.compiler.blob);
   const observation=JSON.parse(blob(root,entry.runtime_observation).bytes);const observed=observation.response.result;
   assert(observed&&sha(Buffer.from(observed.slice(2),'hex'))===entry.runtime_sha256,'OBSERVED_RUNTIME_BINDING');
   assert.equal(source.runtimeBytecode.onchainBytecode.toLowerCase(),observed.toLowerCase(),'SOURCE_REPOSITORY_RUNTIME');
   const compilation=source.compilation,input=structuredClone(source.stdJsonInput);
   const official=JSON.parse(blob(root,entry.compiler.official_list).bytes).builds.find(b=>b.longVersion===entry.compiler.version);
   assert(official&&official.sha256.replace(/^0x/,'')===entry.compiler.blob.sha256,'OFFICIAL_COMPILER_LIST_PIN');
   assert.equal(input.language,'Solidity','SOURCE_LANGUAGE');
   let compiler=compilers.get(compilerBlob.path);
   if(!compiler){compiler=wrapper(require(compilerBlob.path));compilers.set(compilerBlob.path,compiler);}
   assert.equal(compiler.version(),compilation.compilerVersion+'.Emscripten.clang','COMPILER_BUILD');
   assert.equal(compilation.compilerVersion,entry.compiler.version,'COMPILER_PIN_VERSION');
   assert.deepEqual(input.settings,compilation.compilerSettings,'COMPILER_SETTINGS_INPUT');
   const fq=compilation.fullyQualifiedName,separator=fq.lastIndexOf(':'),file=fq.slice(0,separator),name=fq.slice(separator+1);
   input.settings.outputSelection={[file]:{[name]:['evm.deployedBytecode','metadata']}};
   const inputText=JSON.stringify(input),key=sha(inputText)+entry.compiler.blob.sha256;
   let output=cache.get(key);
   if(!output){output=JSON.parse(compilation.compilerVersion.startsWith('0.7.6+')?compileStandardHeap(require(compilerBlob.path),inputText):compiler.compile(inputText));assert(!(output.errors||[]).some(e=>e.severity==='error'),JSON.stringify(output.errors));cache.set(key,output);}
   const contract=output.contracts[file][name];
   const linked=bindImmutables(contract.evm.deployedBytecode.object,observed,contract.evm.deployedBytecode.immutableReferences);
   result.immutable_bindings=linked.bindings;
   result.source_rebuild_verified=true;result.compiler=compiler.version();result.compiler_sha256=entry.compiler.blob.sha256;
   result.standard_input_sha256=sha(JSON.stringify(source.stdJsonInput));result.compiled_runtime_sha256=sha(Buffer.from(linked.code,'hex'));
   result.source_files=Object.fromEntries(Object.entries(source.stdJsonInput.sources).map(([name,s])=>{assert.equal(typeof s.content,'string','SOURCE_CONTENT_REQUIRED');return [name,sha(s.content)];}));
   result.settings=source.compilation.compilerSettings;result.metadata=JSON.parse(contract.metadata);result.wrapper_sha256=sha(fs.readFileSync(wrapperPath));
  }catch(e){result.error=e.message;}
  return result;
 });
}
module.exports={compareRuntime,bindImmutables,compileStandardHeap,compile};
if(require.main===module){const [root,manifest,output]=process.argv.slice(2);const results=compile(path.resolve(root),JSON.parse(fs.readFileSync(manifest)));fs.writeFileSync(output,JSON.stringify({schema_version:1,entries:results},null,2)+'\n',{flag:'wx'});console.log(JSON.stringify(results.map(({id,source_rebuild_verified,error})=>({id,source_rebuild_verified,error})),null,2));}
