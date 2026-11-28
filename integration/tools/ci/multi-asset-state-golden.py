#!/usr/bin/env python3
"""Build the synthetic three-token weighted pool used by the contract tests."""

import hashlib
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[3]
UNIT = 10**18


def address(number):
    return f'0x{number:040x}'


def code_digest(number):
    return hashlib.sha256(bytes((0x60, number, 0x00))).hexdigest()


def main():
    pool_id = address(5)
    assets = [
        {
            'token': address(number),
            'balance': str(10**6 * UNIT),
            'scaling': str(UNIT),
            'code_sha256': code_digest(number),
        }
        for number in (1, 2, 7)
    ]
    pool = {
        'chain_key': 'evm:31337',
        'pool_id': pool_id,
        'protocol': 'balancer-v2-weighted-multi',
        'token0': assets[0]['token'],
        'token1': assets[1]['token'],
        'reserve0': assets[0]['balance'],
        'reserve1': assets[1]['balance'],
        'fee_numerator': 3 * 10**15,
        'fee_denominator': UNIT,
        'adapter_version': 'balancer-v2-weighted-multi/static-1',
        'token_behavior': 'standard',
        'multi_asset': {
            'code_sha256': code_digest(5),
            'vault': address(8),
            'vault_code_sha256': code_digest(8),
            'pool_id': f'{pool_id}0001{1:020x}',
            'assets': assets,
            'weights': [str(weight * UNIT // 10) for weight in (4, 3, 3)],
            'amplification': '0',
            'admin_fee': 0,
        },
    }
    text = json.dumps({'pools': [pool]}, indent=2) + '\n'
    for service in ('integration', 'chain-parser-go', 'chain-parser-rust', 'dex-router'):
        (ROOT / service / 'test/contracts/multi-asset-state.json').write_text(text, encoding='ascii')


if __name__ == '__main__':
    main()
