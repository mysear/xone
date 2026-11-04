#!/usr/bin/env python3
"""Regression: every generator invalidates stale success before startup/compile.
Runs exact script copies in a temporary tree with read-only input symlinks;
never mutates source pins, generated fixtures or real execution receipts.
"""
import json,os,subprocess,tempfile,unittest
from pathlib import Path
ROOT=Path(__file__).resolve().parents[3]
SCRIPTS={'balancer-multi-golden.cjs':'weighted','balancer-multi-managed.cjs':'managed','balancer-multi-composable.cjs':'composable'}
class FailClosed(unittest.TestCase):
    def exercise(self,name,family,phase,negative=False):
        with tempfile.TemporaryDirectory(prefix='balancer-multi-failclosed-') as tmp:
            root=Path(tmp);ci=root/'integration/tools/ci';ci.mkdir(parents=True)
            for script in SCRIPTS:(ci/script).write_bytes((ROOT/'integration/tools/ci'/script).read_bytes())
            (root/'integration/tools/reference').symlink_to(ROOT/'integration/tools/reference',target_is_directory=True)
            (root/'integration/.runtime').symlink_to(ROOT/'integration/.runtime',target_is_directory=True)
            for svc in ['integration','dex-router']:
                (root/svc/'test').mkdir(parents=True,exist_ok=True)
                (root/svc/'test/contracts').symlink_to(ROOT/svc/'test/contracts',target_is_directory=True)
            artifacts=root/'integration/artifacts/local';artifacts.mkdir(parents=True)
            receipt=artifacts/f'balancer-multi-{family}-generation.json';receipt.write_text('{"passed":true}\n')
            if negative:
                p=ci/name;s=p.read_text();s=s.replace("fs.writeFileSync(receiptPath,JSON.stringify({passed:false,state:'RUNNING'})+'\\n');","void 0;")
                p.write_text(s)
            hook=root/'inject.cjs';hook.write_text("const Module=require('module'),load=Module._load;Module._load=function(id,...args){if(id.endsWith('/solc07')){"+
                ("throw Error('INJECTED_STARTUP_FAILURE');" if phase=='startup' else "const real=load.call(this,id,...args);return {...real,compile(){throw Error('INJECTED_COMPILE_FAILURE');}};")+
                "}return load.call(this,id,...args);};\n")
            env={**os.environ,'NODE_OPTIONS':'--require '+str(hook)}
            result=subprocess.run(['node',str(ci/name)],env=env,capture_output=True,text=True,timeout=90)
            self.assertNotEqual(result.returncode,0,result.stdout)
            self.assertIn('INJECTED_',result.stderr)
            self.assertEqual(json.loads(receipt.read_text())['passed'],negative,(name,phase,receipt.read_text()))
    def test_stale_success_is_invalidated_for_startup_and_compilation(self):
        for name,family in SCRIPTS.items():
            for phase in ['startup','compile']:
                with self.subTest(generator=name,phase=phase):self.exercise(name,family,phase)
    def test_negative_control_reproduces_stale_success_without_invalidation(self):
        for name,family in SCRIPTS.items():
            with self.subTest(generator=name):self.exercise(name,family,'compile',negative=True)
if __name__=='__main__':unittest.main(verbosity=2)
