#!/usr/bin/env python3
"""Verify phase53 original bytes, full projections, transfers and hard budgets."""
import json
import sys
from solana_expanded_reference import ROOT, OUT, module, require, sha, tokens, delta_token

ORIGINAL = OUT / 'core-meteora-orders-exact-out-original-20261111.json'
FIXTURE = ROOT / 'integration/test/contracts/meteora-orders-exact-out.json'
PROGRAM = '942f609d03440911f15c40e8df1733d808670768fd9042af1de956964fb2f0e6'

def verify(reexecute=False, evidence=None, fixture=None):
    e = json.loads(ORIGINAL.read_bytes()) if evidence is None else evidence
    f = json.loads(FIXTURE.read_bytes()) if fixture is None else fixture
    gen = module('orders_xo_verify', 'integration/tools/ci/meteora-orders-exact-out-reference.py')
    require(e['generator_sha256'] == sha(gen.__file__) and
            e['native_sha256'] == sha(gen.base.native.native.__file__) and
            f['source_sha256'] == sha(ORIGINAL), 'orders-xo-source-binding')
    require(len(e['records']) == len(f['cases']) == len(f['decoder']['cases']) == 192,
            'orders-xo-coverage')
    full = rejected = controls = 0
    for index, (entry, case, decoded) in enumerate(zip(e['records'], f['cases'], f['decoder']['cases'])):
        n = entry['native']
        require(index == entry['index'] == case['index'] and case['exact_out'] is True and
                case['zero_for_one'] == entry['zero'] and n['program_sha256'] == PROGRAM,
                'orders-xo-identity')
        raw_steps = n['sequence'] if n['passed'] else [n]
        require(len(raw_steps) == len(case['steps']), 'orders-xo-sequence-length')
        require(decoded['pool'] == case['steps'][0]['pool'], 'orders-xo-decoder-projection')
        previous = None
        for raw, step in zip(raw_steps, case['steps']):
            require(raw['passed'] == step['passed'] == ('err' not in raw['native']), 'orders-xo-result')
            if previous is not None:
                require(previous == raw['before'], 'orders-xo-sequential-bank')
            require(gen.base.pool(raw['before'], n['clock'], PROGRAM) == step['pool'] and
                    gen.base.pool(raw['after'], n['clock'], PROGRAM) == step['after'],
                    'orders-xo-full-projection')
            require(step['amount'] == raw['specified_amount'] and
                    step['actual_input'] == raw['payer_input_delta'] and
                    step['actual_output'] == raw['recipient_output_delta'], 'orders-xo-reported-amounts')
            if not raw['passed']:
                require(raw['before'] == raw['after'], 'orders-xo-failure-rollback')
                rejected += 1
            else:
                p = step['pool']; zero = entry['zero']
                paid = -delta_token(raw, p['token0' if zero else 'token1'], n['payer'])
                received = delta_token(raw, p['token1' if zero else 'token0'], n['recipient'])
                require(str(paid) == step['actual_input'] and str(received) == step['amount'] == step['actual_output'],
                        'orders-xo-actual-transfers')
                tokens(raw)
                full += 1
            previous = raw['after']
        args = {k: entry[k] for k in ['zero', 'amount', 'mm', 'processed', 'open_amount']}
        control = entry['input_minus_one']
        if n['passed']:
            require(control is not None and not control['passed'] and 'err' in control['native'] and
                    control['before'] == control['after'] == n['before'] and
                    control['payer_input_delta'] == control['recipient_output_delta'] == '0',
                    'orders-xo-input-minus-one')
            controls += 1
        else:
            require(control is None, 'orders-xo-no-fabricated-control')
        if reexecute:
            fresh = gen.execute(**args, followups=(100, 1000) if entry['amount'] == 1000 else ())
            require(fresh['before'] == n['before'] and fresh['after'] == n['after'] and
                    fresh['passed'] == n['passed'] and fresh['sequence'] == n['sequence'],
                    'orders-xo-fresh-original')
            if control:
                fresh = gen.execute(**args, maximum=int(n['payer_input_delta']) - 1)
                require(fresh['before'] == control['before'] and fresh['after'] == control['after'] and
                        fresh['passed'] == control['passed'], 'orders-xo-fresh-budget-control')
    require((full, rejected, controls) == (193, 31, 161), 'orders-xo-result-counts')
    return dict(passed=True, full_steps=full, rejected_steps=rejected, original_budget_controls=controls,
                reexecuted=reexecute, all_requested_features_complete=False, production_ready=False)

if __name__ == '__main__':
    print(json.dumps(verify('--reexecute' in sys.argv)))
