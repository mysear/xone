"""Independent multi-physical-pool schedule reference contract."""
import os
from pathlib import Path
import shutil
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[4]
CLI = ROOT / 'integration/tools/ci/managed-v2-scheduled-shared-golden.cjs'
REF = ROOT / 'integration/tools/reference/balancer-managed-v2-scheduled-shared'
NODE = '/usr/local/bin/node'


class ScheduledSharedEntryTests(unittest.TestCase):
    def node(self, script):
        result = subprocess.run([NODE, '-e', script], cwd=ROOT, capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout

    def test_registered_entrypoint_modes_are_explicit(self):
        self.assertTrue(CLI.is_file(), 'formal scheduled shared CLI is required')
        self.node(r'''
const assert=require('node:assert/strict'),path=require('node:path');
const c=require('./integration/tools/ci/managed-v2-scheduled-shared-golden.cjs');
assert.equal(typeof c.main,'function');assert.equal(c.defaults.length,4);
assert(c.defaults.every(p=>p.endsWith('/test/contracts/managed-v2-scheduled-shared.json')));
assert.equal(c.args([]).mode,'generate');assert.equal(c.args(['--validate']).mode,'validate');
assert.equal(c.args(['--check']).mode,'check');
const p=path.resolve('/tmp/new-shared.json');assert.deepEqual(c.args(['--check','--input',p]).targets,[p]);
assert.throws(()=>c.args(['--validate','--check']));
assert.throws(()=>c.args(['--output','relative.json']));
assert.throws(()=>c.args(['--output',p,'--input',p]));
''')

    def test_output_refuses_legacy_inputs_and_aliases_before_execution(self):
        self.assertTrue(CLI.is_file(), 'formal scheduled shared CLI is required')
        self.node(r'''
const assert=require('node:assert/strict'),fs=require('node:fs'),os=require('node:os'),path=require('node:path');
const {output,defaults}=require('./integration/tools/ci/managed-v2-scheduled-shared-golden.cjs');
const dir=fs.mkdtempSync(path.join(os.tmpdir(),'scheduled-shared-output-'));
try {
 const p=path.join(dir,'candidate.json'),source=path.resolve('integration/test/contracts/managed-v2-scheduled.json');
 assert.equal(output(p),p);
 for(const name of ['managed-v2-scheduled.json','managed-v2-aum.json','managed-v2-shared-vault.json'])assert.throws(()=>output(path.join(dir,name)));
 assert.throws(()=>output(source));
 assert.throws(()=>output(path.resolve('integration/test/contracts/unregistered-shared.json')));
 fs.symlinkSync(source,p);assert.throws(()=>output(p));fs.unlinkSync(p);
 fs.linkSync(source,p);assert.throws(()=>output(p));fs.unlinkSync(p);
 assert(defaults.every(p=>path.isAbsolute(p)));
} finally {fs.rmSync(dir,{recursive:true,force:true});}
''')

    def test_per_pool_observation_view_retains_factory_vault_and_exact_source(self):
        self.node(r'''
const fs=require('node:fs'),assert=require('node:assert/strict');
const C=require('./integration/tools/reference/balancer-managed-v2-scheduled-shared/checker.cjs');
const old=JSON.parse(fs.readFileSync('integration/test/contracts/managed-v2-scheduled.json')),source=old.branches.three,config=source.configuration,state=source.states[0];
const p={name:'A',pool:config.pool,pool_id:config.pool_id,params:config.params,settings:config.settings,creation:config.creation,initialize:config.initialize,requested_protocol:'balancer-v2-managed-scheduled'};
const branch={...source,mode:'active',factory:config.factory,vault:config.vault,roles:config.roles,tokens:config.tokens,decimals:config.decimals};
const I=require('./integration/tools/reference/balancer-managed-v2-scheduled-shared/inputs.cjs');
const raw={banks:[{id:'active/B0',namespace:'active',label:'syntax view of an original one-pool observation',pools:[{name:'A',state}],accounts:state.accounts,storage:state.storage}],original_reference:{source_manifest:I.sourceManifest(),compiler_jobs:I.originalBank().reference.compiler_jobs,source_pins:I.A.load().pins},inputs:{original:I.closure().original}};
const before=JSON.stringify(raw),view=C.poolContext(raw,branch,p,I.originalBank());
assert.equal(view.configuration.factory,config.factory);assert.equal(view.configuration.vault,config.vault);
assert.equal(view.states[0].header,state.header);assert.equal(view.states[0].packed,state.packed);
assert.equal(JSON.stringify(raw),before,'observation projection is read-only');
''')

    def test_input_manifest_is_exact_and_inherits_original_authority(self):
        self.assertTrue((REF / 'inputs.cjs').is_file(), 'new scope needs its own source closure')
        self.node(r'''
const assert=require('node:assert/strict'),fs=require('node:fs'),path=require('node:path');
const I=require('./integration/tools/reference/balancer-managed-v2-scheduled-shared/inputs.cjs');
const closure=I.closure();assert.equal(closure.original.original_bank.sha256,'1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66');
assert.equal(closure.original.node.node.version,'v25.3.0');
const extra=path.join(I.DIR,'unexpected-source.cjs');
try{fs.writeFileSync(extra,'module.exports=0;\n',{flag:'wx'});assert.throws(()=>I.closure());}
finally{fs.rmSync(extra,{force:true});}
''')


class ScheduledSharedNativeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        import json
        cls.paths = [ROOT / project / 'test/contracts/managed-v2-scheduled-shared.json'
                     for project in ('integration', 'chain-parser-go', 'chain-parser-rust', 'dex-router')]
        cls.raw = cls.paths[0].read_bytes()
        cls.bank = json.loads(cls.raw)

    def test_independent_common_h_bank_and_case_inventory(self):
        self.assertEqual([p.read_bytes() for p in self.paths], [self.raw] * 4)
        self.assertEqual(len({p.stat().st_ino for p in self.paths}), 4)
        self.assertEqual(set(self.bank['branches']), {'mixed', 'active'})
        self.assertEqual(len(self.bank['banks']), 72)
        self.assertEqual(len(self.bank['checkpoints']), 12)
        self.assertEqual(sum(c['operation'] == 'swap' for c in self.bank['cases']), 48)
        self.assertEqual(sum(c['operation'] == 'collect' for c in self.bank['cases']), 4)
        self.assertEqual(len({c['id'] for c in self.bank['cases']}), 52)
        for bank in self.bank['banks']:
            self.assertEqual([p['name'] for p in bank['pools']], ['A', 'B'])
            self.assertEqual(bank['header'], bank['header_anchor_after'])
            for row in bank['pools']:
                state = row['state']
                self.assertEqual(state['header'], bank['header'])
                for getter in state['getters']:
                    self.assertEqual((getter['block_number'], getter['block_hash']),
                                     (bank['header']['number'], bank['header']['hash']))
            for wallet in bank['global_wallets']:
                vault = self.bank['branches'][bank['namespace']]['vault']
                cash = sum(int(next(t for t in p['state']['token_states'] if t['token'] == wallet['token'])['info'][0])
                           for p in bank['pools'])
                self.assertGreaterEqual(int(wallet['balances'][vault]), cash)

    def test_mixed_profiles_and_actual_swap_collection_observations(self):
        for mode, branch in self.bank['branches'].items():
            self.assertEqual([p['requested_protocol'] for p in branch['pools']],
                             ['balancer-v2-managed-scheduled',
                              'balancer-v2-managed-aum' if mode == 'mixed' else 'balancer-v2-managed-scheduled'])
            self.assertEqual(len({p['pool'] for p in branch['pools']}), 2)
            self.assertEqual(branch['prefix_replay']['count'], 28)
            self.assertFalse(branch['chain_options']['chain']['allowUnlimitedContractSize'])
            self.assertFalse(branch['chain_options']['chain']['allowUnlimitedInitCodeSize'])
        for case in self.bank['cases']:
            native = case['expected_native']
            self.assertEqual(native['status'], 1)
            if case['operation'] == 'swap':
                self.assertGreater(int(native['actual_amount_out']), 0)
                self.assertEqual(native['actual_amount_in'], case['amount_in'])
                self.assertEqual(native['actual_amount_out'], native['swap_event']['amount_out'])
            else:
                self.assertGreater(int(native['minted_bpt']), 0)
                self.assertEqual(int(native['minted_bpt']), int(native['manager_bpt_delta']) + int(native['collector_bpt_delta']))
        self.assertFalse(self.bank['production_ready'])
        self.assertFalse(self.bank['all_requested_features_complete'])
