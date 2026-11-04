'use strict';
const fs=require('node:fs'),path=require('node:path');
const ref=path.resolve(__dirname,'../reference/infinity-bin-detached'),receiptPath=path.join(ref,'guard-node-receipt.json');fs.writeFileSync(receiptPath,JSON.stringify({passed:false,state:'RUNNING'})+'\n');
const {root,sha,requireThat}=require('./infinity-bin-detached-lib.cjs');
try{
 const {verify,validateDocument,validateBindings}=require('./infinity-bin-detached-check.cjs');verify();const f=JSON.parse(fs.readFileSync(path.join(root,'integration/test/contracts/infinity-bin-detached.json'))),execution=JSON.parse(fs.readFileSync(path.join(ref,'execution-receipt.json'))),cases=[];
 function negative(name,edit,fn=validateDocument,source=f){const changed=structuredClone(source);edit(changed);let error;try{fn(changed);}catch(e){error=String(e);}requireThat(error,'accepted negative '+name);cases.push({name,rejected:true,error});}
 const A='8358608';
 negative('detached omitted after burn',d=>{d.cases[0].burn.expected.post.detached_active=null;});
 negative('ghost added to real bins',d=>{const s=d.cases[0].burn.expected.post;s.bins[A]=s.raw_bins[A];});
 negative('removed raw membership restored',d=>{d.cases[0].burn.expected.post.raw_bins[A].in_tree=true;});
 negative('ghost raw evidence missing',d=>{delete d.cases[0].burn.expected.post.raw_bins[A];});
 negative('active incorrectly changed by burn',d=>{d.cases[0].burn.expected.post.active_id--;});
 negative('dust rounded away',d=>{d.cases[0].burn.expected.post.raw_bins[A].reserve0='0';});
 negative('wrong minimum share',d=>{d.cases[0].burn.expected.post.raw_bins[A].total_shares='0';});
 negative('position not burned',d=>{d.cases[0].burn.expected.post.raw_bins[A].position_share='1';});
 negative('wrong original position key',d=>{d.cases[0].burn.expected.post.raw_bins[A].position_key='0x'+'0'.repeat(64);});
 negative('burn shares off by one',d=>{d.cases[0].burn.shares=(BigInt(d.cases[0].burn.shares)-1n).toString();});
 negative('burn delta tampered',d=>{d.cases[0].burn.expected.delta0='1';});
 negative('burn packed output tampered',d=>{d.cases[0].burn.expected.amounts='0x'+'0'.repeat(64);});
 negative('empty root not removed',d=>{d.cases[0].burn.expected.post.tree.level0='0x'+'1'.padStart(64,'0');});
 negative('L1 missing',d=>{const s=d.cases[0].burn.expected.post;delete s.tree.level1[Object.keys(s.tree.level1)[0]];});
 negative('L2 missing',d=>{const s=d.cases[0].burn.expected.post;delete s.tree.level2[Object.keys(s.tree.level2)[0]];});
 negative('raw share slot missing',d=>{const s=d.cases[0].burn.expected.post;delete s.raw_storage[Object.keys(s.raw_storage).find(k=>s.raw_storage[k]==='0x'+'3e8'.padStart(64,'0'))];});
 negative('next traverses removed active',d=>{const c=d.cases.find(c=>c.name.includes('real-tree'));c.burn.expected.post.raw_bins['8358351'].next.false=8358608;});
 negative('after crossing optional ghost restored',d=>{const c=d.cases.find(c=>c.name.includes('real-tree')),s=c.trades[0].expected.post;s.detached_active=s.raw_bins[A];});
 negative('reverse active returns to ghost',d=>{const c=d.cases.find(c=>c.name.includes('real-tree'));c.trades[1].expected.post.active_id=8358608;});
 negative('local loop consumes ghost raw reserves',d=>{const c=d.cases.find(c=>c.name.includes('real-tree'));c.trades[2].expected.post.raw_bins[A].reserve0='0';});
 negative('native exact input altered',d=>{d.cases[0].trades[0].amount_in='1';});
 negative('native output altered',d=>{d.cases[0].trades[0].expected.amount_out='2';});
 negative('native protocol altered',d=>{d.cases[0].trades[0].expected.protocol_amount='999999';});
 negative('native fee altered',d=>{d.cases[0].trades[0].expected.swap_fee=1;});
 negative('empty root exhaustion pretends success',d=>{d.cases[0].trades[1].expected.native_reverted=false;});
 negative('revert evidence missing',d=>{d.cases[0].trades[1].expected.revert_data='0x';});
 negative('revert state not atomic',d=>{d.cases[0].trades[1].expected.post.raw_bins[A].reserve1='5';});
 negative('local state chain broken',d=>{d.cases[0].trades[1].state.active_id=1;});
 negative('domain step altered',d=>{d.cases[0].seed.bin_step=0;});
 negative('protocol fee direction exceeds4000',d=>{d.cases[0].seed.protocol_fee=4001;});
 negative('settlement claimed',d=>{d.reference.scope.full_manager_swap_settlement_verified=true;});
 negative('MINIMUM_SHARE claimed different',d=>{d.reference.minimum_share='1';});
 for(const field of ['passed','fixture_sha256','source_metadata_sha256','artifact_sha256'])negative('receipt '+field,r=>{r[field]=field==='passed'?false:'0'.repeat(64);},r=>verify({receipt:r}),execution);
 for(const [name,edit]of [
  ['compiler identity',r=>{r.compiler='wrong';}],
  ['unlimited size',r=>{r.unlimited_contract_size=true;}],
  ['source pin',r=>{r.files[0].sha256='0'.repeat(64);}],
  ['source closure omitted',r=>{r.files=[];}],
  ['compile input',r=>{r.compile_input_sha256='0'.repeat(64);}],
  ['generator input',r=>{r.inputs[Object.keys(r.inputs)[0]]='0'.repeat(64);}],
  ['dependency lock',r=>{r.dependency_lock_sha256='0'.repeat(64);}],
  ['compiler package',r=>{r.dependencies.solc.sha256='0'.repeat(64);}],
  ['loaded dependency',r=>{r.loaded_dependencies[0].sha256='0'.repeat(64);}],
  ['loaded dependencies omitted',r=>{r.loaded_dependencies=[];}],
  ['compiler binary',r=>{r.binary_dependencies[0].sha256='0'.repeat(64);}],
  ['Node binary',r=>{r.binary_dependencies[1].sha256='0'.repeat(64);}],
  ['new artifact',r=>{r.artifact_sha256='0'.repeat(64);}],
  ['original manager source max references',r=>{r.manager_source_proof.references=[];}],
  ['frozen service binding',r=>{r.frozen_service.sha256='0'.repeat(64);}]
 ])negative(name,edit,validateBindings,f.reference);
 const inputs=Object.fromEntries([__filename,path.join(__dirname,'infinity-bin-detached-check.cjs'),path.join(__dirname,'infinity-bin-detached-lib.cjs'),path.join(ref,'execution-receipt.json')].map(p=>[path.relative(root,p),sha(fs.readFileSync(p))]));
 const result={passed:true,negative_cases:cases.length,cases,inputs};fs.writeFileSync(receiptPath,JSON.stringify(result,null,2)+'\n');console.log(JSON.stringify({passed:true,negative_cases:cases.length}));
}catch(e){fs.writeFileSync(receiptPath,JSON.stringify({passed:false,error:String(e),stack:e.stack},null,2)+'\n');console.error(e);process.exitCode=1;}
