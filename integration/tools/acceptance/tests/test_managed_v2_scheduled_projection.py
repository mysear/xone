"""Independent original getter projections, with every field retained."""
import copy
import importlib
import json
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / 'integration/tools/acceptance'))


class ScheduledProjectionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.projection = importlib.import_module('managed_v2_scheduled_projection')
        cls.bank = cls.projection.load(ROOT / 'integration/test/contracts/managed-v2-scheduled.json')
        cls.thin = cls.projection.project(cls.bank)

    def test_complete_inventory_and_source_clocks(self):
        self.assertEqual(len(self.thin['states']), 234)
        self.assertEqual(len(self.thin['cases']), 53)
        self.assertEqual(len(self.thin['checkpoints']), 41)
        self.assertEqual(len(self.thin['collections']), 2)
        for ident, p in self.thin['states'].items():
            branch, state = ident.split('/')
            raw = next(s for s in self.bank['branches'][branch]['states'] if s['id'] == state)
            m = p['multi_asset']['managed_v2']
            self.assertEqual(m['block_hash'], raw['header']['hash'])
            self.assertEqual(m['timestamp'], int(raw['header']['timestamp'], 16))
            self.assertEqual(m['block_number'], int(raw['header']['number'], 16))
            self.assertNotIn('execution_context', m)
            self.assertEqual(m['schedule']['pool_state_word'], str(int(raw['packed']['pool_word'], 16)))

    def test_output_and_poststate_only_from_original_observations(self):
        for c in self.thin['cases']:
            original = next(x for x in self.bank['cases'] if x['id'] == c['id'])
            self.assertEqual(c['expected_raw_out'], original['expected_raw_out'])
            after = copy.deepcopy(self.thin['states'][c['after']])
            m = after['multi_asset']['managed_v2']
            source = self.thin['states'][c['before']]['multi_asset']['managed_v2']
            expected_clock = {'block_number': m['block_number'], 'timestamp': m['timestamp']}
            for k in ('block_number', 'timestamp', 'block_hash'):
                m[k] = source[k]
            m['execution_context'] = expected_clock
            self.assertEqual(c['expected_pool_after'], after)
        sums = {sum(map(int, p['multi_asset']['weights'])) - 10**18
                for p in self.thin['states'].values()}
        self.assertTrue({-2, -1, 0, 1, 2}.issubset(sums))

    def test_collection_links_resolve_to_original_signed_transactions(self):
        for c in self.thin['collections']:
            ns = c['id'].split('/')[0]
            branch = self.bank['branches'][ns]
            ref = c['transaction'].split('/')
            self.assertEqual(ref, [ns, branch['aum_collection']['transaction']])
            tx = next(t for t in branch['transactions'] if t['id'] == ref[1])
            self.assertEqual(tx['method'], 'collectAumManagementFees')
            self.assertEqual(tx['receipt']['status'], '0x1')

    def test_changed_qualified_bank_rejected(self):
        with self.assertRaisesRegex(ValueError, 'qualified'):
            self.projection.project(json.loads(json.dumps(self.bank)))
        mutated = copy.deepcopy(self.bank)
        mutated['branches']['three']['states'][0]['getters'].pop()
        with self.assertRaisesRegex(ValueError, 'changed|seal'):
            self.projection.project(mutated)


class ScheduledProjectionBoundaryTests(unittest.TestCase):
    def test_output_boundary_protects_raw_inputs_and_existing_inodes(self):
        import tempfile
        module = importlib.import_module('managed_v2_scheduled_projection')
        source = ROOT / 'integration/test/contracts/managed-v2-scheduled.json'
        with tempfile.TemporaryDirectory(prefix='scheduled-projection-boundary-') as folder:
            tmp = Path(folder)
            target = tmp / 'new-projection.json'
            self.assertEqual(module.output_path(target, source), target.resolve())
            for path in (source, source.with_name('managed-v2-aum.json'),
                         ROOT / 'integration/tools/reference/balancer-managed-v2-scheduled/README.md',
                         ROOT / 'integration/test/contracts/unregistered.json', tmp / 'managed-v2-aum.json'):
                with self.assertRaises(ValueError):
                    module.output_path(path, source)
            explicit_original = tmp / 'original-copy.json'
            with self.assertRaises(ValueError):
                module.output_path(explicit_original, source, original_input=explicit_original)
            target.symlink_to(source)
            with self.assertRaises(ValueError):
                module.output_path(target, source)
            target.unlink()
            original = tmp / 'existing.json'
            original.write_text('preserve')
            target.hardlink_to(original)
            with self.assertRaises(ValueError):
                module.output_path(target, source)
            self.assertEqual(original.read_text(), 'preserve')

    def test_cli_preserves_explicit_original_input_bytes_and_mtime(self):
        import os
        import shutil
        import subprocess
        import tempfile
        with tempfile.TemporaryDirectory(prefix='scheduled-projection-original-') as folder:
            original = Path(folder) / 'original-copy.json'
            shutil.copy2(ROOT / 'integration/test/contracts/managed-v2-aum.json', original)
            before = (original.read_bytes(), original.stat().st_mtime_ns)
            command = [sys.executable, str(ROOT / 'integration/tools/acceptance/managed_v2_scheduled_projection.py'),
                       '--input', str(ROOT / 'integration/test/contracts/managed-v2-scheduled.json'),
                       '--original-input', str(original), '--output', str(original)]
            result = subprocess.run(command, cwd=ROOT, capture_output=True, text=True,
                                    env={**os.environ, 'PATH': '/usr/local/bin:' + os.environ['PATH']})
            self.assertNotEqual(result.returncode, 0)
            self.assertIn('input overwrite refused', result.stderr)
            self.assertEqual((original.read_bytes(), original.stat().st_mtime_ns), before)
