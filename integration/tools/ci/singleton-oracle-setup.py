#!/usr/bin/env python3
"""Install the exact locked compiler/runtime without npm scripts, verify primary sources, run oracle."""
import argparse
import shutil
import subprocess
from pathlib import Path
ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'integration/tools/reference/singleton-cl'
RUNTIME = ROOT / 'integration/.runtime/singleton-reference'

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--reuse-environment', action='store_true')
    parser.add_argument('--skip-download', action='store_true', help='Generator still verifies every source hash before compiling')
    args = parser.parse_args()
    RUNTIME.mkdir(parents=True, exist_ok=True)
    for name in ['package.json', 'package-lock.json']:
        shutil.copyfile(SOURCE / name, RUNTIME / name)
    if not args.reuse_environment:
        subprocess.run(['npm', 'ci', '--prefix', str(RUNTIME), '--ignore-scripts', '--no-audit', '--no-fund'], check=True)
    if not args.skip_download:
        subprocess.run(['python3', str(ROOT / 'integration/tools/ci/singleton-reference-fetch.py')], check=True)
    subprocess.run(['node', str(ROOT / 'integration/tools/ci/singleton-golden.cjs')], check=True)

if __name__ == '__main__':
    main()
