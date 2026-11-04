#!/usr/bin/env python3
"""Fail-closed wrapper, deliberately independent of Python optimization flags."""
import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
REF = ROOT / 'integration/tools/reference/infinity-bin-service'

def require(condition, message):
    if not condition:
        raise RuntimeError(message)

def main():
    receipt_path = REF / 'check-receipt.json'
    receipt_path.write_text(json.dumps({'passed': False, 'state': 'RUNNING'}) + '\n')
    try:
        checker = ROOT / 'integration/tools/ci/infinity-bin-service-check.cjs'
        completed = subprocess.run(['node', str(checker), *sys.argv[1:]], text=True, capture_output=True)
        require(completed.returncode == 0, completed.stderr)
        result = json.loads(completed.stdout)
        inputs = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in [Path(__file__).resolve(), checker, REF / 'execution-receipt.json']}
        receipt_path.write_text(json.dumps({'passed': True, 'optimized': sys.flags.optimize, 'inputs': inputs, **result}, indent=2) + '\n')
        print(json.dumps(result, sort_keys=True))
    except BaseException as error:
        receipt_path.write_text(json.dumps({'passed': False, 'error': str(error)}, indent=2) + '\n')
        raise

if __name__ == '__main__':
    main()
