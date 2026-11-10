"""Qualification must bind observed runtime and the complete ordinary source."""
from pathlib import Path
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[4]


class ScheduledAuthenticationTests(unittest.TestCase):
    def check_mutation(self, mutation):
        script = r'''
const fs=require('node:fs'),assert=require('node:assert/strict');
const root='./integration/tools/reference/balancer-managed-v2-scheduled/',I=require(root+'inputs.cjs'),C=require(root+'checker.cjs');
const bank=JSON.parse(fs.readFileSync('integration/test/contracts/managed-v2-scheduled.json'));
''' + mutation + r'''
assert.throws(()=>C.verifyBranch(bank.branches.reschedule,'reschedule',I.originalBank(),I.A.load().builds),undefined,'unqualified original source must be rejected');
console.log('rejected');
'''
        result = subprocess.run(['/usr/local/bin/node', '-e', script], cwd=ROOT, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr[-4000:])

    def test_state_pool_runtime_cannot_detach_from_authenticated_identity(self):
        self.check_mutation(r'''
const branch=bank.branches.reschedule,identity=branch.new_identities.find(i=>i.address===branch.configuration.pool);
const code=branch.native_runtime[identity.runtime_sha256],changed='0x61'+code.slice(4),digest=I.sha(Buffer.from(changed.slice(2),'hex'));
branch.native_runtime[digest]=changed;
for(const state of branch.states)state.accounts.find(a=>a.address===identity.address).code_sha256=digest;
''')

    def test_factory_dependency_getter_cannot_be_omitted(self):
        self.check_mutation(r'''
const branch=bank.branches.reschedule,state=branch.states[0];
const size=state.getters.length;
state.getters=state.getters.filter(g=>!(g.to===branch.configuration.factory&&g.method==='getWeightedMath'));
assert.equal(state.getters.length,size-1,'mutation removed original dependency getter');
''')

    def test_pool_owner_immutable_cannot_detach_from_manager_getter(self):
        self.check_mutation(r'''
const branch=bank.branches.reschedule,identity=branch.new_identities.find(i=>i.address===branch.configuration.pool);
let code=branch.native_runtime[identity.runtime_sha256];
for(const immutable of identity.immutables.filter(i=>i.name==='_owner'))for(const span of immutable.spans){
 const changed='0x'+'0'.repeat(63)+'1';span.value=changed;
 code=code.slice(0,2+span.start*2)+changed.slice(2)+code.slice(2+(span.start+span.length)*2);
}
const digest=I.sha(Buffer.from(code.slice(2),'hex'));branch.native_runtime[digest]=code;identity.runtime_sha256=digest;
for(const state of branch.states)state.accounts.find(a=>a.address===identity.address).code_sha256=digest;
''')

    def test_factory_creation_addresses_must_match_pinned_original_parts(self):
        self.check_mutation(r'''
const branch=bank.branches.reschedule,abi=new (I.E.rr('ethers').ethers.utils.Interface)(I.A.locate(I.A.load().builds,'ManagedPoolFactory','ManagedPoolFactory').c.abi);
for(const state of branch.states){const g=state.getters.find(g=>g.to===branch.configuration.factory&&g.method==='getCreationCodeContracts');g.decoded=['0x'+'1'.repeat(40),'0x'+'2'.repeat(40)];g.result=abi.encodeFunctionResult(g.method,g.decoded);}
''')

    def test_vault_weth_getter_must_match_pinned_original_weth(self):
        self.check_mutation(r'''
const branch=bank.branches.reschedule,abi=new (I.E.rr('ethers').ethers.utils.Interface)(I.A.locate(I.A.load().builds,'Vault','Vault').c.abi);
for(const state of branch.states){const g=state.getters.find(g=>g.to===branch.configuration.vault&&g.method==='WETH');g.decoded=['0x'+'1'.repeat(40)];g.result=abi.encodeFunctionResult(g.method,g.decoded);}
''')

    def test_unmodified_pinned_branches_still_qualify(self):
        script = r'''
const fs=require('node:fs'),assert=require('node:assert/strict');
const root='./integration/tools/reference/balancer-managed-v2-scheduled/',I=require(root+'inputs.cjs'),C=require(root+'checker.cjs');
const bank=JSON.parse(fs.readFileSync('integration/test/contracts/managed-v2-scheduled.json'));
const original=I.originalBank(),{builds}=I.A.load();let count=0;
for(const [name,branch]of Object.entries(bank.branches))count+=C.verifyBranch(branch,name,original,builds).states;
assert.equal(count,234);console.log('qualified');
'''
        result = subprocess.run(['/usr/local/bin/node', '-e', script], cwd=ROOT, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr[-4000:])

    def test_new_identity_cannot_shadow_pinned_prefix_identity(self):
        self.check_mutation(r'''
const branch=bank.branches.reschedule,prefix=branch.prefix_identities.find(i=>i.label==='ManagedPoolFactory');
const account=branch.states[0].accounts.find(a=>a.address===prefix.address);
branch.new_identities.push({address:prefix.address,artifact:prefix.artifact,runtime_sha256:account.code_sha256});
''')

    def test_invalid_hex_cannot_masquerade_as_empty_eoa_runtime(self):
        self.check_mutation(r'''
const branch=bank.branches.reschedule,empty=I.sha(Buffer.alloc(0));
assert.equal(branch.native_runtime[empty],'0x');branch.native_runtime[empty]='0xzz';
for(const state of branch.states)for(const account of state.accounts)if(account.code_sha256===empty)account.runtime_bytes=1;
''')

    def test_fixed_constructor_swap_yield_ids_cannot_follow_forged_getters(self):
        for kind in (0, 2):
            with self.subTest(kind=kind):
                self.check_mutation('const kind=' + str(kind) + r''';
const branch=bank.branches.reschedule,identity=branch.new_identities.find(i=>i.address===branch.configuration.pool);
let code=branch.native_runtime[identity.runtime_sha256],replacement=BigInt(kind+1),shift=BigInt(kind===0?0:8);
for(const immutable of identity.immutables.filter(i=>i.name==='_feeIds'))for(const span of immutable.spans){
 const value=(BigInt(span.value)&~(255n<<shift))|(replacement<<shift),word='0x'+value.toString(16).padStart(64,'0');
 span.value=word;code=code.slice(0,2+span.start*2)+word.slice(2)+code.slice(2+(span.start+span.length)*2);
}
const digest=I.sha(Buffer.from(code.slice(2),'hex'));branch.native_runtime[digest]=code;identity.runtime_sha256=digest;
const abi=new (I.E.rr('ethers').ethers.utils.Interface)(I.A.locate(I.A.load().builds,'ManagedPoolFactory','ManagedPool').c.abi);
for(const state of branch.states){state.accounts.find(a=>a.address===identity.address).code_sha256=digest;
 const g=state.getters.find(g=>g.to===identity.address&&g.method==='getProviderFeeId'&&String(g.args[0])===String(kind));
 g.decoded=[replacement.toString()];g.result=abi.encodeFunctionResult(g.method,g.decoded);
}
''')
