#!/usr/bin/env python3
"""Rebuild pinned executed-contract references outside the business services."""
from pathlib import Path
import argparse
import shutil
import subprocess

ROOT = Path(__file__).resolve().parents[3]
SOURCE = ROOT / 'integration/tools/reference/stable-models'
RUNTIME = ROOT / 'integration/.runtime/stable-reference'
VENV = ROOT / 'integration/.runtime/curve-vyper'
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--python', default='/usr/bin/python3',
                    help='Python 3.9 or 3.10, required by Vyper 0.3.1')
parser.add_argument('--reuse-environment', action='store_true',
                    help='Use existing npm packages and Vyper venv; still verify versions and source hashes')
args = parser.parse_args()
RUNTIME.mkdir(parents=True, exist_ok=True)
for name in ['package.json', 'package-lock.json']:
    shutil.copyfile(SOURCE / name, RUNTIME / name)
if not args.reuse_environment:
    subprocess.run(['npm', 'ci', '--prefix', str(RUNTIME), '--ignore-scripts',
                    '--no-audit', '--no-fund'], check=True)
    subprocess.run([args.python, '-c',
                    'import sys; assert (3,9) <= sys.version_info[:2] <= (3,10), "Vyper 0.3.1 requires Python 3.9/3.10"'], check=True)
    subprocess.run([args.python, '-m', 'venv', str(VENV)], check=True)
    subprocess.run([str(VENV / 'bin/python'), '-m', 'pip', 'install',
                    '-r', str(SOURCE / 'requirements-vyper.txt')], check=True)
version = subprocess.run([str(VENV / 'bin/vyper'), '--version'], check=True,
                         capture_output=True, text=True).stdout.strip()
if not version.startswith('0.3.1+') and version != '0.3.1':
    raise SystemExit('Expected pinned Vyper 0.3.1, got ' + version)
subprocess.run(['python3', str(ROOT / 'integration/tools/ci/stable-reference-fetch.py')], check=True)
subprocess.run(['node', str(ROOT / 'integration/tools/ci/stable-golden.cjs')], check=True)
