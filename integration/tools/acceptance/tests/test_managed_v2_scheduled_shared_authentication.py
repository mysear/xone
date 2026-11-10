"""All retained raw observations and native execution references must be closed."""
from pathlib import Path
import subprocess
import unittest

ROOT=Path(__file__).resolve().parents[4]


class SharedObservationQualificationTests(unittest.TestCase):
    def node(self, script):
        result=subprocess.run(['/usr/local/bin/node','-e',script],cwd=ROOT,capture_output=True,text=True)
        self.assertEqual(result.returncode,0,result.stderr[-5000:])
        return result.stdout

    def test_data_validation_has_a_real_positive_control_separate_from_source_manifest(self):
        self.node(r'''
const fs=require('node:fs'),assert=require('node:assert/strict');
const C=require('./integration/tools/reference/balancer-managed-v2-scheduled-shared/checker.cjs');
const b=JSON.parse(fs.readFileSync('integration/test/contracts/managed-v2-scheduled-shared.json'));
assert.equal(typeof C.checkObservations,'function');
assert.deepEqual(C.checkObservations(b),{banks:72,physical_states:144,swaps:48,collections:4,checkpoints:12,funded_orders:12});
''')

    def rejection(self, mutation):
        self.node(r'''
const fs=require('node:fs'),assert=require('node:assert/strict');
const C=require('./integration/tools/reference/balancer-managed-v2-scheduled-shared/checker.cjs');
const I=require('./integration/tools/reference/balancer-managed-v2-scheduled-shared/inputs.cjs');
const b=JSON.parse(fs.readFileSync('integration/test/contracts/managed-v2-scheduled-shared.json'));
''' + mutation + r'''
assert.throws(()=>C.checkObservations(b),undefined,'retained native observation graph must reject this contradiction');
''')

    def test_duplicate_execution_references_cannot_replace_b_pool_coverage(self):
        self.rejection(r'''
for(const namespace of ['mixed','active']){
 const rows=b.cases.filter(c=>c.namespace===namespace),swap=rows.find(c=>c.operation==='swap'&&c.pool_name==='A'),collect=rows.find(c=>c.operation==='collect'&&c.pool_name==='A');
 const replacements=Array.from({length:26},(_,i)=>({...JSON.parse(JSON.stringify(i<24?swap:collect)),id:namespace+'/C'+i}));
 b.cases=b.cases.filter(c=>c.namespace!==namespace).concat(replacements);
}
''')

    def test_same_clock_sibling_weight_view_cannot_change_after_other_pool_swap(self):
        self.rejection(r'''
const c=b.cases.find(c=>c.id==='active/C0'),bank=b.banks.find(s=>s.id===c.after),state=bank.pools.find(p=>p.name==='B').state,pool=b.branches.active.pools.find(p=>p.name==='B').pool;
state.weights[0]=String(BigInt(state.weights[0])+1n);state.sum_weights=String(BigInt(state.sum_weights)+1n);
const g=state.getters.find(g=>g.to===pool&&g.method==='getNormalizedWeights');g.decoded=[state.weights];
const abi=new (I.E.rr('ethers').ethers.utils.Interface)(I.A.locate(I.A.load().builds,'ManagedPoolFactory','ManagedPool').c.abi);g.result=abi.encodeFunctionResult(g.method,g.decoded);
''')

    def test_foreign_getter_cannot_be_discarded_before_abi_or_anchor_validation(self):
        self.rejection(r'''
const branch=b.branches.active,bank=b.banks.find(s=>s.namespace==='active'),state=bank.pools.find(p=>p.name==='A').state,peer=branch.pools.find(p=>p.name==='B');
state.getters.push({to:peer.pool,method:'getNormalizedWeights',args:[],data:'0x',result:'0x',decoded:[],block_number:'0x0',block_hash:'0x'+'0'.repeat(64)});
''')

    def test_management_references_must_resolve_to_the_original_control_call(self):
        self.rejection("b.management[0].before='missing-bank';b.management[0].transaction='missing-transaction';")

    def test_clock_operations_cannot_be_removed_while_checkpoints_survive(self):
        self.rejection("for(const branch of Object.values(b.branches))branch.clock_operations=[];")
