"""New native catalog/shared evidence is bound in every independent copy."""
from pathlib import Path
import hashlib
import json
import sys
import unittest

ROOT=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from weighted_evidence import snapshot_inputs

class MultiInputClosureTests(unittest.TestCase):
    def test_every_native_service_copy_and_projection_is_bound(self):
        actual=snapshot_inputs()
        for family,services in [('managed-v2-aum-catalog.json',('integration','chain-parser-go','dex-router')),
                                ('managed-v2-aum-catalog-pools.json',('integration','dex-router')),
                                ('managed-v2-shared-vault.json',('integration','chain-parser-go','chain-parser-rust','dex-router'))]:
            for service in services:
                key=service+'/test/contracts/'+family
                self.assertEqual(actual.get(key),hashlib.sha256((ROOT/key).read_bytes()).hexdigest(),key)
    def test_all_shared_reference_scripts_and_original_runtime_members_are_bound(self):
        actual=snapshot_inputs()
        shared=json.loads((ROOT/'integration/test/contracts/managed-v2-shared-vault.json').read_bytes())
        for item in shared['script_closure']:
            self.assertEqual(actual.get(item['path']),item['sha256'],item['path'])
        for item in shared['original_reference']['input_closure']['files']+shared['original_reference']['node_closure']['files']:
            self.assertEqual(actual.get(item['path']),item['sha256'],item['path'])
        catalog=json.loads((ROOT/'integration/test/contracts/managed-v2-aum-catalog.json').read_bytes())
        for item in catalog['source_preservation']['before']:
            self.assertEqual(actual.get(item['path']),item['sha256'],item['path'])

if __name__=='__main__':unittest.main()
