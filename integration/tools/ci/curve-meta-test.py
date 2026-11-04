#!/usr/bin/env python3
"""Run MetaUSD offline checks, with optional generation failure injection.

--failure-injection invalidates receipts deliberately, tests dependency/pin/
compiler/execution failures, and fully regenerates in finally before verifying.
"""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[3]
REF = ROOT / 'integration/tools/reference/curve-meta'
GEN = ROOT / 'integration/tools/ci/curve-meta-golden.cjs'
VERIFY = ROOT / 'integration/tools/ci/curve-meta-verify.py'


def require(ok, message):
    if not ok:
        raise ValueError(message)


def main():
    if '--failure-injection' in sys.argv:
        try:
            with tempfile.TemporaryDirectory(prefix='curve-meta-') as temp:
                malformed = Path(temp) / 'pin.json'
                malformed.write_text('{broken')
                wrong = Path(temp) / 'wrong.json'
                wrong.write_text('{}')
                cases = [('missing dependency', {'CURVE_META_NODE_MODULES': temp}),
                         ('malformed source pin', {'CURVE_META_SOURCE_PIN': str(malformed)}),
                         ('wrong source pin', {'CURVE_META_SOURCE_PIN': str(wrong)}),
                         ('compiler failure', {'CURVE_META_VYPER': '/usr/bin/false'}),
                         ('execution failure', {'CURVE_META_FAIL_STAGE': 'execution'})]
                for name, overrides in cases:
                    run = subprocess.run(['node', str(GEN)], env={**os.environ, **overrides}, capture_output=True, text=True)
                    require(run.returncode != 0, name + ' did not fail')
                    require(json.loads((REF / 'generation.json').read_text())['generation_complete'] is False, name + ' retained completed receipt')
                    rejected = subprocess.run([sys.executable, '-O', str(VERIFY)], capture_output=True, text=True)
                    require(rejected.returncode != 0 and 'No completed generation receipt' in rejected.stderr, name + ' accepted under -O')
                    print('failure injection rejected:', name, flush=True)
        finally:
            subprocess.run(['node', str(GEN)], check=True)
    subprocess.run([sys.executable, str(VERIFY), '--recompile'], check=True)
    subprocess.run([sys.executable, '-O', str(VERIFY)], check=True)
    # Tamper checks operate on in-memory values; corpus bytes stay unchanged.
    spec = importlib.util.spec_from_file_location('curve_meta_verify', VERIFY)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    rows = json.loads((ROOT / 'integration/test/contracts/curve-meta-direct.json').read_text())
    row = rows['scenarios'][0]['steps'][0]
    row['expected']['next_admin_balances'][0] = str(int(row['expected']['next_admin_balances'][0]) + 1)
    try:
        module.verify_row(row)
    except ValueError:
        pass
    else:
        raise ValueError('Admin tamper accepted')
    print('MetaUSD offline checks and accounting tamper rejection passed')


if __name__ == '__main__':
    main()
