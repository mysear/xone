"""BPT projections retain authenticated observations without local pricing."""
import importlib
from pathlib import Path
import sys
import unittest
TOOLS=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(TOOLS))

class BptProjectionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.p=importlib.import_module('managed_v2_bpt_projection')
        cls.bank=cls.p.load(TOOLS.parents[1]/'test/contracts/managed-v2-bpt.json')
        cls.projected=cls.p.project(cls.bank)

    def test_qualified_bank_has_every_source_bank_and_finite_case(self):
        v=self.projected
        self.assertEqual(len(v['banks']),145)
        self.assertEqual(len(v['cases']),116)
        self.assertFalse(v['production_ready'])
        self.assertTrue(v['unsupported_pools'])
        for row in v['banks']:
            for entry in row['pools']:
                pool=entry['pool'];m=pool['multi_asset']['managed_v2']
                self.assertEqual(pool['protocol'],self.p.PROTOCOL)
                self.assertEqual(m['bpt']['minimum_bpt_locked'],'1000000')
                self.assertEqual(m['block_hash'],row['header']['hash'])
                self.assertIn('schedule',m)
                self.assertEqual(m['bpt']['allowlist_members'],sorted(m['bpt']['allowlist_members'],key=lambda m:m['address']))

    def test_plain_data_and_changed_native_or_metadata_are_rejected(self):
        with self.assertRaises(ValueError):self.p.project(dict(self.bank))
        original=self.bank['banks'][0]['header']['hash']
        try:
            self.bank['banks'][0]['header']['hash']='0x'+'1'*64
            with self.assertRaises(ValueError):self.p.project(self.bank)
        finally:self.bank['banks'][0]['header']['hash']=original
        self.assertEqual(self.projected,self.p.project(self.bank))

    def test_case_outputs_are_native_and_source_identity_is_preserved(self):
        indexed={c['id']:c for c in self.bank['cases']}
        for c in self.projected['cases']:
            self.assertEqual(c['expected_native'],indexed[c['id']]['expected_native'])
            self.assertEqual(c['actor'],indexed[c['id']]['actor'])
        self.assertGreaterEqual(sum(c['expected_native']['status']==1 and c['operation']=='bpt_join' for c in self.projected['cases']),42)
