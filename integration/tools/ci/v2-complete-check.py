#!/usr/bin/env python3
"""Fail closed before imports; Python -OO cannot disable validation."""
from pathlib import Path
ROOT = Path(__file__).resolve().parents[3]
SUCCESS = ROOT / 'integration/tools/reference/v2-complete/verified.json'
SUCCESS.unlink(missing_ok=True)
import argparse
import hashlib
import json
import shutil
import subprocess
import sys
import tempfile

def main():
    p = argparse.ArgumentParser()
    p.add_argument('--input', type=Path)
    p.add_argument('--check', action='store_true')
    p.add_argument('--validate-only', action='store_true')
    args = p.parse_args()
    targets = [ROOT / s / 'test/contracts/v2-complete.json' for s in ('integration', 'chain-parser-go', 'dex-router')]
    target = (args.input or targets[0]).resolve()
    if not args.input and len({t.read_bytes() for t in targets}) != 1:
        raise ValueError('three fixture copies differ')
    node = shutil.which('node')
    if not node:
        raise RuntimeError('pinned Node unavailable')
    script = ROOT / 'integration/tools/ci/v2-complete-golden.cjs'
    subprocess.run([node, str(script), '--validate', str(target)], cwd=ROOT, check=True)
    if args.validate_only:
        return
    with tempfile.TemporaryDirectory(prefix='v2-complete-') as d:
        generated = Path(d) / 'regenerated.json'
        subprocess.run([node, str(script), '--output', str(generated), '--trace-dir', str(Path(d) / 'traces')], cwd=ROOT, check=True)
        if generated.read_bytes() != target.read_bytes():
            raise ValueError('exact full original-contract regeneration mismatch')
    value = json.loads(target.read_text())
    receipt = {'schema_version': 1, 'status': 'passed', 'fixture_sha256': hashlib.sha256(target.read_bytes()).hexdigest(),
               'case_count': len(value['cases']), 'sequence_count': len(value['sequences']), 'regeneration': 'exact-temp-byte-comparison'}
    SUCCESS.write_text(json.dumps(receipt, indent=2) + '\n')
    print(json.dumps(receipt))

if __name__ == '__main__':
    try:
        main()
    except Exception as e:
        SUCCESS.unlink(missing_ok=True)
        print('v2-complete check failed:', e, file=sys.stderr)
        sys.exit(1)
