import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import pancake_lm_exact_out_reference as subject

class LegacyLmExactOutReference(unittest.TestCase):
    @classmethod
    def setUpClass(cls): cls.fixture=json.loads(subject.DEFAULT.read_bytes())
    def mutated(self,change):
        value=copy.deepcopy(self.fixture);change(value)
        with tempfile.TemporaryDirectory(prefix='bsc-xo-guard-',dir=subject.OUT) as folder:
            path=Path(folder)/'reference.json';path.write_text(json.dumps(value))
            return subject.validate(path)
    def test_original_reference_contains_exact_output_and_real_funds_observations(self):
        v=subject.validate();self.assertEqual((v['full_original_swaps'],v['partial_swaps_rejected'],v['invalid_banks_rejected'],v['original_sequence_steps']),(80,20,4,12))
    def test_changed_original_digest_generator_and_quote_kind_are_rejected(self):
        for change in [lambda d:d['original_reference'].__setitem__('sha256','00'*32),lambda d:d.__setitem__('generator_sha256','00'*32),lambda d:d.__setitem__('quote_kind','EXACT_IN')]:
            with self.assertRaises(ValueError):self.mutated(change)
    def test_case_output_partial_classification_and_source_bank_cannot_be_changed(self):
        for change in [lambda d:d['cases'][0].__setitem__('target_output','2'),lambda d:d['cases'][0]['expected'].__setitem__('partial',True),lambda d:d['source_banks'][0]['original_bank']['state']['lm'].__setitem__('liquidity','1')]:
            with self.assertRaises(ValueError):self.mutated(change)
    def test_runtime_and_original_case_records_cannot_be_replaced(self):
        for change in [lambda d:d['deployment']['lm'].__setitem__('code','0x6000'),lambda d:d['cases'][0]['original']['expected']['payer_deltas'].__setitem__(0,'1'),lambda d:d['sequences'][0]['steps'][0].__setitem__('target_output','1')]:
            with self.assertRaises(ValueError):self.mutated(change)

    def test_normalized_target_fee_balance_and_clock_cannot_diverge_from_original_projection(self):
        for change in [
            lambda d:d['cases'][0]['expected']['expected_pool']['pancake_lm'].__setitem__('fee_growth_global0','1'),
            lambda d:d['cases'][0]['pool']['pancake_lm'].__setitem__('raw_balance0','1'),
            lambda d:d['cases'][0]['pool']['pancake_lm'].__setitem__('timestamp',1),
        ]:
            with self.assertRaises(ValueError):self.mutated(change)

if __name__=='__main__':unittest.main()
