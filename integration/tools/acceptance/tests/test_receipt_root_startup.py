"""A failed startup must replace any retained success from a previous attempt."""
import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

class ReceiptRootStartup(unittest.TestCase):
 def test_missing_harness_dependency_publishes_failed_process_record(self):
  with tempfile.TemporaryDirectory() as temp:
   root=Path(temp);script=root/'integration/tools/acceptance/receipt_root_smoke.py';script.parent.mkdir(parents=True)
   shutil.copyfile(Path(__file__).resolve().parents[1]/'receipt_root_smoke.py',script)
   out=root/'integration/artifacts/local';out.mkdir(parents=True)
   result=out/'receipt-root-three-service-smoke.json';result.write_text(json.dumps({'passed':True}))
   process=subprocess.run([sys.executable,str(script)],capture_output=True,text=True,timeout=10)
   self.assertNotEqual(process.returncode,0)
   self.assertIs(json.loads(result.read_text())['passed'],False)

if __name__=='__main__':unittest.main()
