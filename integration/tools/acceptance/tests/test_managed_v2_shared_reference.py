"""T34.9 native reference only; no routing/optimizer completion assertion."""
import copy
import hashlib
import json
import os
import shutil
from pathlib import Path
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[4]
REF = ROOT / 'integration/tools/reference/balancer-managed-v2-shared'
CLI = ROOT / 'integration/tools/ci/managed-v2-shared-vault-golden.cjs'
NODE = '/usr/local/bin/node25.3.0' if Path('/usr/local/bin/node25.3.0').exists() else '/usr/local/bin/node'
OUTPUTS = [ROOT / p / 'test/contracts/managed-v2-shared-vault.json' for p in ('integration', 'chain-parser-go', 'chain-parser-rust', 'dex-router')]
ORIGINAL = ROOT / 'integration/tools/reference/balancer-managed-v2-aum'


def fingerprint(paths):
    return {str(p): (hashlib.sha256(p.read_bytes()).hexdigest(), p.stat().st_mtime_ns, p.stat().st_size) for p in paths}


def managed(snapshot):
    return snapshot['pool']['multi_asset']['managed_v2']


class SharedReference(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        required = [CLI] + [REF / n for n in ('generate.cjs', 'validate.cjs', 'shared_checks.cjs', 'README.md')]
        for p in required:
            if not p.is_file():
                raise AssertionError('missing formal shared reference script: ' + str(p))
        cls.tmp = tempfile.TemporaryDirectory(prefix='managed-v2-shared-test-')
        cls.output = Path(cls.tmp.name) / 'bank.json'
        cls.frozen = [ROOT / f['path'] for f in json.loads((ORIGINAL / 'file-manifest.json').read_text())['files']]
        cls.frozen += [ORIGINAL / 'file-manifest.json'] + [ROOT / p / 'test/contracts/managed-v2-aum.json' for p in ('integration', 'chain-parser-go', 'dex-router')]
        cls.before = fingerprint(cls.frozen)
        result = cls.run_cli('--output'+str(cls.output))
        if result.returncode:
            raise AssertionError(result.stdout + result.stderr)
        cls.raw = cls.output.read_bytes()
        cls.value = json.loads(cls.raw)

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    @classmethod
    def run_cli(cls, *args):
        return subprocess.run([NODE, str(CLI), *args], cwd=ROOT.parent, text=True, capture_output=True, timeout=300)

    def validate_edit(self, edit):
        value = copy.deepcopy(self.value)
        edit(value)
        target = Path(self.tmp.name) / 'tampered.json'
        target.write_text(json.dumps(value))
        before = fingerprint([target])
        result = self.run_cli('--validate', str(target))
        self.assertNotEqual(result.returncode, 0, result.stdout)
        self.assertEqual(fingerprint([target]), before)

    def test_native_two_pool_scopes_and_full_common_h(self):
        c = self.value
        self.assertEqual(c['public_adapter_version'], 'balancer-v2/20230411-managed-aum-3')
        self.assertTrue(c['generator']['collect_30d'])
        self.assertEqual(c['counts']['banks'], 7)
        self.assertEqual(c['counts']['raw_getters'], 1064)
        self.assertEqual(c['counts']['new_transactions'], 15)
        self.assertEqual(c['prefix_replay']['count'], 28)
        self.assertEqual([x['expected_native']['status'] for x in c['cases']], [1]*5)
        self.assertEqual([x['scenario'] for x in c['cases']], ['PoolA', 'PoolB', 'PoolA', 'PoolB', 'PoolA'])
        self.assertEqual([int(b['header']['number'],16) for b in c['banks'].values()], list(range(38,45)))
        for b in c['banks'].values():
            self.assertEqual(len(b['pools']), 2)
            self.assertEqual(b['getter_count'], 152)
            for s in b['pools']:
                m = managed(s)
                self.assertEqual(len(m), 39)
                self.assertEqual(s['header'], b['header'])
                self.assertEqual(m['block_hash'], b['header']['hash'])
                self.assertEqual(s['getter_count'], len(s['getters']))
                for g in s['getters']:
                    self.assertEqual(g['block_tag'], b['header']['number'])
                    self.assertEqual(g['block_hash'], b['header']['hash'])
        self.assertEqual(len(c['scenarios'][0]['tokens']), 2)
        self.assertEqual(c['scenarios'][0]['tokens'], c['scenarios'][1]['tokens'])
        self.assertEqual(c['original_bank_bytes'], 7905624)
        self.assertEqual(c['original_bank_sha256'], '1ee7ec5dd981994af6dd42eb75717cf83a5b209857883fa2923885ce7181fb66')

    def test_wallet_propagation_counterexample_and_independent_bpt_ledgers(self):
        c = self.value
        for case in c['cases']:
            a, b = (c['banks'][case[k]] for k in ('source_bank2pools','expected_after_bank2pools'))
            selected = next(i for i,s in enumerate(a['pools']) if s['scenario'] == case['scenario'])
            other = 1-selected
            am, bm = managed(a['pools'][other]), managed(b['pools'][other])
            self.assertNotEqual(a['bpt_ledgers'][0]['pool'], a['bpt_ledgers'][1]['pool'])
            for key in ('total_supply','virtual_supply','actual_supply','pending_aum','manager_bpt_balance','protocol_collector_bpt_balance'):
                self.assertEqual(am[key],bm[key])
            for i in range(2):
                for key in ('cash','managed','last_change_block'):
                    self.assertEqual(am['token_info'][i][key],bm['token_info'][i][key])
                before = [managed(s)['token_info'][i]['vault_actual_balance'] for s in a['pools']]
                after = [managed(s)['token_info'][i]['vault_actual_balance'] for s in b['pools']]
                self.assertEqual(before[0],before[1]); self.assertEqual(after[0],after[1])
                if case['operation'] == 'swap':
                    self.assertNotEqual(before[other],after[other], 'selected-only wallet overlay loses shared wallet change')
        collect = c['cases'][-1]
        self.assertGreater(int(collect['expected_native']['minted_bpt']),0)
        self.assertEqual(int(collect['expected_native']['minted_bpt']),int(collect['expected_native']['manager_bpt_delta'])+int(collect['expected_native']['protocol_collector_bpt_delta']))

    def test_auth_closure_and_original_bytes_preserved(self):
        c = self.value
        self.assertEqual(fingerprint(self.frozen),self.before)
        self.assertEqual(len(c['original_reference']['node_closure']['files']),359)
        self.assertEqual(c['source_preservation']['before'],c['source_preservation']['after'])
        paths = [x['path'] for x in c['script_closure']]
        self.assertEqual(len(paths), len(set(paths)))
        for f in c['script_closure']:
            self.assertFalse(Path(f['path']).is_absolute())
            self.assertEqual(hashlib.sha256((ROOT / f['path']).read_bytes()).hexdigest(),f['sha256'])
        text = b'\n'.join(p.read_bytes() for p in REF.glob('*.cjs'))
        self.assertNotIn(b'/tmp/xone-',text)
        self.assertNotIn(b'ORIGINAL_CATALOG',text)
        self.assertNotIn(b'Date.now(',text)

    def test_decoding_validate_and_precise_single_copy_regeneration_readonly(self):
        before = fingerprint(self.frozen+OUTPUTS+[self.output])
        for args in (('--validate',str(self.output)),('--check','--input',str(self.output))):
            result = self.run_cli(*args)
            self.assertEqual(result.returncode,0,result.stdout+result.stderr)
            self.assertTrue(json.loads(result.stdout)['passed'])
        self.assertEqual(fingerprint(self.frozen+OUTPUTS+[self.output]),before)

    def test_all_service_copies_and_default_exact_regeneration_readonly(self):
        for output in OUTPUTS:
            self.assertEqual(output.read_bytes(),self.raw)
        before = fingerprint(self.frozen+OUTPUTS)
        result = self.run_cli('--check')
        self.assertEqual(result.returncode,0,result.stdout+result.stderr)
        self.assertTrue(json.loads(result.stdout)['exact_regeneration'])
        self.assertEqual(fingerprint(self.frozen+OUTPUTS),before)

    def test_fail_closed_mutations(self):
        def wallet(v):
            x=v['cases'][0]; a=v['banks'][x['source_bank2pools']]; b=v['banks'][x['expected_after_bank2pools']]
            managed(b['pools'][1])['token_info'][0]['vault_actual_balance']=managed(a['pools'][1])['token_info'][0]['vault_actual_balance']
        def actor(v):
            case=v['cases'][0];after=v['banks'][case['expected_after_bank2pools']]['pools'][0]
            trader=case['execution']['transaction']['from']
            token=case['expected_native']['swap_event']['token_out']
            row=next(g for g in after['getters'] if g['to']==token and g['method']=='balanceOf' and g['args']==[trader])
            row['result']='0x'+format(int(row['result'],16)+1,'064x')
        edits = [
            actor,
            lambda v: v['generator'].update(actual_node='/wrong-node'),
            wallet,
            lambda v: v['banks'][v['initial_bank2pools']]['pools'].pop(),
            lambda v: v['banks'][v['initial_bank2pools']]['pools'][0]['getters'].pop(),
            lambda v: v['banks'][v['initial_bank2pools']]['pools'][0]['getters'][0].update(block_hash='0x'+'00'*32),
            lambda v: v['cases'][0]['expected_native'].update(amount_out='1'),
            lambda v: v['banks'][v['initial_bank2pools']]['pools'][0]['pool']['multi_asset']['managed_v2']['token_info'][0].update(cash='1'),
            lambda v: v['banks'][v['final_bank2pools']]['pools'][1]['pool']['multi_asset']['managed_v2'].update(manager_bpt_balance='1'),
            lambda v: v['banks'][v['initial_bank2pools']]['pools'][0]['getters'][-1].update(result='0x'+'00'*32),
            lambda v: v['cases'][0]['execution'].update(signed_transaction='0x00'),
            lambda v: v['identities'][0].update(code_hex='0x00'),
            lambda v: v['original_reference']['input_closure']['files'].pop(),
            lambda v: v['script_closure'].pop(),
            lambda v: v['source_preservation']['before'].append({'path':'unknown.json','sha256':'0'*64}),
            lambda v: v['cases'][0]['execution']['decoded_events'].clear(),
            lambda v: v['counts'].update(raw_getters=1),
            lambda v: v['banks'].pop(v['clock_bank2pools']),
            lambda v: v['identities'].pop(),
            lambda v: v['factory_creation_code_storage'].pop(),
            lambda v: v['original_reference'].update(source_pins_sha256='0'*64),
            lambda v: v['original_reference']['compiler_jobs'].clear(),
            lambda v: v['cases'].pop(),
            lambda v: v['headers'].pop(),
            lambda v: v['transactions'].pop(0),
            lambda v: v['generator'].update(collect_30d=False),
            lambda v: v['cases'][0]['expected_native']['swap_event'].update(amount_out='1'),
            lambda v: v['banks'][v['initial_bank2pools']]['pools'][0]['pool'].update(dex='unknown'),
        ]
        for edit in edits:
            with self.subTest(edit=repr(edit)):
                self.validate_edit(edit)

    def test_clean_repo_inputs_unknown_member_missing_artifact_tamper(self):
        # A private copy proves relocation and negative input authentication without
        # touching a single original source, runtime, manifest or output.
        clone = Path(self.tmp.name) / 'clean-repo'
        clone.mkdir()
        for row in self.value['source_preservation']['before'] + self.value['script_closure']:
            source = ROOT / row['path']; target = clone / row['path']
            target.parent.mkdir(parents=True,exist_ok=True)
            shutil.copyfile(source,target)
        for row in self.value['original_reference']['node_closure']['files']:
            source=ROOT/row['path'];target=clone/row['path']
            target.parent.mkdir(parents=True,exist_ok=True)
            shutil.copyfile(source,target)
        command=[NODE,str(clone/CLI.relative_to(ROOT)),'--validate',str(self.output)]
        def run():
            return subprocess.run(command,cwd=self.tmp.name,text=True,capture_output=True,timeout=120)
        good=run(); self.assertEqual(good.returncode,0,good.stdout+good.stderr)
        self.assertNotIn('ganache',good.stderr.lower())
        clean_output=Path(self.tmp.name)/'clean-native.json'
        generated=subprocess.run([NODE,str(clone/CLI.relative_to(ROOT)),'--output',str(clean_output)],cwd=self.tmp.name,text=True,capture_output=True,timeout=300)
        self.assertEqual(generated.returncode,0,generated.stdout+generated.stderr)
        self.assertEqual(clean_output.read_bytes(),self.raw,'clean relocated repo and exact pinned runtime reproduce all bytes')
        unknown=clone/ORIGINAL.relative_to(ROOT)/'unknown.json'
        unknown.write_text('{}')
        self.assertNotEqual(run().returncode,0);unknown.unlink()
        artifact=next(clone/f['path'] for f in self.value['original_reference']['input_closure']['files'] if '/build-info/' in f['path'])
        original=artifact.read_bytes()
        artifact.write_bytes(original+b' ')
        self.assertNotEqual(run().returncode,0)
        artifact.write_bytes(original)
        artifact.unlink()
        self.assertNotEqual(run().returncode,0)
        artifact.write_bytes(original)
        manifest=clone/ORIGINAL.relative_to(ROOT)/'file-manifest.json'
        original=manifest.read_bytes();manifest.write_bytes(original+b' ')
        self.assertNotEqual(run().returncode,0)
        manifest.write_bytes(original)
        self.assertEqual(fingerprint(self.frozen),self.before)

    def test_explicit_modes_and_protected_output_guards(self):
        bad = [('--input',str(self.output)),('--check','--output',str(self.output)),('--validate',str(self.output),'--check'),('--output',str(OUTPUTS[0])),('--output','relative.json'),('--collect-30d',)]
        for args in bad:
            r=self.run_cli(*args); self.assertNotEqual(r.returncode,0,r.stdout)
        for target in (ROOT/'integration/test/contracts/managed-v2-aum.json', ORIGINAL/'output-0.pending', REF/'validate.cjs'):
            before=fingerprint([target]) if target.exists() else {}
            r=self.run_cli('--output',str(target)); self.assertNotEqual(r.returncode,0,r.stdout)
            self.assertEqual(fingerprint([target]) if target.exists() else {},before)
        link=Path(self.tmp.name)/'symlink.json'; link.symlink_to(self.output)
        hard=Path(self.tmp.name)/'hardlink.json'; os.link(self.output,hard)
        try:
            for target in (link,hard):
                r=self.run_cli('--output',str(target)); self.assertNotEqual(r.returncode,0,r.stdout)
        finally:
            hard.unlink(); link.unlink()
        parent=Path(self.tmp.name)/'linked-parent';parent.symlink_to(self.tmp.name,target_is_directory=True)
        try:
            r=self.run_cli('--output',str(parent/'out.json'));self.assertNotEqual(r.returncode,0,r.stdout)
        finally:
            parent.unlink()
        r=self.run_cli('--check','--input',str(self.output)+'missing')
        self.assertNotEqual(r.returncode,0)


if __name__ == '__main__':
    unittest.main()
