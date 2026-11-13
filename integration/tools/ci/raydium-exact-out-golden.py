#!/usr/bin/env python3
"""Generate independent fixed-source native ExactOut vectors; no service math imported."""
from pathlib import Path
import hashlib,json,os,subprocess
ROOT=Path(__file__).resolve().parents[3]
def main():
 env=dict(os.environ,PATH=str(Path.home()/'.cargo/bin')+os.pathsep+os.environ['PATH'],CARGO_BUILD_JOBS='2',CARGO_TARGET_DIR=str(ROOT/'integration/.runtime/raydium-reference/target'))
 subprocess.run(['cargo','+1.90.0','build','--locked','--offline','--manifest-path',str(ROOT/'integration/tools/reference/raydium-cpmm/Cargo.toml')],env=env,cwd=ROOT,check=True)
 binary=ROOT/'integration/.runtime/raydium-reference/target/debug/raydium-cpmm-reference'
 raw=subprocess.check_output([binary,'exact-out-corpus'],cwd=ROOT,stderr=subprocess.PIPE)
 again=subprocess.check_output([binary,'exact-out-corpus'],cwd=ROOT,stderr=subprocess.PIPE)
 if raw!=again:raise ValueError('NONDETERMINISTIC_REFERENCE')
 doc=json.loads(raw)
 if doc['reference']['revision']!='f4b9b26342afe9122d5ff67cd7a4f19ded4ac0a5':raise ValueError('REFERENCE_REVISION')
 passed=sum('reverted'not in c['expected']for c in doc['cases'])
 for service in ['integration','dex-router']:(ROOT/service/'test/contracts/raydium-exact-out.json').write_bytes(raw)
 print(json.dumps(dict(cases=len(doc['cases']),success=passed,rejections=len(doc['cases'])-passed,sequences=len(doc['sequences']),sequence_steps=sum(len(s['steps'])for s in doc['sequences']),sha256=hashlib.sha256(raw).hexdigest())))
if __name__=='__main__':main()
