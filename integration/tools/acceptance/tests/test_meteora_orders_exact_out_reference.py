import copy
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from meteora_orders_exact_out_reference import ORIGINAL, FIXTURE, verify

class OrdersExactOutReferenceControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.evidence = json.loads(ORIGINAL.read_bytes())
        cls.fixture = json.loads(FIXTURE.read_bytes())
        cls.index = next(i for i, r in enumerate(cls.evidence['records']) if r['native']['passed'])

    def test_original_transfers_projections_and_budget_controls(self):
        self.assertTrue(verify()['passed'])

    def test_fabricated_required_input_rejected(self):
        f = copy.deepcopy(self.fixture)
        f['cases'][self.index]['steps'][0]['actual_input'] = '1'
        with self.assertRaises(ValueError):
            verify(fixture=f)

    def test_changed_complete_order_state_rejected(self):
        f = copy.deepcopy(self.fixture)
        f['cases'][self.index]['steps'][0]['after']['meteora']['bin_arrays'][0]['data'] = '00'
        with self.assertRaises(ValueError):
            verify(fixture=f)

    def test_missing_budget_failure_rejected(self):
        e = copy.deepcopy(self.evidence)
        e['records'][self.index]['input_minus_one']['passed'] = True
        with self.assertRaises(ValueError):
            verify(evidence=e)

    def test_rejected_transaction_with_mutation_rejected(self):
        e = copy.deepcopy(self.evidence)
        i = next(i for i, r in enumerate(e['records']) if not r['native']['passed'])
        n = e['records'][i]['native']
        key = next(iter(n['after']))
        n['after'][key]['lamports'] += 1
        with self.assertRaises(ValueError):
            verify(evidence=e)

if __name__ == '__main__':
    unittest.main()
