#!/usr/bin/env python3
"""Adversarial guards for the actual official-bytecode execution fixture."""
import copy
import json
import hashlib
from pathlib import Path
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[3]
CHECK = ROOT / 'integration/tools/ci/evm-execution-check.py'
FIXTURE = ROOT / 'integration/test/contracts/evm-execution.json'


def main():
    assert CHECK.is_file(), 'missing fail-closed checker'
    original = json.loads(FIXTURE.read_text())
    canonical = lambda x: json.dumps(x, indent=2, ensure_ascii=False) + '\n'
    assert canonical(original) == FIXTURE.read_text(), 'guard baseline is not canonical'
    assert len(original['cases']) == 10, 'ten actual native cases required'
    assert original['cases'][-1]['name'] == 'input-donation-before-swap', 'donation must be last'
    donated = original['cases'][-1]
    donation = donated['evidence']['input_donation']
    pair_token = next(a for a in donated['accounts'] if a['address'] == donated['token_in'])
    raw_pair_balance = int(next(s['value'] for s in pair_token['storage'] if s['slot'] == donation['balance_slot']), 16)
    assert raw_pair_balance == int(donated['pool']['reserve0']) + 50 * 10**18
    assert donated['token_in'] == donated['pool']['token0']
    assert donation['reserves_before_transfer']['reserve0'] == donated['pool']['reserve0']
    assert donation['reserves_before_transfer']['reserve1'] == donated['pool']['reserve1']
    assert int(donation['swap_event']['amount0_in']) == int(donated['amount_in']) + 50 * 10**18
    assert donated['expected']['amount_out'] == donated['expected']['router_amount_out']
    for c in original['cases']:
        assert set(c['transaction_context']) == {'nonce', 'gas_limit', 'gas_price', 'chain_id'}, 'strict Rust TransactionContext fields'
        assert type(c['transaction_context']['gas_limit']) is int, 'transaction gas_limit must be a schema-1 integer'
        assert c['transaction_context']['gas_limit'] == 7000000
        post = c['expected'].get('post_accounts')
        assert isinstance(post, list), f"actual post accounts required: {c['name']}"
        assert [a['address'] for a in post] == [a['address'] for a in c['accounts']]
        for before, after in zip(c['accounts'], post):
            assert set(after) == {'address', 'exists', 'balance', 'nonce', 'code', 'storage'}
            assert [s['slot'] for s in after['storage']] == [s['slot'] for s in before['storage']]
            assert after['exists'] is True
        pool_after = c['expected'].get('pool_after')
        assert pool_after is not None and set(pool_after) == {'reserve0', 'reserve1', 'block_timestamp_last'}
        q = c.get('quote_context')
        assert q is not None, f"missing native quote context: {c['name']}"
        assert set(q) == {'slippage_bps', 'deadline_ms', 'quoted_buy_amount'}
        assert q['slippage_bps'] == 50
        words = c['transaction']['data'][10:]
        encoded_minimum = int(words[64:128], 16)
        encoded_deadline = int(words[256:320], 16)
        assert q['deadline_ms'] == encoded_deadline * 1000
        minimum = int(q['quoted_buy_amount']) * 9950 // 10000
        if c['name'] == 'min-out-revert':
            assert encoded_minimum > int(q['quoted_buy_amount'])
        else:
            assert encoded_minimum == minimum, f"unbound native minimum: {c['name']}"
        if c['name'] == 'fee-token-output-counterexample':
            assert c['expected']['receiptstatus'] == 1
            assert int(c['expected']['amount_out']) < minimum <= int(c['expected']['router_amount_out'])
    print('10 native quote/slippage/deadline bindings plus unsynced donation proof passed', flush=True)
    def rejected(label, edit):
        candidate = copy.deepcopy(original)
        edit(candidate)
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / 'mutated.json'
            p.write_text(canonical(candidate))
            r = subprocess.run(['python3', str(CHECK), '--input', str(p)], capture_output=True, text=True)
            assert r.returncode != 0, f'accepted mutation: {label}'
        print('rejected:', label, flush=True)
    rejected('amount', lambda x: x['cases'][0].update(amount_in='999'))
    rejected('quote context omission', lambda x: x['cases'][0].pop('quote_context'))
    rejected('quoted native view output', lambda x: x['cases'][0]['quote_context'].update(quoted_buy_amount='1'))
    rejected('slippage context', lambda x: x['cases'][0]['quote_context'].update(slippage_bps=51))
    rejected('deadline context', lambda x: x['cases'][0]['quote_context'].update(deadline_ms=1))
    rejected('recipient delta', lambda x: x['cases'][0]['expected'].update(amount_out='1'))
    rejected('recipient', lambda x: x['cases'][0].update(recipient=x['cases'][0]['transaction']['from']))
    rejected('status', lambda x: x['cases'][0]['expected'].update(status='revert'))
    rejected('gas', lambda x: x['cases'][0]['expected'].update(gas_used='21000'))
    rejected('receipt', lambda x: x['cases'][0]['expected'].update(receiptstatus=0))
    rejected('prebalance', lambda x: x['cases'][0]['expected'].update(caller_sell_before='0'))
    rejected('storage omission', lambda x: next(a for a in x['cases'][0]['accounts'] if a['storage'])['storage'].pop())
    rejected('account omission', lambda x: x['cases'][0]['accounts'].pop())
    rejected('source omission', lambda x: x['source_pins']['packages'].pop())
    rejected('dependency omission', lambda x: x['reference']['loaded_dependencies'].pop())
    rejected('node identity', lambda x: x['reference']['node'].update(sha256='0'*64))
    rejected('creation receipt omission', lambda x: x['reference']['setup_transactions'].pop())
    rejected('case omission', lambda x: x['cases'].pop())
    rejected('duplicate case', lambda x: x['cases'].append(x['cases'][0]))
    rejected('anchor hash', lambda x: x['cases'][0]['state_anchor'].update(block_hash='0x'+'01'*32))
    rejected('execution timestamp', lambda x: x['cases'][0]['execution_environment'].update(timestamp=1))
    rejected('executor identity', lambda x: x['cases'][0]['executor'].update(code_sha256='0'*64))
    rejected('blockhash omission', lambda x: x['cases'][0]['block_hashes'].pop())
    rejected('transaction nonce', lambda x: x['cases'][0]['transaction_context'].update(nonce=0))
    rejected('string transaction gas limit', lambda x: x['cases'][0]['transaction_context'].update(gas_limit='7000000'))
    rejected('unknown transaction context type', lambda x: x['cases'][0]['transaction_context'].update(type=0))
    rejected('runtime code', lambda x: next(a for a in x['cases'][0]['accounts'] if a['code']!='0x').update(code='0x'))
    rejected('explicit zero omission', lambda x: next(a['storage'].remove(s) for a in x['cases'][0]['accounts'] for s in a['storage'] if int(s['value'],16)==0))
    rejected('source digest', lambda x: x['source_pins']['packages'][0]['files'][0].update(sha256='0'*64))
    rejected('script omission', lambda x: x['reference']['scripts'].pop())
    rejected('caller post balance', lambda x: x['cases'][0]['expected'].update(caller_sell_after='0'))
    rejected('recipient post balance', lambda x: x['cases'][0]['expected'].update(recipient_buy_after='0'))
    rejected('receipt gas', lambda x: x['cases'][0]['evidence']['receipt'].update(gasUsed='0x5208'))
    rejected('source file omission', lambda x: x['source_pins']['packages'][0]['files'].pop())
    rejected('empty package file pins', lambda x: x['source_pins']['packages'][0].update(files=[]))
    def reseal_expected(x):
        c = x['cases'][0]
        e = c['expected']
        for key in ['amount_out', 'recipient_buy_after', 'router_amount_out']:
            e[key] = str(int(e[key]) + 1)
        # A coherent but hostile output plus a recomputed evidence digest is still untrusted.
        c['evidence']['trace_sha256'] = hashlib.sha256(canonical(e).encode()).hexdigest()
    rejected('coherent hostile output with recomputed evidence digest', reseal_expected)
    rejected('donation case omission', lambda x: x['cases'].pop())
    rejected('donation proof omission', lambda x: x['cases'][-1]['evidence'].pop('input_donation'))
    rejected('post account image omission', lambda x: x['cases'][0]['expected'].pop('post_accounts'))
    rejected('post account omission', lambda x: x['cases'][0]['expected']['post_accounts'].pop())
    rejected('post storage omission', lambda x: next(a for a in x['cases'][0]['expected']['post_accounts'] if a['storage'])['storage'].pop())
    rejected('post storage value', lambda x: next(a for a in x['cases'][0]['expected']['post_accounts'] if a['storage'])['storage'][0].update(value='0x'+'01'*32))
    rejected('post account native balance', lambda x: x['cases'][0]['expected']['post_accounts'][0].update(balance='1'))
    rejected('post account nonce', lambda x: x['cases'][0]['expected']['post_accounts'][0].update(nonce=999))
    rejected('post contract code', lambda x: next(a for a in x['cases'][0]['expected']['post_accounts'] if a['code']!='0x').update(code='0x'))
    rejected('post pool getter omission', lambda x: x['cases'][0]['expected'].pop('pool_after'))
    rejected('post pool reserve', lambda x: x['cases'][0]['expected']['pool_after'].update(reserve0='1'))
    def reseal_post_state(x):
        c = x['cases'][0]
        # Mutate an ordinary existing account without any token/receipt arithmetic dependency.
        actor = next(a for a in c['expected']['post_accounts'] if a['code'] == '0x' and a['address'] not in {c['transaction']['from'], c['execution_environment']['coinbase']})
        actor['balance'] = str(int(actor['balance']) + 1)
        c['evidence']['trace_sha256'] = hashlib.sha256(canonical(c['expected']['post_accounts']).encode()).hexdigest()
    rejected('hostile post state with recomputed evidence digest', reseal_post_state)
    def donation_removed_and_resealed(x):
        c = x['cases'][-1]
        d = c['evidence']['input_donation']
        token = next(a for a in c['accounts'] if a['address'] == c['token_in'])
        slot = next(s for s in token['storage'] if s['slot'] == d['balance_slot'])
        slot['value'] = '0x' + format(int(c['pool']['reserve0']), '064x')
        d['pair_input_balance_at_anchor'] = c['pool']['reserve0']
        c['evidence']['trace_sha256'] = hashlib.sha256(canonical(c['accounts']).encode()).hexdigest()
    rejected('donation removed with resealed state evidence', donation_removed_and_resealed)
    def reserves_forged_and_resealed(x):
        c = x['cases'][-1]
        d = c['evidence']['input_donation']
        forged = int(d['pair_input_balance_at_anchor'])
        c['pool']['reserve0'] = str(forged)
        c['evidence']['pool_getters']['reserve0'] = str(forged)
        d['reserves_before_transfer']['reserve0'] = str(forged)
        pair = next(a for a in c['accounts'] if a['address'] == c['pool']['pool_id'])
        packed = next(s for s in pair['storage'] if int(s['slot'], 16) == 8)
        value = int(packed['value'], 16)
        packed['value'] = '0x' + format((value >> 112 << 112) | forged, '064x')
        c['evidence']['trace_sha256'] = hashlib.sha256(canonical(c['accounts']).encode()).hexdigest()
    rejected('raw balance substituted for stored reserves with resealed evidence', reserves_forged_and_resealed)
    # A copied entrypoint with its generator absent must invalidate BOTH old copies first.
    reference = ROOT / 'integration/tools/reference/evm-execution'
    with tempfile.TemporaryDirectory(dir=reference) as d:
        scratch = Path(d)
        entry = scratch / 'integration/tools/ci/evm-execution-golden.cjs'
        entry.parent.mkdir(parents=True)
        entry.write_bytes((ROOT / 'integration/tools/ci/evm-execution-golden.cjs').read_bytes())
        copies = [scratch / 'integration/test/contracts/evm-execution.json', scratch / 'dex-router/test/contracts/evm-execution.json']
        for output in copies:
            output.parent.mkdir(parents=True, exist_ok=True)
            output.write_text('stale success')
        result = subprocess.run(['node', str(entry)], capture_output=True, text=True)
        assert result.returncode != 0 and all(not output.exists() for output in copies), 'failed generator preserved old success'
    print('50 adversarial fixture guards + stale-success invalidation passed')

if __name__ == '__main__':
    main()
