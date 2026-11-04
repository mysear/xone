import builtins,json,tempfile,unittest
from pathlib import Path
from types import SimpleNamespace
from unittest.mock import patch
import weighted_regression as M
import weighted_static_regression as S
import weighted_evidence as E
class CallableFailure(unittest.TestCase):
 def test_imported_generic_rerun_invalidates_all_results_before_binding_read(self):
  with tempfile.TemporaryDirectory() as directory:
   out=Path(directory);names=['callable-regression-command.json','old-scope.json']
   for n in names:E.write_result(out/n,{'passed':True})
   helper=SimpleNamespace(load=lambda _:(_ for _ in ()).throw(OSError('transient binding failure')),require=E.require,runtime_fingerprint=lambda:{},snapshot_inputs=lambda:{},sha=lambda _:'hash',write_result=E.write_result)
   with patch.object(M,'OUT',out):
    with self.assertRaises(OSError):M.run(helper,[('producer.py','old-scope.json')],'callable')
   for n in names:self.assertIs(json.loads((out/n).read_text())['passed'],False)
 def test_imported_static_rerun_invalidates_before_evidence_import(self):
  original=builtins.__import__
  def importing(name,*args,**kwargs):
   if name=='weighted_static_evidence':raise ImportError('helper failed')
   return original(name,*args,**kwargs)
  with tempfile.TemporaryDirectory() as directory:
   out=Path(directory);names=['weighted-static-regression-command.json']+[n for _,n in S.PRODUCERS]
   for n in names:E.write_result(out/n,{'passed':True})
   with patch.object(S,'OUT',out),patch('builtins.__import__',side_effect=importing):
    with self.assertRaises(ImportError):S.run()
   for n in names:self.assertIs(json.loads((out/n).read_text())['passed'],False)
