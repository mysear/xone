"""Reject mutations to original getter, code, and allocation observations."""
import copy
import json
from pathlib import Path
import sys
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import exact_out_reference as subject


class ExactOutReference(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.document = json.loads(subject.DEFAULT.read_bytes())

    def test_original_bank_retains_factory_pair_binding_and_all_complete_fields(self):
        bank = self.document['cases'][0]['source']
        subject.validate_bank(bank, self.document['code_ledger'])
        self.assertEqual(len(bank['canonical_pools']), 6)
        factory_calls = [r for r in bank['reads'] if r['method'] == 'eth_call'
                         and r['params'][0]['data'].startswith('0xe6a43905')]
        self.assertEqual(len(factory_calls), 12)

    def test_missing_or_changed_raw_factory_pair_cannot_be_invented_from_pool_rows(self):
        for remove in [True, False]:
            bank = copy.deepcopy(self.document['cases'][0]['source'])
            read = next(r for r in bank['reads'] if r['method'] == 'eth_call'
                        and r['params'][0]['data'].startswith('0xe6a43905'))
            if remove:
                bank['reads'].remove(read)
            else:
                read['result'] = '0x'+'00'*32
            with self.assertRaisesRegex(ValueError, 'EXACT_OUT_RAW_GETTER_PROJECTION'):
                subject.validate_bank(bank, self.document['code_ledger'])

    def test_raw_balance_cumulative_lock_code_and_canonical_projection_are_bound(self):
        for field in ['raw_balance0', 'price0_cumulative_last', 'unlocked', 'factory_code_sha256']:
            bank = copy.deepcopy(self.document['cases'][0]['source'])
            state = bank['canonical_pools'][0]['v2']['swap']
            state[field] = False if field == 'unlocked' else '1'
            with self.assertRaises(ValueError, msg=field):
                subject.validate_bank(bank, self.document['code_ledger'])
        bank = copy.deepcopy(self.document['cases'][0]['source'])
        bank['pools'][0]['v2']['swap']['price1_cumulative_last'] = '1'
        with self.assertRaisesRegex(ValueError, 'EXACT_OUT_PROJECTED_BANK_FIELDS'):
            subject.validate_bank(bank, self.document['code_ledger'])

    def test_contradictory_raw_reads_fail_without_last_write_wins(self):
        bank = copy.deepcopy(self.document['cases'][0]['source'])
        read = copy.deepcopy(next(r for r in bank['reads'] if r['method']=='eth_call'))
        read['result'] = '0x'+'ff'*32
        bank['reads'].append(read)
        with self.assertRaisesRegex(ValueError, 'EXACT_OUT_CONTRADICTORY_RAW_READ'):
            subject.validate_bank(bank, self.document['code_ledger'])


if __name__ == '__main__':
    unittest.main()
