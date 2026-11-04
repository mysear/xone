'use strict';
const fs=require('node:fs'),path=require('node:path'),cp=require('node:child_process');
const ref=path.resolve(__dirname,'../reference/infinity-bin-detached'),receiptPath=path.join(ref,'receipt-test-receipt.json');fs.writeFileSync(receiptPath,JSON.stringify({passed:false,state:'RUNNING'})+'\n');
const {root,sha,requireThat,frozenService}=require('./infinity-bin-detached-lib.cjs');
try{
 const frozen=frozenService(),paths=['integration/test/contracts/infinity-bin-detached.json','chain-parser-go/test/contracts/infinity-bin-detached.json','dex-router/test/contracts/infinity-bin-detached.json','integration/tools/reference/infinity-bin-detached/execution-receipt.json'];const inputs=Object.fromEntries(paths.map(p=>[p,sha(fs.readFileSync(path.join(root,p)))])),cases=[];
 for(const mode of ['compiler-loader','compiler-binary','source-read','source-hash','source-manifest']){
  const target=path.join(ref,'fault-'+mode+'-receipt.json');fs.writeFileSync(target,JSON.stringify({passed:true,stale:true})+'\n');
  const code=`const fs=require('node:fs'),Module=require('node:module');const read=fs.readFileSync,write=fs.writeFileSync,load=Module._load,mode=${JSON.stringify(mode)},receipt=${JSON.stringify(path.join(ref,'execution-receipt.json'))},target=${JSON.stringify(target)};
fs.writeFileSync=function(p,...args){return write.call(this,String(p)===receipt?target:p,...args);};
fs.readFileSync=function(p,...args){const n=String(p);
if(mode==='compiler-binary'&&n.endsWith('/solc/soljson.js'))throw Error('injected compiler binary');
if(mode==='source-read'&&n.endsWith('/sources/infinity/src/pool-bin/libraries/BinPool.sol'))throw Error('injected source read');
if(mode==='source-hash'&&n.endsWith('/sources/infinity/src/pool-bin/libraries/BinPool.sol'))return Buffer.from('forged source');
if(mode==='source-manifest'&&n.endsWith('/reference/singleton-cl/source-metadata.json'))throw Error('injected source manifest');
return read.call(this,p,...args);};
Module._load=function(name,...args){if(mode==='compiler-loader'&&String(name).endsWith('/node_modules/solc'))throw Error('injected compiler loader');return load.call(this,name,...args);};
require(${JSON.stringify(path.join(__dirname,'infinity-bin-detached-golden.cjs'))});`;
  const child=cp.spawnSync(process.execPath,['-e',code],{encoding:'utf8',timeout:20000});requireThat(!child.error&&child.status!==0,'injected failure did not exit '+mode);const r=JSON.parse(fs.readFileSync(target));requireThat(r.passed===false&&!r.stale,'stale receipt retained '+mode);cases.push({mode,failed:true,stale_receipt_invalidated:true});
 }
 for(const [p,h]of Object.entries(inputs))requireThat(sha(fs.readFileSync(path.join(root,p)))===h,'detached frozen input changed '+p);frozenService();inputs[path.relative(root,__filename)]=sha(fs.readFileSync(__filename));
 fs.writeFileSync(receiptPath,JSON.stringify({passed:true,cases,inputs,frozen_service_manifest_sha256:frozen.sha256},null,2)+'\n');console.log(JSON.stringify({passed:true,receipt_faults:cases.length,frozen_service_unchanged:true,detached_fixture_unchanged:true}));
}catch(e){fs.writeFileSync(receiptPath,JSON.stringify({passed:false,error:String(e),stack:e.stack},null,2)+'\n');console.error(e);process.exitCode=1;}
