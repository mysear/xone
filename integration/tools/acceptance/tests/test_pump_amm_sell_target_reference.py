import copy,json,sys,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from pump_amm_sell_target_reference import OUT,FIXTURE,verify
class PumpTargetControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):cls.e=json.loads(OUT.read_bytes());cls.f=json.loads(FIXTURE.read_bytes())
    def test_original_projection_transfers_and_controls(self):self.assertTrue(verify()['passed'])
    def test_reported_output_cannot_override_actual_wallet_credit(self):
        f=copy.deepcopy(self.f);f['cases'][1]['actual_output']='999'
        with self.assertRaises(ValueError):verify(fixture=f)
    def test_budget_failure_cannot_be_marked_success(self):
        e=copy.deepcopy(self.e);e['records'][1]['input_minus_one']['passed']=True
        with self.assertRaises(ValueError):verify(evidence=e)
    def test_duplicate_targets_cannot_replace_matrix_coverage(self):
        e=copy.deepcopy(self.e);e['records'][2]['target']=e['records'][1]['target']
        with self.assertRaises(ValueError):verify(evidence=e)
if __name__=='__main__':unittest.main()
