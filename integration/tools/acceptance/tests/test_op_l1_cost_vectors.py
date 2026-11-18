import importlib.util
from pathlib import Path
import unittest

PATH = Path(__file__).resolve().parents[1] / 'op_l1_cost_vectors.py'
SPEC = importlib.util.spec_from_file_location('op_l1_vectors_under_test', PATH)
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)


class OpL1CostReferenceTests(unittest.TestCase):
    def test_retained_public_deposit_decodes_to_three_supported_signed_wires(self):
        rows, references = MODULE.public_cases()
        self.assertEqual(len(rows), 3)
        self.assertEqual(len(references), 2)
        self.assertEqual(len(bytes.fromhex(rows[0]['attributes'])), 260)
        self.assertTrue(all(int(r['receipt_fee']) > 0 for r in rows))

    def test_rlp_rejects_truncation_and_noncanonical_length_or_short_bytes(self):
        for data in [b'', b'\x81', b'\x81\x01', b'\xb8\x00', b'\xb8\x01\xff', b'\xf8\x01\xff', b'\xc1\x81']:
            with self.subTest(data=data), self.assertRaises(ValueError):
                MODULE.rlp_one(data)
        self.assertEqual(MODULE.rlp_one(b'\xc3\x80\x01\x80'), ([b'', b'\x01', b''], b''))

    def test_require_survives_optimized_python(self):
        with self.assertRaisesRegex(ValueError, 'required'):
            MODULE.require(False, 'required')

    def test_original_functions_are_extracted_verbatim_and_hash_is_fixed(self):
        source = (MODULE.ROOT/'integration/artifacts/local/op-l1-cost-reference/rollup_cost.go').read_text()
        self.assertEqual(MODULE.digest(MODULE.ROOT/'integration/artifacts/local/op-l1-cost-reference/rollup_cost.go'), MODULE.SOURCE_SHA256)
        for name in ['newL1CostFuncBedrockHelper', 'l1CostHelper']:
            selected = MODULE.function(source, name)
            self.assertIn(selected, source)
            self.assertTrue(selected.startswith('func '+name+'('))
            self.assertTrue(selected.endswith('}'))


if __name__ == '__main__':
    unittest.main()
