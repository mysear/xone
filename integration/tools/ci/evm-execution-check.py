#!/usr/bin/env python3
"""Fail closed on identity, state, receipt, source closure, or regeneration drift."""
import argparse
from pathlib import Path
import shutil
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]

def main():
    p = argparse.ArgumentParser()
    p.add_argument('--input', type=Path)
    args = p.parse_args()
    primary = ROOT / 'integration/test/contracts/evm-execution.json'
    mirror = ROOT / 'dex-router/test/contracts/evm-execution.json'
    target = args.input or primary
    if not args.input:
        if primary.read_bytes() != mirror.read_bytes():
            raise ValueError('fixture copies differ')
    node = shutil.which('node')
    if not node:
        raise RuntimeError('pinned Node executable unavailable')
    subprocess.run([node, str(ROOT / 'integration/tools/ci/evm-execution-golden.cjs'), '--check', str(target.resolve())], check=True, cwd=ROOT)

if __name__ == '__main__':
    try:
        main()
    except Exception as e:
        print('evm-execution check failed:', e, file=sys.stderr)
        sys.exit(1)
