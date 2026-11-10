"""BPT selected-plan parser fails before starting a VM on forged input."""
from pathlib import Path
import subprocess
import unittest
ROOT=Path(__file__).resolve().parents[4]
class BptPlanTests(unittest.TestCase):
    def test_exact_api_plan_input_and_source_boundaries(self):
        code=r'''
const fs=require('node:fs'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const P=require('./integration/tools/ci/managed-v2-bpt-plan-replay.cjs');
const bytes=fs.readFileSync('integration/test/contracts/managed-v2-bpt.json'),b=JSON.parse(bytes),digest=crypto.createHash('sha256').update(bytes).digest('hex'),c=b.cases[0],bank=b.banks.find(x=>x.id===c.before),branch=b.branches[c.namespace],h=bank.header;
const r={chainKey:'evm:31337',kind:'EXACT_IN',verification:'FAST',caller:c.actor,recipient:c.actor,sellToken:c.token_in,buyToken:c.token_out,sellAmount:c.amount_in,slippageBps:50,constraints:{maxHops:1}};
const q={snapshotRef:{block:{chain_key:'evm:31337',number_or_slot:Number(BigInt(h.number)),hash:h.hash,parent_hash:h.parentHash,timestamp_ms:Number(BigInt(h.timestamp))*1000}},consumedIn:c.amount_in,refund:'0',grossBuyAmount:c.expected_native.actual_amount_out,minBuyAmount:String(BigInt(c.expected_native.actual_amount_out)*9950n/10000n),verification:{level:'INDICATIVE',success:false},quality:{candidateScope:branch.pools.map(p=>p.pool),excludedPools:{}},routes:[{amount_in:c.amount_in,amount_out:c.expected_native.actual_amount_out,path:{amount_in:c.amount_in,amount_out:c.expected_native.actual_amount_out,tokens:[c.token_in,c.token_out],edges:[{pool_id:branch.pools[0].pool,protocol:'balancer-v2-managed-bpt',from_token:c.token_in,to:c.token_out}]}}]};
q.snapshotRef.online_generation=1;q.snapshotRef.scope_hash='s';q.onlineGeneration=1;q.scopeHash='s';
const input={schema_version:1,original_bpt_sha256:digest,quotes:[{id:'Q0',namespace:c.namespace,source_bank:c.before,request:r,quote:q}]};
assert.equal(P.validateInput(input,b,digest).length,1);
for(const mutate of [i=>i.original_bpt_sha256='0'.repeat(64),i=>i.quotes.push(i.quotes[0]),i=>i.quotes[0].quote.snapshotRef.block.hash='0x'+'0'.repeat(64),i=>i.quotes[0].request.caller='0x'+'1'.repeat(40),i=>i.quotes[0].quote.routes[0].path.edges[0].pool_id='0x'+'2'.repeat(40),i=>i.quotes[0].quote.routes[0].amount_in='01',i=>i.quotes[0].quote.minBuyAmount='0',i=>i.quotes[0].quote.verification.success=true,i=>i.quotes[0].request.sellAmount=String(1n<<256n)]){
 const copy=structuredClone(input);mutate(copy);assert.throws(()=>P.validateInput(copy,b,digest));
}
'''
        result=subprocess.run(['/usr/local/bin/node','-e',code],cwd=ROOT,capture_output=True,text=True)
        self.assertEqual(result.returncode,0,result.stderr[-4000:])

    def test_selected_after_bindings_and_input_links_reject_offline_forgery(self):
        code=r'''
const fs=require('node:fs'),assert=require('node:assert/strict'),crypto=require('node:crypto');
const P=require('./integration/tools/ci/managed-v2-bpt-plan-replay.cjs');
const bytes=fs.readFileSync('integration/test/contracts/managed-v2-bpt.json'),b=JSON.parse(bytes),digest=crypto.createHash('sha256').update(bytes).digest('hex'),c=b.cases[0],bank=b.banks.find(x=>x.id===c.before),branch=b.branches[c.namespace],h=bank.header;
const r={chainKey:'evm:31337',kind:'EXACT_IN',verification:'FAST',caller:c.actor,recipient:c.actor,sellToken:c.token_in,buyToken:c.token_out,sellAmount:c.amount_in,slippageBps:50,constraints:{maxHops:1}};
const q={snapshotRef:{block:{chain_key:'evm:31337',number_or_slot:Number(BigInt(h.number)),hash:h.hash,parent_hash:h.parentHash,timestamp_ms:Number(BigInt(h.timestamp))*1000}},consumedIn:c.amount_in,refund:'0',grossBuyAmount:c.expected_native.actual_amount_out,minBuyAmount:String(BigInt(c.expected_native.actual_amount_out)*9950n/10000n),verification:{level:'INDICATIVE',success:false},quality:{candidateScope:branch.pools.map(p=>p.pool),excludedPools:{}},routes:[{amount_in:c.amount_in,amount_out:c.expected_native.actual_amount_out,path:{amount_in:c.amount_in,amount_out:c.expected_native.actual_amount_out,tokens:[c.token_in,c.token_out],edges:[{pool_id:branch.pools[0].pool,protocol:'balancer-v2-managed-bpt',from_token:c.token_in,to:c.token_out}]}}]};
q.snapshotRef.online_generation=1;q.snapshotRef.scope_hash='s';q.onlineGeneration=1;q.scopeHash='s';
const input={schema_version:1,original_bpt_sha256:digest,quotes:[{id:'Q0',namespace:c.namespace,source_bank:c.before,request:r,quote:q}]};
const after=b.banks.find(x=>x.id===c.after),pool=branch.pools.find(p=>p.name===c.pool_name),tx=branch.transactions.find(t=>t.id===c.transaction);
const execution={id:'E0',namespace:c.namespace,source_bank:c.before,source:bank,request:r,quote:q,passed:true,source_restored:true,actual_amount_in:c.amount_in,actual_amount_out:c.expected_native.actual_amount_out,complete_native_after:after,routes:[{index:0,amount_in:c.amount_in,amount_out:c.expected_native.actual_amount_out,steps:[{index:0,pool_name:c.pool_name,token_in:c.token_in,token_out:c.token_out,amount_in:c.amount_in,amount_out:c.expected_native.actual_amount_out,before:bank,after,transaction:tx}]}]};
const report={schema_version:1,passed:true,original_bpt_sha256:digest,verified:false,production_ready:false,all_requested_features_complete:false,selected_input_sha256:require('./integration/tools/reference/balancer-managed-v2-bpt/inputs.cjs').sha(require('./integration/tools/reference/balancer-managed-v2-bpt/inputs.cjs').V.canonical(input)),selected_transactions_executed:1,executions:[execution],quote_links:[{id:'Q0',index:0,execution_id:'E0'}]};
assert.equal(P.validateReport(report,b,digest,input).selected_transactions,1);
const {ethers}=require('./integration/tools/reference/balancer-managed-v2-bpt/inputs.cjs').E.rr('ethers');
const I=require('./integration/tools/reference/balancer-managed-v2-bpt/inputs.cjs'),abi=new ethers.utils.Interface(I.A.locate(I.A.load().builds,'ManagedPoolFactory','ManagedPool').c.abi);
for(const mutation of [
 x=>{const e=x.executions[0],a=e.routes[0].steps[0].after;const state=a.pools.find(p=>p.name===c.pool_name).state;for(const g of state.getters.filter(g=>g.to===pool.pool&&g.method==='getOwner')){g.decoded=[branch.roles.trader];g.result=abi.encodeFunctionResult(g.method,g.decoded);}e.complete_native_after=structuredClone(a);},
 x=>{const e=x.executions[0],a=e.routes[0].steps[0].after;const slot=a.storage.find(s=>s.address===pool.pool&&s.key==='0x9');slot.value='0x'+(BigInt(slot.value)^(1n<<255n)).toString(16);e.complete_native_after=structuredClone(a);},
 x=>{const e=x.executions[0],a=e.routes[0].steps[0].after,state=a.pools.find(p=>p.name===c.pool_name).state,slot=a.storage.find(s=>s.address===pool.pool&&s.key==='0xa');slot.value='0x'+(BigInt(slot.value)^1n).toString(16);state.aum.raw_word=slot.value;e.complete_native_after=structuredClone(a);},
 x=>{const e=x.executions[0],a=e.routes[0].steps[0].after,state=a.pools.find(p=>p.name===c.pool_name).state,slot=a.storage.find(s=>s.address===pool.pool&&s.key==='0xa');slot.value='0x'+(BigInt(slot.value)^(1n<<60n)).toString(16);state.aum.raw_word=slot.value;e.complete_native_after=structuredClone(a);},
 x=>{const e=x.executions[0],a=e.routes[0].steps[0].after,state=a.pools.find(p=>p.name===c.pool_name).state,slot=a.storage.find(s=>s.address===pool.pool&&s.key==='0xa');slot.value='0x'+(BigInt(slot.value)|(1n<<92n)).toString(16);state.aum.raw_word=slot.value;e.complete_native_after=structuredClone(a);},
 x=>{x.quote_links[0].id='fabricated';x.quote_links[0].index=999999;},
 x=>{x.executions[0].quote.quoteId='fabricated';x.executions[0].quote.planHash='0'.repeat(64);},
 x=>{x.selected_input_sha256='0'.repeat(64);}
]){const copy=structuredClone(report);mutation(copy);assert.throws(()=>P.validateReport(copy,b,digest,input));}
assert.throws(()=>P.validateReport(report,b,digest),'actual input must be supplied independently');
'''
        result=subprocess.run(['/usr/local/bin/node','-e',code],cwd=ROOT,capture_output=True,text=True)
        self.assertEqual(result.returncode,0,result.stderr[-5000:])

    def test_aum_raw_decoder_preserves_recovery_effective_fee_semantics(self):
        code=r'''
const assert=require('node:assert/strict'),C=require('./integration/tools/reference/balancer-managed-v2-bpt/checker.cjs');
const fee=20000000000000000n,last=1767229200n,word=fee|(last<<60n);
C.validateAumRaw('0x'+word.toString(16),[String(fee),String(last)],false);
C.validateAumRaw('0x'+word.toString(16),['0',String(last)],true);
for(const [raw,params,recovery]of [[word^1n,[String(fee),String(last)],false],[word^(1n<<60n),[String(fee),String(last)],false],[word|(1n<<92n),['0',String(last)],true],[word,[String(fee),String(last)],true],[word,['0',String(last)],false]]){
 assert.throws(()=>C.validateAumRaw('0x'+raw.toString(16),params,recovery));
}
'''
        result=subprocess.run(['/usr/local/bin/node','-e',code],cwd=ROOT,capture_output=True,text=True)
        self.assertEqual(result.returncode,0,result.stderr[-4000:])
