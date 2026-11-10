#!/usr/bin/env python3
"""Explicit read-only refresh of the two finite public raw-transaction corpora."""
import concurrent.futures
import datetime
import importlib.util
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('receipt_capture', ROOT / 'capture.py')
capture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(capture)
for name, endpoint in [('bsc', 'https://bsc-dataseed.bnbchain.org'),
                       ('optimism-regolith', 'https://optimism-rpc.publicnode.com'),
                       ('arbitrum', 'https://arbitrum-one-rpc.publicnode.com'),
                       ('avalanche', 'https://avalanche-c-chain-rpc.publicnode.com'),
                       ('avalanche-current', 'https://avalanche-c-chain-rpc.publicnode.com')]:
    output = ROOT / name / 'body.json'
    if output.exists():
        print(name, 'already captured')
        continue
    vector = json.loads((ROOT / name / 'vector.json').read_text())
    _, identity = capture.rpc(endpoint, 'eth_chainId', [])
    genesis, genesis_record = capture.rpc(endpoint, 'eth_getBlockByNumber', ['0x0', False])
    def one(tx):
        _, record = capture.rpc(endpoint, 'eth_getRawTransactionByHash', [tx])
        return record
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        transactions = list(pool.map(one, vector['block']['transactions']))
    # Empty 0x responses are retained negative raw-method observations; V2
    # verifies the explicit complete JSON authority instead of accepting them.
    output.write_text(json.dumps({'chain_key': vector['chain_key'], 'genesis_hash': genesis['hash'],
        'endpoint': endpoint, 'captured_at': datetime.datetime.now(datetime.timezone.utc).isoformat(),
        'identity': identity, 'genesis': genesis_record, 'raw_transactions': transactions}, indent=2) + '\n')
    print(name, 'captured', len(transactions))
