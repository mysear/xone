#!/usr/bin/env node
'use strict';
const fs=require('node:fs'),path=require('node:path');
const ROOT=path.resolve(__dirname,'../../..'),success=path.join(ROOT,'integration/tools/reference/v2-complete/verified.json');
fs.rmSync(success,{force:true}); // Entry failure invalidates previous success before dependencies load.
const args=process.argv.slice(2),mode=args[0];
const defaults=['integration','chain-parser-go','dex-router'].map(p=>path.join(ROOT,p,'test/contracts/v2-complete.json'));
const targets=mode==='--output'?[path.resolve(args[1])]:defaults;
if(mode!=='--validate'&&mode!=='--check')for(const p of targets)fs.rmSync(p,{force:true});
async function main(){
 const {validate}=require('../reference/v2-complete/validate.cjs');
 if(mode==='--validate'){validate(JSON.parse(fs.readFileSync(args[1])));console.log('24 complete banks: independent validator passed');return;}
 if(mode==='--check'){const {spawnSync}=require('node:child_process');const r=spawnSync('python3',[path.join(__dirname,'v2-complete-check.py'),'--check',...(args[1]?['--input',args[1]]:[])],{stdio:'inherit'});if(r.status!==0)throw Error('full regeneration failed');return;}
 if(args.length&&mode!=='--output')throw Error('usage: v2-complete-golden.cjs [--output PATH [--trace-dir PATH] | --validate PATH | --check [PATH]]');
 const traceDir=args.includes('--trace-dir')?path.resolve(args[args.indexOf('--trace-dir')+1]):path.join(ROOT,'integration/tools/reference/v2-complete/traces');
 const {generate}=require('../reference/v2-complete/generate.cjs');
 const result=validate(await generate({traceDir}),{traceDir}),data=JSON.stringify(result,null,2)+'\n';
 try{for(const p of targets)fs.writeFileSync(p+'.pending',data);for(const p of targets)fs.renameSync(p+'.pending',p);}catch(e){for(const p of targets){fs.rmSync(p,{force:true});fs.rmSync(p+'.pending',{force:true});}throw e;}
 console.log(`${result.cases.length} complete banks written; ${result.reference.success_count} success / ${result.reference.revert_count} revert; ${Buffer.byteLength(data)} bytes`);
}
main().catch(e=>{fs.rmSync(success,{force:true});console.error(e);process.exitCode=1;});
