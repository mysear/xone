import copy,sys,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import infinity_bin_evidence as m
class BinEvidence(unittest.TestCase):
 def report(self):return {'passed':True,'state':'PASSED','input_hashes':{'fixture':'current'},'inputs_unchanged':True,'started_at_unix_ms':1000,'completed_at_unix_ms':2000,'source_binding':{'s':'x'},'checks':list(m.CHECKS),'pool_count':2,'banks':5,'quote_checks':10,'successful_quotes':8,'large_input_rejections':2,'native_plan_steps':8,'receipt_reads':1,'read_counts':{'eth_getCode':1,'eth_call':1},'test_quote_budget':{'max_hops':1,'max_candidates':8,'max_expansions':64,'split_parts':4,'max_splits':2,'timeout_ms':10000},'verified':False,'production_ready':False,'all_requested_features_complete':False,'trust':'ORIGINAL_MANAGER_GETTERS_SYNTHETIC_BANK_BINPOOL_REFERENCE_NOT_SETTLEMENT'}
 def test_all_current_source_original_plan_and_fault_coverage_is_required(self):
  m.validate_process(self.report(),{'s':'x'},inputs={'fixture':'current'},now_ms=3000)
  for k,v in [('source_binding',{}),('checks',m.CHECKS[:-1]),('banks',4),('large_input_rejections',0),('native_plan_steps',0),('receipt_reads',0),('verified',True),('all_requested_features_complete',True),('trust','SETTLEMENT_VERIFIED')]:
   r=self.report();r[k]=v
   with self.assertRaises(ValueError,msg=k):m.validate_process(r,{'s':'x'},inputs={'fixture':'current'},now_ms=3000)
 def test_two_current_independent_reviews_are_required(self):
  r={'source_tree_sha256':'a','scope':'3c5ef1be-static-zero-hook-bin-manager-tree-and-core','reviews':{k:{'passed':True,'open_p1_p2':0,'agent_id':k}for k in ['spec','quality']}};m.validate_review(r,{'source_tree_sha256':'a'})
  for bad in ['source','missing','open']:
   q=copy.deepcopy(r)
   if bad=='source':q['source_tree_sha256']='b'
   elif bad=='missing':del q['reviews']['quality']
   else:q['reviews']['spec']['open_p1_p2']=1
   with self.assertRaises(ValueError):m.validate_review(q,{'source_tree_sha256':'a'})
 def test_complete_original_manager_getter_projection_and_scope(self):m.check_projection()
 def test_mutable_receipts_are_excluded_but_original_binary_and_source_closures_bound(self):
  from weighted_evidence import snapshot_inputs
  inputs=snapshot_inputs()
  self.assertIn('integration/tools/reference/infinity-bin-service/execution-receipt.json',inputs)
  self.assertIn('integration/tools/reference/infinity-bin-detached/execution-receipt.json',inputs)
  self.assertIn('integration/.runtime/singleton-reference/node_modules/solc/soljson.js',inputs)
  for key in ['check-node-receipt.json','check-receipt.json','guard-node-receipt.json','guards-receipt.json','guards-optimized-receipt.json','receipt-test-receipt.json','replay-receipt.json','fault-compiler-binary-receipt.json','fault-source-read-receipt.json']:
   for family in ['infinity-bin-service','infinity-bin-detached']:self.assertNotIn('integration/tools/reference/'+family+'/'+key,inputs)

if __name__=='__main__':unittest.main()

class BinInputBinding(unittest.TestCase):
 def test_old_process_input_closure_cannot_be_accepted_as_current(self):
  p=BinEvidence().report();p.update(input_hashes={'fixture':'old'},inputs_unchanged=True,started_at_unix_ms=1000,completed_at_unix_ms=2000)
  with self.assertRaises(ValueError):m.validate_process(p,{'s':'x'})
 def test_manager_limit_receipt_is_output_not_reference_input(self):
  from weighted_evidence import snapshot_inputs
  inputs=snapshot_inputs();self.assertFalse('integration/tools/reference/infinity-bin-detached/manager-limit-receipt.json'in inputs,'derived manager limit receipt treated as source')

class BinInputWindows(unittest.TestCase):
 def test_modified_reference_or_wrong_execution_window_is_rejected(self):
  base=BinEvidence().report()
  for key,value in [('input_hashes',{'fixture':'old'}),('inputs_unchanged',False),('started_at_unix_ms',0),('started_at_unix_ms',3000),('completed_at_unix_ms',4000),('completed_at_unix_ms','2000')]:
   p=copy.deepcopy(base);p[key]=value
   with self.assertRaises(ValueError,msg=key):m.validate_process(p,{'s':'x'},inputs={'fixture':'current'},now_ms=3000)
  with self.assertRaises(ValueError):m.validate_process(base,{'s':'x'},inputs={'fixture':'current'},now_ms=3_603_000)
