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
    receipt = REF / ('guards-optimized-receipt.json' if sys.flags.optimize else 'guards-receipt.json')
    receipt.write_text(json.dumps({'passed': False, 'state': 'RUNNING'}) + '\n')
    try:
        guard = ROOT / 'integration/tools/ci/infinity-bin-detached-guards.cjs'
        run = subprocess.run(['node', str(guard)], text=True, capture_output=True)
        require(run.returncode == 0, run.stderr)
        result = json.loads(run.stdout)
        require(result['passed'] is True and result['negative_cases'] >= 45, 'negative coverage')
        checker = ROOT / 'integration/tools/ci/infinity-bin-detached-check.py'
        command = [sys.executable, *(['-OO'] if sys.flags.optimize else []), str(checker)]
        bad = subprocess.run([*command, '--semantic-document', str(REF / 'nonexistent.json')], text=True, capture_output=True)
        require(bad.returncode != 0, 'missing input accepted')
        for name in ['check-receipt.json', 'check-node-receipt.json']:
            require(json.loads((REF / name).read_text())['passed'] is False, 'stale success receipt ' + name)
        good = subprocess.run(command, text=True, capture_output=True)
        require(good.returncode == 0, good.stderr)
        inputs = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in [Path(__file__).resolve(), guard, checker, ROOT / 'integration/tools/ci/infinity-bin-detached-check.cjs', REF / 'execution-receipt.json']}
        receipt.write_text(json.dumps({'passed': True, 'optimized': sys.flags.optimize, 'inputs': inputs, **result, 'stale_receipt_invalidated': True}, indent=2) + '\n')
        print(json.dumps({**result, 'optimized': sys.flags.optimize, 'stale_receipt_invalidated': True}))
    except BaseException as error:
        receipt.write_text(json.dumps({'passed': False, 'error': str(error)}) + '\n')
        raise

if __name__ == '__main__':
    main()
