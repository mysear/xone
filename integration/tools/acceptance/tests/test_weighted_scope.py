import copy
import unittest
from weighted_smoke import validate_quote
class WeightedScope(unittest.TestCase):
 def quote(self):return {'grossBuyAmount':'99','consumedIn':'100','refund':'0','verification':{'level':'INDICATIVE'},'snapshotRef':{'block':{'hash':'bank','number_or_slot':100},'online_generation':2},'onlineGeneration':2,'quality':{'candidateScope':['a','b','c']}}
 def check(self,v,status=200):return validate_quote(status,v,{},'100','99',self.quote()['snapshotRef'],{'a','b','c'})
 def test_exact_scope_and_bank_pass(self):self.check(self.quote())
 def test_partial_scope_fails(self):
  q=self.quote();q['quality']['candidateScope'].pop()
  with self.assertRaises(ValueError):self.check(q)
 def test_duplicate_scope_fails(self):
  q=self.quote();q['quality']['candidateScope'][-1]='a'
  with self.assertRaises(ValueError):self.check(q)
 def test_old_complete_bank_fails(self):
  q=self.quote();q['snapshotRef']['block']['hash']='other'
  with self.assertRaises(ValueError):self.check(q)
 def test_outer_generation_and_amount_and_verification_fail(self):
  for key,value in [('onlineGeneration',1),('grossBuyAmount','98'),('refund','1')]:
   q=self.quote();q[key]=value
   with self.assertRaises(ValueError):self.check(q)
  q=self.quote();q['verification']['level']='VERIFIED'
  with self.assertRaises(ValueError):self.check(q)
 def test_optimized_fail_closed(self):
  import weighted_smoke as module
  ns={'__file__':module.__file__,'__name__':'optimized_weighted_smoke'}
  from pathlib import Path
  exec(compile(Path(module.__file__).read_text(),module.__file__,'exec',optimize=2),ns)
  with self.assertRaises(ValueError):ns['validate_quote'](503,{}, {},'100','99',{},set())
