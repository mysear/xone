#!/usr/bin/env python3
"""Pack the retained body and Avalanche settlement observations for Go replay."""

import argparse
import gzip
import json
from pathlib import Path


FILES = {
    'arbitrum': ('body.json', 'full-block.json'),
    'avalanche': ('body.json', 'full-block.json'),
    'avalanche-current': ('vector.json', 'body.json', 'full-block.json', 'settlement.json',
                          'settlement-multi.json', 'settlement-fresh.json'),
    'bsc': ('body.json',),
    'optimism-regolith': ('body.json', 'full-block.json'),
}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    projected = {}
    for name, files in FILES.items():
        for file in files:
            value = json.loads((args.source / name / file).read_bytes())
            relative = Path(name, file + '.gz')
            if file == 'vector.json':
                relative = Path(name + '.json.gz')
            projected[relative] = json.dumps(value, sort_keys=True, separators=(',', ':')).encode()
    for relative, data in projected.items():
        target = args.output / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        with target.open('xb') as output:
            output.write(gzip.compress(data, mtime=0))
    print(f'Projected {len(projected)} fixed body and settlement observations')


if __name__ == '__main__':
    main()
