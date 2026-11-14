import copy,json,sys,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from verify_rayclmm_exact_out import CORPUS,verify
class RayClmmOutputControls(unittest.TestCase):
 @classmethod
 def setUpClass(cls):cls.value=json.loads(CORPUS.read_bytes())
 def bad(self,mutation):
  d=copy.deepcopy(self.value);mutation(d)
  with self.assertRaises(ValueError):verify(d)
 def row(self,d,profile=None):return next(r for r in d['cases']if r['passed']and(profile is None or r['profile']==profile))
 def test_native_input_cannot_be_understated(self):self.bad(lambda d:self.row(d).update(requiredInput='0'))
 def test_exact_net_output_cannot_include_protocol_fee(self):self.bad(lambda d:self.row(d).update(actualOutput='999999'))
 def test_mode_cannot_be_changed_to_another_direction(self):self.bad(lambda d:self.row(d).update(zeroForOne=not self.row(d)['zeroForOne']))
 def test_protocol_fee_selected_token_poststate_is_required(self):
  def change(d):r=self.row(d,'raydium-clmm-token-fee')['after']['raydium_clmm'];r['protocol_fees0']+=1
  self.bad(change)
 def test_dynamic_fee_variables_are_not_optional(self):
  def change(d):self.row(d,'raydium-clmm-dynamic')['after']['raydium_clmm']['dynamic_fee']=None
  self.bad(change)
 def test_rewards_cannot_be_replaced_by_pre_trade_state(self):
  def change(d):r=self.row(d,'raydium-clmm-rewards');r['after']['raydium_clmm']['reward_infos']=r['pool']['raydium_clmm']['reward_infos']
  self.bad(change)
 def test_bitmap_proof_cannot_be_dropped(self):self.bad(lambda d:self.row(d,'raydium-clmm-bitmap')['after']['raydium_clmm'].update(bitmap_extension=None))
 def test_next_clock_cannot_mutate_previous_native_poststate(self):
  def change(d):d['sequences'][0]['steps'][0]['after']['raydium_clmm']['bank_slot']+=1
  self.bad(change)
 def test_step_rounding_dust_cannot_be_assumed_zero(self):
  def change(d):r=next(s for s in d['steps']if s['request']['fee']==0 and not s['request']['fee_on_input']and not s['response'].get('reverted')and int(s['response']['fee_amount'])>0);r['response']['fee_amount']='0'
  self.bad(change)
 def test_unexecuted_grid_minimum_cannot_replace_native_cost(self):self.bad(lambda d:d['pairs'][0].update(bestRequiredInput='1'))
 def test_failed_native_outcome_cannot_be_declared_success(self):self.bad(lambda d:next(r for r in d['cases']if not r['passed']).update(passed=True))
 def test_source_binding_cannot_be_removed(self):self.bad(lambda d:d.update(source_sha256='0'*64))
if __name__=='__main__':unittest.main()
