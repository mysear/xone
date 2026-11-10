#!/usr/bin/env node
'use strict';
// Reconstruct candidate original V2 source layouts from public V1 source headers
// and public verified V2 contract bodies. Never accept a metadata-only/prefix match.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const ROOT=path.resolve(__dirname,'../../..'),BASE=path.join(ROOT,'integration/test/source/deployment-catalog'),DIR=path.join(BASE,'metadata-exact'),PKG=path.join(DIR,'official-inputs/pancake-core-0.1.0');
const wrapper=require(path.join(ROOT,'integration/.runtime/stable-reference/node_modules/solc/wrapper.js')),sha=x=>crypto.createHash('sha256').update(x).digest('hex');
const manifest=JSON.parse(fs.readFileSync(path.join(BASE,'public-source-build-inputs.json'))),entry=manifest.entries.find(e=>e.id==='bsc-pancake-v2'),record=JSON.parse(fs.readFileSync(path.join(ROOT,entry.source.blob.path))),compiler=wrapper(require(path.join(ROOT,entry.compiler.blob.path)));
const packageFactory=JSON.parse(fs.readFileSync(path.join(PKG,'build/PancakeFactory.json'))),packageMetadata=JSON.parse(packageFactory.metadata),original={};
for(const filename of Object.keys(packageMetadata.sources))original[filename]={content:fs.readFileSync(path.join(PKG,filename),'utf8')};
const published=[record,JSON.parse(fs.readFileSync(path.join(DIR,'official-inputs/pancake-ethereum-factory-source.json')))];
function definitions(text,files=null,filename='verified.sol'){const input={language:'Solidity',sources:files||{'verified.sol':{content:text}},settings:{outputSelection:{'*':{'':['ast']}}}};const out=JSON.parse(compiler.compile(JSON.stringify(input)));assert(!out.errors?.some(e=>e.severity==='error'));const nodes=out.sources[filename].ast.nodes.filter(n=>n.nodeType==='ContractDefinition');return Object.fromEntries(nodes.map(n=>{const [start,length]=n.src.split(':').map(Number);return [n.name,Buffer.from(text).subarray(start,start+length).toString('utf8')]}));}
const bodies=published.map(p=>definitions(Object.values(p.stdJsonInput.sources)[0].content));const versions=[];
for(const [index,body] of bodies.entries()){
 const sources=structuredClone(original);
 for(const [filename,source] of Object.entries(sources)){
  const ast=definitions(source.content,original,filename),name=Object.keys(ast)[0];if(!body[name])continue;
  const offset=source.content.indexOf(ast[name]);source.content=source.content.slice(0,offset)+body[name]+source.content.slice(offset+ast[name].length);
 }
 versions.push({label:'public-package-file-headers-and-published-V2-bodies-'+index,sources});
}
const patched=structuredClone(original);patched['contracts/PancakePair.sol'].content=patched['contracts/PancakePair.sol'].content.replace('totalSupply.mul(rootK.sub(rootKLast));','totalSupply.mul(rootK.sub(rootKLast)).mul(8);').replace('rootK.mul(3).add(rootKLast);','rootK.mul(17).add(rootKLast.mul(8));').replaceAll('balance0.mul(1000).sub(amount0In.mul(2))','balance0.mul(10000).sub(amount0In.mul(25))').replaceAll('balance1.mul(1000).sub(amount1In.mul(2))','balance1.mul(10000).sub(amount1In.mul(25))').replace('mul(1000**2)','mul(10000**2)');versions.unshift({label:'public-V1-layout-with-public-V2-fee-delta',sources:patched});
let attempts=[],found=null;
outer:for(const version of versions)for(const newline of ['LF','CRLF'])for(const rootPrefix of ['contracts/','','src/'])for(const runs of [200,999999,99999,1000000])for(const literal of [false,true]){
 const sources={};for(const [filename,value] of Object.entries(version.sources)){let content=value.content.replace(/\r\n/g,'\n');if(newline==='CRLF')content=content.replace(/\n/g,'\r\n');sources[rootPrefix+filename.replace(/^contracts\//,'')]={content};}
 const file=rootPrefix+'PancakeFactory.sol',input={language:'Solidity',sources,settings:structuredClone(record.stdJsonInput.settings)};input.settings.optimizer.runs=runs;input.settings.metadata={useLiteralContent:literal};input.settings.outputSelection={[file]:{PancakeFactory:['metadata','evm.deployedBytecode','abi']}};
 const output=JSON.parse(compiler.compile(JSON.stringify(input))),errors=(output.errors||[]).filter(e=>e.severity==='error');const label={source:version.label,newline,rootPrefix,runs,literal};
 if(errors.length){attempts.push({...label,error:errors[0].message});continue;}
 const artifact=output.contracts[file].PancakeFactory,actual=artifact.evm.deployedBytecode.object,expected=record.runtimeBytecode.onchainBytecode.slice(2);
 if(actual===expected){found={...label,input,artifact};break outer;}
 let difference=-1;for(let i=0;i<Math.min(actual.length,expected.length);i+=2)if(actual.slice(i,i+2)!==expected.slice(i,i+2)){difference=i/2;break;}attempts.push({...label,first_difference_byte:difference,compiled_runtime_bytes:actual.length/2});
}
const report={schema_version:1,id:entry.id,source_rebuild_verified:!!found,attempts,source_reconstruction:'public package import headers + explicitly retained public verified V2 bodies or V2 fee delta; no metadata stripping'};
if(found){const directory=path.join(DIR,entry.id);fs.mkdirSync(directory,{recursive:true});fs.writeFileSync(path.join(directory,'exact-standard-input.json'),JSON.stringify(found.input,null,2)+'\n',{flag:'wx'});fs.writeFileSync(path.join(directory,'compiled-artifact.json'),JSON.stringify(found.artifact,null,2)+'\n',{flag:'wx'});report.selection={...found};delete report.selection.input;delete report.selection.artifact;report.runtime_sha256=sha(Buffer.from(record.runtimeBytecode.onchainBytecode.slice(2),'hex'));}
fs.writeFileSync(path.join(DIR,'pancake-unflattened-recovery.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify({matched:!!found,attempts:attempts.length,selection:report.selection},null,2));
