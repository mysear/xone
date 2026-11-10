#!/usr/bin/env node
'use strict';
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),assert=require('node:assert/strict');
const I=require('../reference/balancer-managed-v2-breaker/inputs.cjs');
const {ROOT,sha}=I,FILENAME='managed-v2-breaker.json';
const defaults=['integration','chain-parser-go','chain-parser-rust','dex-router'].map(p=>path.join(ROOT,p,'test/contracts',FILENAME));
function args(argv){
 const a=argv.flatMap(s=>/^--(?:input|output)=/.test(s)?[s.slice(0,s.indexOf('=')),s.slice(s.indexOf('=')+1)]:[s]);
 if(a.length===0)return {mode:'generate',targets:defaults};
 if(a.length===2&&a[0]==='--output'){assert(path.isAbsolute(a[1]),'output must be absolute');return {mode:'generate',targets:[a[1]]};}
 if(a.length===1&&['--check','--validate'].includes(a[0]))return {mode:a[0].slice(2),targets:defaults};
 if(a.length===3&&['--check','--validate'].includes(a[0])&&a[1]==='--input')return {mode:a[0].slice(2),targets:[path.resolve(a[2])]};
 throw Error('usage: [--output ABSOLUTE_TMP_JSON | --validate [--input PATH] | --check [--input PATH]]');
}
function output(p){
 assert(path.isAbsolute(p)&&path.resolve(p)===p,'absolute canonical output');
 const parent=fs.realpathSync(path.dirname(p)),actual=path.join(parent,path.basename(p));
 const roots=[os.tmpdir(),'/tmp'].map(p=>fs.realpathSync(p));
 const temporary=roots.some(root=>actual.startsWith(root+path.sep))&&!actual.startsWith(ROOT+path.sep);
 const protectedNames=new Set(defaults.flatMap(p=>fs.readdirSync(path.dirname(p))).filter(n=>n!==FILENAME));
 assert(defaults.includes(actual)||(temporary&&actual.endsWith('.json')&&!protectedNames.has(path.basename(p))), 'new breaker output boundary');
 if(fs.existsSync(p)){const st=fs.lstatSync(p);assert(st.isFile()&&!st.isSymbolicLink()&&st.nlink===1,'output alias/nonfile refused');}
 return p;
}
async function main(argv=process.argv.slice(2)){
 const a=args(argv);I.closure();
 if(a.mode==='generate')for(const p of a.targets)output(p);
 const before=a.mode==='generate'?[]:a.targets.map(p=>({p,bytes:fs.readFileSync(p),mtime:fs.statSync(p,{bigint:true}).mtimeNs}));
 let bank,bytes;
 if(a.mode==='validate'){
  assert(before.every(s=>s.bytes.equals(before[0].bytes)),'independent raw copies differ');
  bytes=before[0].bytes;bank=JSON.parse(bytes);
  require('../reference/balancer-managed-v2-breaker/checker.cjs').check(bank);
 }else{
  bank=await require('../reference/balancer-managed-v2-breaker/generate.cjs').generate();
  bytes=Buffer.from(JSON.stringify(bank,null,2)+'\n');
  if(a.mode==='check')for(const s of before)assert(s.bytes.equals(bytes),'fresh original breaker VM differs');
  else for(const p of a.targets){const target=output(p),fd=fs.openSync(target,fs.constants.O_WRONLY|fs.constants.O_CREAT|fs.constants.O_TRUNC|fs.constants.O_NOFOLLOW,0o644);try{fs.writeFileSync(fd,bytes);}finally{fs.closeSync(fd);}}
 }
 for(const s of before){assert(fs.readFileSync(s.p).equals(s.bytes),'input bytes changed');assert.equal(fs.statSync(s.p,{bigint:true}).mtimeNs,s.mtime,'input mtime changed');}
 const result={mode:a.mode,bytes:bytes.length,sha256:sha(bytes),banks:bank.banks.length,cases:bank.cases.length};
 console.log(JSON.stringify(result));return result;
}
module.exports={args,defaults,output,main};
if(require.main===module)main().catch(e=>{console.error(e.stack);process.exitCode=1;});
