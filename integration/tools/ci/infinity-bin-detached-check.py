#!/usr/bin/env python3
import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
REF = ROOT / 'integration/tools/reference/infinity-bin-detached'

def require(ok, message):
    if not ok:
        raise RuntimeError(message)

def main():
    receipt = REF / 'check-receipt.json'
    receipt.write_text(json.dumps({'passed': False, 'state': 'RUNNING'}) + '\n')
    try:
        checker = ROOT / 'integration/tools/ci/infinity-bin-detached-check.cjs'
        run = subprocess.run(['node', str(checker), *sys.argv[1:]], capture_output=True, text=True)
        require(run.returncode == 0, run.stderr)
        result = json.loads(run.stdout)
        inputs = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in [Path(__file__).resolve(), checker, ROOT / 'integration/tools/ci/infinity-bin-detached-lib.cjs', REF / 'execution-receipt.json']}
        receipt.write_text(json.dumps({'passed': True, 'optimized': sys.flags.optimize, 'inputs': inputs, **result}, indent=2) + '\n')
        print(json.dumps(result))
    except BaseException as error:
        receipt.write_text(json.dumps({'passed': False, 'error': str(error)}) + '\n')
        raise

if __name__ == '__main__':
    main()
