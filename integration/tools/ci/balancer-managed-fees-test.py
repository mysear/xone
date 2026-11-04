#!/usr/bin/env python3
"""Isolated failure-atomicity tests; never load or start a shared provider."""
import json
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[3]
SCRIPT = ROOT / 'integration/tools/ci/balancer-managed-fees.cjs'
REV = 'e281ff40583c39bf1d46c322334b2b13936e0e33'


class ReceiptInvalidation(unittest.TestCase):
    def check_failure(self, stage, optimized=False):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            script = root / 'integration/tools/ci/balancer-managed-fees.cjs'
            script.parent.mkdir(parents=True)
            shutil.copyfile(SCRIPT, script)
            ref = root / 'integration/tools/reference/balancer-managed-fees'
            ref.mkdir(parents=True)
            receipt = ref / 'generation.json'
            receipt.write_text('{"passed":true,"stale_success":true}\n')
            if stage != 'missing_dependencies':
                modules = root / 'integration/.runtime/stable-reference/node_modules'
                stubs = {
                    'ethers': 'module.exports={ethers:{}};',
                    'ganache': 'throw Error("provider construction must not be reached");' if stage == 'dependency_exception' else 'module.exports={};',
                    'solc07': 'module.exports={version:()=>"0.7.6+test",compile:()=>JSON.stringify({errors:[{severity:"error",formattedMessage:"forced compile failure"}]})};',
                }
                for name, source in stubs.items():
                    module = modules / name
                    module.mkdir(parents=True)
                    (module / 'index.js').write_text(source)
                pin_dir = root / 'integration/tools/reference/balancer-multi'
                pin_dir.mkdir(parents=True)
                pin = pin_dir / (REV + '.json')
                if stage == 'malformed_pin':
                    pin.write_text('{malformed')
                elif stage == 'source_hash':
                    pin.write_text(json.dumps({'revision': REV, 'files': [{'path': 'test.sol', 'sha256': '0' * 64}]}))
                    source = pin_dir / 'sources' / REV / 'test.sol'
                    source.parent.mkdir(parents=True)
                    source.write_text('changed source')
                else:
                    pin.write_text(json.dumps({'revision': REV, 'files': []}))
                    (ref / 'ManagedFeesHarness.sol').write_text('pragma solidity ^0.7.0;')
            result = subprocess.run(['node', str(script)], capture_output=True, text=True, timeout=30)
            self.assertNotEqual(result.returncode, 0, stage)
            value = json.loads(receipt.read_bytes())
            self.assertIs(value['passed'], False, stage)
            self.assertEqual(value['state'], 'FAILED', stage)
            self.assertNotIn('stale_success', value)
            self.assertFalse((ref / 'runtime-metadata.json').exists())
            self.assertFalse((root / 'integration/test/contracts/balancer-managed-fees.json').exists())

    def test_missing_dependencies_invalidates_old_success(self):
        self.check_failure('missing_dependencies')

    def test_dependency_exception_invalidates_old_success(self):
        self.check_failure('dependency_exception')

    def test_malformed_pin_invalidates_old_success(self):
        self.check_failure('malformed_pin')

    def test_source_hash_mismatch_invalidates_old_success(self):
        self.check_failure('source_hash')

    def test_compiler_failure_invalidates_old_success(self):
        self.check_failure('compiler')


class SourceClosureFailures(unittest.TestCase):
    def check_tamper(self, kind):
        import importlib.util
        spec = importlib.util.spec_from_file_location('managed_fees_verify', SCRIPT.with_name('balancer-managed-fees-verify.py'))
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            ref = root / 'integration/tools/reference/balancer-managed-fees'
            ref.mkdir(parents=True)
            for name in ['generation.json', 'runtime-metadata.json', 'standard-evm-deployment.json', 'ManagedFeesHarness.sol']:
                shutil.copyfile(ROOT / 'integration/tools/reference/balancer-managed-fees' / name, ref / name)
            (root / 'integration/.runtime').symlink_to(ROOT / 'integration/.runtime', target_is_directory=True)
            (ref.parent / 'balancer-multi').symlink_to(ROOT / 'integration/tools/reference/balancer-multi', target_is_directory=True)
            ci = root / 'integration/tools/ci'
            ci.mkdir(parents=True)
            shutil.copyfile(SCRIPT, ci / SCRIPT.name)
            for service in ['integration', 'dex-router']:
                dest = root / service / 'test/contracts/balancer-managed-fees.json'
                dest.parent.mkdir(parents=True)
                shutil.copyfile(ROOT / service / 'test/contracts/balancer-managed-fees.json', dest)
            if kind == 'source':
                (ref / 'ManagedFeesHarness.sol').write_text('tampered source')
            elif kind == 'corpus':
                (root / 'integration/test/contracts/balancer-managed-fees.json').write_text('{}')
            else:
                name = {'receipt': 'generation.json', 'runtime': 'runtime-metadata.json', 'standard_deployment': 'standard-evm-deployment.json'}[kind]
                target = ref / name
                value = json.loads(target.read_bytes())
                if kind == 'receipt':
                    value['passed'] = False
                elif kind == 'runtime':
                    value['immutable_spans'][0]['start'] += 1
                else:
                    value['deployed'] = True
                target.write_text(json.dumps(value))
            with self.assertRaises(ValueError, msg=kind):
                module.verify(root)

    def test_failed_receipt_rejected(self):
        self.check_tamper('receipt')

    def test_changed_original_harness_rejected(self):
        self.check_tamper('source')

    def test_changed_corpus_rejected(self):
        self.check_tamper('corpus')

    def test_changed_immutable_spans_rejected(self):
        self.check_tamper('runtime')

    def test_false_standard_deployment_claim_rejected(self):
        self.check_tamper('standard_deployment')


if __name__ == '__main__':
    unittest.main()
