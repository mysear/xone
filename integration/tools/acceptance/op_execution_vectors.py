#!/usr/bin/env python3
"""Authenticate and replay signed historical OP user execution references."""
import json
from pathlib import Path
import shutil
import subprocess
import sys
from op_chain_cost_vectors import ROOT, SOURCE, original_inputs, cases as fee_cases, digest, require

RUNNER=ROOT/'integration/tools/acceptance/reference/op_execution.go'


def cases():
    base=fee_cases()
    rows=[]
    for rule,source,timestamp in [('regolith','regolith',1686068904),('canyon','regolith',1704992403),('ecotone','ecotone',1710374403),('fjord','fjord',1720627203),('granite','fjord',1726070403),('holocene','fjord',1736445603),('isthmus','isthmus',1746806403)]:
        for i,original in enumerate(r for r in base if r['rule']==source):
            row=dict(original,rule=rule,timestamp=timestamp,chain=10 if i%2==0 else 8453,access=original['kind']!='legacy' and i%3==0)
            if row['chain']==8453 and rule=='regolith':row['timestamp']=1686789348
            row['id']=rule+'-'+original['id'].split('-',1)[1]+'-'+str(row['chain'])
            rows.append(row)
    require(len(rows)==448 and len({r['id'] for r in rows})==448,'Execution scenario coverage changed')
    return rows


def audit(rows, expected_inputs=None):
    expected = cases() if expected_inputs is None else expected_inputs
    require(len(rows)==len(expected) and [r['input'] for r in rows]==expected,'Original execution coverage changed')
    coverage={}
    for r in rows:
        s=r['input'];coverage[s['rule']]=coverage.get(s['rule'],0)+1
        gas=r['actual']['gas_used'];cap=int(s['maximum']);base=int(s['base_fee']);tip=int(s['priority'])
        price=min(cap,base+tip)if s['kind']=='dynamic'else cap
        require(gas>=21000 and r['actual']['accepted'] is True and r['exact_funding']['accepted'] is True,'Original execution rejected')
        require(r['actual']['failed']==(s['outcome'] in ['revert','halt']),'Original execution result differs')
        require(int(r['maximum_funding'])==s['gas_limit']*cap+int(r['l1_fee'])+int(r['operator_maximum']),'Original maximum funding differs')
        for funding in ['actual','exact_funding']:
            a=r[funding];before={k.lower():int(v) for k,v in a['before'].items()};after={k.lower():int(v) for k,v in a['after'].items()}
            credits={k:0 for k in before};payer=r['payer'].lower()
            for address,amount in [(r['beneficiary'].lower(),gas*(price-base)),('0x4200000000000000000000000000000000000019',gas*base),('0x420000000000000000000000000000000000001a',int(r['l1_fee'])),('0x420000000000000000000000000000000000001b',int(a['operator_fee']))]: credits[address]+=amount
            require(a['gas_used']==gas and a['nonce_after']==1 and int(a['storage_after'],16)==0,'Original gas/nonce/storage differs')
            require(before.keys()==after.keys() and sum(before.values())==sum(after.values()),'Original fee value not conserved')
            for address in before:require(after[address]==before[address]+credits[address]-(gas*price+int(r['l1_fee'])+int(a['operator_fee']) if address==payer else 0),'Original account delta differs')
        b=r['below_funding']
        require(b is None or b['accepted'] is False and b['insufficient_funds'] is True and b['nonce_after']==0 and b['before']==b['after'],'Underfunded original execution accepted')
    require(set(coverage)=={r['rule'] for r in expected} and set(coverage.values())=={64},'Fork coverage differs')


def write(output, selected=None):
    output=Path(output).resolve();output.parent.mkdir(parents=True,exist_ok=False)
    refs=original_inputs();refs[str(RUNNER.relative_to(ROOT))]=digest(RUNNER)
    module=output.parent/'runner';module.mkdir();shutil.copyfile(RUNNER,module/'main.go')
    (module/'go.mod').write_text('module xone-op-execution-reference\n\ngo 1.24.0\n\nrequire github.com/ethereum/go-ethereum v0.0.0\n\nreplace github.com/ethereum/go-ethereum => '+str(SOURCE)+'\n')
    inputs=cases() if selected is None else selected;(output.parent/'inputs.json').write_text(json.dumps(inputs,indent=2)+'\n')
    result=subprocess.run(['go','run','-mod=mod','.'],cwd=module,input=json.dumps(inputs),text=True,capture_output=True,timeout=300)
    (output.parent/'original.stderr').write_text(result.stderr)
    require(result.returncode==0,'Original execution failed: '+result.stderr[-3000:])
    actual=json.loads(result.stdout);require([r['input'] for r in actual]==inputs,'Original scenario identity differs');audit(actual, inputs)
    for name in ['main.go','go.mod','go.sum']:p=module/name;refs[str(p.relative_to(ROOT))]=digest(p)
    report=dict(passed=True,cases=actual,scenario_count=len(actual),transition_count=sum(2+(r['below_funding'] is not None) for r in actual),references=refs,production_ready=False,all_requested_features_complete=False,scope='LOCAL_SIGNED_ZERO_VALUE_HISTORICAL_OP_EXECUTION_NOT_HISTORICAL_BANK_AUTHENTICATION_OR_CURRENT_CHAIN')
    output.write_text(json.dumps(report,indent=2)+'\n')
    return dict(cases=len(actual),references=refs,sha256=digest(output))


def replay(vector,output):
    vector=Path(vector).resolve();v=json.loads(vector.read_bytes())
    for name,expected in v['references'].items():require(digest(ROOT/name)==expected,'Original operand changed: '+name)
    result=subprocess.run(['go','run','-mod=readonly','.'],cwd=vector.parent/'runner',input=json.dumps([r['input'] for r in v['cases']]),text=True,capture_output=True,timeout=300)
    Path(output).with_suffix('.stderr').write_text(result.stderr)
    require(result.returncode==0,'Original replay failed: '+result.stderr[-2000:]);actual=json.loads(result.stdout)
    require(actual==v['cases'],'Original execution replay changed');audit(actual)
    Path(output).write_text(json.dumps(dict(passed=True,cases=len(actual),vectors_sha256=digest(vector)),indent=2)+'\n')




def public_inputs(output):
    """Reuse immutable stage137 normal-source projections without upgrading trust."""
    report=ROOT/'integration/artifacts/local/op-packed-source-inputs-check/runs/5b702f530c124f618ee95f6ba8369435/report.json'
    v=json.loads(report.read_bytes());require(v.get('passed') is True,'Verified archived normal-source run required')
    refs={str(report.relative_to(ROOT)):digest(report)};users=[]
    for name,expected in v['artifact_reports'].items():
        if 'normal-' not in name:continue
        p=ROOT/name;require(digest(p)==expected,'Archived public projection changed');refs[name]=expected
        d=json.loads(p.read_bytes())
        groups=[d['production_projection']]if 'production_projection' in d else [b['inputs']for b in d['blocks']]if 'blocks' in d else [d['inputs']]
        for g in groups:
            for u in g['user_transactions']:users.append(dict(chain_key=g['chain_key'],block_number=g['block_number'],**u))
    require(len(users)==180,'Archived signed-user coverage changed')
    output=Path(output);output.write_text(json.dumps(dict(passed=True,users=users,scope='ARCHIVED_NORMAL_SOURCE_SIGNATURE_AUTHENTICATION_NOT_VM_EXECUTION_OR_CONSENSUS'),indent=2)+'\n')
    refs[str(output.relative_to(ROOT))]=digest(output)
    return refs


if __name__=='__main__':
    if len(sys.argv)==4 and sys.argv[1]=='--replay':replay(sys.argv[2],sys.argv[3])
    else:print(json.dumps(write(sys.argv[1])))
