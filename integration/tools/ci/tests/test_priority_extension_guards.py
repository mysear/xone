"""Reference evidence validation must remain effective with Python optimization."""
from pathlib import Path
import subprocess,sys,textwrap,unittest
ROOT=Path(__file__).resolve().parents[4]
class PriorityEvidenceGuards(unittest.TestCase):
 def test_infinity_failed_execution_receipt_is_rejected_even_when_optimized(self):
  script=ROOT/'integration/tools/ci/infinity-bin-multi-check.py'
  code=textwrap.dedent('''
   import runpy,json
   from pathlib import Path
   original=Path.read_text
   def read(self,*a,**kw):
    text=original(self,*a,**kw)
    if self.name=='execution-receipt.json' and self.parent.name=='infinity-bin-multi':
     data=json.loads(text);data['passed']=False;return json.dumps(data)
    return text
   Path.read_text=read
   module=runpy.run_path(SCRIPT,run_name='probe')
   module['verify']()
  ''').replace('SCRIPT',repr(str(script)))
  for optimized in [False,True]:
   result=subprocess.run([sys.executable,*(['-OO']if optimized else []),'-c',code],capture_output=True,text=True)
   self.assertNotEqual(result.returncode,0,(optimized,result.stdout,result.stderr))
   self.assertIn('original EVM execution did not pass',result.stderr)
 def test_orca_wrong_native_revision_is_rejected_even_when_optimized(self):
  script=ROOT/'integration/tools/ci/orca-adaptive-golden.py'
  code=textwrap.dedent("""
   import runpy,json,io,subprocess
   from pathlib import Path
   class Process:
    def __init__(self):
     self.stdin=io.StringIO()
     self.stdout=io.StringIO(json.dumps({'revision':'wrong','executed_binary_sha256':'0'*64,'source_files_verified':1111})+'\\n')
    def poll(self):return 0
   subprocess.Popen=lambda *a,**kw:Process()
   module=runpy.run_path(SCRIPT,run_name='probe')
   module['Oracle'](Path(SCRIPT))
  """).replace('SCRIPT',repr(str(script)))
  for optimized in [False,True]:
   result=subprocess.run([sys.executable,*(['-OO']if optimized else []),'-c',code],capture_output=True,text=True)
   self.assertNotEqual(result.returncode,0,(optimized,result.stdout,result.stderr))
   self.assertIn('Reference validation failed',result.stderr)
if __name__=='__main__':unittest.main()
