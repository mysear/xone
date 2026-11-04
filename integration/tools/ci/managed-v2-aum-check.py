#!/usr/bin/env python3
"""Exact original-bytecode replay and local-copy integrity; safe under -OO."""
from pathlib import Path
ROOT = Path(__file__).resolve().parents[3]
REF = ROOT / 'integration/tools/reference/balancer-managed-v2-aum'
SUCCESS = REF / 'verified.json'
SUCCESS.unlink(missing_ok=True)
import argparse
import hashlib
import json
import subprocess
import tempfile

def main():
    p = argparse.ArgumentParser()
    p.add_argument('--input', type=Path)
    p.add_argument('--validate-only', action='store_true')
    p.add_argument('--check', action='store_true')
    args = p.parse_args()
    targets = [ROOT / s / 'test/contracts/managed-v2-aum.json' for s in ('integration', 'chain-parser-go', 'dex-router')]
    target = args.input.resolve() if args.input else targets[0]
    if args.input is None and len({f.read_bytes() for f in targets}) != 1:
        raise ValueError('three local fixture copies differ')
    script = ROOT / 'integration/tools/ci/managed-v2-aum-golden.cjs'
    subprocess.run(['node', str(script), '--validate', str(target)], check=True, cwd=ROOT)
    if args.validate_only:
        return
    with tempfile.TemporaryDirectory(prefix='managed-v2-aum-exact-') as temp:
        generated = Path(temp) / 'managed-v2-aum.json'
        subprocess.run(['node', str(script), '--output', str(generated)], check=True, cwd=ROOT)
        if generated.read_bytes() != target.read_bytes():
            raise ValueError('exact temporary original-bytecode regeneration mismatch')
    bank = json.loads(target.read_bytes())
    receipt = {'schema_version': 1, 'state': 'PASSED', 'passed': True,
               'fixture_sha256': hashlib.sha256(target.read_bytes()).hexdigest(),
               'cases': len(bank['cases']), 'transactions': len(bank['reference']['transactions']),
               'objects': len(bank['objects']), 'regeneration': 'exact-temporary-byte-comparison',
               'file_manifest_sha256': bank['reference']['inputs']['file_manifest_sha256']}
    SUCCESS.write_text(json.dumps(receipt, sort_keys=True) + '\n')
    print(json.dumps(receipt, sort_keys=True))
if __name__ == '__main__':
    main()
