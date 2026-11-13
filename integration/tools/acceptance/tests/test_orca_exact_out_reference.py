import copy,json,sys,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from verify_orca_exact_out import CORPUS,verify
class OrcaExactOutControls(unittest.TestCase):
 @classmethod
 def setUpClass(cls):cls.fixture=json.loads(CORPUS.read_bytes())
 def bad(self,mutation):
  f=copy.deepcopy(self.fixture);mutation(f)
  with self.assertRaises(ValueError):verify(f)
 def row(self,f,profile=None):return next(c for c in f['cases']if c['passed']and(profile is None or c['profile']==profile))
 def test_native_input_cannot_be_replaced_by_smaller_estimate(self):self.bad(lambda f:self.row(f).update(requiredInput='1'))
 def test_output_target_cannot_be_relabelled_as_surplus(self):self.bad(lambda f:self.row(f).update(actualOutput='1000000'))
 def test_native_outcome_cannot_be_changed(self):self.bad(lambda f:next(c for c in f['cases']if not c['passed']).update(passed=True))
 def test_output_side_cannot_be_relabelled_as_input_mode(self):self.bad(lambda f:self.row(f).update(zeroForOne=not self.row(f)['zeroForOne']))
 def test_pool_liquidity_poststate_cannot_be_copied_from_before(self):
  def change(f):
   c=self.row(f);c['after']['orca_whirlpool']['sqrt_price_x64']=c['pool']['orca_whirlpool']['sqrt_price_x64']
  self.bad(change)
 def test_adaptive_oracle_reserve_bytes_are_not_optional(self):
  def change(f):self.row(f,'orca-whirlpools-adaptive')['after']['orca_whirlpool']['adaptive']['reserved'][0]^=1
  self.bad(change)
 def test_initialized_reward_growth_is_not_ignorable(self):
  def change(f):
   reward=self.row(f,'orca-whirlpools-rewards')['after']['orca_whirlpool']['reward_infos'][0];reward['growth_global_x64']=str(int(reward['growth_global_x64'])+1)
  self.bad(change)
 def test_shared_pool_sequence_cannot_skip_evolving_state(self):self.bad(lambda f:f['sequences'][0]['steps'][1].update(pool=f['sequences'][0]['steps'][0]['pool']))
 def test_grid_cannot_claim_a_lower_unexecuted_input(self):self.bad(lambda f:f['pairs'][0].update(bestRequiredInput='1'))
 def test_reference_digest_cannot_be_omitted(self):self.bad(lambda f:f.update(source_sha256='0'*64))
if __name__=='__main__':unittest.main()
