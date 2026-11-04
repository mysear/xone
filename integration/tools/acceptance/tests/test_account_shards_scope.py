import copy
import importlib.util
from pathlib import Path
import unittest
PATH=Path(__file__).resolve().parents[1]/'account_shards_smoke.py'
SPEC=importlib.util.spec_from_file_location('account_shards_smoke',PATH)
M=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(M)
class ExactScope(unittest.TestCase):
 def reference(self,slot=100):return {'block':{'number_or_slot':slot,'hash':'bank'+str(slot)},'online_generation':2}
 def quote(self):return {'snapshotRef':self.reference(),'onlineGeneration':2,'grossBuyAmount':str(M.expected(10000,True)),'verification':{'level':'INDICATIVE'},'quality':{'source':'RPC_SAME_FINALIZED_CONTEXT_SHARDS','candidateScope':['p'+str(i)for i in range(16)]},'consumedIn':'10000','refund':'0'}
 def test_partial_scope_cannot_pass_a_mathematically_correct_quote(self):
  quote=self.quote();expected=set(quote['quality']['candidateScope']);quote['quality']['candidateScope'].pop()
  with self.assertRaises(AssertionError):M.validate_quote_response(200,quote,True,expected,self.reference())
 def test_duplicate_scope_ids_cannot_hide_a_missing_pool(self):
  quote=self.quote();expected=set(quote['quality']['candidateScope']);quote['quality']['candidateScope'][-1]='p0'
  with self.assertRaises(AssertionError):M.validate_quote_response(200,quote,True,expected,self.reference())
 def test_old_quote_bank_is_rejected_despite_full_ids_and_correct_amount(self):
  quote=self.quote();expected=set(quote['quality']['candidateScope']);quote['snapshotRef']=self.reference(99)
  with self.assertRaises(AssertionError):M.validate_quote_response(200,quote,True,expected,self.reference())
 def test_optimized_mode_must_still_reject_failed_evm_health(self):
  ns={'__file__':str(PATH),'__name__':'optimized_shard_smoke'};exec(compile(PATH.read_text(),str(PATH),'exec',optimize=2),ns)
  with self.assertRaises(AssertionError):ns['validate_evm_response'](503,{'error':'not ready'})
if __name__=='__main__':unittest.main()
