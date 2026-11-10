"""Selected two-route API syntax must bind source, budgets and physical pools."""
from pathlib import Path
import subprocess
import unittest

ROOT=Path(__file__).resolve().parents[4]


class SharedSelectedPlanTests(unittest.TestCase):
    def test_real_validator_rejects_plan_mismatches_before_any_vm(self):
        script=r'''
const fs=require('node:fs'),assert=require('node:assert/strict');
const P=require('./integration/tools/ci/managed-v2-scheduled-shared-plan-replay.cjs');
const I=require('./integration/tools/reference/balancer-managed-v2-scheduled-shared/inputs.cjs');
const bytes=fs.readFileSync('integration/test/contracts/managed-v2-scheduled-shared.json'),b=JSON.parse(bytes),order=b.orders[0],branch=b.branches[order.namespace],source=b.banks.find(s=>s.id===order.source_bank);
const amount=(BigInt(order.amount_in)/2n).toString(),request={chainKey:'evm:31337',kind:'EXACT_IN',verification:'FAST',sellToken:order.token_in,buyToken:order.token_out,sellAmount:order.amount_in,caller:order.caller,recipient:order.recipient,slippageBps:50,constraints:{maxHops:1}};
// Arbitrary outputs are syntax only: this unit test starts no VM and supplies no native price proof.
const routes=branch.pools.map(p=>({amount_in:amount,amount_out:'100',path:{tokens:[order.token_in,order.token_out],amount_in:amount,amount_out:'100',edges:[{pool_id:p.pool,protocol:p.requested_protocol,from_token:order.token_in,to:order.token_out}]}}));
const ref={block:{chain_key:'evm:31337',number_or_slot:Number(BigInt(source.header.number)),hash:source.header.hash,parent_hash:source.header.parentHash,timestamp_ms:Number(BigInt(source.header.timestamp))*1000},online_generation:1,canonical_epoch:1,scope_hash:'a'.repeat(64)};
const quote={snapshotRef:ref,onlineGeneration:1,scopeHash:ref.scope_hash,grossBuyAmount:'200',minBuyAmount:'199',consumedIn:order.amount_in,refund:'0',verification:{level:'INDICATIVE',success:false},routes,quality:{candidateScope:branch.pools.map(p=>p.pool),excludedPools:{}}};
const input={schema_version:1,original_shared_sha256:I.sha(bytes),quotes:[{id:'syntax-only',namespace:order.namespace,source_bank:source.id,request,quote}]};
assert.equal(P.validateInput(input,b,I.sha(bytes)).length,1);
const mutations=[x=>x.original_shared_sha256='0'.repeat(64),x=>x.quotes[0].source_bank='missing',x=>x.quotes[0].namespace='active',x=>x.quotes[0].request.caller='0x'+'0'.repeat(40),x=>x.quotes[0].request.recipient=x.quotes[0].request.sellToken,x=>x.quotes[0].request.sellAmount='1',x=>x.quotes[0].quote.refund='1',x=>x.quotes[0].quote.routes.pop(),x=>x.quotes[0].quote.routes[1].path.edges[0].pool_id=x.quotes[0].quote.routes[0].path.edges[0].pool_id,x=>x.quotes[0].quote.routes[0].path.edges[0].protocol='uniswap-v2',x=>x.quotes[0].quote.routes[0].amount_in='1',x=>x.quotes[0].quote.routes[0].path.tokens.reverse(),x=>x.quotes[0].quote.snapshotRef.block.hash='0x'+'f'.repeat(64),x=>x.quotes[0].quote.onlineGeneration=0,x=>x.quotes[0].quote.grossBuyAmount='201',x=>x.quotes[0].quote.minBuyAmount='201',x=>x.quotes[0].quote.verification.level='VERIFIED',x=>x.quotes[0].quote.quality.candidateScope.pop()];
for(const mutate of mutations){const x=JSON.parse(JSON.stringify(input));mutate(x);assert.throws(()=>P.validateInput(x,b,I.sha(bytes)));}
assert.throws(()=>P.validateInput({...input,quotes:[]},b,I.sha(bytes)));assert.throws(()=>P.validateInput({...input,quotes:Array(65).fill(input.quotes[0])},b,I.sha(bytes)));
console.log(JSON.stringify({accepted_syntax:1,rejected:mutations.length+2,vm_started:false}));
'''
        result=subprocess.run(['/usr/local/bin/node','-e',script],cwd=ROOT,capture_output=True,text=True)
        self.assertEqual(result.returncode,0,result.stderr[-5000:])
