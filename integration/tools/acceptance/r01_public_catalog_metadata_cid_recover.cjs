#!/usr/bin/env node
'use strict';
// Search only public, unused remapping configurations by the on-chain IPFS CID.
// A hit is not accepted until the original solc rebuild matches the ENTIRE runtime.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const ROOT=path.resolve(__dirname,'../../..'),BASE=path.join(ROOT,'integration/test/source/deployment-catalog'),DIR=path.join(BASE,'metadata-exact'),PUB=path.join(DIR,'official-inputs');
const wrapper=require(path.join(ROOT,'integration/.runtime/stable-reference/node_modules/solc/wrapper.js'));
const {bindImmutables}=require('./r01_public_catalog_sources.cjs');
const sha=x=>crypto.createHash('sha256').update(x).digest(),hex=x=>sha(x).toString('hex');
const varint=n=>{const out=[];while(n>=128){out.push((n&127)|128);n>>>=7;}out.push(n);return Buffer.from(out)};
const field=(tag,value)=>Buffer.concat([Buffer.from([tag]),varint(value.length),value]);
function ipfsDigest(text){const data=Buffer.from(text);assert(data.length<=262144,'SINGLE_UNIXFS_BLOCK_LIMIT');const unix=Buffer.concat([Buffer.from([8,2]),field(18,data),Buffer.from([24]),varint(data.length)]);return sha(field(10,unix)).toString('hex');}
const manifest=JSON.parse(fs.readFileSync(path.join(BASE,'public-source-build-inputs.json'))),entry=manifest.entries.find(e=>e.id==='base-aerodrome-v2'),source=JSON.parse(fs.readFileSync(path.join(ROOT,entry.source.blob.path))),original=source.stdJsonInput;
const compiler=wrapper(require(path.join(ROOT,entry.compiler.blob.path)));assert.equal(compiler.version(),'0.8.19+commit.7dd6d404.Emscripten.clang');assert.equal(hex(fs.readFileSync(path.join(ROOT,entry.compiler.blob.path))),entry.compiler.blob.sha256);
const observed=source.runtimeBytecode.onchainBytecode,target=observed.slice(-106).slice(20,84);assert.equal(target.length,64);const baselines=[];
function add(label,input,file){baselines.push({label,input,file});}
for(const newline of ['preserved','LF']){const input=structuredClone(original);if(newline==='LF')Object.values(input.sources).forEach(s=>s.content=s.content.replace(/\r\n/g,'\n'));add('public-flattened-'+newline,input,'PoolFactory.sol');}
const names={'contracts/factories/PoolFactory.sol':'aero-factory.txt','contracts/interfaces/IPool.sol':'aero-ipool.txt','contracts/interfaces/factories/IPoolFactory.sol':'aero-ifactory.txt'};
const manual=fs.readFileSync(path.join(PUB,'aero-remappings.json'),'utf8').trim().split(/\r?\n/);
for(const style of ['lib','npm'])for(const clone of ['git','npm480']){
 const sources=Object.fromEntries(Object.entries(names).map(([name,file])=>[name,{content:fs.readFileSync(path.join(PUB,file),'utf8')}]));
 const key=style==='lib'?'lib/openzeppelin-contracts/contracts/proxy/Clones.sol':'@openzeppelin/contracts/proxy/Clones.sol';sources[key]={content:fs.readFileSync(path.join(PUB,clone==='git'?'aero-clones.txt':'openzeppelin480/proxy/Clones.sol'),'utf8')};
 const settings=structuredClone(original.settings);settings.libraries={};settings.remappings=style==='lib'?manual:[];add('official-source-'+style+'-'+clone,{language:'Solidity',sources,settings},'contracts/factories/PoolFactory.sol');
}
const extras=[['erc4626-tests/=lib/openzeppelin-contracts/lib/erc4626-tests/'],['eth-gas-reporter/=node_modules/eth-gas-reporter/'],['gsn/=lib/gsn/'],['hardhat/=node_modules/hardhat/'],['hardhat-deploy/=node_modules/hardhat-deploy/'],['openzeppelin-contracts/=lib/openzeppelin-contracts/','openzeppelin-contracts/=lib/openzeppelin-contracts/contracts/'],['v3-core/=lib/v3-core/'],['solidity-lib/=lib/v3-core/lib/solidity-lib/contracts/']];
function combinations(i,values,out){if(i===extras.length){out.push(values);return;}combinations(i+1,values,out);for(const value of extras[i])combinations(i+1,[...values,value],out);}
const choices=[];combinations(0,[],choices);let attempts=0,found=null;const failures=[];
for(const baseline of baselines){
 if(found)break;
 for(const evm of ['paris','london'])for(const literal of [false,true]){
  if(found)break;
  const input=structuredClone(baseline.input);input.settings.evmVersion=evm;input.settings.metadata={bytecodeHash:'ipfs',useLiteralContent:literal};input.settings.outputSelection={[baseline.file]:{PoolFactory:['metadata','evm.deployedBytecode','abi']}};
  const output=JSON.parse(compiler.compile(JSON.stringify(input)));const errors=(output.errors||[]).filter(e=>e.severity==='error');if(errors.length){failures.push({baseline:baseline.label,error:errors[0].message});continue;}
  const contract=output.contracts[baseline.file].PoolFactory,metadata=JSON.parse(contract.metadata),cb=contract.evm.deployedBytecode.object;
  const currentDigest=cb.slice(-106).slice(20,84);assert.equal(ipfsDigest(contract.metadata),currentDigest,'VALIDATE_UNIXFS_CID_AGAINST_ORIGINAL_COMPILER');
  for(const extra of choices){
   for(const ds of ['manual','forge-nested']){
    attempts++;const remaps=new Map((input.settings.remappings||[]).map(v=>{const pos=v.indexOf('=');return [v.slice(0,pos),v.slice(pos+1)]}));
    for(const v of extra){const pos=v.indexOf('=');remaps.set(v.slice(0,pos),v.slice(pos+1));}
    if(ds==='forge-nested')remaps.set('ds-test/','lib/forge-std/lib/ds-test/src/');
    const values=[...remaps].map(([from,to])=>from+'='+to);metadata.settings.remappings=values.map(v=>v.includes(':')?v:':'+v).sort();
    if(ipfsDigest(JSON.stringify(metadata))!==target)continue;
    const exact=structuredClone(input);exact.settings.remappings=values;const out=JSON.parse(compiler.compile(JSON.stringify(exact))),compiled=out.contracts[baseline.file].PoolFactory;
    const linked=bindImmutables(compiled.evm.deployedBytecode.object,observed,compiled.evm.deployedBytecode.immutableReferences);
    found={baseline:baseline.label,evm,literal,remappings:values,input:exact,artifact:compiled,immutable_bindings:linked.bindings,runtime_sha256:hex(Buffer.from(observed.slice(2),'hex'))};break;
   }
   if(found)break;
  }
 }
}
const report={schema_version:1,id:entry.id,attempts,public_remapping_models:choices.length,source_rebuild_verified:!!found,target_ipfs_sha256:target,failures};
if(found){const directory=path.join(DIR,entry.id);for(const [file,data] of [['exact-standard-input.json',found.input],['compiled-artifact.json',found.artifact]]){const p=path.join(directory,file);if(!fs.existsSync(p))fs.writeFileSync(p,JSON.stringify(data,null,2)+'\n',{flag:'wx'});}report.selection={...found};delete report.selection.input;delete report.selection.artifact;report.compiler=compiler.version();report.metadata=JSON.parse(found.artifact.metadata);}
fs.writeFileSync(path.join(DIR,'aero-cid-recovery.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify({id:entry.id,attempts,matched:!!found,selection:report.selection},null,2));
