// SPDX-License-Identifier: GPL-2.0-or-later
// Offline exact predecessor/Factory source build, not historical-state qualification.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../../..'),dir=path.join(root,'integration/tools/reference/pancake-v3-lm-bsc');
const engine=require(path.join(root,'integration/.runtime/stable-reference/node_modules/solc07'));
const sha=b=>crypto.createHash('sha256').update(b).digest('hex'),must=(v,m)=>{if(!v)throw Error(m);};
const records=[...JSON.parse(fs.readFileSync(path.join(dir,'predecessor-acquisition.json'))).records,...JSON.parse(fs.readFileSync(path.join(dir,'factory-acquisition.json'))).records];
const results={};
for(const record of records){
 must(record.passed,'BSC4_SOURCE_ACQUISITION:'+record.role);const raw=fs.readFileSync(path.join(root,record.path));must(sha(raw)===record.sha256,'BSC4_SOURCE_DIGEST');
 const source=JSON.parse(raw),input=structuredClone(source.stdJsonInput);must(engine.version().startsWith(source.compilation.compilerVersion),'BSC4_COMPILER_VERSION');
 input.settings.outputSelection={'*':{'':['ast'],'*':['abi','storageLayout','evm.bytecode.object','evm.deployedBytecode.object','evm.deployedBytecode.immutableReferences']}};
 const compiled=JSON.parse(engine.compile(JSON.stringify(input))),errors=(compiled.errors||[]).filter(x=>x.severity==='error');must(!errors.length,JSON.stringify(errors));
 const[file,name]=source.compilation.fullyQualifiedName.split(':'),artifact=compiled.contracts[file][name],deployed=artifact.evm.deployedBytecode;
 const observed=Buffer.from(source.runtimeBytecode.onchainBytecode.slice(2),'hex'),normalized=Buffer.from(observed),names={};
 function visit(n){if(!n||typeof n!=='object')return;if(n.nodeType==='VariableDeclaration'&&n.mutability==='immutable')names[n.id]=n.name;for(const v of Object.values(n)){if(Array.isArray(v))v.forEach(visit);else if(v&&typeof v==='object')visit(v);}}Object.values(compiled.sources).forEach(visit);
 const bindings=[];for(const[id,items]of Object.entries(deployed.immutableReferences))for(const b of items){must(names[id]&&b.length===32&&b.start+32<=observed.length,'BSC4_IMMUTABLE_SCOPE');bindings.push({name:names[id],offset:b.start,word:observed.subarray(b.start,b.start+32).toString('hex')});normalized.fill(0,b.start,b.start+32);}
 const fullRuntimeMatch=normalized.toString('hex')===deployed.object;
 const metadataBytes=observed.readUInt16BE(observed.length-2)+2,body=normalized.subarray(0,observed.length-metadataBytes),compiledBytes=Buffer.from(deployed.object,'hex'),compiledMetadataBytes=compiledBytes.readUInt16BE(compiledBytes.length-2)+2;must(body.equals(compiledBytes.subarray(0,compiledBytes.length-compiledMetadataBytes)),'BSC4_ORIGINAL_EXECUTION_BODY:'+record.role);
 results[record.role]={source_path:record.path,source_sha256:record.sha256,address:record.address,compiler:engine.version(),compiler_settings:source.compilation.compilerSettings,compilation_input_sha256:sha(Buffer.from(JSON.stringify(input))),full_runtime_match:fullRuntimeMatch,metadata_match:fullRuntimeMatch,runtime_sha256:sha(observed),runtime_bytes:observed.length,body_bytes:body.length,normalized_execution_body_sha256:sha(body),immutable_bindings:bindings,artifact};
 console.log(JSON.stringify({role:record.role,passed:true,template:sha(body),bindings:[...new Set(bindings.map(b=>b.name))]}));
}
fs.writeFileSync(path.join(root,'integration/artifacts/local/core-pancake-bsc-predecessor-artifacts.json'),JSON.stringify({schema_version:1,passed:true,contracts:results,authority:'original source/settings recompiled; normalized execution bodies match retained public code; per-role metadata/full-runtime comparison reported separately; no independent historical-bank qualification',all_requested_features_complete:false,production_ready:false},null,2)+'\n');
