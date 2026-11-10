#!/usr/bin/env node
'use strict';
// Bounded exact-source candidates; never accept executable-prefix equivalence.
const fs=require('node:fs'),path=require('node:path'),h=require('./r01_public_catalog_evm_sources.cjs');
const ROOT=path.resolve(__dirname,'../../..'),BASE=path.join(ROOT,'integration/test/source/deployment-catalog/evm-continuation-v2/source-inputs'),record=JSON.parse(fs.readFileSync(path.join(BASE,'base-aerodrome-v2-pool-source.json'))),raw=record.stdJsonInput.sources['Pool.sol'].content,actual=record.runtimeBytecode.onchainBytecode;
let found=null;const attempts=[];
for(const filename of ['Pool.sol','contracts/Pool.sol','contracts/pool.sol']){if(found)break;for(const newline of ['LF','CRLF']){
 const input=structuredClone(record.stdJsonInput);input.sources={[filename]:{content:raw.replace(/\r\n/g,'\n').replace(/\n/g,newline==='CRLF'?'\r\n':'\n')}};
 const result=h.compileExact(input,record.compilation.compilerVersion,filename+':Pool',actual);attempts.push({filename,newline,passed:result.source_rebuild_verified,failures:result.failed_candidates});if(result.source_rebuild_verified){found={input,result,filename,newline};break;}
}}
const result={matched:!!found,attempts};if(found){const directory=path.join(BASE,'aero-pool-exact');fs.mkdirSync(directory,{recursive:true});fs.writeFileSync(path.join(directory,'standard-input.json'),JSON.stringify(found.input,null,2)+'\n',{flag:'wx'});fs.writeFileSync(path.join(directory,'rebuild-report.json'),JSON.stringify(found.result,null,2)+'\n',{flag:'wx'});result.selection={filename:found.filename,newline:found.newline};}
fs.writeFileSync(path.join(BASE,'aero-pool-candidates.json'),JSON.stringify(result,null,2)+'\n',{flag:'wx'});console.log(JSON.stringify(result,null,2));
