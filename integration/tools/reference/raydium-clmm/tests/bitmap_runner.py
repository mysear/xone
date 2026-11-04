"""Mutation checks for full-Pool accounting and source-derived bitmap evidence."""
import copy
import importlib.util
import json
from pathlib import Path
import sys
import unittest

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[5]
spec = importlib.util.spec_from_file_location('bitmap_golden', ROOT / 'integration/tools/ci/rayclmm-bitmap-golden.py')
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
CORPUS = json.loads((ROOT / 'integration/test/contracts/rayclmm-bitmap-quotes.json').read_bytes())
INDICES = json.loads((ROOT / 'integration/test/contracts/rayclmm-bitmap-indices.json').read_bytes())


class BitmapAuditTests(unittest.TestCase):
    def test_full_pool_vault_accounting_and_fee_token_mutations_reject(self):
        for mode in (0,1,2):
            for direction in (False,True):
                c=next(c for c in CORPUS['cases'] if c['pool']['raydium_clmm'].get('collect_fee_on',0)==mode and c['zero_for_one']==direction and not c['expected'].get('reverted'))
                runner.audit_result(c['pool'],c['input'],direction,c['expected'])
                bad=copy.deepcopy(c['expected'])
                bad['next_pool']['raydium_clmm']['raw_vault0']+=1
                with self.assertRaises(AssertionError):
                    runner.audit_result(c['pool'],c['input'],direction,bad)
                bad=copy.deepcopy(c['expected'])
                input_token=0 if direction else 1
                fee_token=input_token if mode==0 else mode-1
                bad['next_pool']['raydium_clmm'][f'fund_fees{1-fee_token}']+=1
                with self.assertRaises(AssertionError):
                    runner.audit_result(c['pool'],c['input'],direction,bad)

    def test_successful_poststate_cannot_drop_or_truncate_extension(self):
        c=next(c for c in CORPUS['cases'] if not c['expected'].get('reverted'))
        for width in (None,0,111,113):
            bad=copy.deepcopy(c['expected'])
            if width is None:
                del bad['next_pool']['raydium_clmm']['bitmap_extension']
            else:
                bad['next_pool']['raydium_clmm']['bitmap_extension']['positive']=[0]*width
            with self.assertRaises(AssertionError):
                runner.audit_result(c['pool'],c['input'],c['zero_for_one'],bad)

    def test_complete_sequence_final_pool_and_independent_vectors_are_enforced(self):
        runner.audit(CORPUS)
        broken=copy.deepcopy(CORPUS)
        broken['sequences'][0]['expected_final_pool']['raydium_clmm']['raw_vault0']+=1
        with self.assertRaises(AssertionError):
            runner.audit(broken)
        broken=dict(CORPUS,independent_vectors=CORPUS['independent_vectors'][:-1])
        with self.assertRaises(AssertionError):
            runner.audit(broken)

    def test_source_vector_groups_and_one_hot_words_are_enforced(self):
        self.assertEqual(runner.audit_indices(INDICES),86)
        broken=copy.deepcopy(INDICES)
        broken['vectors'][0]['negative']=[0]*112
        with self.assertRaises(AssertionError):
            runner.audit_indices(broken)
        broken=copy.deepcopy(INDICES)
        broken['invalid_source_starts'][0]['rejected']=False
        with self.assertRaises(AssertionError):
            runner.audit_indices(broken)


if __name__=='__main__':
    unittest.main()
