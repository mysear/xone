#!/usr/bin/env node
'use strict';
const fs=require('node:fs'),path=require('node:path'),os=require('node:os'),assert=require('node:assert/strict');
const S=require('../reference/balancer-managed-v2-scheduled/inputs.cjs'),{ROOT,sha}=S;
const defaults=['integration','chain-parser-go','chain-parser-rust','dex-router'].map(p=>path.join(ROOT,p,'test/contracts/managed-v2-scheduled.json'));
function args(argv){
 const a=argv.flatMap(s=>/^--(?:input|output)=/.test(s)?[s.slice(0,s.indexOf('=')),s.slice(s.indexOf('=')+1)]:[s]);
 if(a.length===0)return {mode:'generate',targets:defaults};
 if(a.length===2&&a[0]==='--output'){assert(path.isAbsolute(a[1]),'output must be absolute');return {mode:'generate',targets:[a[1]]};}
 if(a.length===1&&['--check','--validate'].includes(a[0]))return {mode:a[0].slice(2),targets:defaults};
 if(a.length===3&&['--check','--validate'].includes(a[0])&&a[1]==='--input')return {mode:a[0].slice(2),targets:[path.resolve(a[2])]};
 throw Error('usage: [--output ABSOLUTE_TMP_PATH | --validate [--input PATH] | --check [--input PATH]]');
}
function output(p){
 assert(path.isAbsolute(p)&&path.resolve(p)===p,'output absolute canonical path');
 const parent=fs.realpathSync(path.dirname(p)),actual=path.join(parent,path.basename(p));
 const roots=[os.tmpdir(),'/tmp'];
 const temporary=roots.some(root=>{const canonical=fs.realpathSync(root);return (p.startsWith(root+path.sep)&&actual===canonical+p.slice(root.length))||p.startsWith(canonical+path.sep)&&p===actual;});
 const protectedNames=new Set(defaults.flatMap(p=>fs.readdirSync(path.dirname(p))).filter(n=>n!=='managed-v2-scheduled.json'));
 assert(defaults.includes(actual)||(temporary&&!actual.startsWith(ROOT+path.sep)&&actual.endsWith('.json')&&!protectedNames.has(path.basename(p))&&path.basename(p)!=='bank.json'),'output boundary');
 assert(!actual.includes('/reference/')&&!actual.includes('/proto/'),'reference/proto output forbidden');
 if(fs.existsSync(p)){const st=fs.lstatSync(p);assert(st.isFile()&&!st.isSymbolicLink()&&st.nlink===1,'output symlink/hardlink/nonfile');}
 return p;
}
async function main(argv=process.argv.slice(2)){
 const a=args(argv);S.closure();
 if(a.mode==='generate')for(const p of a.targets)output(p);
 const before=a.mode!=='generate'?a.targets.map(p=>({p,bytes:fs.readFileSync(p),mtime:fs.statSync(p,{bigint:true}).mtimeNs})):[];
 let summary,bytes;
 if(a.mode==='validate'){
  const {check}=require('../reference/balancer-managed-v2-scheduled/checker.cjs');
  for(const s of before)assert(s.bytes.equals(before[0].bytes),'independent raw copies differ');
  summary=check(JSON.parse(before[0].bytes));
  bytes=before[0].bytes;
 }else{
  const value=await require('../reference/balancer-managed-v2-scheduled/generate.cjs').generate();bytes=Buffer.from(JSON.stringify(value,null,2)+'\n');
  summary={swaps:value.cases.length,checkpoints:value.checkpoints.length};
  if(a.mode==='check'){
   const dir=fs.mkdtempSync(path.join(os.tmpdir(),'xone-scheduled-check-'));
   try{const file=path.join(dir,'managed-v2-scheduled.json');fs.writeFileSync(file,bytes);const fresh=fs.readFileSync(file);for(const s of before)assert(s.bytes.equals(fresh),'fresh original VM byteexact mismatch: '+s.p);}finally{fs.rmSync(dir,{recursive:true,force:true});}
  }else{
   for(const p of a.targets){output(p);const fd=fs.openSync(p,fs.constants.O_WRONLY|fs.constants.O_CREAT|fs.constants.O_TRUNC|fs.constants.O_NOFOLLOW,0o644);try{fs.writeFileSync(fd,bytes);}finally{fs.closeSync(fd);}}
   const inos=a.targets.map(p=>fs.statSync(p).ino);assert.equal(new Set(inos).size,inos.length,'four independent output inodes');
  }
 }
 for(const s of before){assert.equal(fs.statSync(s.p,{bigint:true}).mtimeNs,s.mtime,'readonly mtime');assert(fs.readFileSync(s.p).equals(s.bytes),'readonly bytes');}
 console.log(JSON.stringify({mode:a.mode,...summary,bytes:bytes.length,sha256:sha(bytes),outputs:a.targets.map(p=>path.relative(ROOT,p))}));
}
module.exports={main,args,output,defaults};
if(require.main===module)main().catch(e=>{console.error(e.stack);process.exitCode=1;});
