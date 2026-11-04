#!/usr/bin/env node
'use strict';
const fs=require('node:fs'),path=require('node:path');
const ROOT=path.resolve(__dirname,'../../..'),REF=path.join(ROOT,'integration/tools/reference/balancer-managed-v2-aum'),verified=path.join(REF,'verified.json');
fs.rmSync(verified,{force:true}); // Before parsing, dependency loading and every failure.
const args=process.argv.slice(2),mode=args[0];
const defaults=['integration','chain-parser-go','dex-router'].map(s=>path.join(ROOT,s,'test/contracts/managed-v2-aum.json'));
const generating=args.length===0||(mode==='--output'&&args.length===2);
const targets=generating?(mode==='--output'?[path.resolve(args[1])]:defaults):[];
const staged=targets.map((_,i)=>path.join(REF,'output-'+i+'.pending'));
for(let i=0;i<3;i++)fs.rmSync(path.join(REF,'output-'+i+'.pending'),{force:true});
if(generating)for(const p of targets)fs.rmSync(p,{force:true});
(async()=>{
 if(mode==='--check'){
  if(args.length!==1)throw Error('--check takes no extra arguments');
  const r=require('node:child_process').spawnSync('python3',[path.join(__dirname,'managed-v2-aum-check.py'),'--check'],{stdio:'inherit'});
  if(r.status!==0)throw Error('exact original replay failed');return;
 }
 const {validate}=require('../reference/balancer-managed-v2-aum/validate.cjs');
 if(mode==='--validate'){
  if(args.length!==2)throw Error('validate requires input');
  validate(JSON.parse(fs.readFileSync(args[1])));console.log('original Managed V2 AUM bank validated');return;
 }
 if(!generating)throw Error('usage: [--output PATH | --validate PATH | --check]');
 const value=validate(await require('../reference/balancer-managed-v2-aum/generate.cjs').generate()),text=JSON.stringify(value,null,2)+'\n';
 try{for(const p of staged)fs.writeFileSync(p,text);for(let i=0;i<targets.length;i++)fs.renameSync(staged[i],targets[i]);}
 catch(e){for(const p of [...targets,...staged])fs.rmSync(p,{force:true});throw e;}
 fs.writeFileSync(path.join(REF,'generation.json'),JSON.stringify({schema_version:1,state:'GENERATED',passed:true,fixture_sha256:require('node:crypto').createHash('sha256').update(text).digest('hex'),cases:value.cases.length,transactions:value.reference.transactions.length,objects:Object.keys(value.objects).length})+'\n');
 console.log(value.cases.length+' original banks, '+value.reference.transactions.length+' transactions, '+Object.keys(value.objects).length+' objects, '+Buffer.byteLength(text)+' bytes');
})().catch(e=>{
 fs.rmSync(verified,{force:true});
 if(!['--validate','--check'].includes(mode)){
  for(const p of [...targets,...staged])fs.rmSync(p,{force:true});
  fs.writeFileSync(path.join(REF,'generation.json'),JSON.stringify({schema_version:1,passed:false,state:'FAILED'})+'\n');
 }
 console.error(e);process.exitCode=1;
});
