#!/usr/bin/env python3
"""Seed a local Agave ledger and capture the finalized Orca quote bank."""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import sys
import urllib.parse
import urllib.request

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'acceptance'))
from r01_public_catalog_solana_bank import native_imports

ORCA_PROGRAMDATA = 'CtXfPzz36dH5Ws4UYKZvrQ1Xqzn42ecDW6y8NKuiN8nD'
SPL_TOKEN = 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'
SPL_TOKEN_PROGRAMDATA = '3gvYRKWyXRR9xKWe1ZjPhLY5ZJRN7KDB4rFZFGoJfFk2'
ASSOCIATED = 'ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL'
EXCLUDED = {
    '11111111111111111111111111111111',
    'ComputeBudget111111111111111111111111111111',
    'TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb',
    'DoU57AYuPFu2QU514RktNPG22QhApEjnKxnBcu4BHDTY',
    'MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr',
    ASSOCIATED,
}


def write(path, value):
    path.write_text(json.dumps(value, separators=(',', ':')) + '\n')


def seed(bank_path, output):
    bank = json.loads(bank_path.read_bytes())
    pool = bank['pool']
    pool.update(protocol='orca-whirlpools-adaptive-dynamic',
                adapter_version='orca-whirlpools/cc683ec9-spl-adaptive-dynamic-1')
    state = pool['orca_whirlpool']
    del state['token_2022']
    state['deployed_slot'] = 0
    accounts = bank['accounts']
    _, pubkey, *_ = native_imports()
    for owner, mint, amount in [(bank['caller'], pool['token0'], 100000),
                                (bank['recipient'], pool['token1'], 155)]:
        owner = bytes(pubkey.from_string(owner))
        mint = bytes(pubkey.from_string(mint))
        token = bytes(pubkey.from_string(SPL_TOKEN))
        address, _ = pubkey.find_program_address(
            [owner, token, mint], pubkey.from_string(ASSOCIATED),
        )
        data = bytearray(165)
        data[:32] = mint
        data[32:64] = owner
        data[64:72] = amount.to_bytes(8, 'little')
        data[108] = 1
        accounts[str(address)] = dict(owner=SPL_TOKEN, executable=False,
                                      lamports=bank['rent']['165'],
                                      data=[base64.b64encode(data).decode(), 'base64'])
    for key in (ORCA_PROGRAMDATA, SPL_TOKEN_PROGRAMDATA):
        data = bytearray(base64.b64decode(accounts[key]['data'][0]))
        data[4:12] = bytes(8)
        accounts[key]['data'][0] = base64.b64encode(data).decode()
    directory = output / 'accounts'
    directory.mkdir(parents=True)
    for key, account in accounts.items():
        if key.startswith('Sysvar') or key in EXCLUDED:
            continue
        account.setdefault('rentEpoch', 0)
        write(directory / (key + '.json'), dict(pubkey=key, account=account))
    write(output / 'staged.json', bank)
    print(bank['caller'])


def capture(output, endpoint):
    url = urllib.parse.urlsplit(endpoint)
    if url.scheme != 'http' or url.hostname != '127.0.0.1':
        raise ValueError('local validator URL required')

    def rpc(method, *params):
        request = urllib.request.Request(
            endpoint, json.dumps(dict(jsonrpc='2.0', id=1, method=method, params=params)).encode(),
            {'Content-Type': 'application/json'},
        )
        with urllib.request.urlopen(request, timeout=15) as response:
            result = json.load(response)
        if 'error' in result:
            raise RuntimeError(f'{method}: {result["error"]}')
        return result['result']

    bank = json.loads((output / 'staged.json').read_bytes())
    bank['genesis'] = rpc('getGenesisHash')
    keys = list(bank['accounts'])
    observed = rpc('getMultipleAccounts', keys, dict(commitment='finalized', encoding='base64'))
    bank['accounts'] = dict(zip(keys, observed['value'], strict=True))
    clock = base64.b64decode(bank['accounts']['SysvarC1ock11111111111111111111111111111111']['data'][0])
    slot = observed['context']['slot']
    if len(clock) != 40 or int.from_bytes(clock[:8], 'little') != slot or slot == 0:
        raise ValueError('finalized post-genesis bank required')
    state = bank['pool']['orca_whirlpool']
    state.update(bank_slot=slot, bank_epoch=int.from_bytes(clock[16:24], 'little'),
                 bank_unix_timestamp=int.from_bytes(clock[32:40], 'little', signed=True))
    for key, digest in [(ORCA_PROGRAMDATA, state['program_code_sha256']),
                        (SPL_TOKEN_PROGRAMDATA, state['token_program_code_sha256'])]:
        data = base64.b64decode(bank['accounts'][key]['data'][0])
        if hashlib.sha256(data[45:]).hexdigest() != digest or data[4:12] != bytes(8):
            raise ValueError(f'validator program differs from the captured original: {key}')
    block = rpc('getBlock', slot, dict(commitment='finalized', transactionDetails='none', rewards=False))
    bank.update(blockhash=block['blockhash'], previousBlockhash=block['previousBlockhash'])
    write(output / 'captured.json', bank)
    print(bank['genesis'])


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    actions = parser.add_subparsers(dest='action', required=True)
    seed_args = actions.add_parser('seed')
    seed_args.add_argument('bank', type=Path)
    seed_args.add_argument('directory', type=Path)
    capture_args = actions.add_parser('capture')
    capture_args.add_argument('directory', type=Path)
    capture_args.add_argument('endpoint')
    args = parser.parse_args()
    if args.action == 'seed':
        seed(args.bank, args.directory)
    else:
        capture(args.directory, args.endpoint)
