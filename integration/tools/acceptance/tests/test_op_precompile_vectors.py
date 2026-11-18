import copy
import json
import os
from pathlib import Path
import sys
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from op_precompile_vectors import audit, cases, require


class OriginalPrecompiles(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.inputs, _ = cases()
        cls.rows = json.loads(Path(os.environ['XONE_OP_PRECOMPILE_VECTORS']).read_bytes())['cases']

    def test_full_corpus_dispatch_and_signed_fee_conservation(self):
        audit(self.inputs, self.rows)
        self.assertEqual(sum(r['input']['transition'] for r in self.rows), 110)
        self.assertEqual(len({r['input']['fork'] for r in self.rows}), 7)

    def test_removed_reordered_or_rebound_inputs_are_rejected(self):
        for change in ['drop', 'order', 'input']:
            rows = list(self.rows)
            if change == 'drop': rows.pop()
            elif change == 'order': rows[0], rows[1] = rows[1], rows[0]
            else:
                rows[0] = copy.deepcopy(rows[0])
                rows[0]['input']['gas'] += 1
            with self.assertRaises(ValueError, msg=change): audit(self.inputs, rows)

    def test_fork_gas_and_signed_nonce_fee_substitutions_are_rejected(self):
        for change in ['present', 'gas', 'nonce', 'balance']:
            rows = list(self.rows)
            index = next(i for i, r in enumerate(rows) if r.get('transition')) if change in ['nonce', 'balance'] else next(i for i, r in enumerate(rows) if r['present'] and r['success'])
            rows[index] = copy.deepcopy(rows[index]); row = rows[index]
            if change == 'present': row['present'] = not row['present']
            elif change == 'gas': row['remaining_gas'] += 1
            elif change == 'nonce': row['transition']['nonce_after'] = 0
            else:
                owner = next(iter(row['transition']['after']))
                row['transition']['after'][owner] = str(int(row['transition']['after'][owner]) + 1)
            with self.assertRaises(ValueError, msg=change): audit(self.inputs, rows)
        with self.assertRaises(ValueError): require(False, 'optimized guard retained')
