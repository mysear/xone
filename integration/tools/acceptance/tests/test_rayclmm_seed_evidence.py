"""Reject evidence drift even when every report still says passed."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
ROOT=Path(__file__).resolve().parents[4]
SPEC=importlib.util.spec_from_file_location('seed_evidence',Path(__file__).resolve().parents[1]/'rayclmm_seed_evidence.py')
MODULE=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(MODULE)
class SeedEvidenceBinding(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.root=Path(self.tmp.name);self.g=json.loads((ROOT/'integration/artifacts/local/rayclmm-seed-native-source.json').read_text());self.native=json.loads((ROOT/'integration/artifacts/local/rayclmm-seed-reference-verification.json').read_text())
  names=set(self.g['harness_files'])|set(self.g['outputs'])|set(self.g['preserved_hashes'])|set(self.g['prior_report_hashes'])|set(self.native['protected_snapshot_hashes'])|{'integration/artifacts/local/rayclmm-seed-native-source.json','integration/tools/reference/raydium-clmm/Cargo.lock','integration/tools/reference/raydium-clmm/upstream/Cargo.lock','integration/tools/reference/raydium-clmm/source-metadata.json'}
  for name in names:
   p=self.root/name;p.parent.mkdir(parents=True,exist_ok=True);p.write_bytes((ROOT/name).read_bytes())
 def validate(self):MODULE.validate_native_binding(self.root,self.g,self.native)
 def test_current_native_proof_is_bound(self):self.validate()
 def test_limits_are_descriptive_and_execution_flags_are_explicit(self):
  self.g['limits']=['descriptive native execution boundaries'];self.g.update(sbf_transaction_executed=False,token_program_executed=False,anchor_instruction_executed=False)
  p=self.root/'integration/artifacts/local/rayclmm-seed-native-source.json';p.write_text(json.dumps(self.g));self.native['native_source_report_sha256']=MODULE.sha(p);self.native['generation_reports'][str(p.relative_to(self.root))]={'sha256':MODULE.sha(p)}
  # Fingerprints change intentionally; explicit false flags remain valid.
  for key in ['sbf_transaction_executed','token_program_executed','anchor_instruction_executed']:
   broken=copy.deepcopy(self.g);broken[key]=True
   with self.assertRaises(AssertionError):MODULE.validate_execution_scope(broken)
  MODULE.validate_execution_scope(self.g)
 def test_old_native_verification_after_generation_change_is_rejected(self):
  self.g['counts']['cases']+=1
  with self.assertRaises(AssertionError):self.validate()
 def test_stale_native_source_report_hash_is_rejected(self):
  self.native['native_source_report_sha256']='0'*64
  with self.assertRaises(AssertionError):self.validate()
 def test_native_output_or_harness_fingerprint_drift_is_rejected(self):
  for key in ['outputs','harness_files']:
   native=copy.deepcopy(self.native);path=next(iter(native[key]));native[key][path]='0'*64
   with self.assertRaises(AssertionError):MODULE.validate_native_binding(self.root,self.g,native)
 def test_recorded_native_test_count_cannot_be_stale(self):
  self.native['counts']['sequences']+=1
  with self.assertRaises(AssertionError):self.validate()
 def test_reference_dependency_lock_or_source_manifest_drift_is_rejected(self):
  for name in ['Cargo.lock','upstream/Cargo.lock','source-metadata.json']:
   p=self.root/'integration/tools/reference/raydium-clmm'/name;old=p.read_bytes();p.write_bytes(old+b'\n')
   with self.assertRaises(AssertionError):self.validate()
   p.write_bytes(old)
 def test_actual_harness_or_output_change_is_rejected(self):
  for mapping in [self.g['harness_files'],self.g['outputs']]:
   name=next(iter(mapping));p=self.root/name;old=p.read_bytes();p.write_bytes(old+b'\n')
   with self.assertRaises(AssertionError):self.validate()
   p.write_bytes(old)
if __name__=='__main__':unittest.main()
