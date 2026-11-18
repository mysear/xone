#!/usr/bin/env python3
"""Run full unchanged op-geth transitions in explicitly local, signed scenarios."""
import hashlib
import json
from pathlib import Path
import shutil
import sys
import subprocess
import tarfile
from op_l1_cost_vectors import ROOT, COMMIT, attributes as bedrock, digest, require
from op_packed_cost_vectors import attributes as packed

ARCHIVE_SHA = 'baee7f36d27a3074e0e601a32618183ce88e4f8f17a86bd23c1b78fef08aa492'
REFERENCE = ROOT/'integration/.runtime/design-review/op-chain-cost-reference'
SOURCE = REFERENCE/'source'/('op-geth-'+COMMIT)
RUNNER = ROOT/'integration/tools/acceptance/reference/op_chain_cost.go'


def original_inputs():
    archive = REFERENCE/'original-op-geth.tar.gz'
    require(digest(archive)==ARCHIVE_SHA,'Pinned original op-geth archive changed')
    refs={str(archive.relative_to(ROOT)):ARCHIVE_SHA}
    # Authenticate the entire extracted source, not a hand-picked helper. Pin
    # all original Go inputs again across the formal validation interval.
    count=0
    with tarfile.open(archive,'r:gz') as tar:
        for member in tar:
            if not member.isfile():
                continue
            parts=Path(member.name).parts
            require(parts[0]=='op-geth-'+COMMIT and '..'not in parts,'Unsafe archive member')
            expected=hashlib.sha256(tar.extractfile(member).read()).hexdigest()
            local=SOURCE.joinpath(*parts[1:])
            require(digest(local)==expected,'Original source modified: '+str(local))
            if local.suffix=='.go' or local.name in ['go.mod','go.sum']:
                refs[str(local.relative_to(ROOT))]=expected
            count+=1
    require(count>1000,'Incomplete original archive')
    return refs


def cases():
    rows=[]
    times=dict(regolith=1686068904,ecotone=1710374403,fjord=1720627203,isthmus=1746806403,jovian=1764691203)
    for rule,timestamp in times.items():
        attrs=(bedrock(17000000000,188,684000).hex() if rule=='regolith' else
               packed('jovian'if rule=='jovian'else 'isthmus'if rule=='isthmus'else 'ecotone',17000000000,1,1368,810949,1234567,99))
        for kind in ['legacy','access','dynamic']:
            for outcome in ['success','refund','revert','halt']:
                for beneficiary in ['separate','payer','base','l1','operator']:
                    rows.append(dict(id='-'.join([rule,kind,outcome,beneficiary]),rule=rule,attributes=attrs,
                                     kind=kind,outcome=outcome,beneficiary=beneficiary,base_fee='1000000000',
                                     maximum='3000000000',priority='2500000000',gas_limit=60000,timestamp=timestamp))
    for rule in times:
        normal=next(r for r in rows if r['rule']==rule and r['kind']=='dynamic'and r['outcome']=='refund'and r['beneficiary']=='payer')
        for name,base,maximum,priority in [('zero',0,0,0),('exact-base',7,7,0),('unclipped',7,100,2),('wide',2**90,2**91,2**89)]:
            r=dict(normal,id=rule+'-'+name,base_fee=str(base),maximum=str(maximum),priority=str(priority))
            if name=='zero':
                r['attributes']=(bedrock(0,0,0).hex() if rule=='regolith'else packed('jovian'if rule=='jovian'else 'isthmus'if rule=='isthmus'else 'ecotone',0,1,1,0,0,0))
            rows.append(r)
    require(len(rows)==320,'Scenario coverage changed')
    return rows


def audit(rows):
    """Independent integer/accounting check of actual original VM deltas."""
    require(len(rows)==320 and len({r['input']['id']for r in rows})==320,'Scenario coverage differs')
    lower_intrinsic=0
    for r in rows:
        s=r['input'];a=r['actual'];gas=a['gas_used'];limit=s['gas_limit']
        base=int(s['base_fee']);cap=int(s['maximum']);priority=int(s['priority'])
        price=cap if s['kind']!='dynamic'else min(cap,base+priority)
        require(int(r['effective_gas_price'])==price and 0<gas<=limit,'Actual gas/price differs')
        if gas<21000:lower_intrinsic+=1
        l1=int(r['l1_fee']);op=int(a['operator_fee']);op_max=int(r['operator_maximum'])
        maximum=limit*cap+l1+op_max
        require(int(r['maximum_funding'])==maximum,'Maximum funding differs')
        require(op<=op_max,'Operator refund negative')
        payer=r['payer'].lower();beneficiary=r['beneficiary'].lower()
        for name in ['actual','exact_funding']:
            before={a.lower():int(n)for a,n in r[name]['before'].items()}
            after={a.lower():int(n)for a,n in r[name]['after'].items()}
            credits={a:0 for a in before}
            for address,value in [(beneficiary,gas*(price-base)),('0x4200000000000000000000000000000000000019',gas*base),('0x420000000000000000000000000000000000001a',l1),('0x420000000000000000000000000000000000001b',op)]:
                require(address in credits,'Missing original fee account')
                credits[address]+=value
            require(before.keys()==after.keys(),'Changed original fee account coverage')
            require(before[payer]>=maximum,'Original underfunded payer accepted')
            for address in before:
                require(after[address]==before[address]+credits[address]-(gas*price+l1+op if address==payer else 0),'Original fee account conservation differs: '+s['id'])
            require(sum(before.values())==sum(after.values()),'OP fee value was burned')
    require(lower_intrinsic==0,'Unexpected charged gas below intrinsic in committed reference')
    return dict(scenarios=len(rows),below_intrinsic=lower_intrinsic)


def write(output):
    output=Path(output).resolve();output.parent.mkdir(parents=True,exist_ok=False)
    refs=original_inputs();refs[str(RUNNER.relative_to(ROOT))]=digest(RUNNER)
    module=output.parent/'runner';module.mkdir()
    shutil.copyfile(RUNNER,module/'main.go')
    (module/'go.mod').write_text('module xone-op-transition-reference\n\ngo 1.24.0\n\nrequire github.com/ethereum/go-ethereum v0.0.0\n\nreplace github.com/ethereum/go-ethereum => '+str(SOURCE)+'\n')
    inputs=cases();(output.parent/'inputs.json').write_text(json.dumps(inputs,indent=2)+'\n')
    run=subprocess.run(['go','run','-mod=mod','.'],cwd=module,input=json.dumps(inputs),text=True,capture_output=True,timeout=300)
    (output.parent/'original-transition.stderr').write_text(run.stderr)
    require(run.returncode==0,'Original state transition failed: '+run.stderr[-3000:])
    actual=json.loads(run.stdout)
    require(len(actual)==len(inputs) and all(row['input']==s for row,s in zip(actual,inputs)),'Original scenario identity changed')
    for row in actual:
        a,e,b=row['actual'],row['exact_funding'],row['below_funding']
        require(a['accepted']is True and e['accepted']is True and a['gas_used']==e['gas_used'],'Original funding execution differs')
        require(a['nonce_after']==1 and e['nonce_after']==1,'Original nonce did not advance')
        require(b is None or b['accepted']is False and b['insufficient_funds']is True and b['nonce_after']==0 and b['before']==b['after'],'Insufficient funds mutated fee accounts')
        require(a['failed']==(row['input']['outcome']in ['revert','halt']),'Original execution result changed')
        require(int(a['storage_after'],16)==0,'Original storage rollback/refund mismatch')
    audit(actual)
    # Record the independent module graph and exact wrapper used as operands.
    for name in ['main.go','go.mod','go.sum']:
        p=module/name;refs[str(p.relative_to(ROOT))]=digest(p)
    report=dict(passed=True,scope='LOCAL_SIGNED_ZERO_VALUE_FEE_ONLY_ORIGINAL_OP_GETH_NOT_HISTORICAL_BANK',
                upstream_commit=COMMIT,cases=actual,scenario_count=len(actual),transition_count=sum(2+(r["below_funding"]is not None)for r in actual),
                references=refs,production_ready=False,all_requested_features_complete=False)
    output.write_text(json.dumps(report,indent=2)+'\n')
    return dict(cases=len(actual),references=refs,sha256=digest(output))


def run_revm(vector, output):
    import os
    import re
    manifest=ROOT/'integration/tools/acceptance/reference/op-revm/Cargo.toml'
    env=dict(os.environ,CARGO_BUILD_JOBS='1',CARGO_TARGET_DIR=str(ROOT/'integration/.runtime/design-review/op-revm-prototype-target'))
    result=subprocess.run(['cargo','+1.90.0','run','--locked','--manifest-path',str(manifest),'--',str(Path(vector).resolve())],cwd=ROOT,env=env,text=True,capture_output=True,timeout=300)
    Path(output).with_suffix('.stderr').write_text(result.stderr)
    require(result.returncode==0,'Independent original REVM comparison failed: '+result.stderr[-2000:]+result.stdout[-2000:])
    r=json.loads(result.stdout)
    require(r.get('passed')is True and r.get('checked')==256 and r.get('unsupported_jovian')==64 and not r.get('failures'),'Independent original REVM results differ')
    if sys.platform=='darwin':
        staged=re.findall(r'XONE_STAGED_NATIVE sha256=([a-f0-9]{64})',result.stderr)
        require(len(staged)==1,'Independent REVM did not execute byte-identical staged binary')
        r['executed_binary_sha256']=staged[0]
    r['vectors_sha256']=digest(vector)
    Path(output).write_text(json.dumps(r,indent=2)+'\n')


def replay(vector, output):
    vector=Path(vector).resolve();report=json.loads(vector.read_bytes())
    require(report.get('passed')is True,'Original vector report required')
    for path,expected in report['references'].items():
        require(digest(ROOT/path)==expected,'Original transition operand changed: '+path)
    module=vector.parent/'runner'
    run=subprocess.run(['go','run','-mod=readonly','.'],cwd=module,input=json.dumps([r['input']for r in report['cases']]),text=True,capture_output=True,timeout=180)
    Path(output).with_suffix('.stderr').write_text(run.stderr)
    require(run.returncode==0,'Frozen original transition replay failed: '+run.stderr[-2000:])
    actual=json.loads(run.stdout)
    require(actual==report['cases'],'Original transition replay changed')
    audit(actual)
    Path(output).write_text(json.dumps(dict(passed=True,scenarios=len(actual),transitions=sum(2+(r['below_funding']is not None)for r in actual),vectors_sha256=digest(vector),stdout_sha256=hashlib.sha256(run.stdout.encode()).hexdigest()),indent=2)+'\n')


if __name__=='__main__':
    import sys
    if len(sys.argv)==4 and sys.argv[1]=='--replay':
        replay(sys.argv[2],sys.argv[3])
    elif len(sys.argv)==4 and sys.argv[1]=='--revm':
        run_revm(sys.argv[2],sys.argv[3])
    else:
        print(json.dumps(write(sys.argv[1])))
