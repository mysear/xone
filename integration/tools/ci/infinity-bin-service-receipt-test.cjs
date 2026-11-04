'use strict';
// Inject read/load failures without changing frozen source, artifact, corpus or receipt bytes.
const fs=require('node:fs'),path=require('node:path'),cp=require('node:child_process'),crypto=require('node:crypto');
const root=path.resolve(__dirname,'../../..'),ref=path.join(root,'integration/tools/reference/infinity-bin-service'),receipt=path.join(ref,'receipt-test-receipt.json');
fs.writeFileSync(receipt,JSON.stringify({passed:false,state:'RUNNING'})+'\n');
const requireThat=(ok,message)=>{if(!ok)throw Error(message);},sha=x=>crypto.createHash('sha256').update(x).digest('hex');
try{
 const frozen=['integration/test/contracts/infinity-bin-service.json','chain-parser-go/test/contracts/infinity-bin-service.json','dex-router/test/contracts/infinity-bin-service.json','integration/tools/reference/infinity-bin-service/execution-receipt.json'];
 const inputs=Object.fromEntries(frozen.map(p=>[p,sha(fs.readFileSync(path.join(root,p)))]));const cases=[];
 for(const mode of ['compiler-load','compiler-binary','manifest-read','source-read','source-changed','loaded-dependency']){
  const target=path.join(ref,'fault-'+mode+'-receipt.json');fs.writeFileSync(target,JSON.stringify({passed:true,stale:true})+'\n');
  const code=`const fs=require('node:fs'),Module=require('node:module'),path=require('node:path');
const read=fs.readFileSync,write=fs.writeFileSync,load=Module._load;
const mode=${JSON.stringify(mode)},target=${JSON.stringify(target)},original=${JSON.stringify(path.join(ref,'execution-receipt.json'))};
fs.writeFileSync=function(file,...args){return write.call(this,String(file)===original?target:file,...args);};
fs.readFileSync=function(file,...args){const name=String(file);
 if(mode==='manifest-read'&&name.endsWith('/reference/singleton-cl/source-metadata.json'))throw Error('injected manifest failure');
 if(mode==='source-read'&&name.endsWith('/sources/infinity/src/pool-bin/BinPoolManager.sol'))throw Error('injected source read failure');
 if(mode==='source-changed'&&name.endsWith('/sources/infinity/src/pool-bin/BinPoolManager.sol'))return Buffer.from('contract Forged {}');
 if(mode==='compiler-binary'&&name.endsWith('/solc/soljson.js'))throw Error('injected compiler binary failure');
 return read.call(this,file,...args);
};
Module._load=function(request,...args){if(mode==='compiler-load'&&String(request).endsWith('/node_modules/solc'))throw Error('injected compiler load failure');if(mode==='loaded-dependency'&&String(request)==='js-sha3')throw Error('injected dependency failure');return load.call(this,request,...args);};
require(${JSON.stringify(path.join(__dirname,'infinity-bin-service-golden.cjs'))});`;
  const result=cp.spawnSync(process.execPath,['-e',code],{encoding:'utf8',timeout:20000});requireThat(result.status!==0,'fault unexpectedly succeeded '+mode);requireThat(!result.error,'fault test child error '+mode+': '+result.error);
  const failed=JSON.parse(fs.readFileSync(target));requireThat(failed.passed===false&&!failed.stale,'stale success retained before fallible work '+mode);cases.push({mode,rejected:true,receipt_invalidated:true});
 }
 for(const [p,h]of Object.entries(inputs))requireThat(sha(fs.readFileSync(path.join(root,p)))===h,'frozen input changed '+p);
 inputs[path.relative(root,__filename)]=sha(fs.readFileSync(__filename));
 const result={passed:true,cases,inputs};fs.writeFileSync(receipt,JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify({passed:true,receipt_faults:cases.length,frozen_fixture_unchanged:true}));
}catch(e){fs.writeFileSync(receipt,JSON.stringify({passed:false,error:String(e),stack:e.stack},null,2)+'\n');console.error(e);process.exitCode=1;}
