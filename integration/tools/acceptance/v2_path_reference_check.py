#!/usr/bin/env python3
"""Validate the retained actual-mined path bank inventory, without quote math."""
import hashlib
import json
from pathlib import Path

ROOT=Path(__file__).resolve().parents[3]
FIXTURE=ROOT/'dex-router/test/contracts/v2-path-execution.json'


def check(document, root=ROOT):
    if document.get('schema_version') != 1 or document.get('publicTransactionsSubmitted') is not False:
        raise ValueError('REFERENCE_SCHEMA_OR_AUTHORITY')
    if document.get('production_ready') is not False:
        raise ValueError('REFERENCE_PRODUCTION_UPGRADE')
    inputs=document['inputs']
    for name,expected in inputs.items():
        relative=Path(name)
        if relative.is_absolute() or '..' in relative.parts:
            raise ValueError('REFERENCE_INPUT_PATH')
        path=root/relative
        if hashlib.sha256(path.read_bytes()).hexdigest()!=expected:
            raise ValueError('REFERENCE_INPUT_DIGEST:'+name)
    cases=document['cases']
    expected_ids={f'{mode}-{hops}-{str(reverse).lower()}' for mode in ['EXACT_IN','EXACT_OUT']
                  for hops in [1,2,3] for reverse in [False,True]}
    if len(cases)!=12 or {c['id'] for c in cases}!=expected_ids:
        raise ValueError('REFERENCE_CASE_INVENTORY')
    for case in cases:
        fixture=case['fixture'];pre=fixture['state_anchor'];post=fixture['execution_environment']
        if post['block_number']!=pre['block_number']+1 or post['parent_hash']!=pre['block_hash']:
            raise ValueError('REFERENCE_BANK_TRANSITION')
        if case['expected']['status']!=1 or case['expected']['actual_input']!=case['amount_in'] or case['expected']['actual_output']!=case['amount_out']:
            raise ValueError('REFERENCE_RECEIPT_DELTA')
        before={a['address']:a for a in fixture['accounts']}
        after={a['address']:a for a in case['expected']['post_accounts']}
        if len(before)!=len(fixture['accounts']) or set(before)!=set(after):
            raise ValueError('REFERENCE_ACCOUNT_BOUNDARY')
        for address,account in before.items():
            left=[s['slot'] for s in account['storage']]
            right=[s['slot'] for s in after[address]['storage']]
            if len(left)!=len(set(left)) or set(left)!=set(right):
                raise ValueError('REFERENCE_STORAGE_BOUNDARY')
        for identity,code_key in [('target','code_sha256'),('factory','factory_code_sha256')]:
            code=before[case['executor'][identity]]['code']
            if hashlib.sha256(bytes.fromhex(code[2:])).hexdigest()!=case['executor'][code_key]:
                raise ValueError('REFERENCE_EXECUTOR_CODE')
    return inputs


if __name__=='__main__':
    inputs=check(json.loads(FIXTURE.read_bytes()))
    print(json.dumps(dict(passed=True,mined_transactions=12,exact_out_paths=6,reference_inputs=len(inputs))))
