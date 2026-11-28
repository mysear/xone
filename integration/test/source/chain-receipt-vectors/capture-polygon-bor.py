#!/usr/bin/env python3
"""Capture the fixed Bor receipt regression from the two public RPC sources."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import gzip
import json
from pathlib import Path

from capture import CASES, rpc


BLOCK_HASH = '0xc63c0ff174816b7983671e4b76140f57aa561e02d928829c22798e13471cc2b0'
GENESIS_HASH = '0xa9c28ce2141b56c474f1dc504bee9b01eb1bd7d1a507580d5519d4437a97de1b'


def collect(endpoint, key, height):
    requests = {
        'chain_id': ('eth_chainId', []),
        'genesis': ('eth_getBlockByNumber', ['0x0', False]),
        'block': ('eth_getBlockByNumber', [hex(height), False]),
        'full_block': ('eth_getBlockByNumber', [hex(height), True]),
        'receipts': ('eth_getBlockReceipts', [BLOCK_HASH]),
    }
    data = {'chain_key': key}
    hashes = {}
    for field, (method, params) in requests.items():
        data[field], record = rpc(endpoint, method, params)
        hashes[field] = record['response_sha256']

    block = data['block']
    full = data['full_block']
    receipts = data['receipts']
    identities = block['transactions']
    if (int(data['chain_id'], 16) != int(key.split(':')[1]) or data['genesis']['hash'] != GENESIS_HASH
            or int(block['number'], 16) != height or block['hash'] != BLOCK_HASH):
        raise ValueError(f'{endpoint}: unexpected chain or fixed block identity')
    if (dict(full, transactions=identities) != block or len(identities) != 390
            or [tx['hash'] for tx in full['transactions']] != identities
            or [receipt['transactionHash'] for receipt in receipts] != identities
            or len(receipts[-1]['logs']) != 41):
        raise ValueError(f'{endpoint}: incomplete or inconsistent Bor responses')
    return data, {'endpoint': endpoint, 'response_sha256': hashes}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    if args.output.exists():
        parser.error('output already exists')
    _, key, height, endpoints = next(case for case in CASES if case[0] == 'polygon-synthetic')
    with ThreadPoolExecutor(max_workers=2) as pool:
        primary, witness = pool.map(lambda endpoint: collect(endpoint, key, height), endpoints)
    if primary[0] != witness[0]:
        raise ValueError('providers disagree on the fixed Bor responses')
    data = dict(primary[0], sources=[primary[1], witness[1]])
    encoded = json.dumps(data, sort_keys=True, separators=(',', ':')).encode()
    with args.output.open('xb') as output:
        output.write(gzip.compress(encoded, mtime=0))
    print(f'{BLOCK_HASH}: two providers matched all five results')


if __name__ == '__main__':
    main()
