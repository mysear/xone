#!/usr/bin/env python3
"""Combine controlled order cases from the native oracle with deployed code."""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'integration/tools/acceptance'))
from r01_public_catalog_solana_bank import native_imports

RAYDIUM = 'CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK'
RAYDIUM_DATA = 'HzD2cCXXT3UQNjMMY6kDv9w6gZ9qquSdfoGXrLL3LXx'
TOKEN = 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'
TOKEN_DATA = '3gvYRKWyXRR9xKWe1ZjPhLY5ZJRN7KDB4rFZFGoJfFk2'
TOKEN22 = 'TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb'
TOKEN22_DATA = 'DoU57AYuPFu2QU514RktNPG22QhApEjnKxnBcu4BHDTY'
MEMO = 'MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr'
LOADER = 'BPFLoaderUpgradeab1e11111111111111111111111'
SYSTEM = '11111111111111111111111111111111'
RENT = 'SysvarRent111111111111111111111111111111111'
BUDGET = 'ComputeBudget111111111111111111111111111111'
GENESIS = '5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d'


def encoded(data):
    return [base64.b64encode(data).decode(), 'base64']


def raw(account):
    return base64.b64decode(account['data'][0], validate=True)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('reference', type=Path)
    parser.add_argument('corpus', type=Path)
    parser.add_argument('deployment_capture', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    svm, pubkey, keypair, *_ = native_imports()
    vm = svm().with_mainnet_features()
    capture = json.loads(args.deployment_capture.read_bytes())['result']
    slot = capture['context']['slot']
    program_ids = [RAYDIUM, RAYDIUM_DATA, TOKEN, TOKEN_DATA, TOKEN22, TOKEN22_DATA, MEMO]
    programs = dict(zip(program_ids, capture['value'], strict=True))
    for account in programs.values():
        if account['data'][1] == 'base64+zstd':
            account['data'] = encoded(subprocess.run(
                ['zstd', '-dc'], input=raw(account), check=True, capture_output=True,
            ).stdout)
        assert account['data'][1] == 'base64'
    for program, programdata in [(RAYDIUM, RAYDIUM_DATA), (TOKEN, TOKEN_DATA), (TOKEN22, TOKEN22_DATA)]:
        executable, data = programs[program], programs[programdata]
        assert executable['executable'] and not data['executable']
        assert executable['owner'] == data['owner'] == LOADER
        assert raw(executable) == bytes([2, 0, 0, 0]) + bytes(pubkey.from_string(programdata))
        assert raw(data)[:4] == bytes([3, 0, 0, 0])
        assert int.from_bytes(raw(data)[4:12], 'little') <= slot
    caller = str(keypair.from_seed(hashlib.sha256(b'XONE CLMM order caller').digest()).pubkey())
    recipient = str(pubkey.from_bytes(hashlib.sha256(b'XONE CLMM order recipient').digest()))
    source = str(pubkey.from_bytes(hashlib.sha256(b'XONE CLMM source token account').digest()))
    destination = str(pubkey.from_bytes(hashlib.sha256(b'XONE CLMM destination token account').digest()))
    shared = programs | {
        key: dict(owner=SYSTEM, executable=False, lamports=100_000_000, data=encoded(b''))
        for key in [caller, recipient]
    }
    for key in [RENT, BUDGET]:
        account = vm.get_account(pubkey.from_string(key))
        shared[key] = dict(owner=str(account.owner), executable=account.executable,
                           lamports=account.lamports, data=encoded(account.data))

    def bind(pool):
        pool['chain_key'] = 'solana:mainnet-beta'
        state = pool['raydium_clmm']
        state.update(bank_slot=slot, deployed_slot=int.from_bytes(raw(programs[RAYDIUM_DATA])[4:12], 'little'),
                     program_code_sha256=sha(raw(programs[RAYDIUM_DATA])[45:]),
                     token_program_code_sha256=sha(raw(programs[TOKEN_DATA])[45:]))
        return state

    def fixture(pool):
        result = subprocess.run([args.reference, 'fixture-from-pool'], input=json.dumps(pool),
                                check=True, capture_output=True, text=True)
        return json.loads(result.stdout)['rpc_accounts']

    def token_account(mint, owner, balance):
        data = bytearray(165)
        data[:32] = bytes(pubkey.from_string(mint))
        data[32:64] = bytes(pubkey.from_string(owner))
        data[64:72] = balance.to_bytes(8, 'little')
        data[108] = 1
        return dict(owner=TOKEN, executable=False,
                    lamports=vm.minimum_balance_for_rent_exemption(len(data)), data=encoded(data))

    corpus = json.loads(args.corpus.read_bytes())
    cases = []
    for case in corpus['cases']:
        if not case['order_only'] or case['amount'] not in ('2100', '13000'):
            continue
        pool, next_pool = case['pool'], case['expected']['next_pool']
        state, after = bind(pool), bind(next_pool)
        # The distant array contains a historical cohort but has no active ticks.
        for view in [state, after]:
            array = view['tick_arrays'][0 if case['left'] else -1]
            start = array['start_tick_index']
            array['ticks'] = [dict(tick=start, liquidity_gross='0', liquidity_net='0',
                                   fee_growth_outside0='0', fee_growth_outside1='0',
                                   limit_orders=dict(order_phase=12, orders_amount=0,
                                                     part_filled_orders_remaining=0, unfilled_ratio_x64='0'))]
            bit = start // (60 * view['tick_spacing']) + 512
            view['bitmap'][bit // 64] &= ~(1 << (bit % 64))
        accounts = {key: value for key, value in fixture(pool).items() if key not in shared}
        input_mint, output_mint = pool['token0'], pool['token1']
        if not case['left']:
            input_mint, output_mint = output_mint, input_mint
        accounts[source] = token_account(input_mint, caller, 1_000_000)
        accounts[destination] = token_account(output_mint, recipient, 10)
        next_accounts = fixture(next_pool)
        order_array = next(a['address'] for a in state['tick_arrays'] if a['start_tick_index'] == 0)
        post_hashes = {
            key: sha(raw(next_accounts[key]))
            for key in [pool['pool_id'], state['vault0'], state['vault1'], order_array]
        }
        cases.append(case | dict(accounts=accounts, post_hashes=post_hashes))
    assert len(cases) == 24
    args.output.write_text(json.dumps(dict(
        caller=caller, recipient=recipient, source=source, destination=destination,
        genesis=GENESIS, blockhash=str(vm.latest_blockhash()),
        shared_accounts=shared, cases=cases,
    ), separators=(',', ':')))


if __name__ == '__main__':
    main()
