import json
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
class ShardStartup(unittest.TestCase):
 def test_import_failure_invalidates_previous_process_success(self):
  with tempfile.TemporaryDirectory()as temp:
   root=Path(temp);script=root/'integration/tools/acceptance/account_shards_smoke.py';script.parent.mkdir(parents=True);shutil.copyfile(Path(__file__).resolve().parents[1]/'account_shards_smoke.py',script)
   output=root/'integration/artifacts/local/account-shards-three-service-smoke.json';output.parent.mkdir(parents=True);output.write_text(json.dumps({'passed':True}))
   process=subprocess.run([sys.executable,str(script)],capture_output=True,text=True,timeout=10)
   self.assertNotEqual(process.returncode,0);self.assertIs(json.loads(output.read_text())['passed'],False)
if __name__=='__main__':unittest.main()
