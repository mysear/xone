import copy,sys,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import priority_extensions_evidence as m
class PriorityEvidence(unittest.TestCase):
 def process(self):return {'passed':True,'state':'PASSED','source_binding':{'s':'x'},'checks':list(m.EXPECTED_CHECKS),'quote_checks':60,'successful_quotes':40,'native_revert_rejections':20,'phase_count':15,'pool_count':2,'receipt_reads':1,'verified':False,'production_ready':False,'all_requested_features_complete':False,'trust':'SYNTHETIC_RPC_NOT_DEPLOYMENT_PROOF'}
 def test_bounded_process_proof_requires_every_source_check_and_failure(self):
  m.validate_process(self.process(),{'s':'x'})
  for key,value in [('passed',False),('source_binding',{}),('checks',m.EXPECTED_CHECKS[:-1]),('quote_checks',59),('successful_quotes',60),('native_revert_rejections',0),('receipt_reads',0),('verified',True),('all_requested_features_complete',True),('trust','LIVE_CHAIN')]:
   p=self.process();p[key]=value
   with self.assertRaises(ValueError,msg=key):m.validate_process(p,{'s':'x'})
 def test_two_independent_current_reviews_required(self):
  v={'source_tree_sha256':'a','scope':'crypto-service-orca-adaptive-swap-infinity-multi-bin','reviews':{key:{'passed':True,'open_p1_p2':0,'agent_id':key}for key in ['spec','quality','adaptive_swap_spec','adaptive_swap_quality','crypto_dependency_quality']}};m.validate_review(v,{'source_tree_sha256':'a'})
  for bad in ['stale','missing','open','scope']:
   q=copy.deepcopy(v)
   if bad=='stale':q['source_tree_sha256']='b'
   elif bad=='missing':del q['reviews']['quality']
   elif bad=='open':q['reviews']['spec']['open_p1_p2']=1
   else:q['scope']='all protocols'
   with self.assertRaises(ValueError,msg=bad):m.validate_review(q,{'source_tree_sha256':'a'})
 def test_corpus_counts_and_offline_boundaries_cannot_be_overstated(self):
  import json
  docs=[json.loads((m.ROOT/'integration/test/contracts'/name).read_text()) for name in ['curve-crypto-full.json','orca-adaptive.json','orca-adaptive-swap.json','infinity-bin-multi.json']]
  m.validate_corpus_counts(*docs)
  for index in range(4):
   changed=list(docs);changed[index]=dict(changed[index]);changed[index]['cases']=changed[index]['cases'][:-1]
   with self.assertRaises(ValueError):m.validate_corpus_counts(*changed)
  for index in [1,2]:
   changed=list(docs);changed[index]=dict(changed[index]);changed[index]['online_profile_enabled']=True
   with self.assertRaises(ValueError):m.validate_corpus_counts(*changed)
if __name__=='__main__':unittest.main()
