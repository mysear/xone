from pathlib import Path
import json,runpy,tempfile,unittest
from unittest.mock import patch
class VerificationFailure(unittest.TestCase):
 def test_interrupted_verification_invalidates_previous_success_before_first_command(self):
  source=Path(__file__).resolve().parents[1]/'verify.py'
  with tempfile.TemporaryDirectory() as directory:
   root=Path(directory);script=root/'integration/tools/acceptance/verify.py';script.parent.mkdir(parents=True);script.write_bytes(source.read_bytes());out=root/'integration/artifacts/local';out.mkdir(parents=True);target=out/'verification.json';target.write_text(json.dumps({'passed':True}))
   with patch('subprocess.run',side_effect=FileNotFoundError('make unavailable')):
    with self.assertRaises(FileNotFoundError):runpy.run_path(str(script),run_name='verify_failure_test')
   self.assertIs(json.loads(target.read_text())['passed'],False)

 def test_regression_import_failure_invalidates_every_selected_scope(self):
  import builtins
  source=Path(__file__).resolve().parents[1]/'weighted_regression.py';original_import=builtins.__import__
  def importing(name,*args,**kwargs):
   if name=='weighted_evidence':raise ImportError('evidence unavailable')
   return original_import(name,*args,**kwargs)
  with tempfile.TemporaryDirectory() as directory:
   root=Path(directory);script=root/'integration/tools/acceptance/weighted_regression.py';script.parent.mkdir(parents=True);script.write_bytes(source.read_bytes());out=root/'integration/artifacts/local';out.mkdir(parents=True)
   names=['three-service-smoke.json','stable-three-service-smoke.json','weighted-three-service-smoke.json']
   for name in names:(out/name).write_text(json.dumps({'passed':True}))
   with patch('builtins.__import__',side_effect=importing):
    with self.assertRaises(ImportError):runpy.run_path(str(script),run_name='__main__')
   for name in names:self.assertIs(json.loads((out/name).read_text())['passed'],False)

 def test_static_gate_and_launcher_import_failures_invalidate_results(self):
  import builtins
  original_import=builtins.__import__
  def importing(name,*args,**kwargs):
   if name in ['weighted_evidence','weighted_static_evidence']:raise ImportError('helper unavailable')
   return original_import(name,*args,**kwargs)
  for scriptname,args,names in [('weighted_static_evidence.py',[],['weighted-static-acceptance.json']),('weighted_static_evidence.py',['--bind-runtime'],['weighted-static-runtime-binding.json']),('weighted_static_evidence.py',['--seal-scopes'],['weighted-static-scope-execution.json']),('weighted_static_regression.py',[],['weighted-static-regression-command.json','three-service-smoke.json','stable-three-service-smoke.json','weighted-three-service-smoke.json','weighted-static-three-service-smoke.json'])]:
   with tempfile.TemporaryDirectory() as directory:
    root=Path(directory);source=Path(__file__).resolve().parents[1]/scriptname;script=root/'integration/tools/acceptance'/scriptname;script.parent.mkdir(parents=True);script.write_bytes(source.read_bytes());out=root/'integration/artifacts/local';out.mkdir(parents=True)
    for name in names:(out/name).write_text(json.dumps({'passed':True}))
    with patch('builtins.__import__',side_effect=importing),patch('sys.argv',[str(script)]+args):
     with self.assertRaises(ImportError):runpy.run_path(str(script),run_name='__main__')
    for name in names:self.assertIs(json.loads((out/name).read_text())['passed'],False)
