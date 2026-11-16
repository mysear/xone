"""The independent ABI/RLP checker rejects missing and rehashed false evidence."""
from pathlib import Path
import hashlib
import json
import os
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[4]


class EvmPreparationInterop(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)
        source = Path(os.environ['XONE_EVM_COMPONENT_RUN'])
        shutil.copytree(source / 'in', self.root / 'in')
        shutil.copytree(source / 'out', self.root / 'out')

    def reject(self):
        result = subprocess.run(['node', str(ROOT / 'integration/tools/acceptance/evm_preparation_interop.cjs'),
                                 str(self.root / 'in'), str(self.root / 'out'), str(self.root / 'report.json')],
                                cwd=ROOT, env=os.environ, stdout=subprocess.PIPE,
                                stderr=subprocess.PIPE, timeout=30)
        self.assertNotEqual(result.returncode, 0)
        self.assertFalse((self.root / 'report.json').exists())

    def mutate_input(self, update):
        path = self.root / 'in/unsigned-false.json'
        doc = json.loads(path.read_bytes())
        update(doc['preparation'])
        prepared = doc['preparation'].copy()
        prepared.pop('preparation_hash')
        encoded = json.dumps(prepared, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()
        doc['preparation']['preparation_hash'] = hashlib.sha256(encoded).hexdigest()
        path.write_text(json.dumps(doc) + '\n')
        self.reject()

    def test_missing_original_target_is_rejected(self):
        next((self.root / 'out').glob('*.json')).unlink()
        self.reject()

    def test_rehashed_rpc_nonce_mismatch_is_rejected(self):
        self.mutate_input(lambda p: p['rpc_transaction'].update(nonce='0xff'))

    def test_rehashed_numeric_nonce_is_rejected(self):
        self.mutate_input(lambda p: p['transaction'].update(nonce=18))

    def test_duplicate_original_case_identity_is_rejected(self):
        first, second, *_ = sorted((self.root / 'out').glob('*.json'))
        a = json.loads(first.read_bytes())
        b = json.loads(second.read_bytes())
        b['case_id'] = a['case_id']
        second.write_text(json.dumps(b) + '\n')
        self.reject()


if __name__ == '__main__':
    unittest.main()
