#!/usr/bin/env python3
"""Receipt failure probes in private workspaces; never touch live receipts."""
import json
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
CI = ROOT / 'integration/tools/ci'
REF = ROOT / 'integration/tools/reference/curve-crypto-full'


class FailClosed(unittest.TestCase):
    def setUp(self):
        runtime = REF / '.runtime'
        runtime.mkdir(exist_ok=True)
        self.tmp = tempfile.TemporaryDirectory(prefix='receipt-probe-', dir=runtime)
        self.root = Path(self.tmp.name)
        self.ref = self.root / 'integration/tools/reference/curve-crypto-full'
        self.ref.mkdir(parents=True)
        self.ci = self.root / 'integration/tools/ci'
        self.ci.mkdir(parents=True)
        for file in ['curve-crypto-full-golden.cjs', 'curve-crypto-full-verify.py']:
            shutil.copyfile(CI / file, self.ci / file)
        for receipt in ['generation.json', 'verification.json']:
            (self.ref / receipt).write_text('{"valid":true}\n')

    def tearDown(self):
        self.tmp.cleanup()

    def test_missing_optional_dependency_invalidates_generation(self):
        run = subprocess.run(['node', str(self.ci / 'curve-crypto-full-golden.cjs')], capture_output=True, timeout=30)
        self.assertNotEqual(run.returncode, 0)
        self.assertIn(b'Cannot find module', run.stderr)
        self.assertIs(json.loads((self.ref / 'generation.json').read_text())['valid'], False)

    def test_malformed_pin_invalidates_generation(self):
        modules = self.root / 'integration/.runtime/stable-reference'
        modules.mkdir(parents=True)
        (modules / 'node_modules').symlink_to(ROOT / 'integration/.runtime/stable-reference/node_modules', target_is_directory=True)
        (self.ref / 'sources.json').write_text('{bad json')
        run = subprocess.run(['node', str(self.ci / 'curve-crypto-full-golden.cjs')], capture_output=True, timeout=30)
        self.assertNotEqual(run.returncode, 0)
        self.assertIn(b'SyntaxError', run.stderr)
        self.assertIs(json.loads((self.ref / 'generation.json').read_text())['valid'], False)

    def test_optimized_malformed_generation_invalidates_verification(self):
        (self.ref / 'sources.json').write_text('[]')
        (self.ref / 'generation.json').write_text('{bad json')
        run = subprocess.run([sys.executable, '-OO', str(self.ci / 'curve-crypto-full-verify.py')], capture_output=True, timeout=30)
        self.assertNotEqual(run.returncode, 0)
        self.assertIn(b'JSONDecodeError', run.stderr)
        self.assertIs(json.loads((self.ref / 'verification.json').read_text())['valid'], False)

    def test_optimized_pin_check_is_not_an_assert(self):
        (self.ref / 'source.vy').write_text('not pinned')
        (self.ref / 'sources.json').write_text(json.dumps([{'local': 'source.vy', 'sha256': '0' * 64}]))
        run = subprocess.run([sys.executable, '-OO', str(self.ci / 'curve-crypto-full-verify.py')], capture_output=True, timeout=30)
        self.assertNotEqual(run.returncode, 0)
        self.assertIn(b'source pin mismatch', run.stderr)
        self.assertIs(json.loads((self.ref / 'verification.json').read_text())['valid'], False)


if __name__ == '__main__':
    unittest.main()
