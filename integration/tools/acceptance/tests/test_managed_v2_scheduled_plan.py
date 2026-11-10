"""Selected API plans must bind exact original sender/recipient and route."""
import json
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[4]


class ScheduledPlanTests(unittest.TestCase):
    def test_exact_original_plan_accepts_and_mutations_fail_before_execution(self):
        script = r'''
const fs=require('node:fs'),assert=require('node:assert/strict');
const {validateRows}=require('./integration/tools/ci/managed-v2-scheduled-plan-replay.cjs');
const b=JSON.parse(fs.readFileSync('integration/test/contracts/managed-v2-scheduled.json'));
const thin=JSON.parse(fs.readFileSync('integration/test/contracts/managed-v2-scheduled-pools.json'));
const c=thin.cases.find(c=>c.id==='three/VM0'),p=thin.states[c.before],br=b.branches.three,native=br.cases.find(x=>x.id==='VM0'),tx=br.transactions.find(t=>t.id===native.transaction),h=br.states.find(s=>s.id===native.before).header;
const request={chainKey:'evm:31337',kind:'EXACT_IN',verification:'FAST',sellToken:c.token_in,buyToken:c.token_out,sellAmount:c.amount_in,caller:tx.transaction.from,recipient:br.configuration.roles.trader};
const q={consumedIn:c.amount_in,grossBuyAmount:c.expected_raw_out,refund:'0',verification:{level:'INDICATIVE'},snapshotRef:{block:{chain_key:'evm:31337',number_or_slot:Number(BigInt(h.number)),hash:h.hash,parent_hash:h.parentHash,timestamp_ms:Number(BigInt(h.timestamp))*1000}},routes:[{amount_in:c.amount_in,amount_out:c.expected_raw_out,path:{tokens:[c.token_in,c.token_out],amount_in:c.amount_in,amount_out:c.expected_raw_out,edges:[{pool_id:p.pool_id,protocol:p.protocol,from_token:c.token_in,to:c.token_out}]}}]};
const row={case:c.id,actor:'router1',request,quote:q};assert.equal(validateRows(b,[row]).length,1);
const changes=[x=>x.request.caller='0x'+'0'.repeat(40),x=>x.request.recipient=x.request.sellToken,x=>x.request.sellAmount='1',x=>x.quote.refund='1',x=>x.quote.grossBuyAmount='1',x=>x.quote.snapshotRef.block.hash='0x'+'f'.repeat(64),x=>x.quote.routes[0].path.edges[0].pool_id=x.request.sellToken,x=>x.quote.routes[0].path.edges[0].protocol='balancer-v2-managed-aum',x=>x.quote.routes[0].amount_in='1',x=>x.quote.routes[0].path.tokens.reverse(),x=>x.quote.verification.level='VERIFIED',x=>x.case='three/VM999',x=>x.quote.routes.push(x.quote.routes[0])];
for(const mutate of changes){const x=JSON.parse(JSON.stringify(row));mutate(x);assert.throws(()=>validateRows(b,[x]));}
assert.throws(()=>validateRows(b,[]));assert.throws(()=>validateRows(b,Array(129).fill(row)));
const two=thin.cases.find(c=>c.id==='two/VM1'),twoNative=b.branches.two.cases.find(c=>c.id==='VM1'),twoHeader=b.branches.two.states.find(s=>s.id===twoNative.before).header;
const x=JSON.parse(JSON.stringify(row));x.case=two.id;
Object.assign(x.request,{sellToken:two.token_in,buyToken:two.token_out,sellAmount:two.amount_in,caller:twoNative.execution.transaction.from,recipient:twoNative.execution.args[1][2]});
Object.assign(x.quote,{consumedIn:two.amount_in,grossBuyAmount:two.expected_raw_out});
x.quote.snapshotRef.block={chain_key:'evm:31337',number_or_slot:Number(BigInt(twoHeader.number)),hash:twoHeader.hash,parent_hash:twoHeader.parentHash,timestamp_ms:Number(BigInt(twoHeader.timestamp))*1000};
x.quote.routes=[{amount_in:two.amount_in,amount_out:two.expected_raw_out,path:{tokens:[two.token_in,two.token_out],amount_in:two.amount_in,amount_out:two.expected_raw_out,edges:[{pool_id:b.branches.two.pool,protocol:p.protocol,from_token:two.token_in,to:two.token_out}]}}];
assert.equal(validateRows(b,[x]).length,1,'original tuple sender,bool,recipient,bool');
console.log(JSON.stringify({accepted:1,rejected:changes.length+2,vm_started:false}));
'''
        result = subprocess.run(['/usr/local/bin/node', '-e', script], cwd=ROOT, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout)['rejected'], 15)
