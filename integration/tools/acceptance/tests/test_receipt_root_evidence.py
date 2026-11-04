import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import sys
from types import SimpleNamespace
from unittest.mock import patch
import unittest

SPEC = importlib.util.spec_from_file_location('receipt_root_evidence', Path(__file__).resolve().parents[1] / 'receipt_root_evidence.py')
M = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(M)

class ReceiptRootEvidence(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.root=Path(self.tmp.name)
  self.out=self.root/'integration/artifacts/local';self.out.mkdir(parents=True)
  self.v={'passed':True,'source_unchanged_during_verification':True,'source_tree_sha256':'a'*64,'measured_at_unix_ms':1}
  self.write(self.out/'verification.json',self.v)
  for name in M.TOOL_FILES:
   path=self.root/name;path.parent.mkdir(parents=True,exist_ok=True);path.write_text('# pinned input\n')
  for name in M.SCOPE_REPORTS:self.write(self.out/(name+'.json'),{'passed':True,'measured_at_unix_ms':2})
  self.process={'passed':True,'measured_at_unix_ms':2,'checks':M.PROCESS_CHECKS,
                'tool_sha256':M.sha(self.root/'integration/tools/acceptance/receipt_root_smoke.py'),
                'harness_sha256':M.sha(self.root/'integration/tools/acceptance/receipt_witness_smoke.py'),
                'header_trust':'RPC_ADVERTISED_NOT_INDEPENDENT_CONSENSUS'}
  self.write(self.out/'receipt-root-three-service-smoke.json',self.process)
  self.review={'source_tree_sha256':self.v['source_tree_sha256'],'spec_review':{'status':'CLOSED','findings':[]},'quality_review':{'status':'CLOSED','findings':[]}}
  self.write(self.out/'receipt-root-review.json',self.review)
  path=self.root/'chain-parser-go/test/fixtures/receipt-roots.json';path.parent.mkdir(parents=True)
  self.write(path,{'cases':[{}]*12,'generator_sha256':M.sha(self.root/'integration/tools/reference/receipt-root/oracle.cjs'),
                   'package_lock_sha256':M.sha(self.root/'integration/tools/reference/receipt-root/package-lock.json')})
  self.report={'passed':True,'source_tree_sha256':self.v['source_tree_sha256'],'verification_sha256':M.sha(self.out/'verification.json'),
               'fresh_scope_reports':M.SCOPE_REPORTS,'scope_report_hashes':{n+'.json':M.sha(self.out/(n+'.json')) for n in M.SCOPE_REPORTS},
               'tool_hashes':{n:M.sha(self.root/n) for n in M.TOOL_FILES},'process_checks':13,'reference_vectors':12,
               'review_sha256':M.sha(self.out/'receipt-root-review.json'),'production_ready':False,'full_work_packages_accepted':0,'limits':['RPC headers only']}
  hashes=M.snapshot_inputs(self.root)
  self.process['input_hashes']=hashes;self.write(self.out/'receipt-root-three-service-smoke.json',self.process)
  self.review['input_hashes']=hashes;self.write(self.out/'receipt-root-review.json',self.review)
  self.report['scope_report_hashes']['receipt-root-three-service-smoke.json']=M.sha(self.out/'receipt-root-three-service-smoke.json')
  self.report['review_sha256']=M.sha(self.out/'receipt-root-review.json')
 def write(self,path,value):path.write_text(json.dumps(value))
 def validate(self,report=None):return M.validate(self.root,self.v,report or self.report,self.v['source_tree_sha256'])
 def update_process(self):
  path=self.out/'receipt-root-three-service-smoke.json';self.write(path,self.process);self.report['scope_report_hashes'][path.name]=M.sha(path)
 def actual_run(self):
  helper=SimpleNamespace(source_hash=lambda:self.v['source_tree_sha256'])
  with patch.object(M,'ROOT',self.root), patch.dict(sys.modules,{'rayclmm_dynamic_evidence':helper}):
   return M.run()
 def test_actual_run_cannot_reuse_process_or_review_after_helper_change(self):
  self.actual_run()
  (self.root/'integration/tools/acceptance/receipt_reference.py').write_text('raise RuntimeError("changed helper")')
  with self.assertRaises(ValueError):self.actual_run()
 def test_rejected_attempt_replaces_both_previous_success_artifacts(self):
  self.actual_run()
  self.v['passed']=False;self.write(self.out/'verification.json',self.v)
  with self.assertRaises(ValueError):self.actual_run()
  for name in ('receipt-root-acceptance.json','receipt-root-evidence-check.json'):
   value=json.loads((self.out/name).read_text());self.assertIs(value['passed'],False,name)
 def test_fresh_bound_evidence_passes(self):self.assertTrue(self.validate())
 def test_stale_missing_and_false_generation_time_fail(self):
  name='v3-differential.json'
  for time in (None,0,True):
   path=self.out/name;self.write(path,{'passed':True,'measured_at_unix_ms':time});self.report['scope_report_hashes'][name]=M.sha(path)
   with self.assertRaises(ValueError):self.validate()
 def test_report_and_source_mutations_fail(self):
  for change in ({'passed':False},{'source_tree_sha256':'wrong'},{'production_ready':True},{'full_work_packages_accepted':1},{'reference_vectors':11},{'fresh_scope_reports':M.SCOPE_REPORTS[:-1]}):
   report=copy.deepcopy(self.report);report.update(change)
   with self.assertRaises(ValueError):self.validate(report)
 def test_shared_omission_and_unknown_receipt_checks_cannot_be_dropped(self):
  for i in range(6,12):
   self.process['checks']=M.PROCESS_CHECKS[:i]+M.PROCESS_CHECKS[i+1:];self.update_process()
   with self.assertRaises(ValueError):self.validate()
 def test_harness_and_reference_lock_mutations_fail(self):
  for name in ('integration/tools/acceptance/receipt_witness_smoke.py','integration/tools/reference/receipt-root/package-lock.json'):
   path=self.root/name;original=path.read_bytes();path.write_text('changed')
   with self.assertRaises(ValueError):self.validate()
   path.write_bytes(original)
 def test_open_review_is_rejected_even_if_rebound(self):
  self.review['quality_review']['status']='OPEN';path=self.out/'receipt-root-review.json';self.write(path,self.review);self.report['review_sha256']=M.sha(path)
  with self.assertRaises(ValueError):self.validate()
 def test_previous_source_review_cannot_be_rebound(self):
  self.review['source_tree_sha256']='previous source';path=self.out/'receipt-root-review.json';self.write(path,self.review);self.report['review_sha256']=M.sha(path)
  with self.assertRaises(ValueError):self.validate()
 def test_independent_consensus_claim_cannot_replace_rpc_trust(self):
  self.process['header_trust']='INDEPENDENT_CONSENSUS';self.update_process()
  with self.assertRaises(ValueError):self.validate()
 def test_optimized_python_cannot_disable_rejection(self):
  ns={'__file__':str(Path(M.__file__)),'__name__':'optimized_receipt_root'}
  exec(compile(Path(M.__file__).read_text(),str(M.__file__),'exec',optimize=2),ns)
  with self.assertRaises(ValueError):ns['validate'](self.root,self.v,self.report,'wrong')

if __name__=='__main__':unittest.main()
