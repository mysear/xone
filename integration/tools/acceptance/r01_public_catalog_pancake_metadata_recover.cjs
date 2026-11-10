#!/usr/bin/env node
'use strict';
// Recover legacy compiler filename/comment/runs inputs by their actual Swarm
// metadata commitment, then recompile and compare the complete factory runtime.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const ROOT=path.resolve(__dirname,'../../..'),BASE=path.join(ROOT,'integration/test/source/deployment-catalog'),DIR=path.join(BASE,'metadata-exact');
const {ethers}=require(path.join(ROOT,'integration/.runtime/stable-reference/node_modules/ethers')),wrapper=require(path.join(ROOT,'integration/.runtime/stable-reference/node_modules/solc/wrapper.js'));
const hash=x=>Buffer.from(ethers.utils.keccak256(x).slice(2),'hex'),sha=x=>crypto.createHash('sha256').update(x).digest('hex');
function chunk(data,span){const b=Buffer.alloc(4096);data.copy(b);let nodes=Array.from({length:128},(_,i)=>b.subarray(i*32,i*32+32));while(nodes.length>1){const next=[];for(let i=0;i<nodes.length;i+=2)next.push(hash(Buffer.concat([nodes[i],nodes[i+1]])));nodes=next;}const size=Buffer.alloc(8);size.writeBigUInt64LE(BigInt(span));return hash(Buffer.concat([size,nodes[0]]));}
function swarm(data){assert(data.length>0&&data.length<=128*1024,'METADATA_SWARM_LIMIT');let nodes=[];for(let i=0;i<data.length;i+=4096){const p=data.subarray(i,i+4096);nodes.push({hash:chunk(p,p.length),span:p.length});}while(nodes.length>1){const next=[];for(let i=0;i<nodes.length;i+=128){const group=nodes.slice(i,i+128);if(group.length===1)next.push(group[0]);else{const span=group.reduce((a,b)=>a+b.span,0);next.push({hash:chunk(Buffer.concat(group.map(n=>n.hash)),span),span});}}nodes=next;}return nodes[0].hash.toString('hex');}
const golden=JSON.parse(fs.readFileSync(path.join(DIR,'official-inputs/pancake-core-0.1.0/build/PancakeFactory.json')));assert.equal(swarm(Buffer.from(golden.metadata)),golden.evm.deployedBytecode.object.slice(-104).slice(18,82),'SWARM_BMT_GOLDEN_COMPILER_CHECK');
const manifest=JSON.parse(fs.readFileSync(path.join(BASE,'public-source-build-inputs.json'))),entry=manifest.entries.find(e=>e.id==='bsc-pancake-v2'),source=JSON.parse(fs.readFileSync(path.join(ROOT,entry.source.blob.path))),compiler=wrapper(require(path.join(ROOT,entry.compiler.blob.path)));
assert.equal(compiler.version(),'0.5.16+commit.9c3226ce.Emscripten.clang');assert.equal(sha(fs.readFileSync(path.join(ROOT,entry.compiler.blob.path))),entry.compiler.blob.sha256);
const observed=source.runtimeBytecode.onchainBytecode,target=observed.slice(-104).slice(18,82),variants=[];
for(const [name,object] of [['public-bsc',source],['public-cross-deployment',JSON.parse(fs.readFileSync(path.join(DIR,'official-inputs/pancake-ethereum-factory-source.json')))]]){
 const text=Object.values(object.stdJsonInput.sources)[0].content;
 for(const headers of ['preserve','remove-explorer-verification-notice'])for(const newline of ['LF','CRLF']){
  let content=text.replace(/\r\n/g,'\n');if(headers!=='preserve')content=content.replace(/^(?:\s*\/\*\*[\s\S]*?Submitted for verification[\s\S]*?\*\/\s*)+/,'');if(newline==='CRLF')content=content.replace(/\n/g,'\r\n');
  variants.push({label:name+':'+headers+':'+newline,content});
 }
}
const names=['PancakeFactory.sol','pancakeFactory.sol','pancakefactory.sol','PancakeSwap.sol','pancakeSwap.sol','pancakeswap.sol','PancakeSwapV2.sol','PancakeFactoryV2.sol','pancake.sol','Pancake.sol','Contract.sol','contract.sol','Pancakeswap.sol'];
const filenames=['',...['','contracts/','./contracts/','browser/','./','contracts/core/'].flatMap(prefix=>names.map(name=>prefix+name))];
let attempts=0,found=null;const failures=[];
outer:for(const variant of variants)for(const literal of [false,true]){
 const input={language:'Solidity',sources:{'PancakeFactory.sol':{content:variant.content}},settings:structuredClone(source.stdJsonInput.settings)};input.settings.metadata={useLiteralContent:literal};input.settings.outputSelection={'PancakeFactory.sol':{PancakeFactory:['metadata','evm.deployedBytecode','abi']}};
 const output=JSON.parse(compiler.compile(JSON.stringify(input)));if((output.errors||[]).some(e=>e.severity==='error'))continue;const baseline=output.contracts['PancakeFactory.sol'].PancakeFactory,meta=JSON.parse(baseline.metadata);
 assert.equal(swarm(Buffer.from(baseline.metadata)),baseline.evm.deployedBytecode.object.slice(-104).slice(18,82),'SWARM_BMT_CURRENT_COMPILER_CHECK');
 for(const filename of filenames)for(const runs of [200,999999,99999,1000000,0,1,1000,10000])for(const evm of ['istanbul','petersburg']){
  attempts++;const metadata=structuredClone(meta);metadata.settings.compilationTarget={[filename]:'PancakeFactory'};metadata.sources={[filename]:Object.values(meta.sources)[0]};metadata.settings.optimizer.runs=runs;metadata.settings.evmVersion=evm;
  if(swarm(Buffer.from(JSON.stringify(metadata)))!==target)continue;
  const exact=structuredClone(input);exact.sources={[filename]:{content:variant.content}};exact.settings.optimizer.runs=runs;exact.settings.evmVersion=evm;exact.settings.outputSelection={[filename]:{PancakeFactory:['metadata','evm.deployedBytecode','abi']}};
  const rebuilt=JSON.parse(compiler.compile(JSON.stringify(exact)));if((rebuilt.errors||[]).some(e=>e.severity==='error')){failures.push({filename,runs,evm,error:'compiler-rejected-input'});continue;}
  const artifact=rebuilt.contracts[filename].PancakeFactory;if('0x'+artifact.evm.deployedBytecode.object!==observed){failures.push({filename,runs,evm,error:'whole-runtime-not-identical'});continue;}
  found={label:variant.label,literal,filename,runs,evm,input:exact,artifact};break outer;
 }
}
const report={schema_version:1,id:entry.id,source_rebuild_verified:!!found,attempts,target_swarm_hash:target,failures};
if(found){const directory=path.join(DIR,entry.id);fs.writeFileSync(path.join(directory,'exact-standard-input.json'),JSON.stringify(found.input,null,2)+'\n',{flag:'wx'});fs.writeFileSync(path.join(directory,'compiled-artifact.json'),JSON.stringify(found.artifact,null,2)+'\n',{flag:'wx'});report.selection={label:found.label,literal:found.literal,filename:found.filename,runs:found.runs,evm:found.evm};report.runtime_sha256=sha(Buffer.from(observed.slice(2),'hex'));report.compiler=compiler.version();}
fs.writeFileSync(path.join(DIR,'pancake-swarm-recovery.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify(report,null,2));
