import copy
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
from types import SimpleNamespace
import unittest
from unittest.mock import patch
SPEC=importlib.util.spec_from_file_location('account_shards_evidence',Path(__file__).resolve().parents[1]/'account_shards_evidence.py')
M=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(M)
class ShardEvidence(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.root=Path(self.tmp.name);self.out=self.root/'integration/artifacts/local';self.out.mkdir(parents=True)
  self.v={'passed':True,'source_unchanged_during_verification':True,'source_tree_sha256':'a'*64,'measured_at_unix_ms':1};self.write(self.out/'verification.json',self.v)
  for name in M.TOOLS:
   path=self.root/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text('# pinned input')
  for name in M.BINARIES:
   path=self.root/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text('checked build')
  self.runtime=M.runtime_fingerprint(self.root,self.v['source_tree_sha256'])
  self.binding={'passed':True,'runtime':self.runtime,'verification_sha256':M.sha(self.out/'verification.json'),'measured_at_unix_ms':2}
  self.write(self.out/'account-shards-runtime-binding.json',self.binding)
  self.inputs=M.snapshot_inputs(self.root)
  self.binding['input_hashes']=self.inputs;self.write(self.out/'account-shards-runtime-binding.json',self.binding)
  for name in M.SCOPES:self.write(self.out/(name+'.json'),{'passed':True,'measured_at_unix_ms':2})
  self.process={'started_at_unix_ms':2,'runtime':self.runtime,'passed':True,'measured_at_unix_ms':2,'input_hashes':self.inputs,'checks':M.PROCESS_CHECKS,'pool_count':16,'account_count':134,'max_account_response_bytes':1_000_000,'header_trust':'PROVIDER_DECLARED_FINALIZED_CONTEXT_NOT_INDEPENDENT_BANK_PROOF','source_evidence_status':{'write_poisoned':False,'retained_bytes':100,'max_retained_bytes':1000,'session_id':2,'records':25,'evidence_only':True,'confers_readiness':False}}
  self.write(self.out/'account-shards-three-service-smoke.json',self.process)
  self.review={'source_tree_sha256':self.v['source_tree_sha256'],'input_hashes':self.inputs,'spec_review':{'status':'CLOSED','findings':[]},'quality_review':{'status':'CLOSED','findings':[]}};self.write(self.out/'account-shards-review.json',self.review)
  self.report={'runtime_binding_sha256':M.sha(self.out/'account-shards-runtime-binding.json'),'process_checks':10,'passed':True,'source_tree_sha256':self.v['source_tree_sha256'],'verification_sha256':M.sha(self.out/'verification.json'),'fresh_scope_reports':M.SCOPES,'scope_hashes':{n+'.json':M.sha(self.out/(n+'.json'))for n in M.SCOPES},'input_hashes':self.inputs,'review_sha256':M.sha(self.out/'account-shards-review.json'),'production_ready':False,'full_work_packages_accepted':0,'limits':['RPC bootstrap only']}
  self.seal={'passed':True,'runtime_binding_sha256':M.sha(self.out/'account-shards-runtime-binding.json'),'input_hashes':self.inputs,'scope_hashes':self.report['scope_hashes'],'measured_at_unix_ms':3}
  self.write(self.out/'account-shards-scope-execution.json',self.seal);self.report['scope_execution_sha256']=M.sha(self.out/'account-shards-scope-execution.json')
 def write(self,path,value):path.write_text(json.dumps(value))
 def validate(self):return M.validate(self.root,self.v,self.report,self.v['source_tree_sha256'])
 def actual_run(self):
  with patch.object(M,'ROOT',self.root),patch.dict(sys.modules,{'rayclmm_dynamic_evidence':SimpleNamespace(source_hash=lambda:self.v['source_tree_sha256'])}):return M.run()
 def process_update(self):
  path=self.out/'account-shards-three-service-smoke.json';self.write(path,self.process);self.report['scope_hashes'][path.name]=M.sha(path)
 def test_current_bound_evidence_passes(self):self.assertTrue(self.validate());self.assertTrue(self.actual_run()['passed'])
 def test_source_or_acceptance_scope_cannot_be_overstated(self):
  for key,value in [('process_checks',9),('passed',False),('source_tree_sha256','old'),('production_ready',True),('full_work_packages_accepted',1)]:
   original=self.report[key];self.report[key]=value
   with self.assertRaises(ValueError):self.validate()
   self.report[key]=original
 def test_missing_stale_or_failed_scope_rejected(self):
  name=M.SCOPES[1]+'.json';path=self.out/name
  for value in [{'passed':False,'measured_at_unix_ms':2},{'passed':True,'measured_at_unix_ms':0},{'passed':True,'measured_at_unix_ms':True}]:
   self.write(path,value);self.report['scope_hashes'][name]=M.sha(path)
   with self.assertRaises(ValueError):self.validate()
 def test_each_failure_barrier_and_recovery_check_is_required(self):
  for i in range(len(M.PROCESS_CHECKS)):
   self.process['checks']=M.PROCESS_CHECKS[:i]+M.PROCESS_CHECKS[i+1:];self.process_update()
   with self.assertRaises(ValueError):self.validate()
 def test_actual_run_rejects_helper_change_without_process_rerun(self):
  self.actual_run();(self.root/'integration/tools/acceptance/account_shards_smoke.py').write_text('changed')
  with self.assertRaises(ValueError):self.actual_run()
 def test_failure_overwrites_previous_success(self):
  self.actual_run();self.v['passed']=False;self.write(self.out/'verification.json',self.v)
  with self.assertRaises(ValueError):self.actual_run()
  for name in ['account-shards-acceptance.json','account-shards-evidence-check.json']:self.assertIs(json.loads((self.out/name).read_text())['passed'],False)
 def test_new_process_cannot_reuse_an_old_review(self):
  self.actual_run();name='integration/tools/acceptance/account_shards_smoke.py';(self.root/name).write_text('new')
  self.process['input_hashes']=M.snapshot_inputs(self.root);self.process_update();self.report['input_hashes']=M.snapshot_inputs(self.root)
  with self.assertRaises(ValueError):self.validate()
 def test_independent_bank_claim_or_wal_readiness_rejected(self):
  self.process['header_trust']='INDEPENDENT';self.process_update()
  with self.assertRaises(ValueError):self.validate()
  self.process['header_trust']='PROVIDER_DECLARED_FINALIZED_CONTEXT_NOT_INDEPENDENT_BANK_PROOF';self.process['source_evidence_status']['confers_readiness']=True;self.process_update()
  with self.assertRaises(ValueError):self.validate()
 def test_poisoned_or_overbudget_final_wal_status_rejected(self):
  for key,value in [('write_poisoned',True),('retained_bytes',1001)]:
   original=self.process['source_evidence_status'][key];self.process['source_evidence_status'][key]=value;self.process_update()
   with self.assertRaises(ValueError):self.validate()
   self.process['source_evidence_status'][key]=original
 def test_changed_binary_or_execution_source_rejected(self):
  binary=self.root/M.BINARIES[0];binary.write_text('replaced binary')
  with self.assertRaises(ValueError):self.validate()
 def test_execution_time_source_cannot_be_relabelled_afterwards(self):
  self.process['runtime']={**self.runtime,'source_tree_sha256':'old execution source'};self.process_update()
  with self.assertRaises(ValueError):self.validate()
 def test_changed_required_regression_producer_rejected_by_actual_run(self):
  self.actual_run();path=self.root/'integration/tools/acceptance/orca_smoke.py';path.write_text('changed regression producer')
  with self.assertRaises(ValueError):self.actual_run()
 def test_changed_regression_fixture_rejected_by_actual_run(self):
  self.actual_run();path=self.root/'integration/test/contracts/orca-quotes.json';path.write_text('changed oracle')
  with self.assertRaises(ValueError):self.actual_run()
 def test_changed_database_schema_rejected_by_actual_run(self):
  self.actual_run();path=self.root/'integration/postgres/schema.sql';path.parent.mkdir(parents=True,exist_ok=True);path.write_text('changed schema')
  with self.assertRaises(ValueError):self.actual_run()
 def test_report_rewritten_after_batch_cannot_reuse_execution_seal(self):
  self.actual_run();path=self.out/(M.SCOPES[1]+'.json');value=json.loads(path.read_text());value['changed_after_execution']=True;self.write(path,value)
  with self.assertRaises(ValueError):self.actual_run()
 def test_scope_seal_failure_invalidates_previous_success(self):
  helper=SimpleNamespace(source_hash=lambda:self.v['source_tree_sha256'])
  with patch.object(M,'ROOT',self.root),patch.dict(sys.modules,{'rayclmm_dynamic_evidence':helper}):
   self.assertTrue(M.seal_scopes()['passed'])
   (self.root/'integration/tools/acceptance/orca_smoke.py').write_text('changed after batch')
   with self.assertRaises(ValueError):M.seal_scopes()
  self.assertIs(json.loads((self.out/'account-shards-scope-execution.json').read_text())['passed'],False)
 def test_optimized_python_cannot_disable_rejection(self):
  ns={'__file__':str(M.__file__),'__name__':'optimized_shard_gate'};exec(compile(Path(M.__file__).read_text(),str(M.__file__),'exec',optimize=2),ns)
  with self.assertRaises(ValueError):ns['validate'](self.root,self.v,self.report,'wrong')
if __name__=='__main__':unittest.main()
