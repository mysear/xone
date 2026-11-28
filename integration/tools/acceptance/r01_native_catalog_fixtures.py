#!/usr/bin/env python3
"""Project pinned public block and receipt responses into the Go service tests."""

import argparse
import gzip
import hashlib
import json
import tarfile

from r01_fixed_evm_replay import ARCHIVES, BASE, ROOT, require

FIXTURES = ROOT / 'chain-parser-go/test/fixtures/native-catalog-replay'
HISTORY = 'integration/test/source/deployment-catalog/evm-continuation-v2'
COMMON_RECORDS = ('002-genesis.json', '005-finalized-header.json', '006-witness-header.json')
CASES = {
    'arbitrum-528150486': (
        'op-arbitrum-fixed-2026-11-28.tar.gz',
        'anchor-iteration-ee66n2qk/arbitrum',
        COMMON_RECORDS + ('017-block-receipts.json',),
    ),
    'avalanche-101597996': (
        'avalanche-uniswap-v3-101597996.tar.gz',
        'avalanche-getter-anchor.43nmhr50',
        COMMON_RECORDS + (
            '007-sae-successor.json', '008-sae-successor.json', '009-sae-successor.json',
            '010-sae-successor.json', '011-sae-successor.json', '021-sae-receipt-window-block.json',
            '022-block-receipts.json', '023-block-receipts.json',
        ),
    ),
}


def project(archive_name, directory, names):
    archive_path = BASE / 'evidence' / archive_name
    require(hashlib.sha256(archive_path.read_bytes()).hexdigest() == ARCHIVES[archive_name],
            f'NATIVE_CATALOG_ARCHIVE_DIGEST {archive_name}')
    results = {}
    with tarfile.open(archive_path, 'r:gz') as archive:
        for name in names:
            member = archive.getmember(f'{HISTORY}/{directory}/{name}')
            require(member.isfile() and member.size <= 16 << 20, f'NATIVE_CATALOG_MEMBER {member.name}')
            with archive.extractfile(member) as record_file:
                record = json.load(record_file)
            method = (
                'eth_getBlockReceipts' if name.endswith('-block-receipts.json')
                else 'eth_getBlockByNumber'
            )
            result = record['response'].get('result')
            require(record['method'] == method and record['http_status'] == 200 and result is not None,
                    f'NATIVE_CATALOG_RPC_RESULT {member.name}')
            results[name] = result
    return json.dumps(results, sort_keys=True, separators=(',', ':')).encode()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--write', action='store_true', help='regenerate the local test vectors')
    args = parser.parse_args()
    for name, case in CASES.items():
        expected = project(*case)
        target = FIXTURES / f'{name}.json.gz'
        if args.write:
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(gzip.compress(expected, mtime=0))
        require(gzip.decompress(target.read_bytes()) == expected, f'NATIVE_CATALOG_FIXTURE_MISMATCH {name}')
        print(f'{name}: verified {len(case[2])} archived responses')


if __name__ == '__main__':
    main()
