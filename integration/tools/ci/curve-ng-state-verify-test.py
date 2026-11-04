#!/usr/bin/env python3
"""Fault injection for evidence authorization, without modifying real artifacts."""
import importlib.util
from pathlib import Path
import unittest
import sys
sys.dont_write_bytecode = True
from unittest.mock import patch

spec = importlib.util.spec_from_file_location('state_verify', Path(__file__).with_name('curve-ng-state-verify.py'))
verify = importlib.util.module_from_spec(spec)
spec.loader.exec_module(verify)


class EvidenceRejection(unittest.TestCase):
    def corrupted(self, relative, replacement):
        selected = verify.ROOT / relative
        original = Path.read_bytes

        def read(path, *args, **kwargs):
            data = original(path, *args, **kwargs)
            return replacement(data) if path == selected else data

        with patch.object(Path, 'read_bytes', read):
            with self.assertRaises(ValueError):
                verify.verify()

    def test_changed_original_source(self):
        self.corrupted('integration/tools/reference/curve-models/ng/CurveStableSwapNG.vy', lambda data: data + b'\n')

    def test_changed_fixture(self):
        self.corrupted('integration/test/contracts/curve-ng-state.json', lambda data: data.replace(b'4000000', b'4000001', 1))

    def test_changed_generator(self):
        self.corrupted('integration/tools/ci/curve-ng-state-golden.cjs', lambda data: data + b'\n')

    def test_missing_completion_receipt(self):
        original = Path.read_text
        selected = verify.REF / 'execution-closure.json'

        def read(path, *args, **kwargs):
            if path == selected:
                raise FileNotFoundError('injected incomplete generation')
            return original(path, *args, **kwargs)

        with patch.object(Path, 'read_text', read):
            with self.assertRaises(FileNotFoundError):
                verify.verify()

    def test_changed_deployed_runtime(self):
        original = Path.read_text
        selected = verify.REF / 'runtime-zero-fee.hex'

        def read(path, *args, **kwargs):
            data = original(path, *args, **kwargs)
            if path == selected:
                return data[:20] + ('0' if data[20] != '0' else '1') + data[21:]
            return data

        with patch.object(Path, 'read_text', read):
            with self.assertRaisesRegex(ValueError, 'Deployed runtime changed'):
                verify.verify()


if __name__ == '__main__':
    unittest.main()
