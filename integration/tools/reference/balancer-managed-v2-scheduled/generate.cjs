'use strict';
const {closure,sourceManifest,sourceText,A}=require('./inputs.cjs');
const PROFILE='balancer-v2/20230411-managed-scheduled-1';
function index(branches){
 const cases=[],checkpoints=[],management=[];
 for(const [namespace,b]of Object.entries(branches)){
  const selected=namespace==='reschedule'?b.cases.filter(c=>b.reschedules.some(r=>r.cases.includes(c.id))):b.cases;
  const txIndex=t=>{const n=b.transactions.findIndex(x=>x.transaction.hash===t.transaction.hash&&x.execution_header.hash===t.execution_header.hash&&x.purpose===t.purpose);if(n<0)throw Error('missing original transaction record');return n;};
  for(const c of selected)cases.push({id:namespace+'/'+c.id,namespace,atomic_case:c.id,before:namespace+'/'+c.before,after:namespace+'/'+c.after,transaction:namespace+'/'+(namespace==='two'?'transactions/'+txIndex(c.execution):c.transaction),expected_raw_out:namespace==='two'?c.expected_output:c.expected_raw_out});
  const cps=namespace==='two'?b.scenarios.flatMap(s=>s.checkpoints):namespace==='reschedule'?b.checkpoints.filter(c=>b.reschedules.some(r=>r.checkpoints.some(p=>p.checkpoint===c.id))):b.checkpoints;
  for(const c of cps)checkpoints.push({id:namespace+'/'+(c.id||c.state),namespace,state:namespace+'/'+c.state,atomic_checkpoint:c.id||c.label});
  const txs=namespace==='two'?b.scenarios.flatMap(s=>s.management_transactions.map(m=>m.execution)):namespace==='reschedule'?b.transactions.filter(t=>b.reschedules.some(r=>r.transaction===t.id||r.invalid_user_end.transaction===t.id)):b.transactions.filter(t=>['updateWeightsGradually','updateSwapFeeGradually'].includes(t.method));
  for(const t of txs)management.push({id:namespace+'/'+(t.id||'transactions/'+txIndex(t)),namespace,method:t.method,transaction_hash:t.transaction.hash,status:t.receipt.status});
 }
 return {cases,checkpoints,management};
}
function provenance(){const {pins,builds}=A.load(),b=builds.ManagedPoolFactory;return {repository:pins.repository,commit:pins.commit,build:pins.builds.find(b=>b.name==='ManagedPoolFactory'),source_manifest:sourceManifest(),licenses:Object.keys(b.input.sources).map(p=>({source_name:p,spdx:sourceText(p).match(/SPDX-License-Identifier:\s*([^\r\n]+)/)?.[1]||null,license:b.output.sources[p].ast.license||null})),packed_layout:require('./layout.cjs').packedLayout(b),source_authority:'fixed original gzip build-info input.sources; no independently copied Solidity authority',prototype_origins:{two:'xone-managed-v2-scheduled-reference:17swaps64states21checkpoints4manager',three:'xone-managed-v2-three-schedule:28swaps69states10checkpoints',reschedule:'xone-managed-v2-reschedule-reference:setup28+8swaps;2valid+2receipt0'},scope:{token_counts:[2,3],other_token_counts:'unknown',rate_tokens:'unsupported',bpt_swaps:'unsupported',circuit_breaking:'unsupported',storage_trie_coverage:'unknown; selected slots and mapping keys only',legacy_aum3_active_schedule_support:false,domain_projection:'pending main-thread decoder and field40 model; captured raw getter schema retained'}};}
async function generate(){const inputs=closure(),two=await require('./two/generate.cjs').generate(),three=await require('./three/generate.cjs').generate(),reschedule=await require('./reschedule/generate.cjs').generate(three),branches={two,three,reschedule};
 const b={schema_version:1,authority:'independent-local-original-managed-schedule',source_public_profile:PROFILE,inputs,provenance:provenance(),branches,...index(branches)};
 require('./checker.cjs').check(b);return b;
}
module.exports={generate,index,provenance,PROFILE};
