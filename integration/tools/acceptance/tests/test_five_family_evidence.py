import copy,json,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
import five_family_evidence as M
from multi_model_smoke import PROCESS_CHECKS
from solana_rewards_smoke import CHECKS
class FamilyEvidence(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.out=Path(self.tmp.name);p=patch.object(M,'OUT',self.out);p.start();self.addCleanup(p.stop)
  for function,value in [('validate_bundle',({'source_tree_sha256':'a'},{'input':'x'}))]:
   p=patch.object(M.common,function,return_value=value);p.start();self.addCleanup(p.stop)
  for function,value in [('original_sources',{'original':1}),('validate_corpora',{'case':{'sha256':'hash'}})]:
   p=patch.object(M,function,return_value=value);p.start();self.addCleanup(p.stop)
  self.multi={'checks':PROCESS_CHECKS.copy(),'pool_count':6,'production_ready':False};self.reward={f:{'family':f,'checks':CHECKS.copy(),'steps':3,'step_native_sha256':['a','b','c'],'production_ready':False,'sbf_executed':False,'token_program_executed':False}for f in['orca','raydium-clmm']};self.review={'source_tree_sha256':'a','input_hashes':{'input':'x'},'spec_review':{'status':'CLOSED','open_important_findings':0},'quality_review':{'status':'CLOSED','open_important_findings':0}}
  for n in ['verification.json','five-family-scope-execution.json']:self.write(n,{})
  self.update()
 def write(self,n,v):M.write_result(self.out/n,v)
 def update(self):
  self.write('multi-model-three-service-smoke.json',self.multi);self.write('five-family-review.json',self.review)
  for f,v in self.reward.items():self.write('rewards-'+f+'-three-service-smoke.json',v)
 def test_bounded_success_does_not_claim_all_user_requested_families_finished(self):
  r=M.validate();self.assertTrue(r['passed']);self.assertFalse(r['all_requested_features_complete']);self.assertFalse(r['production_ready'])
 def test_every_multi_pool_process_check_is_required(self):
  for i in range(len(PROCESS_CHECKS)):
   self.multi['checks']=PROCESS_CHECKS[:i]+PROCESS_CHECKS[i+1:];self.update()
   with self.assertRaises(ValueError):M.validate()
 def test_each_reward_native_poststate_step_and_scope_is_required(self):
  for f in self.reward:
   original=copy.deepcopy(self.reward[f])
   for key,v in [('steps',2),('step_native_sha256',['a','b']),('family','other'),('checks',CHECKS[:-1]),('sbf_executed',True),('token_program_executed',True),('production_ready',True)]:
    self.reward[f]=copy.deepcopy(original);self.reward[f][key]=v;self.update()
    with self.assertRaises(ValueError):M.validate()
   self.reward[f]=original
 def test_stale_or_open_review_rejected(self):
  for key in ['spec_review','quality_review']:
   self.review[key]['open_important_findings']=1;self.update()
   with self.assertRaises(ValueError):M.validate()
   self.review[key]['open_important_findings']=0
  self.review['source_tree_sha256']='old';self.update()
  with self.assertRaises(ValueError):M.validate()
 def test_failed_validation_destroys_prior_success(self):
  self.write('five-family-acceptance.json',{'passed':True})
  with patch.object(M,'validate',side_effect=RuntimeError('failed')):
   with self.assertRaises(RuntimeError):M.run()
  self.assertFalse(json.loads((self.out/'five-family-acceptance.json').read_text())['passed'])
 def test_optimized_checks_remain_effective(self):
  ns={'__file__':M.__file__,'__name__':'optimized_family_gate'};exec(compile(Path(M.__file__).read_text(),M.__file__,'exec',optimize=2),ns)
  with self.assertRaises(ValueError):ns['require'](False,'rejected')
class OriginalSourceEvidence(unittest.TestCase):
 def test_current_original_sources_and_corpus_bounds(self):self.assertEqual(M.original_sources()['singleton'],85);self.assertEqual(len(M.validate_corpora()),11)
 def test_changed_original_source_is_rejected(self):
  original=M.sha
  with patch.object(M,'sha',side_effect=lambda p:'changed'if str(p).endswith('pkg/pool-weighted/contracts/WeightedMath.sol')else original(p)):
   with self.assertRaises(ValueError):M.original_sources()
 def test_failed_singleton_generator_cannot_reuse_corpus(self):
  original=M.load
  with patch.object(M,'load',side_effect=lambda n:{'passed':False}if n=='singleton-advanced-generation.json'else original(n)):
   with self.assertRaises(ValueError):M.validate_corpora()
