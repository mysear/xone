#!/usr/bin/env python3
"""Generate/check a thin public-Pool derivative of each independent original bank.

--input uses only that local original copy. Without --output the sibling
managed-v2-aum-pools.json is selected. No reference artifacts are modified.
"""
from pathlib import Path
import argparse
import hashlib
import json
import os
import sys
import tempfile

ROOT = Path(__file__).resolve().parents[3]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / 'integration/tools/acceptance'))
import managed_v2_projection as projection


def generate(path):
    return (json.dumps(projection.project_bank(projection.load(path)), indent=2, ensure_ascii=False) + '\n').encode()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input', type=Path)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--check', action='store_true')
    args = parser.parse_args()
    if args.output is not None and args.input is None:
        parser.error('--output requires --input')
    inputs = [args.input] if args.input else [ROOT / project / 'test/contracts/managed-v2-aum.json'
                                             for project in ('integration', 'chain-parser-go', 'dex-router')]
    # Fully authenticate/generate before any publication. Every copy is read
    # independently; a --input invocation never opens another project's bank.
    rows = [(path, args.output or path.with_name('managed-v2-aum-pools.json'), generate(path)) for path in inputs]
    if len({data for _, _, data in rows}) != 1:
        raise ValueError('independent thin projections differ')
    for original, output, data in rows:
        if output.name == 'managed-v2-aum.json' or output.resolve() == original.resolve() or output.resolve().is_relative_to(projection.REFERENCE):
            raise ValueError('refuse original-bank/reference output')
        if args.check:
            if not output.is_file() or output.read_bytes() != data:
                raise ValueError('thin public Pool golden mismatch: ' + str(output))
    for _, output, data in rows:
        if not args.check:
            fd, temporary = tempfile.mkstemp(prefix=output.name + '.', suffix='.tmp', dir=output.parent)
            try:
                with os.fdopen(fd, 'wb') as stream:
                    stream.write(data)
                Path(temporary).replace(output)
            finally:
                Path(temporary).unlink(missing_ok=True)
    print(json.dumps({'checked': args.check, 'copies': len(rows), 'cases': 27,
                      'bytes': len(rows[0][2]), 'sha256': hashlib.sha256(rows[0][2]).hexdigest(),
                      'original_bank_sha256': projection.BANK_SHA256}, sort_keys=True))


if __name__ == '__main__':
    main()
