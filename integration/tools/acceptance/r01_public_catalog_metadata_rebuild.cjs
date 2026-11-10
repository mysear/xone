#!/usr/bin/env node
'use strict';
// Recover exact compiler input from public explorer settings and lossless source
// candidates. Acceptance always compares every runtime byte including metadata.
const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const ROOT=path.resolve(__dirname,'../../..'),BASE=path.join(ROOT,'integration/test/source/deployment-catalog'),OUT=path.join(BASE,'metadata-exact');
const sha=x=>crypto.createHash('sha256').update(x).digest('hex');
const wrapper=require(path.join(ROOT,'integration/.runtime/stable-reference/node_modules/solc/wrapper.js'));
const {ethers}=require(path.join(ROOT,'integration/.runtime/stable-reference/node_modules/ethers'));
const {compileStandardHeap,bindImmutables}=require('./r01_public_catalog_sources.cjs');
const phase=process.argv[2]||'first';
const manifest=JSON.parse(fs.readFileSync(path.join(BASE,'public-source-build-inputs.json'))),report=[];
function checksumLibraries(settings){for(const libraries of Object.values(settings.libraries||{}))for(const [name,address] of Object.entries(libraries))libraries[name]=ethers.utils.getAddress(address);}
for(const id of ['base-aerodrome-v2','base-aerodrome-slipstream','bsc-pancake-v2','optimism-velodrome-v2']){
 const entry=manifest.entries.find(e=>e.id===id),record=JSON.parse(fs.readFileSync(path.join(ROOT,entry.source.blob.path))),original=record.stdJsonInput;
 const compiler=wrapper(require(path.join(ROOT,entry.compiler.blob.path)));const candidates=[];
 const add=(label,input)=>candidates.push({label,input});
 const explorer=path.join(OUT,id,'explorer-standard-input.json');
 if(fs.existsSync(explorer)){
  const input=JSON.parse(fs.readFileSync(explorer)),settings=input.sources['settings.json'];
  if(settings){delete input.sources['settings.json'];input.settings=JSON.parse(settings.content);delete input.settings.viaIR;input.language='Solidity';add('exact-public-explorer-settings-and-filenames',input);}
 }
 for(const checksum of [false,true])for(const literal of [false,true])for(const newline of ['preserved','LF']){
  const input=structuredClone(original);input.settings.metadata||={};input.settings.metadata.useLiteralContent=literal;if(checksum)checksumLibraries(input.settings);
  if(newline==='LF')for(const source of Object.values(input.sources))source.content=source.content.replace(/\r\n/g,'\n');
  add(`public-source/canonical-libraries=${checksum}/literal=${literal}/newline=${newline}`,input);
 }
 if(Object.keys(original.sources).length===1){
  const [name,source]=Object.entries(original.sources)[0];
  const paths=id==='base-aerodrome-v2'?['contracts/factories/PoolFactory.sol','contracts/PoolFactory.sol']:['contracts/PancakeFactory.sol','PancakeSwap.sol','contracts/PancakeSwap.sol',''];
  for(const newName of paths)for(const newline of ['preserved','LF']){
   const input=structuredClone(original);input.sources={[newName]:structuredClone(source)};if(newline==='LF')input.sources[newName].content=input.sources[newName].content.replace(/\r\n/g,'\n');add('source-file-candidate:'+newName+':'+newline,input);
  }
 }
 if(phase!=='first'){
  if(fs.existsSync(path.join(OUT,id,'exact-standard-input.json')))add('retained-exact-compiler-input',JSON.parse(fs.readFileSync(path.join(OUT,id,'exact-standard-input.json'))));
  candidates.length=0;
  for(const libMode of ['none','original'])for(const remapMode of ['original','none','only-used'])for(const literal of [false,true]){
   const input=structuredClone(original);if(libMode==='none')input.settings.libraries={};
   if(remapMode==='none')input.settings.remappings=[];
   if(remapMode==='only-used')input.settings.remappings=(input.settings.remappings||[]).filter(x=>x.includes('@openzeppelin/='));
   input.settings.metadata||={};input.settings.metadata.useLiteralContent=literal;
   if(remapMode!=='original'){
    const key='lib/openzeppelin-contracts/contracts/proxy/Clones.sol';
    if(input.sources[key]){input.sources['@openzeppelin/contracts/proxy/Clones.sol']=input.sources[key];delete input.sources[key];input.settings.remappings=(input.settings.remappings||[]).filter(x=>!x.includes('@openzeppelin/='));}
   }
   add(`recovered-unused-libraries=${libMode}/remaps=${remapMode}/literal=${literal}`,input);
  }
  if(id==='base-aerodrome-v2'){
   const folder=path.join(OUT,'official-inputs'),sourceFiles={'contracts/factories/PoolFactory.sol':'aero-factory.txt','contracts/interfaces/IPool.sol':'aero-ipool.txt','contracts/interfaces/factories/IPoolFactory.sol':'aero-ifactory.txt'};
   for(const mode of ['native-remappings','npm-import-path'])for(const literal of [false,true]){
    const sources=Object.fromEntries(Object.entries(sourceFiles).map(([key,file])=>[key,{content:fs.readFileSync(path.join(folder,file),'utf8')}]));
    const key=mode==='native-remappings'?'lib/openzeppelin-contracts/contracts/proxy/Clones.sol':'@openzeppelin/contracts/proxy/Clones.sol';sources[key]={content:fs.readFileSync(path.join(folder,'aero-clones.txt'),'utf8')};
    const settings=structuredClone(original.settings);settings.libraries={};settings.remappings=mode==='native-remappings'?fs.readFileSync(path.join(folder,'aero-remappings.json'),'utf8').trim().split(/\r?\n/):[];settings.metadata={bytecodeHash:'ipfs',useLiteralContent:literal};
    add('official-unflattened-source-'+mode+'-literal='+literal,{language:'Solidity',sources,settings});
   }
  }
 }
 if(phase==='third')candidates.sort((a,b)=>Number(b.label.startsWith('official-unflattened'))-Number(a.label.startsWith('official-unflattened')));
  if(phase==='fourth'){
  if(id==='bsc-pancake-v2'){
   candidates.length=0;
   for(const file of ['pancake-ethereum-factory-source.json','pancake-base-factory-source.json']){
    const published=JSON.parse(fs.readFileSync(path.join(OUT,'official-inputs',file))).stdJsonInput;
    for(const name of ['contracts/PancakeFactory.sol','PancakeFactory.sol'])for(const newline of ['LF','CRLF'])for(const literal of [false,true]){
     const input=structuredClone(published),source=structuredClone(Object.values(input.sources)[0]);source.content=source.content.replace(/\r\n/g,'\n');if(newline==='CRLF')source.content=source.content.replace(/\n/g,'\r\n');
     input.sources={[name]:source};input.settings=structuredClone(original.settings);input.settings.metadata={useLiteralContent:literal};
     add('exact-public-cross-deployment-source:'+file+':'+name+':'+newline+':literal='+literal,input);
    }
   }
  }
  if(id==='base-aerodrome-v2'){
   for(const candidate of [...candidates])if(candidate.label.startsWith('official-unflattened')){
    const input=structuredClone(candidate.input),key=Object.keys(input.sources).find(k=>k.endsWith('/Clones.sol'));
    input.sources[key].content=fs.readFileSync(path.join(OUT,'official-inputs/openzeppelin480/proxy/Clones.sol'),'utf8');add(candidate.label+'-official-npm-openzeppelin-4.8.0',input);
   }
  }
 }
 if(phase==='fifth'){
  if(id==='bsc-pancake-v2'){
   candidates.length=0;
   const published=JSON.parse(fs.readFileSync(path.join(OUT,'official-inputs/pancake-ethereum-factory-source.json'))).stdJsonInput;
   for(const version of ['original','public-2021-04-23'])for(const name of ['PancakeFactory.sol','contracts/PancakeFactory.sol'])for(const newline of ['preserved','LF','CRLF'])for(const runs of [999999,99999,1000000,0,1,1000]){
    const input=structuredClone(version==='original'?original:published),source=structuredClone(Object.values(input.sources)[0]);
    if(newline!=='preserved'){source.content=source.content.replace(/\r\n/g,'\n');if(newline==='CRLF')source.content=source.content.replace(/\n/g,'\r\n');}
    input.sources={[name]:source};input.settings=structuredClone(original.settings);input.settings.optimizer={enabled:false,runs};
    add('public-source-disabled-optimizer-runs='+runs+':'+version+':'+name+':'+newline,input);
   }
  }
  if(id==='base-aerodrome-v2'){
   const previous=[...candidates];candidates.length=0;
   for(const c of previous)if(c.label.startsWith('official-unflattened')){
    const bases=[c.input];const npm=structuredClone(c.input),key=Object.keys(npm.sources).find(k=>k.endsWith('/Clones.sol'));npm.sources[key].content=fs.readFileSync(path.join(OUT,'official-inputs/openzeppelin480/proxy/Clones.sol'),'utf8');bases.push(npm);
    for(const [i,base] of bases.entries())for(const evm of ['london','paris','berlin','istanbul']){
     const input=structuredClone(base);input.settings.evmVersion=evm;add(c.label+':clone='+i+':evm='+evm,input);
    }
   }
  }
 }
const result={id,attempts:[],source_rebuild_verified:false};
 const [file,contractName]=[record.compilation.fullyQualifiedName.slice(0,record.compilation.fullyQualifiedName.lastIndexOf(':')),record.compilation.name];
 for(const {label,input} of candidates){
  const sourceFile=input.sources[file]?file:(Object.keys(input.sources).length===1?Object.keys(input.sources)[0]:Object.keys(input.sources).find(p=>p.endsWith('/'+contractName+'.sol')));
  input.settings.outputSelection={[sourceFile]:{[contractName]:['metadata','evm.deployedBytecode','abi']}};
  try{
   const text=JSON.stringify(input),output=JSON.parse(entry.compiler.version.startsWith('0.7.6+')?compileStandardHeap(require(path.join(ROOT,entry.compiler.blob.path)),text):compiler.compile(text));
   const errors=(output.errors||[]).filter(e=>e.severity==='error');assert(!errors.length,errors.map(e=>e.formattedMessage).join('\n').slice(0,500));
   const contract=output.contracts[sourceFile][contractName],observed=record.runtimeBytecode.onchainBytecode;
   const linked=bindImmutables(contract.evm.deployedBytecode.object,observed,contract.evm.deployedBytecode.immutableReferences);
   const directory=path.join(OUT,id);fs.mkdirSync(directory,{recursive:true});
   if(!fs.existsSync(path.join(directory,'exact-standard-input.json')))fs.writeFileSync(path.join(directory,'exact-standard-input.json'),JSON.stringify(input,null,2)+'\n',{flag:'wx'});
   if(!fs.existsSync(path.join(directory,'compiled-artifact.json')))fs.writeFileSync(path.join(directory,'compiled-artifact.json'),JSON.stringify(contract,null,2)+'\n',{flag:'wx'});
   result.source_rebuild_verified=true;result.selected_input=label;result.runtime_sha256=sha(Buffer.from(observed.slice(2),'hex'));result.compiler=compiler.version();result.immutable_bindings=linked.bindings;
   result.settings=input.settings;result.source_files=Object.fromEntries(Object.entries(input.sources).map(([n,s])=>[n,sha(s.content)]));result.metadata=JSON.parse(contract.metadata);break;
  }catch(error){result.attempts.push({label,error:error.message.slice(0,500)});}
 }
 report.push(result);
}
fs.writeFileSync(path.join(OUT,'exact-build-results-'+phase+'.json'),JSON.stringify({schema_version:1,entries:report},null,2)+'\n',{flag:'wx'});
console.log(JSON.stringify(report.map(({id,source_rebuild_verified,selected_input,attempts})=>({id,source_rebuild_verified,selected_input,failed_attempts:attempts.length})),null,2));
