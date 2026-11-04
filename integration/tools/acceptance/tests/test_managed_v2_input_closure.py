"""Execution commitments include original Managed V2 builds and replay inputs."""
import hashlib
import json
from pathlib import Path
import shutil
import sys
import unittest

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / 'integration/tools/acceptance'))
from weighted_evidence import snapshot_inputs


class ManagedV2ClosureTests(unittest.TestCase):
    def test_all_original_compiler_builds_dependencies_and_independent_copies_are_bound(self):
        bank = json.loads((ROOT / 'integration/test/contracts/managed-v2-aum.json').read_text())
        actual = snapshot_inputs()
        rows = bank['reference']['inputs']['files'] + bank['reference']['node_closure']['files']
        for row in rows:
            self.assertEqual(actual.get(row['path']), row['sha256'], row['path'])
        for project in ('integration', 'chain-parser-go', 'dex-router'):
            for name in ('managed-v2-aum.json', 'managed-v2-aum-pools.json'):
                path = f'{project}/test/contracts/{name}'
                self.assertEqual(actual.get(path), hashlib.sha256((ROOT / path).read_bytes()).hexdigest(), path)
        node = str(Path(shutil.which('node')).resolve())
        self.assertEqual(actual.get(node), bank['reference']['node_closure']['node']['sha256'])

    def test_original_input_manifest_itself_is_bound(self):
        bank = json.loads((ROOT / 'integration/test/contracts/managed-v2-aum.json').read_text())
        key = 'integration/tools/reference/balancer-managed-v2-aum/file-manifest.json'
        self.assertEqual(snapshot_inputs().get(key), bank['reference']['inputs']['file_manifest_sha256'])

    def test_distinct_pause_reference_producer_and_all_service_copies_are_bound(self):
        actual = snapshot_inputs()
        fixture = ROOT/'integration/test/contracts/managed-v2-aum-paused-vault.json'
        reference = json.loads(fixture.read_text())
        producer = 'integration/tools/ci/managed-v2-aum-paused-vault-golden.cjs'
        self.assertEqual(actual.get(producer),reference['reference']['source_script_sha256'])
        for project in ('integration','chain-parser-go','dex-router'):
            relative = project+'/test/contracts/managed-v2-aum-paused-vault.json'
            self.assertEqual(actual.get(relative),hashlib.sha256((ROOT/relative).read_bytes()).hexdigest(),relative)

    def test_recovery_branch_helpers_and_service_copies_are_bound(self):
        actual = snapshot_inputs()
        value = json.loads((ROOT/'integration/test/contracts/managed-v2-aum-recovery.json').read_text())
        for dependency in value['reference']['script_dependencies']:
            self.assertEqual(actual.get(dependency['path']),dependency['sha256'])
        for service in ('integration','chain-parser-go','dex-router'):
            relative = service+'/test/contracts/managed-v2-aum-recovery.json'
            self.assertEqual(actual.get(relative),hashlib.sha256((ROOT/relative).read_bytes()).hexdigest(),relative)

    def test_mutable_native_receipts_are_not_inputs(self):
        actual = snapshot_inputs()
        for name in ('verified.json', 'generation.json'):
            self.assertNotIn('integration/tools/reference/balancer-managed-v2-aum/' + name, actual)


if __name__ == '__main__':
    unittest.main()
