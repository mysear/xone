import importlib.util
from pathlib import Path
import sys
import unittest

BASE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(BASE))
SPEC = importlib.util.spec_from_file_location('packed_vectors_under_test', BASE/'op_packed_cost_vectors.py')
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class PackedOriginalReferenceTests(unittest.TestCase):
    def test_original_source_and_methods_remain_verbatim(self):
        path = MODULE.ROOT/'integration/artifacts/local/op-l1-cost-reference/rollup_cost.go'
        self.assertEqual(MODULE.digest(path), MODULE.SOURCE_SHA256)
        source = path.read_text()
        for name in ['FlzCompressLen', 'NewL1CostFuncFjord', 'newL1CostFuncEcotone', 'newOperatorCostFuncIsthmus', 'newOperatorCostFuncOperatorFeeFix']:
            self.assertIn(MODULE.function(source, name), source)
        selected = MODULE.method(source, 'func (cd RollupCostData) estimatedDASizeScaled()')
        self.assertTrue(selected.endswith('\n}'))
        self.assertIn('L1CostIntercept', selected)

    def test_probe_packing_has_exact_width_and_big_endian_parameters(self):
        for format_name, length in [('ecotone',164),('isthmus',176),('jovian',178)]:
            data = bytes.fromhex(MODULE.attributes(format_name, 2**256-1, 7, 2**32-1, 11, 23, 2**64-1))
            self.assertEqual(len(data), length)
            self.assertEqual(int.from_bytes(data[36:68], 'big'), 2**256-1)
            self.assertEqual(int.from_bytes(data[68:100], 'big'), 7)
            self.assertEqual(int.from_bytes(data[4:8], 'big'), 2**32-1)
            if length >= 176:
                self.assertEqual(int.from_bytes(data[164:168], 'big'), 23)
                self.assertEqual(int.from_bytes(data[168:176], 'big'), 2**64-1)
        for values in [(-1,0,0,0),(2**256,0,0,0),(0,0,2**32,0)]:
            with self.assertRaises(OverflowError):
                MODULE.attributes('ecotone', *values)

    def test_require_survives_optimized_python(self):
        with self.assertRaisesRegex(ValueError, 'probe rejection'):
            MODULE.require(False, 'probe rejection')

    def test_malformed_original_method_is_rejected(self):
        with self.assertRaises(ValueError):
            MODULE.method('func broken() {', 'func broken()')


if __name__ == '__main__':
    unittest.main()
