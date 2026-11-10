'use strict';
const fs=require('node:fs'),path=require('node:path');
const S=require('../balancer-managed-v2-scheduled-shared/inputs.cjs');
const {ROOT,sha,V}=S,DIR=__dirname;
function files(){
 const rows=[];
 function walk(dir){for(const e of fs.readdirSync(dir,{withFileTypes:true})){
  const p=path.join(dir,e.name);if(e.isSymbolicLink())throw Error('BPT source symlink');
  if(e.isDirectory())walk(p);else if(p!==path.join(DIR,'file-manifest.json'))rows.push({path:path.relative(ROOT,p),sha256:sha(fs.readFileSync(p))});
 }}
 walk(DIR);
 for(const p of ['integration/tools/ci/managed-v2-bpt-golden.cjs','integration/tools/acceptance/tests/test_managed_v2_bpt_reference.py'])
  rows.push({path:p,sha256:sha(fs.readFileSync(path.join(ROOT,p)))});
 return rows.sort((a,b)=>a.path.localeCompare(b.path));
}
function closure(){
 const original=S.closure(),bytes=fs.readFileSync(path.join(DIR,'file-manifest.json')),manifest=JSON.parse(bytes);
 if(V.canonical(files())!==V.canonical(manifest.files))throw Error('exact BPT producer/checker inputs');
 return {manifest_sha256:sha(bytes),files:manifest.files,original};
}
module.exports={...S,DIR,files,closure};
