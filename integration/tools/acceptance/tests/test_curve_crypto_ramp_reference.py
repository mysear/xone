import copy
import importlib.util
import json
import subprocess
import tempfile
from unittest.mock import patch
from types import SimpleNamespace
import unittest
from pathlib import Path
ROOT = Path(__file__).resolve().parents[4]
spec = importlib.util.spec_from_file_location('ramp_verify', ROOT / 'integration/tools/ci/curve-crypto-ramp-verify.py')
m = importlib.util.module_from_spec(spec)
spec.loader.exec_module(m)

class CryptoRampReference(unittest.TestCase):
    def test_public_ramp_and_complete_carried_native_state_cannot_be_truncated(self):
        ref = ROOT / 'integration/tools/reference/curve-crypto-ramp'
        corpus = json.loads((ROOT / 'integration/test/contracts/curve-crypto-ramp-native.json').read_text())
        metadata = json.loads((ref / 'runtime-metadata.json').read_text())
        receipt = json.loads((ref / 'generation.json').read_text())
        self.assertEqual(m.validate(corpus, metadata, receipt)['carried_transitions'], 22)
        for bad in ['missing', 'duplicate', 'name', 'schedule', 'stop', 'balances', 'raw', 'lp', 'trader', 'sentinel', 'endpoint', 'missing-stops', 'duplicate-schedules', 'sequence-membership']:
            document = copy.deepcopy(corpus)
            cases = document['cases']
            if bad == 'missing':
                cases.pop()
            elif bad == 'duplicate':
                cases[-1] = copy.deepcopy(cases[0])
            elif bad == 'name':
                cases[0]['name'] = 'invented'
            elif bad == 'schedule':
                cases[0]['schedule_transaction']['status'] = 0
            elif bad == 'stop':
                cases[12]['stop_transaction']['status'] = 0
            elif bad == 'balances':
                cases[1]['balances'][0] = '1'
            elif bad == 'raw':
                cases[0]['expected']['raw_balances'][0] = '1'
            elif bad == 'lp':
                cases[0]['expected']['total_supply'] = '1'
            elif bad == 'trader':
                cases[0]['expected']['trader_balances'][0] = '1'
            elif bad == 'sentinel':
                cases[4]['expected']['state']['future_A_gamma_time'] = '1'
            elif bad == 'endpoint':
                cases[0]['ramp']['initial_amp'] = '1'
            elif bad == 'missing-stops':
                for case in cases: case.pop('stop_transaction', None)
            elif bad == 'duplicate-schedules':
                for case in cases: case['schedule_transaction'] = copy.deepcopy(cases[0]['schedule_transaction'])
            else:
                for case in cases:
                    if case['sequence'].startswith('stop-'): case['sequence'] = 'invented-' + case['sequence']
            with self.assertRaises(ValueError, msg=bad):
                m.validate(document, metadata, receipt)
        invalid = copy.deepcopy(metadata)
        invalid['allow_unlimited_contract_size'] = True
        with self.assertRaises(ValueError):
            m.validate(corpus, invalid, receipt)

    def test_generator_failure_invalidates_old_success_before_dependency_loading(self):
        with tempfile.TemporaryDirectory(prefix='xone-ramp-generation-fault-') as folder:
            directory = Path(folder)
            output = directory / 'receipt'
            output.mkdir()
            (output / 'generation.json').write_text('{"valid":true}')
            inventory = directory / 'failed-inventory.py'
            inventory.write_text('raise RuntimeError("inventory fault")\n')
            source = (ROOT / 'integration/tools/ci/curve-crypto-ramp-golden.cjs').read_text()
            source = source.replace("path.resolve(__dirname,'../../..')", json.dumps(str(ROOT)), 1)
            source = source.replace("path.join(root,'integration/tools/reference/curve-crypto-ramp')", json.dumps(str(output)), 1)
            source = source.replace("path.join(root,'integration/tools/ci/curve-crypto-ramp-inventory.py')", json.dumps(str(inventory)), 1)
            script = directory / 'generation.cjs'
            script.write_text(source)
            completed = subprocess.run(['node', str(script)], capture_output=True, text=True, timeout=30)
            self.assertNotEqual(completed.returncode, 0)
            receipt = json.loads((output / 'generation.json').read_text())
            self.assertIs(receipt['valid'], False)
            self.assertEqual(receipt['status'], 'failed')
            self.assertIn('inventory fault', receipt['error'])

    def test_executable_startup_hooks_and_bytecode_cache_bytes_are_committed(self):
        inventory = m.inventory_module()
        original_sha = inventory.sha
        # Current frozen venvs contain no bytecode cache. Exercise the exact
        # filesystem walk using a controlled compiler root that contains both
        # executable input forms, without modifying an executing real oracle.
        with tempfile.TemporaryDirectory(prefix='xone-ramp-python-inputs-') as folder:
            compiler = Path(folder)
            package = compiler / 'lib/python3.9/site-packages'
            cache = package / '__pycache__'
            cache.mkdir(parents=True)
            (compiler / 'bin').mkdir()
            (compiler / 'bin/python').write_bytes(b'controlled interpreter')
            (compiler / 'bin/vyper').write_bytes(b'controlled compiler entrypoint')
            (compiler / 'pyvenv.cfg').write_text('include-system-site-packages = false\n')
            (package / 'startup.pth').write_text('import controlled_hook\n')
            (cache / 'controlled.cpython-39.pyc').write_bytes(b'controlled executable bytecode')
            info = SimpleNamespace(stdout=json.dumps({'paths': [], 'stdlib': str(compiler / 'lib/python3.9'), 'platstdlib': str(compiler / 'lib/python3.9')}))
            with patch.object(inventory, 'compiler_roots', return_value=[compiler]), patch.object(inventory.subprocess, 'run', return_value=info):
                before = inventory.expected_inputs()
                for extension in ['.pth', '.pyc']:
                    with patch.object(inventory, 'sha', lambda path: '0' * 64 if path.suffix == extension else original_sha(path)):
                        after = inventory.expected_inputs()
                    self.assertNotEqual(before, after, extension + ' executable input was not hashed')
                    self.assertTrue(any(Path(path).suffix == extension for path in before))

    def test_duplicate_json_inventory_and_source_members_are_rejected_before_projection(self):
        for text in ['{"input_inventory":{"compiler":"a","compiler":"a"}}', '{"source_pins":[],"source_pins":[]}', '{"cases":[{"sequence":"up","sequence":"stop"}]}']:
            with self.assertRaises(ValueError):
                m.parse_json(text)

    def test_independent_source_manifest_rejects_empty_duplicate_or_substituted_inputs(self):
        inventory = m.inventory_module()
        with tempfile.TemporaryDirectory(prefix='xone-ramp-pin-fault-') as folder:
            path = Path(folder)
            for values in [[], [{'local':'fake'}], [{'local':'fake'}] * 5]:
                (path / 'sources.json').write_text(json.dumps(values))
                with patch.object(inventory, 'ORIGINAL', path):
                    with self.assertRaises(ValueError):
                        inventory.pins()

    def test_empty_substituted_sources_and_rehashed_truncated_closure_are_rejected(self):
        pins = [{'local': 'pool', 'sha256': 'original'}]
        inputs = {'node': 'n', 'compiler': 'c', 'pool': 'p'}
        document = {'source_pins': pins}
        metadata = {'source_pins': pins, 'input_inventory_kind': 'complete-fixed-node-and-compiler-filesystem-v1', 'input_inventory': inputs}
        m.validate_inputs(document, metadata, pins, inputs)
        for bad in ['empty-pins', 'duplicate-pins', 'substituted-pins', 'truncated', 'substituted', 'declared-only']:
            d, meta = copy.deepcopy(document), copy.deepcopy(metadata)
            if bad == 'empty-pins': d['source_pins'] = meta['source_pins'] = []
            elif bad == 'duplicate-pins': d['source_pins'] = meta['source_pins'] = pins * 2
            elif bad == 'substituted-pins': d['source_pins'] = meta['source_pins'] = [{'local': 'other', 'sha256': 'original'}]
            elif bad == 'truncated': meta['input_inventory'] = {'node': 'n'}
            elif bad == 'substituted': meta['input_inventory'] = {'other-node': 'n', 'compiler': 'c', 'pool': 'p'}
            else: meta['loaded_dependencies'] = [{'path': 'node', 'sha256': 'n'}]
            with self.assertRaises(ValueError, msg=bad):
                m.validate_inputs(d, meta, pins, inputs)

    def test_derived_reference_verification_is_output_and_native_dependencies_are_inputs(self):
        import sys
        sys.path.insert(0, str(ROOT / 'integration/tools/acceptance'))
        from weighted_evidence import snapshot_inputs
        inputs = snapshot_inputs()
        prefix = 'integration/tools/reference/curve-crypto-ramp/'
        self.assertNotIn(prefix + 'verification.json', inputs)
        self.assertIn(prefix + 'generation.json', inputs)
        self.assertIn('integration/test/contracts/curve-crypto-ramp-native.json', inputs)
        metadata = json.loads((ROOT / prefix / 'runtime-metadata.json').read_text())
        for entry in metadata['input_inventory']:
            path = Path(entry)
            key = str(path.relative_to(ROOT)) if path.is_relative_to(ROOT) else str(path)
            self.assertIn(key, inputs)

if __name__ == '__main__':
    unittest.main()
