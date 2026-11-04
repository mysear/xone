"""Fail-closed publication of the explicitly limited Managed reference result."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
ROOT=Path(__file__).resolve().parents[4]
class ReferenceEvidence(unittest.TestCase):
 def setUp(self):
  spec=importlib.util.spec_from_file_location('managed_reference_evidence',ROOT/'integration/tools/acceptance/managed_reference_evidence.py')
  self.module=importlib.util.module_from_spec(spec);spec.loader.exec_module(self.module)
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.out=Path(self.tmp.name)
 def test_missing_evidence_destroys_prior_success_before_optional_import(self):
  target=self.out/'managed-reference-acceptance.json';target.write_text('{"passed":true}')
  with patch.object(self.module,'OUT',self.out),patch.object(self.module,'evaluate',side_effect=ValueError('missing evidence')):
   with self.assertRaises(ValueError):self.module.run()
  self.assertIs(json.loads(target.read_text())['passed'],False)
 def test_readset_detects_replacement_even_with_identical_content(self):
  p=self.out/'review.json';p.write_text('{"status":"CLOSED"}')
  captured=self.module.ReadSet([p]);replacement=self.out/'new.json';replacement.write_bytes(p.read_bytes());replacement.replace(p)
  with self.assertRaises(ValueError):captured.check()
 def test_readset_loads_exact_captured_bytes_and_detects_tampering(self):
  p=self.out/'review.json';p.write_text('{"status":"CLOSED"}');captured=self.module.ReadSet([p]);self.assertEqual(captured.load(p)['status'],'CLOSED')
  p.write_text('{"status":"OPEN"}');self.assertEqual(captured.load(p)['status'],'CLOSED')
  with self.assertRaises(ValueError):captured.check()
 def test_failed_publication_guard_revokes_success(self):
  target=self.out/'managed-reference-acceptance.json'
  def guard():
   if target.exists()and json.loads(target.read_text()).get('passed'):raise ValueError('changed during publish')
  with patch.object(self.module,'OUT',self.out),patch.object(self.module,'evaluate',return_value=({'passed':True},guard)):
   with self.assertRaises(ValueError):self.module.run()
  self.assertIs(json.loads(target.read_text())['passed'],False)
 def test_frozen_rust_sources_use_verified_source_tree_and_keep_exact_bytes(self):
  from types import SimpleNamespace
  relative='dex-router/crates/protocol-math/src/balancer_managed_fees.rs'
  path=self.out/relative;path.parent.mkdir(parents=True);path.write_text('reference math')
  digest=self.module.sha(path)
  common=SimpleNamespace(source_hash=lambda:'checked-source')
  with patch.object(self.module,'ROOT',self.out):
   self.module.validate_frozen_files(common,{'sha256':{relative:digest}},{},{'source_tree_sha256':'checked-source'})
   with self.assertRaises(ValueError):self.module.validate_frozen_files(common,{'sha256':{relative:digest}},{},{'source_tree_sha256':'other-source'})
   path.write_text('modified math')
   with self.assertRaises(ValueError):self.module.validate_frozen_files(common,{'sha256':{relative:digest}},{},{'source_tree_sha256':'checked-source'})
 def test_frozen_reference_inputs_require_pre_execution_membership(self):
  from types import SimpleNamespace
  relative='integration/tools/reference/balancer-managed-fees/README.md'
  path=self.out/relative;path.parent.mkdir(parents=True);path.write_text('technical source')
  digest=self.module.sha(path);common=SimpleNamespace(source_hash=lambda:'checked-source')
  with patch.object(self.module,'ROOT',self.out):
   with self.assertRaises(ValueError):self.module.validate_frozen_files(common,{'sha256':{relative:digest}},{},{'source_tree_sha256':'checked-source'})
   self.module.validate_frozen_files(common,{'sha256':{relative:digest}},{relative:digest},{'source_tree_sha256':'checked-source'})
 def test_live_frozen_manifest_is_fully_covered_by_both_commitments(self):
  import weighted_evidence as common
  frozen=json.loads((ROOT/'integration/tools/reference/balancer-managed-fees/frozen-manifest.json').read_text())
  self.module.validate_frozen_files(common,frozen,common.snapshot_inputs(),{'source_tree_sha256':common.source_hash()})
if __name__=='__main__':unittest.main()
