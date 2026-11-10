'use strict';
const fs=require('node:fs'),path=require('node:path'),crypto=require('node:crypto');
const ROOT=path.resolve(__dirname,'../../../..'),REF=path.join(ROOT,'integration/tools/reference/balancer-managed-v2-aum');
const E=require(REF+'/environment.cjs'),originalA=require(REF+'/artifacts.cjs'),V=require(REF+'/validate.cjs');
const sha=b=>crypto.createHash('sha256').update(b).digest('hex');
let loaded,validatedOriginal;
function original(){return loaded||(loaded=originalA.load());}
const A={...originalA,load:original};
function originalBank(){if(!validatedOriginal){const bytes=fs.readFileSync(ROOT+'/integration/test/contracts/managed-v2-aum.json');if(bytes.length!==7905624||sha(bytes)!=='1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66')throw Error('original bank immutable pin');validatedOriginal=V.validate(JSON.parse(bytes));}return validatedOriginal;}
function sourceText(name){const s=original().builds.ManagedPoolFactory.input.sources[name];if(!s)throw Error('unknown fixed SOURCE_NAME '+name);return s.content;}
function sourceManifest(){return Object.entries(original().builds.ManagedPoolFactory.input.sources).map(([path,s])=>({path,sha256:sha(s.content),bytes:Buffer.byteLength(s.content)}));}
function inputClosure(){return [...E.verifyInputs().files,...E.nodeClosure().files].map(f=>({path:f.path,sha256:f.sha256}));}
function scriptMetadata(dir){return {filename:path.relative(ROOT,dir+'/generate.cjs'),sha256:sha(fs.readFileSync(dir+'/generate.cjs')),checker_sha256:sha(fs.readFileSync(dir+'/checker.cjs')),layout_sha256:sha(fs.readFileSync(__dirname+'/layout.cjs')),node_version:process.version,node_sha256:sha(fs.readFileSync(process.execPath))};}
function files(){const rows=[];function walk(d){for(const e of fs.readdirSync(d,{withFileTypes:true})){const p=path.join(d,e.name);if(e.isSymbolicLink())throw Error('scheduled source symlink');if(e.isDirectory())walk(p);else if(p!==__dirname+'/file-manifest.json')rows.push({path:path.relative(ROOT,p),sha256:sha(fs.readFileSync(p))});}}walk(__dirname);for(const p of ['integration/tools/ci/managed-v2-scheduled-golden.cjs','integration/tools/acceptance/tests/test_managed_v2_scheduled_reference.py'])rows.push({path:p,sha256:sha(fs.readFileSync(path.join(ROOT,p)))});return rows.sort((a,b)=>a.path.localeCompare(b.path));}
function closure(){E.verifyInputs();const node=E.nodeClosure(),p=__dirname+'/file-manifest.json',bytes=fs.readFileSync(p),manifest=JSON.parse(bytes);if(V.canonical(files())!==V.canonical(manifest.files))throw Error('exact scheduled producer/checker input closure');return {manifest_sha256:sha(bytes),files:manifest.files,original:E.verifyInputs(),node,original_bank:{path:'integration/test/contracts/managed-v2-aum.json',bytes:7905624,sha256:'1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66'}};}
module.exports={originalBank,ROOT,REF,E,A,V,sha,inputClosure,sourceManifest,sourceText,scriptMetadata,closure,files};
