"""Exercise real signal handling and child-group cleanup without running gates."""
from pathlib import Path
import json
import os
import signal
import subprocess
import sys
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[4]
DRIVER = ROOT / 'integration/tools/acceptance/validate_rayclmm_execution.py'

class AcceptanceLifecycle(unittest.TestCase):
    def interruption(self, signum):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder)
            marker = root / 'children.json'
            grandchild = 'import time; time.sleep(120)'
            child = (
                'import json,os,subprocess,sys,time;from pathlib import Path;'
                f'p=subprocess.Popen([sys.executable,"-c",{grandchild!r}]);'
                'Path(sys.argv[1]).write_text(json.dumps([os.getpid(),p.pid]));'
                'time.sleep(120)'
            )
            launcher = f'''
import importlib.util,subprocess,sys
from pathlib import Path
spec=importlib.util.spec_from_file_location('controlled_driver',{str(DRIVER)!r})
m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
m.ROOT=Path({str(root)!r});m.BASE=m.ROOT/'evidence'
m.fingerprint=lambda:'controlled-lifecycle-fixture'
m.runtime_inputs=lambda:{{}}
real_popen=subprocess.Popen
child_source={child!r}
def substitute(command,*args,**kwargs):
    return real_popen([sys.executable,'-c',child_source,{str(marker)!r}],*args,**kwargs)
m.subprocess.Popen=substitute
m.main()
'''
            process = subprocess.Popen([sys.executable, '-c', launcher], stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            children = []
            try:
                deadline = time.monotonic() + 10
                while not marker.exists():
                    self.assertIsNone(process.poll(), 'Controlled driver exited before starting its child')
                    self.assertLess(time.monotonic(), deadline, 'Controlled child did not start')
                    time.sleep(0.02)
                children = json.loads(marker.read_text())
                self.assertEqual(len(children), 2)
                os.kill(process.pid, signum)
                stdout, stderr = process.communicate(timeout=10)
                self.assertNotEqual(process.returncode, 0, (stdout + stderr).decode(errors='replace'))
                report = json.loads((root / 'evidence/report.json').read_text())
                self.assertEqual(report['state'], 'CANCELLED')
                self.assertIs(report['passed'], False)
                self.assertEqual(report['error'], 'INTERRUPTED')
                self.assertEqual(report['interrupted_signal'], signum)
                self.assertIs(report['all_requested_features_complete'], False)
                self.assertIs(report['production_ready'], False)
                self.assertEqual(report, json.loads((root / report['run_directory'] / 'report.json').read_text()))
                for pid in children:
                    deadline = time.monotonic() + 5
                    while self.running(pid):
                        self.assertLess(time.monotonic(), deadline, f'Child {pid} survived driver interruption')
                        time.sleep(0.02)
            finally:
                if process.poll() is None:
                    process.kill()
                    process.communicate(timeout=5)
                if children and self.running(children[0]):
                    try:
                        os.killpg(children[0], signal.SIGKILL)
                    except ProcessLookupError:
                        pass

    @staticmethod
    def running(pid):
        status = subprocess.run(['ps', '-p', str(pid), '-o', 'stat='], capture_output=True, text=True)
        return status.returncode == 0 and status.stdout.strip() and not status.stdout.strip().startswith('Z')

    def test_sigterm_persists_cancelled_and_stops_the_child_group(self):
        self.interruption(signal.SIGTERM)

    def test_sigint_persists_cancelled_and_stops_the_child_group(self):
        self.interruption(signal.SIGINT)

if __name__ == '__main__':
    unittest.main()
