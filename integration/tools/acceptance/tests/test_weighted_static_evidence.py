import copy,json,tempfile,unittest
from pathlib import Path
from unittest.mock import patch
import weighted_static_evidence as M
from weighted_static_smoke import PROCESS_CHECKS
from weighted_smoke import PROCESS_CHECKS as LEGACY_CHECKS
class StaticEvidence(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.out=Path(self.tmp.name);p=patch.object(M,'OUT',self.out);p.start();self.addCleanup(p.stop)
  self.runtime={'source_tree_sha256':'source'};self.inputs={'input':'sha'}
  p=patch.object(M.common,'validate_bundle',return_value=(self.runtime,self.inputs));p.start();self.addCleanup(p.stop)
  p=patch.object(M,'validate_oracle',return_value={'cases':5355,'sequences':315,'steps':1260,'power_cases':240});p.start();self.addCleanup(p.stop)
  self.current={'passed':True,'checks':PROCESS_CHECKS.copy(),'pool_count':6,'protocol':'balancer-v2-weighted2-static','trust':'SYNTHETIC_RPC_NOT_DEPLOYMENT_PROOF','production_ready':False}
  self.old={'passed':True,'checks':LEGACY_CHECKS.copy(),'pool_count':3,'protocol':'balancer-v2-weighted2-equal','trust':'SYNTHETIC_RPC_NOT_DEPLOYMENT_PROOF','production_ready':False}
  self.review={'source_tree_sha256':'source','input_hashes':self.inputs,'spec_review':{'status':'CLOSED','open_important_findings':0},'quality_review':{'status':'CLOSED','open_important_findings':0}}
  for n in ['verification.json','weighted-static-scope-execution.json','weighted-static-golden-generation.json']:self.write(n,{})
  self.update()
 def write(self,n,v):M.write_result(self.out/n,v)
 def update(self):
  for n,v in [('weighted-static-three-service-smoke.json',self.current),('weighted-three-service-smoke.json',self.old),('weighted-static-review.json',self.review)]:self.write(n,v)
 def test_valid_static_scope_passes(self):self.assertTrue(M.validate()['passed'])
 def test_every_new_check_is_required(self):
  for i in range(len(PROCESS_CHECKS)):
   self.current['checks']=PROCESS_CHECKS[:i]+PROCESS_CHECKS[i+1:];self.update()
   with self.assertRaises(ValueError):M.validate()
 def test_every_legacy_check_is_required(self):
  for i in range(len(LEGACY_CHECKS)):
   self.old['checks']=LEGACY_CHECKS[:i]+LEGACY_CHECKS[i+1:];self.update()
   with self.assertRaises(ValueError):M.validate()
 def test_old_and_new_protocol_labels_and_counts_cannot_be_interchanged(self):
  for target in [self.old,self.current]:
   for key,value in [('pool_count',2),('protocol','balancer-v2-managed'),('production_ready',True),('trust','DEPLOYMENT_VERIFIED')]:
    original=target[key];target[key]=value;self.update()
    with self.assertRaises(ValueError):M.validate()
    target[key]=original
 def test_stale_and_open_review_rejected(self):
  for stage in ['spec_review','quality_review']:
   self.review[stage]['open_important_findings']=1;self.update()
   with self.assertRaises(ValueError):M.validate()
   self.review[stage]['open_important_findings']=0
  self.review['source_tree_sha256']='old';self.update()
  with self.assertRaises(ValueError):M.validate()
 def test_exception_clears_previous_success(self):
  self.write('weighted-static-acceptance.json',{'passed':True})
  with patch.object(M,'validate',side_effect=RuntimeError('bad before reads')):
   with self.assertRaises(RuntimeError):M.run()
  self.assertFalse(json.loads((self.out/'weighted-static-acceptance.json').read_text())['passed'])
 def test_optimized_validation_has_no_assertion_escape(self):
  ns={'__file__':M.__file__,'__name__':'optimized_static_gate'};exec(compile(Path(M.__file__).read_text(),M.__file__,'exec',optimize=2),ns)
  with self.assertRaises(ValueError):ns['require'](False,'reject')

class StaticOracleEvidence(unittest.TestCase):
 def setUp(self):
  self.generation=M.load('weighted-static-golden-generation.json')
  p=patch.object(M.common,'oracle_runtime_hash',return_value=self.generation['runtime_sha256']);p.start();self.addCleanup(p.stop)
 def test_actual_original_flow_reference_and_legacy_bytes_are_valid(self):
  self.assertEqual(M.validate_oracle()['cases'],5355)
 def test_changed_generation_counts_and_fingerprints_are_rejected(self):
  for key,value in [('passed',False),('corpus_sha256','bad'),('powers_sha256','bad'),('cases',5354),('sequences',314),('steps',1259),('power_cases',239),('runtime_sha256','bad'),('oracle_inputs',{}),('legacy_inputs',{})]:
   bad=copy.deepcopy(self.generation);bad[key]=value
   with patch.object(M,'load',return_value=bad):
    with self.assertRaises(ValueError,msg=key):M.validate_oracle()
 def test_changed_original_source_or_frozen_legacy_fixture_is_rejected(self):
  original_sha=M.sha
  for suffix in ['integration/test/contracts/weighted-quotes.json','integration/test/contracts/weighted-frames.json','integration/test/contracts/weighted-state.json','sources/pkg/solidity-utils/contracts/math/LogExpMath.sol']:
   with patch.object(M,'sha',side_effect=lambda p:'changed' if str(p).endswith(suffix) else original_sha(p)):
    with self.assertRaises(ValueError,msg=suffix):M.validate_oracle()
