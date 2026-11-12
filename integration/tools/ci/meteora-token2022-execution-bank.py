#!/usr/bin/env python3
"""Project existing pinned Token2022 original SBF banks for execution preparation tests."""
from pathlib import Path
import base64
import hashlib
import json
import sys

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'integration/tools/acceptance'))
from r01_public_catalog_solana_bank import native_imports

TOKEN22 = 'TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb'
TOKEN = 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'
ATA = 'ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL'


def main():
    original = ROOT / 'integration/artifacts/local/core-meteora-token2022-original-20261111.json'
    fixture = json.loads((ROOT / 'integration/test/contracts/meteora-token2022.json').read_bytes())
    records = json.loads(original.read_bytes())['records']
    base = json.loads((ROOT / 'integration/test/contracts/meteora-execution-bank.json').read_bytes())
    S, P, *_ = native_imports()
    svm = S().with_mainnet_features()
    if hashlib.sha256(svm.get_account(P.from_string('DoU57AYuPFu2QU514RktNPG22QhApEjnKxnBcu4BHDTY')).data[45:]).hexdigest() != '495e9d7680dd555cb126a6a8e5464af5be9b01f02f2cd70634352722d22e3cad':
        raise ValueError('TOKEN2022_RUNTIME_CODE_CHANGED')
    banks = []
    for index in [7, 17, 57, 77, 97]:
        n = records[index]['native']
        step = fixture['cases'][index]['steps'][0]
        if not n['passed'] or not fixture['cases'][index]['exact_out']:
            raise ValueError('ORIGINAL_SUCCESSFUL_EXACT_OUT_REQUIRED')
        pool = step['pool']
        accounts = dict(fixture['decoder']['shared_accounts'])
        accounts.update(fixture['decoder']['cases'][index]['accounts'])
        for key, a in n['before'].items():
            if key not in accounts:
                accounts[key] = dict(owner=a['owner'], executable=a['executable'], lamports=a['lamports'], data=[a['data_base64'], 'base64'])
        for key in [ATA, '11111111111111111111111111111111']:
            if key in base['accounts']:
                accounts[key] = base['accounts'][key]
        memo = 'MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr'
        accounts[memo] = base['accounts'][memo]
        accounts[n['payer']] = dict(owner='11111111111111111111111111111111', executable=False, lamports=10_000_000_000, data=['', 'base64'])
        zero = n['zero_for_one']
        sell, buy = (pool['token0'], pool['token1']) if zero else (pool['token1'], pool['token0'])
        def wallet(mint, owner):
            found = []
            for key, a in accounts.items():
                raw = base64.b64decode(a['data'][0])
                if a['owner'] in [TOKEN, TOKEN22] and len(raw) >= 165 and raw[:32] == bytes(P.from_string(mint)) and raw[32:64] == bytes(P.from_string(owner)):
                    found.append(key)
            if len(found) != 1:
                raise ValueError('UNIQUE_WALLET_REQUIRED')
            return found[0]
        source, destination = wallet(sell, n['payer']), wallet(buy, n['recipient'])
        output_program = TOKEN if zero else TOKEN22
        canonical = str(P.find_program_address([bytes(P.from_string(n['recipient'])), bytes(P.from_string(output_program)), bytes(P.from_string(buy))], P.from_string(ATA))[0])
        if canonical in accounts:
            raise ValueError('DESTINATION_ATA_MUST_START_ABSENT')
        banks.append(dict(schema_version=1, source_sha256=hashlib.sha256(original.read_bytes()).hexdigest(), source_index=index,
                          authority='controlled original SBF bank; pinned programs/epoch/transfer fee, funded test payer, no production source qualification',
                          pool=pool, accounts=accounts, caller=n['payer'], recipient=n['recipient'], sourceTokenAccount=source,
                          destinationTokenAccount=destination, canonicalDestination=canonical, zeroForOne=zero,
                          genesis=base['genesis'], blockhash=base['blockhash'], target=step['amount'], requiredInput=step['actual_input'],
                          after=step['after'], destinationSize=165 if zero else 182,
                          destinationRent=svm.minimum_balance_for_rent_exemption(165 if zero else 182)))
    for service in ['integration', 'dex-router']:
        (ROOT / service / 'test/contracts/meteora-token2022-execution-bank.json').write_text(json.dumps(dict(cases=banks), separators=(',', ':')) + '\n')
    print(json.dumps(dict(passed=True, cases=len(banks), indexes=[b['source_index'] for b in banks], destinationRent=[b['destinationRent'] for b in banks])))


if __name__ == '__main__':
    main()
