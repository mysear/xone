#!/usr/bin/env python3
"""Original op-geth expectations for versioned normal upgrade-source users."""
import json
from pathlib import Path
import subprocess
from op_l1_cost_vectors import ROOT, digest, require


def write(output, source_input, packed_reference):
    output, source_input = Path(output).resolve(), Path(source_input).resolve()
    output.parent.mkdir(parents=True, exist_ok=False)
    source = json.loads(source_input.read_bytes())
    require(source.get('passed') is True and len(source['blocks'])==8 and source.get('projection_mutations_rejected')==60, 'Normal upgrade source required')
    packed = json.loads(Path(packed_reference).read_bytes())
    runner = Path(packed_reference).resolve().parent/'original-packed-reference.go'
    relative_runner = str(runner.relative_to(ROOT))
    require(packed['references'].get(relative_runner)==digest(runner), 'Original packed runner changed')
    rows=[]
    names=[]
    for block in source['blocks']:
        names.append(block['name'])
        p = block['inputs']
        require(p['authority']=='RPC_HEADER_BODY_RECEIPTS_COMMITMENT_NOT_CONSENSUS', 'Source authority changed')
        if p['upgrade_block']:
            require(not p['user_transactions'], 'Upgrade user fee inferred without state')
            continue
        for user in p['user_transactions']:
            receipt=block['observed_rpc_receipts'][user['index']]
            require(receipt['transactionHash']==user['hash'], 'Observed transaction differs')
            row=dict(id=block['name']+'-'+str(user['index']), block_name=block['name'],
                     encoded_transaction=user['encoded_transaction'][2:], attributes=p['attributes_calldata'][2:],
                     transaction_hash=user['hash'],data_rule=p['data_fee_rule'],gas_used=str(user['gas_used']),
                     receipt_fee=str(int(receipt['l1Fee'],16)),receipt_gas=str(int(receipt['l1GasUsed'],16)))
            if p['operator_fee_rule']!='NotActive':
                require(p['operator_fee_rule']in ['Isthmus','Jovian'],'Unknown operator rule')
                row['operator_rule']=p['operator_fee_rule']
            rows.append(row)
    require(len(rows)==128 and len(set(names))==8,'Unexpected upgrade coverage')
    result=subprocess.run(['go','-C',str(ROOT/'chain-parser-go'),'run',str(runner)],input=json.dumps(rows),text=True,capture_output=True,timeout=120)
    (output.parent/'original-upgrade-reference.stderr').write_text(result.stderr)
    require(result.returncode==0,'Original upgrade reference failed: '+result.stderr[-1800:])
    actual=json.loads(result.stdout)
    require(len(actual)==len(rows) and all(all(r.get(k)==v for k,v in expected.items())for r,expected in zip(actual,rows)),'Original upgrade inputs changed')
    for r in actual:
        require(r['fee']==r['receipt_fee'] and r['l1_gas_used']==r['receipt_gas'],'Original upgrade cost differs from RPC observation: '+r['id'])
    refs={str(p.relative_to(ROOT)):digest(p)for p in [source_input,Path(packed_reference).resolve(),runner]}
    output.write_text(json.dumps(dict(passed=True,cases=actual,source_blocks=8,public_signed_users=128,
                                      upgrade_deposits_only_blocks=4,references=refs,production_ready=False),indent=2)+'\n')
    return dict(cases=len(actual),references=refs,sha256=digest(output))
