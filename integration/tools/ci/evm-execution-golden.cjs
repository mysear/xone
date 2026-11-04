#!/usr/bin/env node
'use strict';
// Invalidate prior success BEFORE loading the generator, dependencies, or source pins.
const fs=require('node:fs');
const path=require('node:path');
const ROOT=path.resolve(__dirname,'../../..');
const defaults=['integration/test/contracts/evm-execution.json','dex-router/test/contracts/evm-execution.json'].map(p=>path.join(ROOT,p));
const args=process.argv.slice(2);
const checking=args[0]==='--check';
const output=args[0]==='--output'?args[1]:null;
const targets=output?[path.resolve(output)]:defaults;
if(!checking)for(const p of targets)fs.rmSync(p,{force:true});
async function main() {
  if(args.length&&!checking&&!output)throw Error('usage: evm-execution-golden.cjs [--output PATH | --check PATH]');
  const {generate,json}=require('../reference/evm-execution/generate.cjs');
  const {validate}=require('../reference/evm-execution/validate.cjs');
  const assert=require('node:assert/strict');
  let existing;
  if(checking) {existing=JSON.parse(fs.readFileSync(args[1],'utf8'));validate(existing);}
  const result=validate(await generate());
  if(checking) {assert.deepEqual(existing,result,'exact original artifact regeneration mismatch');assert.equal(fs.readFileSync(args[1],'utf8'),json(result),'noncanonical fixture bytes');process.stdout.write('10 official-bytecode cases: exact regeneration passed\n');}
  else {
    const data=json(result);
    // All capture/validation succeeds before either success artifact is published.
    try {for(const p of targets)fs.writeFileSync(p+'.pending',data);for(const p of targets)fs.renameSync(p+'.pending',p);}
    catch(e) {for(const p of targets){fs.rmSync(p,{force:true});fs.rmSync(p+'.pending',{force:true});}throw e;}
    process.stdout.write(`10 official-bytecode cases written (${Buffer.byteLength(data)} bytes per copy)\n`);
  }
}
main().catch(e=>{console.error(e);process.exitCode=1;});
