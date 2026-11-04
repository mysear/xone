"""A resumed acceptance may reuse only fresh, unchanged, successful evidence."""
import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
SPEC=importlib.util.spec_from_file_location('dynamic_evidence',Path(__file__).resolve().parents[1]/'rayclmm_dynamic_evidence.py')
MODULE=importlib.util.module_from_spec(SPEC);SPEC.loader.exec_module(MODULE)
NAMES=['three-service-smoke','journal-integration','kafka-three-service-smoke','reorg-smoke','reorg-http-smoke','v3-differential','v3-three-service-smoke','multidex-differential','multidex-three-service-smoke','stable-differential','stable-three-service-smoke','singleton-differential','singleton-three-service-smoke','raydium-differential','raydium-three-service-smoke']
TARGETS=['check','fixture-test','durable-journal-test','reorg-journal-test','v3-quote-test','multidex-quote-test','stable-quote-test','singleton-quote-test','raydium-quote-test']
class DynamicResumeEvidence(unittest.TestCase):
 def setUp(self):
  self.tmp=tempfile.TemporaryDirectory();self.addCleanup(self.tmp.cleanup);self.root=Path(self.tmp.name);self.out=self.root/'integration/artifacts/local';self.out.mkdir(parents=True)
  self.v={'measured_at_unix_ms':1000,'source_tree_sha256':'a'*64};self.write('verification.json',self.v)
  tool=self.root/'integration/tools/acceptance/raydium_smoke.py';tool.parent.mkdir(parents=True);tool.write_text('# exact tested tool\n')
  for name in NAMES:self.write(name+'.json',{'passed':True})
  self.write('raydium-three-service-smoke.json',{'passed':True,'tool_sha256':MODULE.sha(tool),'lease_recovery_timeout_seconds':55})
  (self.out/'full-local-acceptance-attempt-1.log').write_text('original failed bounded lease recovery test\n')
  self.audit={'started_at_unix_ms':1100,'exit_code':0,'unchanged_source':True,'source_tree_sha256':self.v['source_tree_sha256'],'fresh_verification_sha256':MODULE.sha(self.out/'verification.json'),'original_attempt':'full-local-acceptance-attempt-1.log','original_attempt_exit_code':2,'passed_targets_not_repeated':TARGETS,'command':['make',*sum((['-o',target]for target in TARGETS),[]),'full-local-acceptance'],'fresh_passed_scope_reports':{name+'.json':MODULE.sha(self.out/(name+'.json')) for name in NAMES}}
  self.save()
 def write(self,name,value):(self.out/name).write_text(json.dumps(value))
 def save(self):self.write('full-local-acceptance-resume.json',self.audit)
 def validate(self):return MODULE.validate_resumed_acceptance(self.root,self.v)
 def test_current_unchanged_fresh_resume_is_valid(self):self.assertTrue(self.validate())
 def test_changed_source_or_verification_is_rejected(self):
  for key in ['source_tree_sha256','fresh_verification_sha256']:
   old=self.audit[key];self.audit[key]='0'*64;self.save()
   with self.assertRaises(AssertionError):self.validate()
   self.audit[key]=old
 def test_changed_reused_report_or_tool_is_rejected(self):
  self.write('v3-differential.json',{'passed':False})
  with self.assertRaises(AssertionError):self.validate()
  self.write('v3-differential.json',{'passed':True});(self.root/'integration/tools/acceptance/raydium_smoke.py').write_text('# changed after execution\n')
  with self.assertRaises(AssertionError):self.validate()
 def test_extra_skipped_scope_or_missing_bound_report_is_rejected(self):
  self.audit['passed_targets_not_repeated']=TARGETS+['rayclmm-dynamic-quote-test'];self.save()
  with self.assertRaises(AssertionError):self.validate()
  self.audit['passed_targets_not_repeated']=TARGETS;self.audit['fresh_passed_scope_reports'].pop('v3-differential.json');self.save()
  with self.assertRaises(AssertionError):self.validate()
 def test_command_cannot_hide_an_extra_skip_or_change_final_target(self):
  original=list(self.audit['command'])
  for command in [original[:-1]+['-o','rayclmm-dynamic-quote-test',original[-1]],original[:-1]+['acceptance-report']]:
   self.audit['command']=command;self.save()
   with self.assertRaises(AssertionError):self.validate()
 def test_failed_resumed_command_cannot_be_accepted(self):
  self.audit['exit_code']=2;self.save()
  with self.assertRaises(AssertionError):self.validate()
 def test_later_full_verification_does_not_use_historical_resume(self):
  later=copy.deepcopy(self.v);later['measured_at_unix_ms']=1200
  self.assertFalse(MODULE.validate_resumed_acceptance(self.root,later))
if __name__=='__main__':unittest.main()
