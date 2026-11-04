"""Only fresh, source-bound WAL process and regression evidence is acceptable."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
SPEC=importlib.util.spec_from_file_location('source_wal_evidence',Path(__file__).resolve().parents[1]/'source_wal_evidence.py')
MODULE=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(MODULE)
class SourceWalEvidence(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.root=Path(self.tmp.name);self.out=self.root/'integration/artifacts/local';self.out.mkdir(parents=True)
  self.v={'passed':True,'source_unchanged_during_verification':True,'source_tree_sha256':'a'*64,'measured_at_unix_ms':1}
  self.review={'spec_review':{'status':'CLOSED','findings':[{'status':'CLOSED'}]},'quality_review':{'status':'CLOSED','findings':[]},'evidence_gate_review':{'status':'CLOSED','findings':[]}}
  self.write('source-wal-review.json',self.review)
  tool=self.root/'integration/tools/acceptance/source_wal_smoke.py';tool.parent.mkdir(parents=True);tool.write_text('# tested tool\n')
  for name in MODULE.SCOPE_REPORTS:self.write(name+'.json',{'passed':True,'measured_at_unix_ms':2})
  process={'passed':True,'measured_at_unix_ms':2,'tool_sha256':MODULE.sha(tool),'checks':MODULE.PROCESS_CHECKS,'source_evidence_status':{'session_id':4,'confers_readiness':False,'evidence_only':True,'write_poisoned':False,'records':25,'retained_bytes':100,'max_retained_bytes':1000}}
  self.write('source-wal-three-service-smoke.json',process)
  self.report={'passed':True,'source_tree_sha256':self.v['source_tree_sha256'],'source_unchanged_during_verification':True,'fresh_scope_reports':MODULE.SCOPE_REPORTS,'scope_report_hashes':{name+'.json':MODULE.sha(self.out/(name+'.json'))for name in MODULE.SCOPE_REPORTS},'review_sha256':MODULE.sha(self.out/'source-wal-review.json'),'process_checks':10,'final_evidence_status':process['source_evidence_status'],'production_ready':False,'full_work_packages_accepted':0,'limits':['RPC evidence only'],'spec_review_closed':True,'quality_review_closed':True}
 def write(self,name,value):(self.out/name).write_text(json.dumps(value))
 def validate(self):return MODULE.validate(self.root,self.v,self.report,self.v['source_tree_sha256'])
 def test_current_fresh_bound_evidence_is_accepted(self):self.validate()
 def test_source_or_report_drift_is_rejected(self):
  with self.assertRaises(AssertionError):MODULE.validate(self.root,self.v,self.report,'b'*64)
  self.write('v3-differential.json',{'passed':False})
  with self.assertRaises(AssertionError):self.validate()
 def test_extra_or_missing_regression_scope_is_rejected(self):
  self.report['fresh_scope_reports']=self.report['fresh_scope_reports'][:-1]
  with self.assertRaises(AssertionError):self.validate()
 def test_runtime_status_cannot_grant_readiness_or_hide_poison(self):
  for key,value in [('confers_readiness',True),('write_poisoned',True),('session_id',1)]:
   report=copy.deepcopy(self.report);report['final_evidence_status'][key]=value
   with self.assertRaises(AssertionError):MODULE.validate(self.root,self.v,report,self.v['source_tree_sha256'])
 def test_historical_startup_marker_or_capacity_checks_cannot_be_omitted(self):
  file=self.out/'source-wal-three-service-smoke.json';process=json.loads(file.read_text());process['checks']=process['checks'][:-1];self.write(file.name,process);self.report['scope_report_hashes'][file.name]=MODULE.sha(file)
  with self.assertRaises(AssertionError):self.validate()
 def test_optimized_python_cannot_disable_rejection(self):
  namespace={'__file__':str(Path(MODULE.__file__)),'__name__':'optimized_source_wal_evidence'}
  exec(compile(Path(MODULE.__file__).read_text(),str(MODULE.__file__),'exec',optimize=2),namespace)
  with self.assertRaises(AssertionError):namespace['validate'](self.root,self.v,self.report,'wrong-source')
 def test_copied_old_report_or_missing_generation_time_is_rejected(self):
  name='v3-differential.json'
  for report in [{'passed':True,'measured_at_unix_ms':0},{'passed':True}]:
   self.write(name,report);self.report['scope_report_hashes'][name]=MODULE.sha(self.out/name)
   with self.assertRaises(AssertionError):self.validate()
 def test_open_reviews_or_modified_process_tool_are_rejected(self):
  self.review['quality_review']['status']='OPEN';self.write('source-wal-review.json',self.review)
  with self.assertRaises(AssertionError):self.validate()
  self.review['quality_review']['status']='CLOSED';self.write('source-wal-review.json',self.review)
  (self.root/'integration/tools/acceptance/source_wal_smoke.py').write_text('# modified after process proof\n')
  with self.assertRaises(AssertionError):self.validate()
if __name__=='__main__':unittest.main()
