import copy,sys,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import orca_adaptive_evidence as m
class AdaptiveEvidence(unittest.TestCase):
 def process(self):return {'passed':True,'state':'PASSED','source_binding':{'s':'x'},'test_quote_budget':{'timeout_ms':10000,'max_hops':1,'max_candidates':8,'max_expansions':64,'split_parts':4,'max_splits':2},'checks':list(m.CHECKS),'quote_checks':20,'successful_quotes':16,'native_rejections':4,'pool_count':2,'phase_count':5,'account_batches':1,'block_reads':2,'native_plan_steps':16,'verified':False,'production_ready':False,'all_requested_features_complete':False,'trust':'SYNTHETIC_RPC_ORIGINAL_PACK_NOT_SBF_TRANSACTION'}
 def test_process_requires_current_complete_scope_recovery_and_failure_counts(self):
  m.validate_process(self.process(),{'s':'x'})
  for key,value in [('source_binding',{}),('checks',m.CHECKS[:-1]),('quote_checks',16),('native_rejections',0),('account_batches',0),('verified',True),('all_requested_features_complete',True),('trust','LIVE')]:
   p=self.process();p[key]=value
   with self.assertRaises(ValueError,msg=key):m.validate_process(p,{'s':'x'})
 def test_both_reviews_must_bind_current_source_and_close_issues(self):
  v={'source_tree_sha256':'a','scope':'cc683ec9-adaptive-online-three-fixed-classic-spl','reviews':{k:{'passed':True,'agent_id':k,'open_p1_p2':0}for k in ['spec','quality']}};m.validate_reviews(v,{'source_tree_sha256':'a'})
  for bad in ['stale','missing','open']:
   q=copy.deepcopy(v)
   if bad=='stale':q['source_tree_sha256']='b'
   elif bad=='missing':del q['reviews']['quality']
   else:q['reviews']['spec']['open_p1_p2']=1
   with self.assertRaises(ValueError):m.validate_reviews(q,{'source_tree_sha256':'a'})
 def test_projection_keeps_original_native_ownership_amount_and_complete_state(self):m.check_projection()
if __name__=='__main__':unittest.main()
