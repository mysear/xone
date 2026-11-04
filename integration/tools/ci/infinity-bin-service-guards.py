#!/usr/bin/env python3
"""Normal and -OO failures must stay closed; no Python assert is used."""
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
    receipt_path = REF / ('guards-optimized-receipt.json' if sys.flags.optimize else 'guards-receipt.json')
    receipt_path.write_text(json.dumps({'passed': False, 'state': 'RUNNING'}) + '\n')
    try:
        guard = ROOT / 'integration/tools/ci/infinity-bin-service-guards.cjs'
        completed = subprocess.run(['node', str(guard)], capture_output=True, text=True)
        require(completed.returncode == 0, completed.stderr)
        result = json.loads(completed.stdout)
        require(result['passed'] is True and result['negative_cases'] >= 50, 'guard coverage')
        # Deliberate missing document: wrapper and child must both fail with invalid receipts.
        checker = ROOT / 'integration/tools/ci/infinity-bin-service-check.py'
        args = [sys.executable]
        if sys.flags.optimize:
            args.append('-OO')
        failed = subprocess.run([*args, str(checker), '--semantic-document', str(REF / 'nonexistent-document.json')], capture_output=True, text=True)
        require(failed.returncode != 0, 'missing document unexpectedly accepted')
        require(json.loads((REF / 'check-receipt.json').read_text())['passed'] is False, 'wrapper retained stale success')
        require(json.loads((REF / 'check-node-receipt.json').read_text())['passed'] is False, 'node retained stale success')
        passed = subprocess.run([*args, str(checker)], capture_output=True, text=True)
        require(passed.returncode == 0, passed.stderr)
        inputs = {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest() for p in [Path(__file__).resolve(), guard, checker, ROOT / 'integration/tools/ci/infinity-bin-service-check.cjs', REF / 'execution-receipt.json']}
        receipt_path.write_text(json.dumps({'passed': True, 'optimized': sys.flags.optimize, 'inputs': inputs, **result, 'missing_document_rejected': True, 'stale_check_receipts_invalidated': True}, indent=2) + '\n')
        print(json.dumps({**result, 'python_optimization': sys.flags.optimize, 'missing_document_rejected': True}))
    except BaseException as error:
        receipt_path.write_text(json.dumps({'passed': False, 'error': str(error)}, indent=2) + '\n')
        raise

if __name__ == '__main__':
    main()
