#!/usr/bin/env python3
"""Build a controlled SwapV2 bank from the adaptive corpus and captured programs."""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'integration/tools/acceptance'))
from orca_adaptive_dynamic_fixture import corpus, pool
from r01_public_catalog_solana_bank import native_imports

ORCA = 'whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc'
ORCA_DATA = 'CtXfPzz36dH5Ws4UYKZvrQ1Xqzn42ecDW6y8NKuiN8nD'
TOKEN22 = 'TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb'
TOKEN22_DATA = 'DoU57AYuPFu2QU514RktNPG22QhApEjnKxnBcu4BHDTY'
TOKEN = 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'
TOKEN_DATA = '3gvYRKWyXRR9xKWe1ZjPhLY5ZJRN7KDB4rFZFGoJfFk2'
MEMO = 'MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr'
CLOCK = 'SysvarC1ock11111111111111111111111111111111'
SYSTEM = '11111111111111111111111111111111'
ATA = 'ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL'
RENT = 'SysvarRent111111111111111111111111111111111'
BUDGET = 'ComputeBudget111111111111111111111111111111'
PROGRAM_IDS = [ORCA, ORCA_DATA, TOKEN22, TOKEN22_DATA, MEMO, TOKEN, TOKEN_DATA]


def b64(data):
    return [base64.b64encode(data).decode(), 'base64']


def raw(account):
    return base64.b64decode(account['data'][0], validate=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('deployment_capture', type=Path, help='getMultipleAccounts response in PROGRAM_IDS order')
    parser.add_argument('output', type=Path)
    args = parser.parse_args()

    svm, pubkey, keypair, *_ = native_imports()
    vm = svm().with_mainnet_features()
    capture = json.loads(args.deployment_capture.read_bytes())['result']
    assert len(capture['value']) == len(PROGRAM_IDS)
    programs = dict(zip(PROGRAM_IDS, capture['value']))
    for account in programs.values():
        if account['data'][1] == 'base64+zstd':
            account['data'] = b64(subprocess.run(
                ['zstd', '-dc'], input=raw(account), check=True, capture_output=True,
            ).stdout)
        assert account['data'][1] == 'base64'

    scenario = corpus()['scenarios'][0]
    bank = scenario['banks'][0]
    model = pool(scenario['config'], bank['native_state'])
    model.update(protocol='orca-whirlpools-adaptive-token2022',
                 adapter_version='orca-whirlpools/cc683ec9-token2022-adaptive-1',
                 chain_key='solana:mainnet-beta')
    state = model['orca_whirlpool']
    slot = capture['context']['slot']
    accounts = bank['accounts'] | programs
    clock = bytearray(raw(accounts[CLOCK]))
    clock[:8] = slot.to_bytes(8, 'little')
    accounts[CLOCK]['data'] = b64(clock)

    def program_hash(key):
        return hashlib.sha256(raw(programs[key])[45:]).hexdigest()

    state.update(bank_slot=slot,
                 deployed_slot=int.from_bytes(raw(programs[ORCA_DATA])[4:12], 'little'),
                 program_code_sha256=program_hash(ORCA_DATA),
                 token_program_code_sha256=program_hash(TOKEN_DATA))
    state['token_2022'] = dict(
        programdata=TOKEN22_DATA,
        deployed_slot=int.from_bytes(raw(programs[TOKEN22_DATA])[4:12], 'little'),
        program_code_sha256=program_hash(TOKEN22_DATA),
    )
    for side, (old_bps, new_bps, cap) in enumerate([(100, 250, 20), (300, 50, 100)]):
        mint = bytearray(278)
        mint[44:46] = bytes([state[f'mint{side}_decimals'], 1])
        mint[165:170] = bytes([1, 1, 0, 108, 0])
        for offset, epoch, bps in [(242, 0, old_bps), (260, state['bank_epoch'] + 1, new_bps)]:
            mint[offset:offset + 18] = (
                epoch.to_bytes(8, 'little') + cap.to_bytes(8, 'little') + bps.to_bytes(2, 'little')
            )
        vault = raw(accounts[state[f'vault{side}']]) + bytes([2, 2, 0, 8, 0])
        vault += (17 + side).to_bytes(8, 'little') + bytes([7, 0, 0, 0])
        state['token_2022'][f'token{side}'] = dict(mint_data=mint.hex(), vault_data=vault.hex())

    caller = keypair.from_seed(hashlib.sha256(b'XONE Orca SwapV2 execution test').digest()).pubkey()
    recipient = pubkey.from_bytes(hashlib.sha256(b'XONE Orca SwapV2 recipient').digest())
    accounts[str(caller)] = dict(owner=SYSTEM, executable=False, lamports=100_000_000, data=b64(b''))
    for key in [SYSTEM, ATA, RENT, BUDGET]:
        account = vm.get_account(pubkey.from_string(key))
        accounts[key] = dict(owner=str(account.owner), executable=account.executable,
                             lamports=account.lamports, data=b64(account.data))
    args.output.write_text(json.dumps(dict(
        pool=model, accounts=accounts, caller=str(caller), recipient=str(recipient),
        blockhash=str(vm.latest_blockhash()),
        rent={size: vm.minimum_balance_for_rent_exemption(size) for size in [165, 178, 182]},
    ), separators=(',', ':')))


if __name__ == '__main__':
    main()
