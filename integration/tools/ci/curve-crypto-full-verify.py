#!/usr/bin/env python3
"""Fail-closed local reference validation; all checks also run under python -O."""
import hashlib
import json
import os
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
REF = ROOT / 'integration/tools/reference/curve-crypto-full'
RECEIPT = REF / 'verification.json'


def atomic(path, obj):
    tmp = path.with_name(path.name + '.tmp-' + str(os.getpid()))
    tmp.write_text(json.dumps(obj, indent=2) + '\n')
    os.replace(tmp, path)


def require(condition, message):
    if not condition:
        raise ValueError(message)


def digest(data):
    return hashlib.sha256(data).hexdigest()


def main():
    # Invalidate before reading pins, generation receipts, metadata or corpus.
    atomic(RECEIPT, {'valid': False, 'status': 'invalidated-before-parsing'})
    pins = json.loads((REF / 'sources.json').read_text())
    for pin in pins:
        require(digest((REF / pin['local']).read_bytes()) == pin['sha256'], 'source pin mismatch: ' + pin['local'])
    generated = json.loads((REF / 'generation.json').read_text())
    require(generated.get('valid') is True, 'generation receipt invalid')
    corpus_bytes = (ROOT / 'integration/test/contracts/curve-crypto-full.json').read_bytes()
    require(corpus_bytes == (ROOT / 'dex-router/test/contracts/curve-crypto-full.json').read_bytes(), 'corpus copies differ')
    require(digest(corpus_bytes) == generated['corpus_sha256'], 'corpus receipt mismatch')
    require(digest((REF / 'runtime-metadata.json').read_bytes()) == generated['runtime_metadata_sha256'], 'metadata receipt mismatch')
    require(digest((ROOT / 'integration/tools/ci/curve-crypto-full-golden.cjs').read_bytes()) == generated['generator_sha256'], 'generator receipt mismatch')
    meta = json.loads((REF / 'runtime-metadata.json').read_text())
    require(meta['allow_unlimited_contract_size'] is False, 'EIP170 disabled')
    require(meta['pool_template_runtime_bytes'] == 20814, 'pool template length')
    require(meta['pool_template_runtime_sha256'] == 'c19b756213feac7b03ee10b12888c3abe427bd10f9ec125004091b56dd995a80', 'pool template hash')
    require(meta['lp_template_runtime_sha256'] == 'd3f43d24527d6a8ed0fa9f24d110ad7cc9325f40da335ed327e8e0e002a8694d', 'original V4 LP runtime hash')
    for prefix in ['pool', 'lp']:
        runtime = bytes.fromhex((REF / (prefix + '-bytecode-runtime.hex')).read_text().strip().removeprefix('0x'))
        require(digest(runtime) == meta[prefix + '_template_runtime_sha256'], prefix + ' runtime artifact digest')
    require(meta['pool_compiler'].startswith('0.3.3+'), 'pool compiler')
    require(meta['lp_compiler'] == '0.3.1', 'LP compiler')
    require(meta['lp_layout']['minter']['slot'] == 8, 'LP minter layout')
    require(meta['lp_layout']['totalSupply']['slot'] == 7, 'LP supply layout')
    require(len(meta['deployments']) == 2, 'deployment coverage')
    for dep in meta['deployments']:
        require(dep['pool_runtime_bytes'] == 20974 <= 24576, 'deployed EIP170 size')
        require(dep['lp_runtime_sha256'] == meta['lp_template_runtime_sha256'], 'deployed original LP hash')
        require(dep['lp_minter'].lower() == dep['pool'].lower(), 'LP minter != pool')
        tail = dep['immutable_tail_hex']
        require(len(tail) == 320, 'immutable tail length')
        template = bytes.fromhex((REF / 'pool-bytecode-runtime.hex').read_text().strip().removeprefix('0x'))
        require(digest(template + bytes.fromhex(tail)) == dep['pool_runtime_sha256'], 'reconstructed deployed runtime digest')
        words = [int(tail[i:i + 64], 16) for i in range(0, 320, 64)]
        require(words[:3] == [int(v, 16) for v in [dep['lp']] + dep['coins']], 'immutable addresses')
        require(words[3:] == [10 ** (18 - d) for d in dep['decimals']], 'immutable precisions')
        require([x['offset'] for x in dep['immutable_layout']] == [20814, 20846, 20878, 20910, 20942], 'immutable offsets')
    corpus = json.loads(corpus_bytes)
    require(corpus['source_pins'] == pins, 'corpus source pins')
    cases = corpus['cases']
    require(len(cases) == generated['cases'] >= 60, 'case count')
    counts = {'success': 0, 'reverts': 0, 'adjusted': 0, 'minted': 0, 'direct_claim': 0}
    for case in cases:
        expected = case['expected']
        tx = expected['transaction']
        require(len(tx['hash']) == 66 and tx['hash'].startswith('0x') and int(tx['gas_used']) > 0, 'missing mined transaction evidence')
        require(tx['status'] == (0 if expected.get('reverted') else 1), 'reference transaction status')
        if expected.get('reverted'):
            counts['reverts'] += 1
            before = {k: case[k] for k in ['state', 'balances', 'total_supply', 'receiver_balance', 'raw_balances', 'trader_balances']}
            require(expected['after'] == before, 'rollback mismatch: ' + case['name'])
            continue
        counts['success'] += 1
        minted = int(expected['minted_admin'])
        require(int(expected['total_supply']) - int(case['total_supply']) == minted, 'supply delta')
        require(int(expected['receiver_balance']) - int(case['receiver_balance']) == minted, 'LP receiver delta')
        claims = [e for e in expected['pool_events'] if e['name'] == 'ClaimAdminFee']
        if claims:
            require(len(claims) == 1 and int(claims[0]['args'][1]) == minted, 'admin mint event')
        if minted:
            counts['minted'] += 1
            require(len(claims) == 1, 'missing claim event')
            lp_events = expected['lp_transfer_events']
            require(len(lp_events) == 1 and int(lp_events[0][2]) == minted, 'original LP Transfer event')
        if case['operation'] == 'claim_admin_fees':
            counts['direct_claim'] += 1
            require(expected['raw_balances'] == case['raw_balances'], 'claim transferred tokens')
            require(expected['balances'] == case['raw_balances'], 'claim did not gulp')
        else:
            i, j = case['index_in'], case['index_out']
            dx, dy = int(case['input']), int(expected['amount_out'])
            raw = [int(x) for x in case['raw_balances']]
            raw[i] += dx
            raw[j] -= dy
            require(raw == [int(x) for x in expected['raw_balances']], 'pool exact ERC20 transfer bookkeeping')
            trader = [int(x) for x in case['trader_balances']]
            trader[i] -= dx
            trader[j] += dy
            require(trader == [int(x) for x in expected['trader_balances']], 'trader exact ERC20 transfer bookkeeping')
        if expected['state']['price_scale'] != case['state']['price_scale']:
            counts['adjusted'] += 1
        if 'adjust-rejected-claim-' in case['name']:
            require(expected['state']['price_scale'] == case['state']['price_scale'] and minted > 0, 'failed adjustment/claim coverage')
    require(counts['reverts'] == generated['reverts'] >= 12, 'revert coverage')
    require(counts['adjusted'] >= 4 and counts['minted'] >= 6 and counts['direct_claim'] >= 8, 'full branch coverage')
    atomic(RECEIPT, {'valid': True, 'reference_only': True, 'cases': len(cases), 'counts': counts, 'corpus_sha256': digest(corpus_bytes), 'verifier_sha256': digest(Path(__file__).read_bytes())})
    print(json.dumps({'cases': len(cases), **counts}))


if __name__ == '__main__':
    main()
