import sys
from pathlib import Path
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from native_cost_vectors import oracle,document,MAX256
class NativeCostVectors(unittest.TestCase):
    def test_effective_cap_legacy_and_free_gas_are_distinct(self):
        eip=oracle(21000,100000,100,110,30);legacy=oracle(21000,100000,100,110,None)
        self.assertEqual(eip['effective_gas_price'],'110');self.assertEqual(legacy['effective_gas_price'],'110')
        clipped=oracle(21000,100000,100,200,30)
        self.assertEqual(clipped['effective_gas_price'],'130');self.assertEqual(clipped['execution_fee'],'2730000');self.assertEqual(clipped['gas_limit_reserve'],'20000000')
        self.assertEqual(oracle(21000,21000,0,0,None)['execution_fee'],'0')
    def test_bad_receipts_and_uint256_overflow_do_not_generate_fake_expected(self):
        self.assertIsNone(oracle(0,100000,1,2,None));self.assertIsNone(oracle(21000,20000,1,2,None));self.assertIsNone(oracle(21000,100000,3,2,None));self.assertIsNone(oracle(21000,100000,1,2,3));self.assertIsNone(oracle(21000,21000,0,MAX256,None))
    def test_fixed_scope_retains_both_roundings_wide_values_and_failure_cases(self):
        rows=document()['cases'];self.assertEqual(len(rows),56);self.assertEqual(sum(row['valid'] for row in rows),48)
        self.assertTrue(any(int(row['maximum'])>(1<<128) and row['valid'] for row in rows))
        for row in rows:
            if row['valid']:
                e=row['expected'];self.assertEqual(int(e['base_fee_burned'])+int(e['priority_fee_paid']),int(e['execution_fee']));self.assertLessEqual(int(e['execution_fee']),int(e['gas_limit_reserve']))
if __name__=='__main__':unittest.main()
