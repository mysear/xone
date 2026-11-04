import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import weighted_evidence as M
class WeightedEvidence(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.root=Path(self.tmp.name);self.out=self.root/'integration/artifacts/local';self.out.mkdir(parents=True)
  self.patches=[patch.object(M,'ROOT',self.root),patch.object(M,'OUT',self.out),patch.object(M,'source_hash',return_value='current')]
  for p in self.patches:p.start();self.addCleanup(p.stop)
  for name in M.BINARIES+['integration/Makefile','integration/postgres/schema.sql','integration/tools/acceptance/verify.py','integration/tools/reference/stable-models/package.json','integration/tools/reference/stable-models/package-lock.json']:
   p=self.root/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_text('original')
  self.write('verification.json',{'passed':True,'source_unchanged_during_verification':True,'source_tree_sha256':'current','measured_at_unix_ms':1,'checked_binary_sha256':M.runtime_fingerprint()['binaries'],'tool_sha256':M.sha(self.root/'integration/tools/acceptance/verify.py')})
 def write(self,n,v):M.write_result(self.out/n,v)
 def setup_scopes(self):
  b=M.bind_runtime();runtime=M.runtime_fingerprint();inputs=M.snapshot_inputs();stamp=b['measured_at_unix_ms']+1
  for n in M.SCOPES:self.write(n,{'passed':True,'started_at_unix_ms':stamp,'measured_at_unix_ms':stamp,'runtime':runtime,'input_hashes':inputs,'execution_binding_sha256':M.sha(self.out/'weighted-runtime-binding.json')})
  self.write('weighted-three-service-smoke.json',{'passed':True,'measured_at_unix_ms':stamp,'started_at_unix_ms':stamp,'runtime':runtime,'input_hashes':inputs,'execution_binding_sha256':M.sha(self.out/'weighted-runtime-binding.json')})
  return b
 def test_valid_preexecution_binding_seals(self):self.setup_scopes();self.assertTrue(M.seal_scopes()['passed'])
 def test_new_binary_rejected(self):
  self.setup_scopes();(self.root/M.BINARIES[0]).write_text('new')
  with self.assertRaises(ValueError):M.seal_scopes()
 def test_new_input_rejected(self):
  self.setup_scopes();(self.root/'integration/postgres/schema.sql').write_text('changed')
  with self.assertRaises(ValueError):M.seal_scopes()
 def test_failed_or_stale_or_boolean_timestamp_cannot_seal(self):
  for value in [{'passed':False,'measured_at_unix_ms':2},{'passed':True,'measured_at_unix_ms':0},{'passed':True,'measured_at_unix_ms':True}]:
   self.setup_scopes();v=M.load(M.SCOPES[0]);v.update(value);self.write(M.SCOPES[0],v)
   with self.assertRaises(ValueError):M.seal_scopes()
 def test_old_execution_binding_rejected(self):
  self.setup_scopes();v=M.load('weighted-three-service-smoke.json');v['runtime']['source_tree_sha256']='old';self.write('weighted-three-service-smoke.json',v)
  with self.assertRaises(ValueError):M.seal_scopes()
 def test_failed_rebind_overwrites_previous_success(self):
  M.bind_runtime();v=M.load('verification.json');v['passed']=False;self.write('verification.json',v)
  with self.assertRaises(ValueError):M.bind_runtime()
  self.assertFalse(M.load('weighted-runtime-binding.json')['passed'])
 def test_failed_seal_overwrites_previous_success(self):
  self.setup_scopes();M.seal_scopes();(self.root/M.BINARIES[-1]).write_text('new')
  with self.assertRaises(ValueError):M.seal_scopes()
  self.assertFalse(M.load('weighted-scope-execution.json')['passed'])
 def test_run_import_or_validation_failure_invalidates_previous_success(self):
  self.write('weighted-acceptance.json',{'passed':True})
  with patch.object(M,'validate',side_effect=RuntimeError('failed before read')):
   with self.assertRaises(RuntimeError):M.run()
  self.assertFalse(M.load('weighted-acceptance.json')['passed'])
 def test_optimized_checks_are_not_assertions(self):
  ns={'__file__':M.__file__,'__name__':'optimized_weighted_evidence'};exec(compile(Path(M.__file__).read_text(),M.__file__,'exec',optimize=2),ns)
  with self.assertRaises(ValueError):ns['require'](False,'rejected')

 def test_binary_replacement_between_verification_and_binding_rejected(self):
  (self.root/M.BINARIES[0]).write_text('replaced after checks')
  with self.assertRaises(ValueError):M.bind_runtime()
 def test_overlapping_prebinding_regression_is_rejected(self):
  self.setup_scopes();v=M.load(M.SCOPES[0]);v['started_at_unix_ms']=0;self.write(M.SCOPES[0],v)
  with self.assertRaises(ValueError):M.seal_scopes()
 def test_other_scope_wrong_execution_runtime_is_rejected(self):
  self.setup_scopes();v=M.load(M.SCOPES[1]);v['runtime']['source_tree_sha256']='old';self.write(M.SCOPES[1],v)
  with self.assertRaises(ValueError):M.seal_scopes()
