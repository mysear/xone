"""Original released Managed BPT join/exit source contract; no local price oracle."""
from pathlib import Path
import subprocess
import unittest

ROOT=Path(__file__).resolve().parents[4]
CLI=ROOT/'integration/tools/ci/managed-v2-bpt-golden.cjs'


class ManagedBptEntryTests(unittest.TestCase):
    def node(self,script):
        p=subprocess.run(['/usr/local/bin/node','-e',script],cwd=ROOT,capture_output=True,text=True)
        self.assertEqual(p.returncode,0,p.stderr[-5000:])

    def test_native_bpt_scope_has_separate_modes_and_no_legacy_overwrite(self):
        self.assertTrue(CLI.is_file(),'BPT needs a formal original-reference producer')
        self.node(r'''
const assert=require('node:assert/strict'),path=require('node:path'),fs=require('node:fs'),os=require('node:os');
const C=require('./integration/tools/ci/managed-v2-bpt-golden.cjs');
assert.equal(C.defaults.length,4);assert(C.defaults.every(p=>p.endsWith('/test/contracts/managed-v2-bpt.json')));
assert.equal(C.args([]).mode,'generate');assert.equal(C.args(['--validate']).mode,'validate');assert.equal(C.args(['--check']).mode,'check');
assert.throws(()=>C.args(['--validate','--output','/tmp/a.json']));assert.throws(()=>C.args(['--output','relative.json']));
const dir=fs.mkdtempSync(path.join(os.tmpdir(),'bpt-output-'));
try{const p=path.join(dir,'candidate.json');assert.equal(C.output(p),p);
for(const name of ['managed-v2-aum.json','managed-v2-scheduled.json','managed-v2-scheduled-shared.json'])assert.throws(()=>C.output(path.join(dir,name)));
const source=path.resolve('integration/test/contracts/managed-v2-scheduled-shared.json');fs.symlinkSync(source,p);assert.throws(()=>C.output(p));fs.unlinkSync(p);fs.linkSync(source,p);assert.throws(()=>C.output(p));fs.unlinkSync(p);
}finally{fs.rmSync(dir,{recursive:true,force:true});}
''')

    def test_bpt_source_manifest_preserves_original_constructor_and_node_authority(self):
        self.node(r'''
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const I=require('./integration/tools/reference/balancer-managed-v2-bpt/inputs.cjs'),c=I.closure();
assert.equal(c.original.original.original_bank.sha256,'1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66');
assert.equal(c.original.original.node.node.version,'v25.3.0');
const p=path.join(I.DIR,'unregistered-bpt-probe.cjs');try{fs.writeFileSync(p,'module.exports=1;\n',{flag:'wx'});assert.throws(()=>I.closure());}finally{fs.rmSync(p,{force:true});}
''')


class ManagedBptNativeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        import json
        cls.paths=[ROOT/p/'test/contracts/managed-v2-bpt.json' for p in ('integration','chain-parser-go','chain-parser-rust','dex-router')]
        cls.raw=cls.paths[0].read_bytes()
        cls.bank=json.loads(cls.raw)

    def test_native_bpt_wallet_supply_and_registered_cash_transitions(self):
        self.assertEqual([p.read_bytes() for p in self.paths],[self.raw]*4)
        self.assertEqual(len({p.stat().st_ino for p in self.paths}),4)
        self.assertEqual(set(self.bank['branches']),{'settled','scheduled','controls','limits'})
        self.assertEqual(len({c['id'] for c in self.bank['cases']}),len(self.bank['cases']))
        banks={b['id']:b for b in self.bank['banks']}
        successful=[c for c in self.bank['cases'] if c['expected_native']['status']==1 and c['operation'] in ('bpt_join','bpt_exit')]
        self.assertGreaterEqual(len(successful),84)
        for c in successful:
            before,after=banks[c['before']],banks[c['after']]
            x=next(p for p in before['pools'] if p['name']==c['pool_name'])['state']['aum']
            y=next(p for p in after['pools'] if p['name']==c['pool_name'])['state']['aum']
            minted=int(y['total_supply'])-int(x['total_supply'])
            self.assertEqual(minted,int(x['pending_aum']))
            actual=int(c['expected_native']['actual_amount_out' if c['operation']=='bpt_join' else 'actual_amount_in'])
            signed=-actual if c['operation']=='bpt_join' else actual
            self.assertEqual(int(y['bpt_info'][0])-int(x['bpt_info'][0]),signed)
            self.assertEqual(int(y['virtual_supply'])-int(x['virtual_supply']),minted-signed)
            self.assertEqual(y['pending_aum'],'0')
        self.assertTrue(any(int(banks[c['before']]['pools'][0]['state']['aum']['pending_aum'])>0 for c in successful if c['pool_name']=='A'))

    def test_original_control_and_minimum_boundaries_are_explicit(self):
        indexed={c['label']:c for c in self.bank['cases'] if c['namespace']=='controls'}
        expected={'join-disabled':0,'exit-disabled':0,'regular-with-join-disabled':1,
                  'join-swaps-disabled':0,'exit-swaps-disabled':0,'regular-swaps-disabled':0,
                  'join-not-allowlisted':0,'exit-with-allowlist':1,'regular-with-allowlist':1,
                  'join-allowlisted':1,'join-after-remove-allowlist':0,
                  'circuit-small-join':1,'circuit-large-join':0}
        for label,status in expected.items():self.assertEqual(indexed[label]['expected_native']['status'],status,label)
        limits=[c for c in self.bank['cases'] if c['namespace']=='limits']
        self.assertTrue(any(c['label']=='zero-underlying-input' and c['expected_native']['status']==0 for c in limits))
        self.assertTrue(any(c['label']=='minimum-18-join' and c['expected_native']['status']==1 and c['expected_native']['actual_amount_out']=='0' for c in limits))
        for bank in self.bank['banks']:
            for holder in bank['bpt_wallets']:
                state=next(p['state'] for p in bank['pools'] if p['name']==holder['pool_name'])
                self.assertEqual(sum(map(int,holder['balances'].values())),int(state['aum']['total_supply']))
        self.assertFalse(self.bank['production_ready'])
        self.assertFalse(self.bank['all_requested_features_complete'])
