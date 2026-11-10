"""Bounded getter planning and admission against retained original VM evidence."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
import r01_getter_proofs as m

CATALOG = m.ROOT / 'integration/test/source/deployment-catalog'
BASE = CATALOG / 'evm-normal-v4/base-uniswap-v3-02/getter-execution.json'
POLYGON = m.FRESH_ROOT / 'polygon-refresh-03'
ARBITRUM = m.FRESH_ROOT / 'arbitrum-refresh-03'
PREVIOUS = CATALOG / 'evm-normal-v4/polygon-uniswap-v3-02/input.json'


class GetterPlanTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.original = m.load_json(BASE)
        cls.values = {c['data']: c['value'] for c in cls.original['calls']}
        cls.slot0 = cls.values['0x3850c7bd']
        cls.spacing_word = cls.values['0xd0c93a7c']
        cls.tick = m.signed_word(cls.slot0[66:130])
        cls.spacing = m.signed_word(cls.spacing_word[2:])
        cls.bitmap_data = next(k for k in cls.values if k.startswith('0x5339c296'))
        cls.bitmap = cls.values[cls.bitmap_data]
        cls.plan = m.slot_plan(cls.tick, cls.spacing, cls.bitmap)

    def test_complete_retained_original_vm_sload_set(self):
        # This is a comparison with the VM ledger, not another implementation
        # of Solidity mapping arithmetic. Every observed key must be present.
        self.assertTrue(self.original['passed'])
        self.assertEqual(self.original['missing'], [])
        self.assertEqual(len(self.original['calls']), 264)
        sloads = [s for c in self.original['calls'] for s in c['sloads']]
        self.assertTrue(all({'address', 'key', 'code_address', 'value'} <= set(s)
                            for s in sloads))
        self.assertTrue(all(c['all_code_and_storage_MPT_bound'] for c in self.original['calls']))
        pool = m.load_json(m.ROOT / self.original['input']['path'])['row']['pool']
        self.assertEqual({s['address'] for s in sloads}, {pool})
        self.assertEqual({s['code_address'] for s in sloads}, {pool})
        actual = {s['key'] for s in sloads}
        self.assertEqual(len(actual), 1027)
        self.assertEqual(set(self.plan['keys']), actual)
        self.assertEqual(len(self.plan['keys']), len(actual))
        tick_calls = [c for c in self.original['calls'] if c['data'].startswith('0xf30dba93')]
        self.assertEqual(len(tick_calls), 256)
        self.assertEqual(self.plan['ticks'], [m.signed_word(c['data'][10:]) for c in tick_calls])
        # Each four-word Tick.Info group is checked against its own VM call.
        for index, call in enumerate(tick_calls):
            self.assertEqual(set(self.plan['keys'][3 + index * 4:7 + index * 4]),
                             {s['key'] for s in call['sloads']})
        print('retained original VM: 264 calls / 1027 exact keys; source sha256=' +
              hashlib.sha256(BASE.read_bytes()).hexdigest())

    def test_negative_tick_floor_and_word_edges(self):
        for tick, spacing, word in [(-1, 10, -1), (-10, 10, -1), (-2559, 10, -1),
                                    (-2560, 10, -1), (-2561, 10, -2),
                                    (0, 10, 0), (2559, 10, 0), (2560, 10, 1)]:
            with self.subTest(tick=tick):
                p = m.slot_plan(tick, spacing, '0x1')
                self.assertEqual(p['word'], word)
                self.assertEqual(p['ticks'], [word * 256 * spacing])
                self.assertEqual(len(p['keys']), 7)
        self.assertLess(self.tick, 0)
        self.assertNotEqual(self.tick % self.spacing, 0)
        self.assertEqual(self.plan['word'], self.tick // self.spacing // 256)

    def test_empty_bitmap_and_bounds(self):
        for tick in (-887272, 887272):
            self.assertEqual(len(m.slot_plan(tick, 1, '0x0')['keys']), 3)
        self.assertEqual(m.slot_plan(887272, 1, hex(1 << (887272 % 256)))['ticks'], [887272])
        self.assertEqual(m.slot_plan(-887272, 1, hex(1 << (-887272 % 256)))['ticks'], [-887272])
        self.assertEqual(len(m.slot_plan(0, 32767, '0x1')['keys']), 7)

    def test_malformed_and_out_of_bounds_plans(self):
        cases = [(True, 10, '0x0'), (1.5, 10, '0x0'), ('1', 10, '0x0'),
                 (-887273, 10, '0x0'), (887273, 10, '0x0'),
                 (0, False, '0x0'), (0, 0, '0x0'), (0, -1, '0x0'),
                 (0, 32768, '0x0'), (0, 1.5, '0x0'),
                 (0, 10, None), (0, 10, '0x'), (0, 10, '0x-1'),
                 (0, 10, '0x' + 'f' * 65), (0, 10, '0x1_0'),
                 (887272, 1, hex(1 << 255)), (-887272, 1, '0x1')]
        # One Node process exercises the exported planner's rejection surface.
        script = """const {slotsForWord}=require(process.argv[1]);
        const assert=require('node:assert/strict');
        for(const args of JSON.parse(process.argv[2]))assert.throws(()=>slotsForWord(...args));"""
        run = subprocess.run(['node', '-e', script, str(TOOLS / 'r01_getter_slots.cjs'),
                              json.dumps(cases)], capture_output=True, text=True, timeout=20)
        self.assertEqual(run.returncode, 0, run.stderr)

    def test_cli_input_budget(self):
        run = subprocess.run(['node', str(TOOLS / 'r01_getter_slots.cjs')],
                             input=' ' * 4097, text=True, capture_output=True, timeout=20)
        self.assertNotEqual(run.returncode, 0)
        self.assertIn('GETTER_PLAN_INPUT_BUDGET', run.stderr)

    def test_signed_words_are_strict_hex_and_signed(self):
        self.assertEqual(m.signed_word('f' * 64), -1)
        self.assertEqual(m.signed_word('0' * 64), 0)
        for word in (None, 1, '0' * 63, '0' * 65, '0' * 63 + '_',
                     ' ' + '0' * 63, '+' + '0' * 63, '0' * 63 + 'g'):
            with self.subTest(word=word), self.assertRaisesRegex(ValueError, 'GETTER_ABI_WORD'):
                m.signed_word(word)

    def test_closure_accepts_actual_vm_and_rejects_tampering(self):
        row = m.load_json(m.ROOT / self.original['input']['path'])['row']
        m.verify_closure(self.original, row, self.plan, self.slot0, self.spacing_word, self.bitmap)
        changes = [({'anchor': {'height': 1, 'hash': '0x00'}}, 'GETTER_REPLAY_ANCHOR'),
                   ({'passed': False}, 'GETTER_PROOF_CLOSURE_INCOMPLETE'),
                   ({'normal_admission_rejection': 'rejected'}, 'GETTER_PROOF_CLOSURE_INCOMPLETE'),
                   ({'public_network_requests': 1}, 'GETTER_REPLAY_AUTHORITY')]
        for fields, error in changes:
            with self.subTest(fields=fields), self.assertRaisesRegex(ValueError, error):
                m.verify_closure(self.original | fields, row, self.plan,
                                 self.slot0, self.spacing_word, self.bitmap)
        short = copy.deepcopy(self.original)
        short['calls'].pop()
        with self.assertRaisesRegex(ValueError, 'GETTER_REPLAY_CALL_SET'):
            m.verify_closure(short, row, self.plan, self.slot0, self.spacing_word, self.bitmap)
        tampered = copy.deepcopy(self.original)
        tampered['calls'][-1]['sloads'][0]['key'] = '0x' + '0' * 64
        with self.assertRaisesRegex(ValueError, 'GETTER_VM_SLOT_SET_DIFFERENCE'):
            m.verify_closure(tampered, row, self.plan, self.slot0, self.spacing_word, self.bitmap)
        with self.assertRaisesRegex(ValueError, 'GETTER_ABI_VM_DIFFERENCE'):
            m.verify_closure(self.original, row, self.plan, self.slot0,
                             self.spacing_word, '0x0')

    def test_complete_replay_plan_matches_authenticated_read_set(self):
        row = m.load_json(m.ROOT / self.original['input']['path'])['row']
        self.assertEqual(m.replay_slot_plan(self.original, row), self.plan)
        broken = copy.deepcopy(self.original)
        broken['calls'][-1]['sloads'].pop()
        # A success flag cannot replace the exact read-set closure.
        with self.assertRaisesRegex(ValueError, 'GETTER_VM_SLOT_SET_DIFFERENCE'):
            m.replay_slot_plan(broken, row)

    def test_complete_new_replay_never_reacquires_public_state(self):
        row = m.load_json(m.ROOT / self.original['input']['path'])['row']
        with tempfile.TemporaryDirectory(prefix='r01-getter-fastpath-', dir=m.OUTPUT_ROOT) as tmp:
            folder = Path(tmp)
            log = folder / 'worker.log'
            log.write_text('mocked worker boundary; real VM execution has a separate test\n')
            def worker(input_path, output, stem, report):
                report[stem + '_probe'] = m.reference(BASE)
                report[stem + '_replay_log'] = m.reference(log)
                return self.original
            with patch.object(m, 'OUTPUT_ROOT', folder), \
                    patch.object(m, 'fresh_row', return_value=(row, {})), \
                    patch.object(m, 'replay', side_effect=worker) as run, \
                    patch.object(m, 'fetch_rpc', side_effect=AssertionError('unexpected public read')):
                result = m.capture(PREVIOUS, POLYGON, folder / 'r01-getter-complete')
            run.assert_called_once()
            self.assertTrue(result['passed'])
            self.assertFalse(result['full_R01_qualified'])
            self.assertEqual(result['rpc_attempts'], 0)
            self.assertEqual(result['proof_slots'], 1027)
            self.assertEqual(result['getter_probe_copy_of'], m.reference(BASE))
            copied = m.ROOT / result['getter_probe']['path']
            self.assertEqual(copied.read_bytes(), BASE.read_bytes())

    def test_rerun_original_vm_against_retained_mpt(self):
        # Reexecute a real ticks getter; retain no modified runtime/state fixtures.
        script = """const fs=require('node:fs'),path=require('node:path'),assert=require('node:assert/strict');
        const m=require(process.argv[1]),g=JSON.parse(fs.readFileSync(process.argv[2]));
        const input=JSON.parse(fs.readFileSync(g.input.path));
        (async()=>{const bank=await m.loadBank(input.row.directory,
          input.supplements.map(r=>r.path));const vm=await m.runtime(bank);
        try{const expected=g.calls.find(c=>c.data.startsWith('0xf30dba93'));
          const actual=await vm.view(expected.to,expected.data);
          assert.equal(actual.error,null);assert.equal(actual.value,expected.value);
          assert.equal(actual.all_code_and_storage_MPT_bound,true);
          assert.deepEqual(actual.sloads,expected.sloads);
          console.log('original VM/MPT ticks rerun: '+actual.sloads.length+' exact SLOAD entries');
        }finally{await vm.close();}})().catch(e=>{console.error(e.stack);process.exitCode=1;});"""
        run = subprocess.run(['node', '-e', script, str(TOOLS / 'r01_public_catalog_evm_readonly.cjs'),
                              str(BASE)], cwd=m.ROOT, capture_output=True, text=True, timeout=60)
        self.assertEqual(run.returncode, 0, run.stderr)
        print(run.stdout.strip())


class FreshAdmissionTests(unittest.TestCase):
    def setUp(self):
        self.previous = m.load_json(PREVIOUS)
        self.files = {name: m.load_json(POLYGON / name) for _, name in m.INPUT_FILES}
        self.original_load = m.load_json

    def admit(self, mutate=None, previous=None):
        files = copy.deepcopy(self.files)
        if mutate:
            mutate(files)
        def load(path):
            path = Path(path)
            return files[path.name] if path.parent == POLYGON and path.name in files else self.original_load(path)
        with patch.object(m, 'load_json', side_effect=load):
            return m.fresh_row(self.previous if previous is None else previous, POLYGON)

    def test_actual_fresh_polygon_admission(self):
        row, anchor = self.admit()
        self.assertEqual(row['anchor']['height'], 96898774)
        self.assertEqual(row['anchor']['hash'], anchor['anchor']['hash'])
        self.assertEqual(row['inputs']['prestate'], m.reference(POLYGON / 'prestate-evidence.json'))
        self.assertNotIn('source_runtime', row['inputs'])

    def test_chain_and_version_mismatches(self):
        for mutation, error in [
            (lambda f: f['anchor.json'].__setitem__('schema_version', 3), 'GETTER_FRESH_VERSION'),
            (lambda f: f['prestate-evidence.json'].__setitem__('schema_version', 1), 'GETTER_FRESH_VERSION'),
            (lambda f: f['prestate-evidence.json'].__setitem__('chain', 'arbitrum'), 'GETTER_CHAIN_BINDING'),
            (lambda f: f['prestate-evidence.json'].__setitem__('chain_id', '42161'), 'GETTER_CHAIN_BINDING'),
            (lambda f: f['anchor.json']['anchor'].__setitem__('chain_id', 42161), 'GETTER_CHAIN_BINDING'),
            (lambda f: f['fork-execution.json'].__setitem__('chain_id', 42161), 'GETTER_CHAIN_BINDING'),
        ]:
            with self.subTest(error=error), self.assertRaisesRegex(ValueError, error):
                self.admit(mutation)
        with self.assertRaisesRegex(ValueError, 'GETTER_PREVIOUS_VERSION'):
            self.admit(previous=self.previous | {'schema_version': 5})
        bad = copy.deepcopy(self.previous)
        bad['row']['chain_id'] = 42161
        with self.assertRaisesRegex(ValueError, 'GETTER_CHAIN_BINDING'):
            self.admit(previous=bad)

    def test_anchor_root_timestamp_genesis_runtime_and_state_mismatches(self):
        for mutation, error in [
            (lambda f: f['anchor.json']['anchor'].__setitem__('height', self.previous['row']['anchor']['height']), 'GETTER_NEW_ANCHOR_REQUIRED'),
            (lambda f: f['prestate-evidence.json'].__setitem__('expected_block_hash', '0x00'), 'GETTER_HEADER_BINDING'),
            (lambda f: f['anchor.json']['anchor'].__setitem__('hash', '0x00'), 'GETTER_HEADER_BINDING'),
            (lambda f: f['anchor.json']['anchor'].__setitem__('state_root', '0x00'), 'GETTER_HEADER_BINDING'),
            (lambda f: f['anchor.json']['anchor'].__setitem__('timestamp', 1), 'GETTER_HEADER_BINDING'),
            (lambda f: f['anchor.json']['anchor']['native_state_semantics'].__setitem__('state_height', 1), 'GETTER_STATE_ANCHOR_BINDING'),
            (lambda f: f['anchor.json'].__setitem__('genesis_hash', '0x00'), 'GETTER_GENESIS_BINDING'),
            (lambda f: next(d for d in f['prestate-evidence.json']['deployments'] if d['address'] == self.previous['row']['pool']).__setitem__('code', '0x00'), 'GETTER_RUNTIME_VERSION_CHANGED'),
            (lambda f: f['fork-execution.json']['anchor'].__setitem__('timestamp', 1), 'GETTER_EXECUTION_ANCHOR'),
            (lambda f: f['fork-execution.json'].__setitem__('storage_overrides', [{}]), 'GETTER_ORIGINAL_STATE_REQUIRED'),
        ]:
            with self.subTest(error=error), self.assertRaisesRegex(ValueError, error):
                self.admit(mutation)

    def test_source_digest_and_deployment_identity_mismatch(self):
        for mutation, error in [
            (lambda f: f['anchor.json']['header'].__setitem__('sha256', '0' * 64), 'BLOB_HASH'),
            (lambda f: f['anchor.json']['genesis'].__setitem__('sha256', '0' * 64), 'BLOB_HASH'),
            (lambda f: f['execution-plan.json']['cases'][0].__setitem__('pool', '0x00'), 'GETTER_DEPLOYMENT_BINDING'),
            (lambda f: f['prestate-evidence.json']['deployments'].append(f['prestate-evidence.json']['deployments'][0]), 'GETTER_DUPLICATE_ACCOUNT'),
        ]:
            with self.subTest(error=error), self.assertRaisesRegex(ValueError, error):
                self.admit(mutation)

    def test_actual_arbitrum_missing_accounts_stay_rejected(self):
        previous = m.load_json(CATALOG / 'evm-normal-v4/arbitrum-uniswap-v3-02/input.json')
        evidence = m.load_json(ARBITRUM / 'prestate-evidence.json')
        self.assertEqual({p['address'] for p in evidence['missing_proofs']}, {
            '0x8b194beae1d3e0788a1a35173978001acdfba668',
            '0x9d271a4e9523d74572b618ec10419a0a330e1bf0'})
        with self.assertRaisesRegex(ValueError, 'GETTER_BASE_PRESTATE_INCOMPLETE'):
            m.fresh_row(previous, ARBITRUM)

    def test_proof_key_set_cannot_duplicate_omit_or_overflow(self):
        proof = {'address': '0xabc', 'accountProof': ['0xc0'],
                 'storageProof': [{'key': '0x0', 'proof': []}, {'key': '0x4', 'proof': []}]}
        keys = ['0x' + format(n, '064x') for n in (0, 4)]
        m.verify_proof_candidate(proof, '0xABC', keys)
        for storage in ([{'key': '0x0'}, {'key': '0x0'}], [{'key': '0x0'}],
                        [{'key': '0x' + 'f' * 65}], [{'key': '-1'}]):
            with self.subTest(storage=storage), self.assertRaisesRegex(ValueError, 'GETTER_PROOF_KEY'):
                m.verify_proof_candidate(proof | {'storageProof': storage}, '0xabc', keys)
        with self.assertRaisesRegex(ValueError, 'GETTER_PROOF_ADDRESS'):
            m.verify_proof_candidate(proof, '0xdef', keys)

    def test_output_scope_and_rejection_report_without_network(self):
        with tempfile.TemporaryDirectory(prefix='r01-getter-test-', dir=m.OUTPUT_ROOT) as tmp:
            with patch.object(m, 'OUTPUT_ROOT', Path(tmp)), patch.object(m, 'fetch_rpc') as fetch:
                with self.assertRaisesRegex(ValueError, 'GETTER_NEW_OUTPUT'):
                    m.capture(PREVIOUS, POLYGON, Path(tmp) / 'outside-prefix')
                previous = CATALOG / 'evm-normal-v4/arbitrum-uniswap-v3-02/input.json'
                output = Path(tmp) / 'r01-getter-rejected'
                report = m.capture(previous, ARBITRUM, output)
                self.assertFalse(report['passed'])
                self.assertFalse(report['full_R01_qualified'])
                self.assertEqual(report['error'], 'GETTER_BASE_PRESTATE_INCOMPLETE')
                self.assertEqual(m.load_json(output / 'report.json'), report)
                fetch.assert_not_called()
                with self.assertRaisesRegex(ValueError, 'GETTER_NEW_OUTPUT'):
                    m.capture(previous, ARBITRUM, output)
                with self.assertRaisesRegex(ValueError, 'GETTER_FRESH_DIRECTORY'):
                    m.capture(PREVIOUS, CATALOG / 'evm-continuation-v2/polygon-01',
                              Path(tmp) / 'r01-getter-old')


if __name__ == '__main__':
    unittest.main()
