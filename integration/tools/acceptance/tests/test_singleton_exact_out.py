import copy
import json
from pathlib import Path
import sys
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from verify_singleton_exact_out import ROOT,validate

class SingletonOutputEvidence(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.original=json.loads((ROOT/'integration/test/contracts/singleton-exact-out.json').read_bytes())
    def rejected(self,mutate,reason):
        c=copy.deepcopy(self.original);mutate(c)
        with self.assertRaisesRegex(ValueError,reason):validate(c)
    def test_target_cannot_change(self):
        self.rejected(lambda c:c['cases'][0].update(target='123'),'OUTPUT_CONSERVATION')
    def test_native_cash_debit_cannot_change(self):
        self.rejected(lambda c:c['cases'][0].update(requiredInput='1'),'NATIVE_INPUT')
    def test_projection_cannot_change_identity(self):
        self.rejected(lambda c:c['cases'][0]['after']['singleton_cl'].update(manager='0x'+'0'*40),'FULL_PRICING_PROJECTION')
    def test_partial_cannot_be_complete(self):
        def mutate(c):next(r for r in c['cases']if not r['passed']and not r['native'].get('reverted'))['passed']=True
        self.rejected(mutate,'PARTIAL_OR_SIGNED_RANGE')
    def test_grid_cannot_ignore_capacity(self):
        def mutate(c):next(r for r in c['grids'][0]['rows']if not r['passed'])['passed']=True
        self.rejected(mutate,'GRID_CAPACITY')
    def test_grid_cannot_claim_lower_unexecuted_input(self):
        self.rejected(lambda c:c['grids'][0].update(bestRequiredInput='1'),'GRID_MINIMUM')
    def test_sequence_cannot_reset_state(self):
        self.rejected(lambda c:c['sequences'][0]['steps'][1].update(pool=copy.deepcopy(c['sequences'][0]['steps'][0]['pool'])),'SEQUENCE_STATE')
    def test_ordered_cannot_change_pool(self):
        self.rejected(lambda c:c['ordered'][0]['steps'][1].update(pool=copy.deepcopy(c['ordered'][0]['pool'])),'ORDERED_PHYSICAL_STATE')
    def test_hooks_cannot_be_enabled(self):
        self.rejected(lambda c:c['cases'][0]['pool']['singleton_cl'].update(hooks='0x'+'1'*40),'HOOKS')
    def test_cases_cannot_be_omitted(self):
        self.rejected(lambda c:c['cases'].pop(),'CASE_COUNT')
if __name__=='__main__':unittest.main()
