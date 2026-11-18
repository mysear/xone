import copy
import json
import os
from pathlib import Path
import sys
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from karst_precompile_vectors import audit, cases, OP_COMMIT, REGISTRY_COMMIT


class OriginalKarstComponents(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.v = json.loads(Path(os.environ['XONE_KARST_PRECOMPILE_VECTORS']).read_bytes())
        cls.rows = cls.v['cases']
        cls.inputs, _ = cases()

    def test_full_modern_component_coverage_and_scope_are_explicit(self):
        audit(self.inputs, self.rows)
        self.assertEqual(len(self.rows), 9452)
        self.assertEqual(self.v['original_op_commit'], OP_COMMIT)
        self.assertEqual(self.v['registry_commit'], REGISTRY_COMMIT)
        self.assertFalse(self.v['current_vm_admitted'])
        self.assertEqual(sum(int(r['address'], 16) == 5 for r in self.inputs), 1248)
        self.assertEqual(sum(int(r['address'], 16) == 256 for r in self.inputs), 6249)

    def test_removed_reordered_or_consistently_rebound_inputs_are_rejected(self):
        for change in ['drop', 'order', 'rebind']:
            inputs, rows = list(self.inputs), list(self.rows)
            if change == 'drop': rows.pop()
            elif change == 'order': rows[0], rows[1] = rows[1], rows[0]
            else:
                inputs[0] = dict(inputs[0], gas=inputs[0]['gas'] + 1)
                rows[0] = dict(rows[0], input=inputs[0])
            with self.assertRaises(ValueError, msg=change): audit(inputs, rows)

    def test_osaka_gas_output_and_success_substitutions_are_rejected(self):
        index = next(i for i, r in enumerate(self.rows) if int(r['input']['address'], 16) == 5 and r['success'] and r['input']['gas'] > r['gas_used'])
        for change in ['gas', 'output', 'success', 'unknown']:
            rows = list(self.rows); rows[index] = copy.deepcopy(rows[index]); row = rows[index]
            if change == 'gas': row['gas_used'] += 1
            elif change == 'output': row['output'] += '00'
            elif change == 'success': row['success'] = False
            else: row['unknown'] = 0
            with self.assertRaises(ValueError, msg=change): audit(self.inputs, rows)

    def test_cap_before_gas_and_p256_threshold_substitutions_are_rejected(self):
        for change in ['bn-cap-order', 'p256-price', 'minimum-order']:
            rows = list(self.rows)
            if change == 'bn-cap-order': index = next(i for i, r in enumerate(rows) if r['input']['id'] == 'bn-cap-order-57601-g0')
            elif change == 'p256-price': index = next(i for i, r in enumerate(rows) if int(r['input']['address'], 16) == 256 and r['success'])
            else: index = next(i for i, r in enumerate(rows) if r['input']['id'] == 'length-0-1025-g499')
            rows[index] = copy.deepcopy(rows[index]); row = rows[index]
            if change == 'p256-price': row['gas_used'] = 3450
            else:
                row['out_of_gas'] = not row['out_of_gas']
                row['status'] = 'Halt(OutOfGas)' if row['out_of_gas'] else 'Halt(ModexpEip7823LimitSize)'
            with self.assertRaises(ValueError, msg=change): audit(self.inputs, rows)
