"""Preserve raw/immutable/receipt/initial-lock authority for every BPT bank."""
from pathlib import Path
import subprocess
import unittest

ROOT=Path(__file__).resolve().parents[4]


class BptObservationTests(unittest.TestCase):
    def run_node(self,mutation=None):
        script=r'''
const fs=require('node:fs'),assert=require('node:assert/strict');
const I=require('./integration/tools/reference/balancer-managed-v2-bpt/inputs.cjs'),C=require('./integration/tools/reference/balancer-managed-v2-bpt/checker.cjs');
const {ethers}=I.E.rr('ethers'),abi=new ethers.utils.Interface(I.A.locate(I.A.load().builds,'ManagedPoolFactory','ManagedPool').c.abi);
const b=JSON.parse(fs.readFileSync('integration/test/contracts/managed-v2-bpt.json'));
function update(g,value){g.decoded=value;g.result=abi.encodeFunctionResult(g.method,value);}
'''
        if mutation is None:script+="assert.equal(C.checkObservations(b).swap_attempts,116);"
        else:script+=mutation+"\nassert.throws(()=>C.checkObservations(b),undefined,'BPT source contradiction must be rejected');"
        result=subprocess.run(['/usr/local/bin/node','-e',script],cwd=ROOT,capture_output=True,text=True)
        self.assertEqual(result.returncode,0,result.stderr[-6000:])

    def test_original_bpt_data_positive_control(self):self.run_node()

    def test_after_bank_owner_getter_cannot_detach_from_constructor(self):
        self.run_node(r'''
const c=b.cases.find(c=>c.id==='settled/C0'),bank=b.banks.find(x=>x.id===c.after),p=b.branches.settled.pools[0],s=bank.pools.find(p=>p.name==='A').state;
update(s.getters.find(g=>g.to===p.pool&&g.method==='getOwner'),[b.branches.settled.roles.trader]);
''')

    def test_after_bank_fee_view_cannot_detach_from_raw_schedule(self):
        self.run_node(r'''
const branch=b.branches.settled,p=branch.pools[0];
for(const bank of b.banks.filter(x=>x.namespace==='settled').slice(1)){
 const s=bank.pools.find(p=>p.name==='A').state;s.fee=String(BigInt(s.fee)+1n);update(s.getters.find(g=>g.to===p.pool&&g.method==='getSwapFeePercentage'),[s.fee]);
}
''')

    def test_revert_must_preserve_packed_metadata_not_only_storage_and_getters(self):
        self.run_node(r'''
const c=b.cases.find(c=>c.namespace==='controls'&&c.label==='join-disabled'),s=b.banks.find(x=>x.id===c.after).pools.find(p=>p.name==='A').state;
s.packed.pool_word='0x'+(BigInt(s.packed.pool_word)^1n).toString(16).padStart(64,'0');
''')

    def test_aum_mint_split_must_match_actual_bpt_transfer_events(self):
        self.run_node(r'''
const branch=b.branches.settled,p=branch.pools[0],c=b.cases.find(c=>c.id==='settled/C0');let reached=false;
for(const bank of b.banks.filter(x=>x.namespace==='settled')){
 if(bank.id===c.after)reached=true;if(!reached)continue;
 const s=bank.pools.find(p=>p.name==='A').state,w=bank.bpt_wallets.find(w=>w.pool_name==='A'),collector=s.getters.find(g=>g.to===branch.vault&&g.method==='getProtocolFeesCollector').decoded[0];
 for(const [who,delta]of [[branch.roles.manager,1n],[collector,-1n]]){
  w.balances[who]=String(BigInt(w.balances[who])+delta);
  for(const g of s.getters.filter(g=>g.to===p.pool&&g.method==='balanceOf'&&g.args[0]===who))update(g,[w.balances[who]]);
 }
 s.aum.manager_bpt=w.balances[branch.roles.manager];s.aum.collector_bpt=w.balances[collector];
}
c.expected_native.manager_bpt_delta=String(BigInt(c.expected_native.manager_bpt_delta)+1n);c.expected_native.collector_bpt_delta=String(BigInt(c.expected_native.collector_bpt_delta)-1n);
''')

    def test_minimum_zero_address_lock_cannot_be_reassigned_to_lp(self):
        self.run_node(r'''
const branch=b.branches.settled,p=branch.pools[0],zero=ethers.constants.AddressZero;
for(const bank of b.banks.filter(x=>x.namespace==='settled')){
 const s=bank.pools.find(p=>p.name==='A').state,w=bank.bpt_wallets.find(w=>w.pool_name==='A'),locked=BigInt(w.balances[zero]);
 assert.equal(locked,1000000n);w.balances[zero]='0';w.balances[branch.roles.lp]=String(BigInt(w.balances[branch.roles.lp])+locked);
 for(const who of [zero,branch.roles.lp])for(const g of s.getters.filter(g=>g.to===p.pool&&g.method==='balanceOf'&&g.args[0]===who))update(g,[w.balances[who]]);
}
''')
