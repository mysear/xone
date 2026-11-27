#!/usr/bin/env python3
"""Build controlled Raydium position settlement cases from a deployed SwapV2 bank."""
import argparse
import base64
import copy
import hashlib
import json
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'integration/tools/acceptance'))
from r01_public_catalog_solana_bank import native_imports

TOKEN = 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'
TOKEN22 = 'TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb'
SYSTEM = '11111111111111111111111111111111'
Q64 = 1 << 64


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('order_bank', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    bank = json.loads(args.order_bank.read_bytes())
    svm, pubkey, *_ = native_imports()
    vm = svm().with_mainnet_features()
    sample = bank['cases'][0]
    caller, recipient = bank['caller'], bank['recipient']
    account_key = lambda name: str(pubkey.from_bytes(hashlib.sha256(name.encode()).digest()))
    pub = lambda key: bytes(pubkey.from_string(key))

    def data(account):
        return bytearray(base64.b64decode(account['data'][0], validate=True))

    def replace(account, payload):
        account['data'] = [base64.b64encode(payload).decode(), 'base64']
        for key in ['space', 'data_base64']:
            account.pop(key, None)

    def put(payload, offset, value, size, signed=False):
        payload[offset:offset + size] = value.to_bytes(size, 'little', signed=signed)

    def account(owner, payload):
        value = dict(owner=owner, executable=False, lamports=vm.minimum_balance_for_rent_exemption(len(payload)))
        replace(value, payload)
        return value

    def token(mint, owner, amount, program):
        payload = bytearray(165)
        payload[:32], payload[32:64] = pub(mint), pub(owner)
        put(payload, 64, amount, 8)
        payload[108] = 1
        if program == TOKEN22:
            payload.extend(bytes([2, 2, 0, 8, 0]) + (19).to_bytes(8, 'little'))
        return account(program, payload)

    pool = sample['pool']['pool_id']
    program = sample['pool']['raydium_clmm']['program_id']

    def pda(*seeds):
        key, bump = pubkey.find_program_address(list(seeds), pubkey.from_string(program))
        return str(key), bump

    nft_mint = account_key('Raydium test position mint')
    nft = account_key('Raydium test position NFT')
    position, bump = pda(b'position', pub(nft_mint))
    fee_accounts = [account_key(f'Raydium fee destination {i}') for i in range(2)]
    reward_accounts = [account_key(f'Raydium reward destination {i}') for i in range(2)]
    cases = []
    for status, classic_balance in [(0, 500), (0, 0), (2, 500), (4, 500), (8, 500)]:
        model = copy.deepcopy(sample['pool'])
        accounts = copy.deepcopy(sample['accounts'])
        state = model['raydium_clmm']
        model.update(protocol='raydium-clmm-rewards', adapter_version='raydium-clmm/6d7a0f82-spl-rewards-1')
        state.update(liquidity='100', fee_growth_global0=str(4 * Q64), fee_growth_global1=str(2 * Q64))
        state['bitmap'][8] &= ~1
        pool_data = data(accounts[pool])
        pool_data[389] = status
        for off, val in [(237, 100), (277, 4 * Q64), (293, 2 * Q64)]:
            put(pool_data, off, val, 16)
        put(pool_data, 968, state['bitmap'][8], 8)
        for array in state['tick_arrays']:
            array_account = accounts[array['address']]
            array_data = data(array_account)
            for tick in array['ticks']:
                off = 44 + (tick['tick'] - array['start_tick_index']) * 168
                array_data[off + 36:off + 156] = bytes(120)
                tick.pop('limit_orders', None)
                tick.update(fee_growth_outside0='0', fee_growth_outside1='0', reward_growths_outside=['0'] * 3)
            array['ticks'] = [tick for tick in array['ticks'] if int(tick['liquidity_gross'])]
            array_data[10124] = len(array['ticks'])
            replace(array_account, array_data)
        state['reward_infos'] = []
        now = state['bank_unix_timestamp']
        for i, asset in enumerate([TOKEN, TOKEN22]):
            mint = account_key(f'Raydium reward mint {i}')
            vault, _ = pda(b'pool_reward_vault', pub(pool), pub(mint))
            authority = account_key(f'Raydium reward authority {i}')
            start = 397 + i * 169
            pool_data[start] = 2 if i == 0 else 3
            time = (now - 100, now + 100, now - 50) if i == 0 else (now - 100, now - 10, now - 10)
            rate = 20 * Q64
            for offset, value in [(1, time[0]), (9, time[1]), (17, time[2]), (41, 2500), (49, 100)]:
                put(pool_data, start + offset, value, 8)
            put(pool_data, start + 25, rate, 16)
            for off, key in [(57, mint), (89, vault), (121, authority)]:
                pool_data[start + off:start + off + 32] = pub(key)
            state['reward_infos'].append(dict(
                reward_state=pool_data[start], open_time=time[0], end_time=time[1], last_update_time=time[2],
                emissions_per_second_x64=str(rate), reward_total_emitted=2500, reward_claimed=100,
                token_mint=mint, token_vault=vault, authority=authority, reward_growth_global_x64='0',
            ))
            mint_data = bytearray(278 if asset == TOKEN22 else 82)
            put(mint_data, 36, 1_000_000, 8)
            mint_data[44:46] = bytes([6, 1])
            if asset == TOKEN22:
                mint_data[165:170] = bytes([1, 1, 0, 108, 0])
                for offset, epoch, bps in [(242, 0, 100), (260, state['bank_epoch'], 250)]:
                    put(mint_data, offset, epoch, 8)
                    put(mint_data, offset + 8, 20, 8)
                    put(mint_data, offset + 16, bps, 2)
            accounts[mint] = account(asset, mint_data)
            accounts[vault] = token(mint, pool, classic_balance if i == 0 else 500, asset)
            accounts[reward_accounts[i]] = token(mint, recipient, 19, asset)
        pool_data[397 + 2 * 169:904] = bytes(169)
        state['reward_infos'].append(dict(
            reward_state=0, open_time=0, end_time=0, last_update_time=0, emissions_per_second_x64='0',
            reward_total_emitted=0, reward_claimed=0, token_mint=SYSTEM, token_vault=SYSTEM,
            authority=SYSTEM, reward_growth_global_x64='0',
        ))
        replace(accounts[pool], pool_data)
        position_data = bytearray(281)
        position_data[:8] = hashlib.sha256(b'account:PersonalPositionState').digest()[:8]
        position_data[8] = bump
        position_data[9:41], position_data[41:73] = pub(nft_mint), pub(pool)
        put(position_data, 73, -20, 4, signed=True)
        put(position_data, 77, 60, 4, signed=True)
        put(position_data, 81, 100, 16)
        put(position_data, 129, 3, 8)
        put(position_data, 137, 5, 8)
        put(position_data, 161, 200, 8)
        put(position_data, 185, 300, 8)
        put(position_data, 217, state['recent_epoch'], 8)
        accounts[position] = account(program, position_data)
        accounts[nft] = token(nft_mint, caller, 1, TOKEN)
        for i, key in enumerate(fee_accounts):
            accounts[key] = token(model[f'token{i}'], recipient, 11, TOKEN)
        fees = ([0, 0] if status & 4 else [3, 5] if status & 2 else [403, 205])
        rewards = [0, 0] if status & 8 else [min(classic_balance, 200 if status & 2 else 1200), 300]
        cases.append(dict(pool=model, accounts=accounts, claim=dict(
            authority=caller, position=position, positionTokenAccount=nft, recipient=recipient,
            feeAccounts=fee_accounts, rewardAccounts=reward_accounts,
        ), expected=dict(fees=fees, rewards=rewards, fee_on_reward1=8 if rewards[1] else 0)))
    bank['cases'] = cases
    args.output.write_text(json.dumps(bank, separators=(',', ':')))


if __name__ == '__main__':
    main()
