#!/usr/bin/env node
'use strict';
// Metadata CID constrains exact public inputs. A CID hit has no acceptance
// authority until the real solc output matches BOTH complete metadata segments.
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto'),assert=require('node:assert/strict');
const ROOT=path.resolve(__dirname,'../../..'),BASE=path.join(ROOT,'integration/test/source/deployment-catalog'),DIR=path.join(BASE,'evm-continuation-v2/source-inputs'),load=p=>JSON.parse(fs.readFileSync(p)),{bindImmutables}=require('./r01_public_catalog_sources.cjs'),compiler=require(path.join(ROOT,'integration/.runtime/stable-reference/node_modules/solc/wrapper'))(require(path.join(ROOT,'integration/artifacts/local/r01-public-compilers/soljson-v0.8.19+commit.7dd6d404.js'))),sha=b=>crypto.createHash('sha256').update(b).digest();
function vari(n){const b=[];while(n>=128){b.push((n&127)|128);n>>>=7;}return Buffer.from([...b,n]);}const field=(tag,value)=>Buffer.concat([Buffer.from([tag]),vari(value.length),value]);
function cid(text){const bytes=Buffer.from(text);if(bytes.length>262144)return null;const data=Buffer.concat([Buffer.from([8,2]),field(18,bytes),Buffer.from([24]),vari(bytes.length)]);return sha(field(10,data)).toString('hex');}
const source=load(path.join(DIR,'base-aerodrome-v2-pool-source.json')),actual=source.runtimeBytecode.onchainBytecode,target=actual.slice(-106).slice(20,84),known=load(path.join(BASE,'metadata-exact/base-aerodrome-v2/exact-standard-input.json')).settings.remappings,original=load(path.join(DIR,'aero-unflattened/standard-input-complete.json'));
let found=null,attempts=0;const failures=[];
outer:for(const style of ['old-LF','old-CRLF','git-LF','git-CRLF','flat-LF','flat-CRLF']){
 const flat=style.startsWith('flat'),input=structuredClone(style.startsWith('old')?load(path.join(DIR,'aero-2023-source/standard-input.json')):flat?source.stdJsonInput:original),file=flat?'Pool.sol':'contracts/Pool.sol';
 for(const [name,value] of Object.entries(input.sources)){if(style.startsWith('npm480')&&name.startsWith('lib/openzeppelin-contracts/contracts/'))value.content=fs.readFileSync(path.join(BASE,'metadata-exact/official-inputs/openzeppelin480',name.slice('lib/openzeppelin-contracts/contracts/'.length)),'utf8');value.content=value.content.replace(/\r\n/g,'\n');if(style.endsWith('CRLF'))value.content=value.content.replace(/\n/g,'\r\n');}
 for(const literal of [false,true]){
  input.settings.metadata={bytecodeHash:'ipfs',useLiteralContent:literal};input.settings.outputSelection={[file]:{Pool:['metadata','evm.deployedBytecode','abi']}};
  const output=JSON.parse(compiler.compile(JSON.stringify(input)));if(output.errors?.some(e=>e.severity==='error')){failures.push({style,literal,error:output.errors.find(e=>e.severity==='error').message});continue;}
  const artifact=output.contracts[file].Pool,meta=JSON.parse(artifact.metadata);assert.equal(cid(artifact.metadata),artifact.evm.deployedBytecode.object.slice(-106).slice(20,84),'VALIDATE_UNIXFS_AGAINST_REAL_COMPILER');
  for(let mask=0;mask<1<<known.length;mask++){
   const remaps=known.filter((v,i)=>mask&(1<<i));if(!flat&&!remaps.some(v=>v.startsWith('@openzeppelin/=')))continue;
   for(const nested of [false,true]){
    const values=remaps.map(v=>nested&&v==='ds-test/=lib/ds-test/src/'?'ds-test/=lib/forge-std/lib/ds-test/src/':v);meta.settings.remappings=values.map(v=>':'+v).sort();attempts++;
    if(cid(JSON.stringify(meta))!==target)continue;
    const exact=structuredClone(input);exact.settings.remappings=values;const compiled=JSON.parse(compiler.compile(JSON.stringify(exact))).contracts[file].Pool;const linked=bindImmutables(compiled.evm.deployedBytecode.object,actual,compiled.evm.deployedBytecode.immutableReferences);found={style,literal,remappings:values,input:exact,artifact:compiled,runtime_sha256:sha(Buffer.from(linked.code,'hex')).toString('hex')};break outer;
   }
  }
 }
}
const report={source_rebuild_verified:!!found,target_ipfs_digest:target,attempts,failures};if(found){const directory=path.join(DIR,'aero-pool-cid-exact');fs.mkdirSync(directory,{recursive:true});fs.writeFileSync(path.join(directory,'standard-input.json'),JSON.stringify(found.input,null,2)+'\n',{flag:'wx'});fs.writeFileSync(path.join(directory,'compiled-artifact.json'),JSON.stringify(found.artifact,null,2)+'\n',{flag:'wx'});report.selection={style:found.style,literal:found.literal,remappings:found.remappings,runtime_sha256:found.runtime_sha256};}
fs.writeFileSync(path.join(DIR,'aero-pool-cid-recovery.json'),JSON.stringify(report,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify(report,null,2));
