'use strict';
const fs=require('node:fs'),path=require('node:path');
const B=require('../balancer-managed-v2-bpt/inputs.cjs');
const S=require('../balancer-managed-v2-scheduled/inputs.cjs');
const {ROOT,sha,V}=B,DIR=__dirname;
const BPT_FILE='integration/test/contracts/managed-v2-bpt.json';
const BPT_SHA256='4cbf03d19a773ba374d8df87e4e3231798f042f576addeb5532300af0bb892f8';
function files(){
 const rows=[];
 function walk(dir){for(const entry of fs.readdirSync(dir,{withFileTypes:true})){
  const p=path.join(dir,entry.name);if(entry.isSymbolicLink())throw Error('breaker source symlink');
  if(entry.isDirectory())walk(p);else if(p!==path.join(DIR,'file-manifest.json'))rows.push({path:path.relative(ROOT,p),sha256:sha(fs.readFileSync(p))});
 }}walk(DIR);
 for(const p of ['integration/tools/ci/managed-v2-breaker-golden.cjs','integration/tools/acceptance/tests/test_managed_v2_breaker_reference.py'])rows.push({path:p,sha256:sha(fs.readFileSync(path.join(ROOT,p)))});
 return rows.sort((a,b)=>a.path.localeCompare(b.path));
}
function bptIdentity(){const bytes=fs.readFileSync(path.join(ROOT,BPT_FILE));if(bytes.length!==51222289||sha(bytes)!==BPT_SHA256)throw Error('fixed original BPT source bytes');return {path:BPT_FILE,bytes:bytes.length,sha256:BPT_SHA256};}
function closure(){
 const original=B.closure(),compiler=S.closure(),bytes=fs.readFileSync(path.join(DIR,'file-manifest.json')),manifest=JSON.parse(bytes);
 if(V.canonical(files())!==V.canonical(manifest.files))throw Error('exact breaker producer/checker input closure');
 return {manifest_sha256:sha(bytes),files:manifest.files,original,compiler,original_bpt:bptIdentity()};
}
let qualifiedBpt;
function originalBpt(){if(!qualifiedBpt){bptIdentity();qualifiedBpt=JSON.parse(fs.readFileSync(path.join(ROOT,BPT_FILE)));require('../balancer-managed-v2-bpt/checker.cjs').check(qualifiedBpt);}return qualifiedBpt;}
module.exports={...B,DIR,files,closure,originalBpt,BPT_SHA256};
