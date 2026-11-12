#!/usr/bin/env python3
"""Pinned original swap_exact_out2 order-layer corpus, separate from phase52."""
from pathlib import Path
import hashlib
import importlib.util
import json

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / 'integration/artifacts/local/core-meteora-orders-exact-out-original-20261111.json'

def module(name, path):
    spec = importlib.util.spec_from_file_location(name, ROOT / path)
    result = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(result)
    return result

base = module('orders_exact_out_base', 'integration/tools/ci/meteora-orders-reference.py')

def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()

def execute(zero, amount, mm, processed, open_amount, maximum=10**15, followups=()):
    seed = dict(amount_x=mm, amount_y=mm, liquidity_supply=mm << 64,
                open_order_amount=open_amount, total_processing_order_amount=processed,
                processed_order_remaining_amount=processed, order_age=1,
                limit_order_ask_side=0 if zero else 1)
    return base.native.native.execute(zero=zero, amount=amount, minimum=maximum,
        exact_out=True, mm_only=True, order_seed=seed, protocol_share=2000,
        elapsed=300, dynamic_control=7500, followups=followups)

def main():
    records, cases, decoder = [], [], []
    old = json.loads((ROOT / 'integration/test/contracts/meteora-orders.json').read_bytes())
    cfg = old['decoder']['config']
    for zero in [True, False]:
        for mm in [0, 1000]:
            for processed in [0, 2000]:
                for open_amount in [0, 3000]:
                    for amount in [1, 100, 999, 1000, 1001, 2999, 3000, 3001, 5999, 6000, 6001, 10**15]:
                        args = dict(zero=zero, amount=amount, mm=mm,
                                    processed=processed, open_amount=open_amount)
                        n = execute(**args, followups=(100, 1000) if amount == 1000 else ())
                        control = execute(**args, maximum=int(n['payer_input_delta']) - 1) if n['passed'] else None
                        index = len(cases)
                        records.append(dict(index=index, **args, native=n, input_minus_one=control))
                        steps = [dict(passed=s['passed'], amount=s['specified_amount'],
                            actual_input=s['payer_input_delta'], actual_output=s['recipient_output_delta'],
                            pool=base.pool(s['before'], n['clock'], n['program_sha256']),
                            after=base.pool(s['after'], n['clock'], n['program_sha256']))
                            for s in n['sequence']] if n['passed'] else [dict(
                            passed=False, amount=n['specified_amount'], actual_input=n['payer_input_delta'],
                            actual_output=n['recipient_output_delta'],
                            pool=base.pool(n['before'], n['clock'], n['program_sha256']),
                            after=base.pool(n['after'], n['clock'], n['program_sha256']))]
                        cases.append(dict(index=index, zero_for_one=zero, exact_out=True, steps=steps))
                        accounts = {k: dict(owner=a['owner'], executable=a['executable'],
                            lamports=a['lamports'], data=[a['data_base64'], 'base64'])
                            for k, a in n['before'].items() if k in set(cfg['accounts'])}
                        clock = 'SysvarC1ock11111111111111111111111111111111'
                        accounts[clock] = old['decoder']['cases'][0]['accounts'][clock]
                        decoder.append(dict(accounts=accounts, pool=steps[0]['pool']))
    OUT.write_text(json.dumps(dict(schema_version=1, passed=True,
        generator_sha256=sha(__file__), native_sha256=sha(base.native.native.__file__),
        records=records, authority='controlled pinned original SBF; independent order ExactOut; no placement/cancellation claim'),
        separators=(',', ':')) + '\n')
    fixture = dict(schema_version=1, source_sha256=sha(OUT), cases=cases,
        decoder=dict(config=cfg, shared_accounts=old['decoder']['shared_accounts'], cases=decoder))
    for service in ['integration', 'dex-router', 'chain-parser-rust']:
        (ROOT / service / 'test/contracts/meteora-orders-exact-out.json').write_text(json.dumps(fixture, separators=(',', ':')) + '\n')
    print(json.dumps(dict(passed=True, cases=len(cases), success=sum(r['native']['passed'] for r in records),
                         steps=sum(len(c['steps']) for c in cases))))

if __name__ == '__main__':
    main()
