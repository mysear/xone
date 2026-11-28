#!/usr/bin/env python3
"""Project the nine historical receipt cases into bounded Go test fixtures."""

import argparse
import gzip
import json
from pathlib import Path

from capture import CASES


def read_json(path):
    return json.loads(path.read_bytes())


def projection(source, case):
    name, key, height, _ = case
    directory = source / name
    vector = read_json(directory / 'vector.json')
    records = read_json(directory / 'rpc.json')
    block = vector['block']
    receipts = vector['receipts']
    if (vector['chain_key'] != key or int(block['number'], 16) != height
            or not receipts or [r['transactionHash'] for r in receipts] != block['transactions']):
        raise ValueError(f'{name}: invalid fixed block or receipt order')
    if (records[0]['request']['method'] != 'eth_chainId'
            or int(records[0]['response']['result'], 16) != int(key.split(':')[1])
            or records[1]['request']['method'] != 'eth_getBlockByNumber'
            or records[1]['request']['params'] != [hex(height), False]
            or records[1]['response']['result'] != block):
        raise ValueError(f'{name}: block differs from the retained RPC response')
    receipt_requests = [record['request'] for record in records[2:]]
    receipt_results = [record['response']['result'] for record in records[2:]]
    if len(receipt_requests) == 1 and receipt_requests[0]['method'] == 'eth_getBlockReceipts':
        observed = receipt_results[0]
        if receipt_requests[0]['params'] != [block['hash']]:
            raise ValueError(f'{name}: bulk receipts requested under a different hash')
    else:
        observed = receipt_results
        if (any(r['method'] != 'eth_getTransactionReceipt' for r in receipt_requests)
                or [r['params'] for r in receipt_requests] != [[h] for h in block['transactions']]):
            raise ValueError(f'{name}: receipt requests differ from the fixed block order')
    if observed != receipts:
        raise ValueError(f'{name}: receipts differ from the retained RPC responses')
    return dict(vector, response_sha256=[record['response_sha256'] for record in records])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('source', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    vectors = {case[0]: projection(args.source, case) for case in CASES[:9]}
    args.output.mkdir(parents=True, exist_ok=True)
    for name, vector in vectors.items():
        encoded = json.dumps(vector, sort_keys=True, separators=(',', ':')).encode()
        with (args.output / f'{name}.json.gz').open('xb') as output:
            output.write(gzip.compress(encoded, mtime=0))
    print(f'Projected {len(vectors)} fixed blocks and {sum(len(v["receipts"]) for v in vectors.values())} receipts')


if __name__ == '__main__':
    main()
