import copy,unittest
from weighted_static_smoke import validate_quote,profile,NAMES,reference_case,WEIGHTS
class StaticScope(unittest.TestCase):
 def reference(self):return {'block':{'number_or_slot':100,'hash':'bank'},'online_generation':2}
 def quote(self):return {'grossBuyAmount':'99','consumedIn':'100','refund':'0','verification':{'level':'INDICATIVE'},'snapshotRef':self.reference(),'onlineGeneration':2,'quality':{'candidateScope':['p'+str(i)for i in range(6)]}}
 def check(self,q):return validate_quote(200,q,{},'100','99',self.reference(),{'p'+str(i)for i in range(6)})
 def test_six_pool_reference_is_required(self):
  self.check(self.quote())
  for bad in ['partial','duplicate','bank','generation','verified']:
   q=self.quote()
   if bad=='partial':q['quality']['candidateScope'].pop()
   elif bad=='duplicate':q['quality']['candidateScope'][-1]='p0'
   elif bad=='bank':q['snapshotRef']['block']['hash']='old'
   elif bad=='generation':q['onlineGeneration']=1
   else:q['verification']['level']='VERIFIED'
   with self.assertRaises(ValueError,msg=bad):self.check(q)
 def test_each_process_identity_is_bound_to_the_matching_native_weight_and_decimals(self):
  self.assertEqual(len(NAMES),6)
  for i,name in enumerate(NAMES):
   p,c=profile(name);self.assertEqual(p['weighted']['weight0'],str(WEIGHTS[i]));self.assertEqual(c['protocol'],'balancer-v2-weighted2-static')
   for zero in [True,False]:amount,output=reference_case(i,zero);self.assertGreater(int(amount),0);self.assertGreater(int(output),0)
 def test_optimized_mode_keeps_failures(self):
  import weighted_static_smoke as m
  from pathlib import Path
  ns={'__file__':m.__file__,'__name__':'optimized_static_scope'};exec(compile(Path(m.__file__).read_text(),m.__file__,'exec',optimize=2),ns)
  with self.assertRaises(ValueError):ns['validate_quote'](503,{}, {},'100','99',self.reference(),set())
