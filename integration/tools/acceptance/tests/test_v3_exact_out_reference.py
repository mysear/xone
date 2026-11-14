import copy,json,sys,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from verify_v3_exact_out import CORPUS,verify
class V3OutputControls(unittest.TestCase):
 @classmethod
 def setUpClass(cls):cls.value=json.loads(CORPUS.read_bytes())
 def bad(self,mutation):
  d=copy.deepcopy(self.value);mutation(d)
  with self.assertRaises(ValueError):verify(d)
 def row(self,d):return next(r for r in d['cases']if r['passed'])
 def test_callback_input_cannot_be_replaced_by_hint(self):self.bad(lambda d:self.row(d).update(requiredInput='0'))
 def test_actual_recipient_credit_cannot_be_relabelled(self):self.bad(lambda d:self.row(d).update(actualOutput='999999'))
 def test_full_output_failure_cannot_be_success(self):self.bad(lambda d:next(r for r in d['cases']if not r['passed']).update(passed=True))
 def test_fee_tier_cannot_be_substituted(self):self.bad(lambda d:self.row(d)['pool'].update(fee_numerator=0))
 def test_deployed_code_digest_cannot_be_removed(self):self.bad(lambda d:self.row(d)['pool']['v3'].update(code_sha256='0'*64))
 def test_post_price_cannot_be_copied_from_before(self):
  def change(d):r=self.row(d);r['after']['v3']['sqrt_price_x96']=r['pool']['v3']['sqrt_price_x96']
  self.bad(change)
 def test_protocol_factory_cannot_be_swapped(self):self.bad(lambda d:self.row(d)['pool']['v3'].update(factory='0x'+'01'*20))
 def test_continuous_state_cannot_be_skipped(self):self.bad(lambda d:d['sequences'][0]['steps'][1].update(pool=d['sequences'][0]['steps'][0]['pool']))
 def test_recursive_callback_path_cannot_be_reordered(self):self.bad(lambda d:d['paths'][0]['routePools'].reverse())
 def test_grid_cannot_claim_unexecuted_input(self):self.bad(lambda d:d['grids'][0].update(bestRequiredInput='1'))
 def test_low_input_revert_receipt_cannot_be_success(self):self.bad(lambda d:self.row(d)['control']['receipt'].update(status=1))
 def test_original_source_digest_is_required(self):self.bad(lambda d:d.update(source_sha256='0'*64))
if __name__=='__main__':unittest.main()
