#!/usr/bin/env python3
"""Bounded read-only native public corpus capture at fixed retained heights.

Existing evidence is never replaced. Three settlement files are exact finite
ranges; state proof capture requires an archive retaining the committed state.
"""
import concurrent.futures
import datetime
import importlib.util
import json
from pathlib import Path
ROOT = Path(__file__).resolve().parent
spec = importlib.util.spec_from_file_location('capture', ROOT / 'capture.py')
capture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(capture)
ENDPOINTS = {
    'arbitrum': 'https://arb1.arbitrum.io/rpc',
    'avalanche': 'https://api.avax.network/ext/bc/C/rpc',
    'avalanche-current': 'https://avalanche-c-chain-rpc.publicnode.com',
    'polygon-synthetic': 'https://polygon-bor-rpc.publicnode.com',
}
for name, endpoint in ENDPOINTS.items():
    output = ROOT / name / 'full-block.json'
    if output.exists():
        print(name, 'full block already captured')
        continue
    v = json.loads((ROOT / name / 'vector.json').read_text())
    _, record = capture.rpc(endpoint, 'eth_getBlockByNumber', [v['block']['number'], True])
    record['endpoint'] = endpoint
    output.write_text(json.dumps(record, indent=2) + '\n')

endpoint = ENDPOINTS['avalanche-current']
genesis_hash = json.loads((ROOT / 'avalanche-current/body.json').read_text())['genesis_hash']
# (file, first settled height, last settled height, settling header, target)
for filename, start, end, anchor, target in [
    ('settlement.json', 99642059, 99642059, 99642066, 99642059),
    ('settlement-multi.json', 99642060, 99642061, 99642068, 99642060),
    ('settlement-fresh.json', 99645675, 99645675, 99645682, 99645675),
]:
    output = ROOT / 'avalanche-current' / filename
    if output.exists():
        print(filename, 'already captured')
        continue
    def get_header(height):
        return capture.rpc(endpoint, 'eth_getBlockByNumber', [hex(height), False])
    with concurrent.futures.ThreadPoolExecutor(max_workers=4) as pool:
        header_pairs = list(pool.map(get_header, range(start, anchor + 1)))
    def get_window(height):
        full, body_record = capture.rpc(endpoint, 'eth_getBlockByNumber', [hex(height), True])
        receipts, receipt_record = capture.rpc(endpoint, 'eth_getBlockReceipts', [full['hash']])
        return {'header': header_pairs[height-start][0], 'transactions_json': full['transactions'],
                'receipts': receipts, 'rpc': [body_record, receipt_record]}
    with concurrent.futures.ThreadPoolExecutor(max_workers=3) as pool:
        window = list(pool.map(get_window, range(start, end + 1)))
    data = {'chain_key': 'evm:43114', 'genesis_hash': genesis_hash, 'target_height': target,
            'endpoint': endpoint, 'captured_at': datetime.datetime.now(datetime.timezone.utc).isoformat(),
            'headers': [b for b, _ in header_pairs], 'window': window,
            'rpc_headers': [record for _, record in header_pairs]}
    if filename == 'settlement-fresh.json':
        value, record = capture.rpc(endpoint, 'eth_getProof',
            ['0xb31f66aa3c1e785363f0875a1b74e27b85fd66c7', [], hex(end)])
        data['state_proof'] = value
        data['rpc_state_proof'] = record
    output.write_text(json.dumps(data, indent=2) + '\n')
    print(filename, 'captured bounded public execution window')
