"""Published breaker reference has its own guarded output and source closure."""
from pathlib import Path
import subprocess
import unittest
ROOT=Path(__file__).resolve().parents[4]
class ManagedBreakerEntryTests(unittest.TestCase):
    def node(self,script):
        p=subprocess.run(['/usr/local/bin/node','-e',script],cwd=ROOT,capture_output=True,text=True)
        self.assertEqual(p.returncode,0,p.stderr[-5000:])
    def test_separate_cli_and_protected_prior_vectors(self):
        self.node(r'''
const assert=require('node:assert/strict'),path=require('node:path'),fs=require('node:fs'),os=require('node:os');
const C=require('./integration/tools/ci/managed-v2-breaker-golden.cjs');
assert.equal(C.defaults.length,4);assert(C.defaults.every(p=>p.endsWith('/test/contracts/managed-v2-breaker.json')));
assert.equal(C.args([]).mode,'generate');assert.equal(C.args(['--validate']).mode,'validate');assert.equal(C.args(['--check']).mode,'check');
assert.throws(()=>C.args(['--output','relative.json']));assert.throws(()=>C.args(['--validate','--output','/tmp/x.json']));
const dir=fs.mkdtempSync(path.join(os.tmpdir(),'managed-breaker-output-'));
try{const output=path.join(dir,'candidate.json');assert.equal(C.output(output),output);
for(const name of ['managed-v2-bpt.json','managed-v2-scheduled-shared.json','managed-v2-aum.json'])assert.throws(()=>C.output(path.join(dir,name)));
const original=path.resolve('integration/test/contracts/managed-v2-bpt.json');fs.symlinkSync(original,output);assert.throws(()=>C.output(output));fs.unlinkSync(output);fs.linkSync(original,output);assert.throws(()=>C.output(output));fs.unlinkSync(output);
}finally{fs.rmSync(dir,{recursive:true,force:true});}
''')
    def test_fixed_original_and_exact_new_source_manifest(self):
        self.node(r'''
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const I=require('./integration/tools/reference/balancer-managed-v2-breaker/inputs.cjs'),c=I.closure();
assert.equal(c.original_bpt.sha256,'4cbf03d19a773ba374d8df87e4e3231798f042f576addeb5532300af0bb892f8');
assert.equal(c.original_bpt.bytes,51222289);assert.equal(c.compiler.node.node.version,'v25.3.0');
const p=path.join(I.DIR,'unregistered-breaker-probe.cjs');try{fs.writeFileSync(p,'module.exports=1;\n',{flag:'wx'});assert.throws(()=>I.closure());}finally{fs.rmSync(p,{force:true});}
''')

class ManagedBreakerNativeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        import json
        cls.paths=[ROOT/p/'test/contracts/managed-v2-breaker.json' for p in ('integration','chain-parser-go','chain-parser-rust','dex-router')]
        cls.data=cls.paths[0].read_bytes();cls.bank=json.loads(cls.data)
    def test_native_reference_has_exact_independent_copies_and_real_directions(self):
        b=self.bank
        self.assertEqual([p.read_bytes() for p in self.paths],[self.data]*4)
        self.assertEqual(len({p.stat().st_ino for p in self.paths}),4)
        self.assertEqual(set(b['branches']),{'cached','dynamic','boundaries','shared'})
        self.assertEqual(len({c['id'] for c in b['cases']}),len(b['cases']))
        for operation in ('bpt_join','bpt_exit','token_swap'):
            self.assertTrue(any(c['operation']==operation and c['expected_native']['status']==1 for c in b['cases']))
        failures=[c for c in b['cases'] if c['expected_native'].get('rejection_reason')=='BAL#356']
        self.assertGreaterEqual(len(failures),8)
        self.assertFalse(b['production_ready']);self.assertFalse(b['all_requested_features_complete'])
    def test_real_cached_and_dynamic_pure_library_paths_cover_both_sides_and_decimals(self):
        hits=misses=0;directions=set();precisions=set();installed_disabled=zero_price=0
        for bank in self.bank['banks']:
            branch=self.bank['branches'][bank['namespace']]
            for pool in bank['pools']:
                config=next(p for p in branch['pools'] if p['name']==pool['name']);state=pool['state']
                for index,token in enumerate(branch['tokens']):
                    row=next(s for s in bank['control_storage'] if s['address']==config['pool'] and s.get('token')==token)
                    word=int(row['value'],16) if row['value']!='0x' else 0
                    if not word:continue
                    precisions.add(branch['decimals'][index])
                    if word&((1<<32)-1)==0:installed_disabled+=1
                    if (word>>96)&((1<<96)-1)==0:zero_price+=1
                    for lower in (True,False):
                        bound=word>>(0 if lower else 16)&65535
                        if not bound:continue
                        if int(state['weights'][index])==(word>>32)&((1<<64)-1):hits+=1
                        else:
                            misses+=1;directions.add(lower)
                            observations=[r for r in bank['circuit_math_calls'] if r['pool_name']==pool['name'] and r['token']==token and r['is_lower']==lower]
                            self.assertEqual(len(observations),1)
                            g=state['getters'][observations[0]['getter_ordinal']]
                            self.assertEqual(g['method'],'calcAdjustedBound');self.assertEqual(g['decoded'][0],observations[0]['ratio'])
        self.assertGreater(hits,0);self.assertGreater(misses,0);self.assertEqual(directions,{True,False});self.assertEqual(precisions,{18,6,8})
        self.assertGreater(installed_disabled,0);self.assertGreater(zero_price,0)
    def test_actual_shared_witness_consumes_two_physical_pools_and_preserves_input_units(self):
        b=self.bank;self.assertEqual(len(b['orders']),1);o=b['orders'][0]
        self.assertEqual(len(o['reference_routes']),2)
        self.assertEqual(sum(int(r['amount_in']) for r in o['reference_routes']),int(o['amount_in']))
        self.assertEqual(sum(int(r['actual_amount_out']) for r in o['reference_routes']),int(o['expected_native']['actual_amount_out']))
        indexed={c['id']:c for c in b['cases']}
        for r in o['reference_routes']:
            first,second=(indexed[c] for c in r['cases'])
            self.assertEqual((first['pool_name'],second['pool_name']),('A','B'))
            self.assertEqual(first['expected_native']['actual_amount_out'],second['amount_in'])
            self.assertEqual(first['token_out'],second['token_in'])

    def test_scaled_dust_can_trip_before_raw_output_is_rounded_to_zero(self):
        by_label={c['label']:c for c in self.bank['cases']}
        for rejected,allowed in [('dust-BPT-exit-internal-bound-trips','dust-BPT-exit-disabled-zero-credit'),('dust-regular-internal-bound-trips','dust-regular-disabled-zero-credit')]:
            bad,good=by_label[rejected],by_label[allowed]
            self.assertEqual(bad['expected_native']['status'],0)
            self.assertEqual(bad['expected_native']['rejection_reason'],'BAL#356')
            self.assertEqual(good['expected_native']['status'],1)
            self.assertEqual(good['expected_native']['actual_amount_out'],'0')
            self.assertEqual(bad['amount_in'],good['amount_in'])
            self.assertEqual(bad['token_out'],good['token_out'])

class ManagedBreakerObservationTests(unittest.TestCase):
    def test_no_orphan_or_duplicated_coverage_and_exact_shared_witness(self):
        code=r'''
const fs=require('node:fs'),assert=require('node:assert/strict'),C=require('./integration/tools/reference/balancer-managed-v2-breaker/checker.cjs');
const raw=JSON.parse(fs.readFileSync('integration/test/contracts/managed-v2-breaker.json'));
assert.equal(C.checkObservations(raw).passed,true);
for(const mutate of [
 b=>{const x=structuredClone(b.banks[0]);x.id='orphan/B0';x.namespace='orphan';b.banks.push(x);},
 b=>{const c=structuredClone(b.cases[0]);c.id='orphan/C0';c.namespace='orphan';b.cases.push(c);},
 b=>{const c=structuredClone(b.controls[0]);c.namespace='orphan';b.controls.push(c);},
 b=>{const c=structuredClone(b.checkpoints[0]);c.id='orphan/CP';c.namespace='orphan';b.checkpoints.push(c);},
 b=>{b.orders[0].reference_routes[0].amount_in=String(BigInt(b.orders[0].reference_routes[0].amount_in)+1n);},
 b=>{b.orders[0].expected_native.actual_amount_out=String(BigInt(b.orders[0].expected_native.actual_amount_out)+1n);},
 b=>{b.orders[0].caller=b.branches.shared.roles.trader;},
]){const b=structuredClone(raw);mutate(b);assert.throws(()=>C.checkObservations(b),'coverage or shared amount contradiction must fail');}
'''
        p=subprocess.run(['/usr/local/bin/node','-e',code],cwd=ROOT,capture_output=True,text=True)
        self.assertEqual(p.returncode,0,p.stderr[-5000:])

    def test_currently_tripped_subunit_upper_still_allows_amount_dependent_recovery(self):
        import json
        raw=json.loads((ROOT/'integration/test/contracts/managed-v2-breaker.json').read_bytes())
        by_label={c['label']:c for c in raw['cases']}
        bad=by_label['subunit-upper-small-join-trips'];good=by_label['subunit-upper-large-join-recovers']
        self.assertEqual(bad['expected_native']['status'],0);self.assertEqual(bad['expected_native']['rejection_reason'],'BAL#356')
        self.assertEqual(good['expected_native']['status'],1)
        self.assertEqual((bad['pool_name'],good['pool_name']),('B','B'))
        self.assertEqual(bad['token_in'],good['token_in']);self.assertGreater(int(good['amount_in']),int(bad['amount_in']))

class ManagedBreakerCompletenessTests(unittest.TestCase):
    def test_native_account_control_clock_and_scenario_deletion_fail_closed(self):
        script=r'''
const fs=require('node:fs'),assert=require('node:assert/strict'),C=require('./integration/tools/reference/balancer-managed-v2-breaker/checker.cjs');
const raw=JSON.parse(fs.readFileSync('integration/test/contracts/managed-v2-breaker.json'));assert(C.checkObservations(raw).passed);
for(const mutate of [
 b=>{const c=b.controls.filter(c=>c.namespace==='boundaries').at(-1),branch=b.branches.boundaries,tx=branch.transactions.find(t=>t.id===c.transaction),after=b.banks.find(x=>x.id===c.after),a=after.accounts.find(a=>a.address===tx.transaction.from);a.nonce='0x0';},
 b=>{const c=b.cases.filter(c=>c.namespace==='cached').at(-1);b.cases=b.cases.filter(x=>x!==c);b.banks=b.banks.filter(x=>x.id!==c.after);const branch=b.branches.cached;branch.transactions=branch.transactions.filter(t=>t.id!==c.transaction);branch.bank_ids=branch.bank_ids.filter(id=>id!==c.after);},
 b=>{const c=b.controls.filter(c=>c.namespace==='boundaries').at(-1);b.controls=b.controls.filter(x=>x!==c);b.banks=b.banks.filter(x=>x.id!==c.after);b.branches.boundaries.bank_ids=b.branches.boundaries.bank_ids.filter(id=>id!==c.after);},
 b=>{b.branches.cached.clock_operations.push(structuredClone(b.branches.cached.clock_operations[0]));},
 b=>{const c=b.branches.cached.clock_operations[0],after=b.banks.find(x=>x.header.hash===c.header.hash),a=after.accounts[0];a.native_balance='0x'+(BigInt(a.native_balance)+1n).toString(16);},
]){const b=structuredClone(raw);mutate(b);assert.throws(()=>C.checkObservations(b),'native account/complete scenario contradiction accepted');}
'''
        p=subprocess.run(['/usr/local/bin/node','-e',script],cwd=ROOT,capture_output=True,text=True)
        self.assertEqual(p.returncode,0,p.stderr[-5000:])

class ManagedBreakerControlIntentTests(unittest.TestCase):
    def test_invalid_manager_control_outcomes_are_fixed_independently_of_observations(self):
        script=r'''
const fs=require('node:fs'),assert=require('node:assert/strict'),S=require('./integration/tools/reference/balancer-managed-v2-breaker/scenarios.cjs'),I=require('./integration/tools/reference/balancer-managed-v2-breaker/inputs.cjs');
const raw=JSON.parse(fs.readFileSync('integration/test/contracts/managed-v2-breaker.json'));
const eq=(a,b)=>assert.equal(I.V.canonical(a),I.V.canonical(b));S.validate(raw,'boundaries',eq);
for(const label of ['below-lower-minimum','lower-above-one','upper-above-ten','upper-below-lower','BPT-is-not-underlying','stored-price-overflows-u96','non-owner-cannot-set-breaker']){
 const b=structuredClone(raw),c=b.controls.find(c=>c.namespace==='boundaries'&&b.branches.boundaries.transactions.find(t=>t.id===c.transaction).purpose===label),tx=b.branches.boundaries.transactions.find(t=>t.id===c.transaction);c.expected_native.status=1;tx.receipt.status='0x1';assert.throws(()=>S.validate(b,'boundaries',eq),undefined,label+' must be independently fixed as failure');
}
'''
        p=subprocess.run(['/usr/local/bin/node','-e',script],cwd=ROOT,capture_output=True,text=True)
        self.assertEqual(p.returncode,0,p.stderr[-5000:])
