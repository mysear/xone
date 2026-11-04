import json,tempfile,time,unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
import weighted_regression as M
import weighted_evidence as E
class LauncherArguments(unittest.TestCase):
 def test_family_arguments_are_executed_inside_the_same_binding(self):
  with tempfile.TemporaryDirectory()as directory:
   out=Path(directory);runtime={'source_tree_sha256':'current','binaries':{}};inputs={'fixture':'hash'};binding={'passed':True,'runtime':runtime,'input_hashes':inputs,'measured_at_unix_ms':1};E.write_result(out/'five-family-runtime-binding.json',binding)
   helper=SimpleNamespace(load=lambda name:json.loads((out/name).read_text()),require=E.require,runtime_fingerprint=lambda:runtime,snapshot_inputs=lambda:inputs,sha=E.sha,write_result=E.write_result)
   def execute(cmd,**kw):
    self.assertEqual(cmd[-2:],['--family','orca']);self.assertNotIn('PYTHONOPTIMIZE',kw['env']);E.write_result(out/'scope.json',{'passed':True,'measured_at_unix_ms':int(time.time()*1000)});return SimpleNamespace(returncode=0)
   with patch.object(M,'OUT',out),patch('subprocess.run',side_effect=execute):M.run(helper,[('solana_rewards_smoke.py','scope.json',['--family','orca'])],'five-family')
   report=json.loads((out/'scope.json').read_text());self.assertTrue(report['passed']);self.assertEqual(report['runtime'],runtime);self.assertEqual(report['input_hashes'],inputs);self.assertIn('execution_binding_sha256',report)
 def test_failed_startup_cannot_leave_any_old_selected_scope_successful(self):
  with tempfile.TemporaryDirectory()as directory:
   out=Path(directory)
   for name in['five-family-regression-command.json','orca.json','ray.json']:E.write_result(out/name,{'passed':True})
   helper=SimpleNamespace(load=lambda name:(_ for _ in()).throw(OSError('binding unavailable')),require=E.require,runtime_fingerprint=lambda:{},snapshot_inputs=lambda:{},sha=E.sha,write_result=E.write_result)
   with patch.object(M,'OUT',out):
    with self.assertRaises(OSError):M.run(helper,[('p.py','orca.json',['--family','orca']),('p.py','ray.json',['--family','raydium-clmm'])],'five-family')
   for name in['five-family-regression-command.json','orca.json','ray.json']:self.assertFalse(json.loads((out/name).read_text())['passed'])
