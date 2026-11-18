#!/usr/bin/env python3
"""Pinned full original Karst VM transitions over explicit local banks.

No current-bank, derivation or consensus qualification is implied. Product
qualification alone does not grant schedule admission or trusted-bank authority.
"""
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import tarfile
from karst_precompile_vectors import ROOT, BASE, OP, originals, dependencies, digest, require

RUNNER = ROOT / 'integration/tools/acceptance/reference/op_karst_execution.rs'
PRIOR = ROOT / 'integration/artifacts/local/karst-precompiles-check/runs/97bbe25ecfa34c0bb7913abed8502762/report.json'
TARGET = '0x1000000000000000000000000000000000000001'
L1 = '0x4200000000000000000000000000000000000015'
CAP = 1 << 24


def retained():
    report = json.loads(PRIOR.read_bytes())
    require(report['passed'] is True, 'Completed component qualification required')
    name = next(n for n in report['reference_inputs'] if '/execution/vectors.json' in n and '/karst-precompiles-check/original-runs/' in n)
    signer = next(n for n in report['reference_inputs'] if n.endswith('/signer/runner'))
    refs = {str(PRIOR.relative_to(ROOT)): digest(PRIOR)}
    for n in [name, signer]:
        require(digest(ROOT/n) == report['reference_inputs'][n], 'Retained original operand changed: ' + n)
        refs[n] = report['reference_inputs'][n]
    vectors = json.loads((ROOT/name).read_bytes())
    for n, h in vectors['references'].items():
        require(digest(ROOT/n) == h, 'Original historical operand changed: '+n)
        refs[n] = h
    for n,h in report['reference_inputs'].items():
        if '/signer/' in n:
            require(digest(ROOT/n)==h,'Original signer operand changed'); refs[n]=h
    return vectors['cases'], ROOT/signer, refs


def word(n): return f'0x{n:064x}'


def fixture(row, funding):
    s = row['input']; a = bytes.fromhex(s['attributes'])
    accounts = {owner.lower(): dict(address=owner.lower(), exists=True, balance=value, nonce=0, code='0x', storage=[]) for owner,value in row[funding]['before'].items()}
    code = {'success':'00','refund':'600060005500','revert':'600160005560006000fd','halt':'6001600055fe'}[s['outcome']]
    accounts[TARGET] = dict(address=TARGET, exists=True, balance='0', nonce=0, code='0x'+code, storage=[dict(slot=word(0),value=word(int(s['outcome']=='refund')))])
    scalars = bytes(16)+a[4:12]+bytes(8)
    operator = bytes(20)+a[164:176]
    slots = [(1,a[36:68]),(3,scalars),(7,a[68:100]),(8,operator),(9,bytes(30)+a[176:178])]
    accounts[L1] = dict(address=L1,exists=True,balance='0',nonce=0,code='0x',storage=[dict(slot=word(n),value=word(int.from_bytes(v,'big'))) for n,v in slots])
    return dict(accounts=list(accounts.values()),execution_environment=dict(chain_id=10,block_number=100,timestamp=1783526403,coinbase=row['beneficiary'].lower(),gas_limit=30_000_000,base_fee=str(min(int(s['base_fee']),2**64-1)),prevrandao='0x'+'00'*32,fork='op-karst'))


def cases():
    rows, signer, refs = retained(); inputs=[]
    for r in rows:
        for funding in ['actual','exact_funding','below_funding']:
            if r[funding] is None: continue
            inputs.append(dict(id=r['input']['id'].replace('jovian-','karst-',1)+'-'+funding,rule='karst',fixture=fixture(r,funding),signed_wire=r['signed_wire'],transaction_hash=r['transaction_hash'],caller=r['payer'].lower(),attributes=r['input']['attributes'],maximum_funding=r['maximum_funding'],expect_accepted=funding!='below_funding',expect_failed=r['input']['outcome'] in ['revert','halt'],fee_kind=r['input']['kind'],fee_maximum=r['input']['maximum'],fee_priority=r['input']['priority'],base_fee_remapped_to_u64=int(r['input']['base_fee'])>2**64-1))
    require(len(inputs)==191,'Original funding matrix incomplete')
    base=copy.deepcopy(inputs[0])
    def add(name, code, **extra):
        v=copy.deepcopy(base);v['id']=name
        next(a for a in v['fixture']['accounts'] if a['address']==TARGET)['code']='0x'+code
        v.update(extra);inputs.append(v)
    for zeros in range(257):
        value=0 if zeros==256 else 1<<(255-zeros)
        add('clz-'+str(zeros),'7f'+f'{value:064x}'+'1e5f5260205ff3',expected_output=f'{zeros:064x}',clz_zeros=zeros)
    add('clz-underflow','1e00',expect_failed=True)
    add('clz-loop','5b5f1e50600056',expect_failed=True)
    # Real signed boundary transactions, rather than independent overrides of
    # authenticated VM fields. The unchanged original Go signer uses test key 1.
    for kind in [0,2]:
        for gas in [CAP-1,CAP,CAP+1,30_000_000,21000,21004,21005,21006,21007]:
            rpc=dict(type=hex(kind),from_=base['caller'],to=TARGET,chainId='0xa',nonce='0x0',value='0x0',gas=hex(gas),data='0x',gasPrice=hex(3_000_000_000),maxFeePerGas=hex(3_000_000_000),maxPriorityFeePerGas=hex(2_500_000_000))
            rpc['from']=rpc.pop('from_')
            result=subprocess.run([str(signer)],input=json.dumps(rpc),capture_output=True,text=True,timeout=30)
            require(result.returncode==0,'Original cap signer failed'); signed=json.loads(result.stdout)
            v=copy.deepcopy(base);v.update(id=f'cap-{kind}-{gas}',signed_wire=signed['signedWire'],transaction_hash=signed['transactionHash'],expect_accepted=gas<=CAP,gas_boundary=gas)
            v.pop('maximum_funding');v.update(fee_kind='legacy' if kind==0 else 'dynamic',fee_maximum='3000000000',fee_priority='2500000000'); target=next(a for a in v['fixture']['accounts'] if a['address']==TARGET)
            # Empty CLZ underflow and its exact pre-instruction Gas boundaries.
            target['code']='0x5f1e00' if gas<22000 else '0x00'
            v['expect_failed']=gas<21007
            next(a for a in v['fixture']['accounts'] if a['address']==v['caller'])['balance']=str(10**24)
            inputs.append(v)
    require(len(inputs)==468 and len({v['id'] for v in inputs})==468,'Full Karst matrix incomplete')
    return inputs, refs


def audit(inputs, rows):
    expected,_=cases(); require(inputs==expected and len(rows)==468,'Original Karst coverage changed')
    for s,r in zip(inputs,rows):
        require(r['id']==s['id'] and r['transaction_hash'].lower()==s['transaction_hash'].lower() and r['caller']==s['caller'],'Original signed identity changed')
        require(r['accepted'] is s['expect_accepted'],'Original admission differs: '+s['id'])
        require(set(r['before'])==set(r['accounts'])=={a['address'] for a in s['fixture']['accounts']},'Original account coverage changed')
        before=r['before'];after=r['accounts']
        required={'id','transaction_hash','caller','before','accepted','accounts'} | ({'failed','gas_used','output','status','l1_fee','operator_fee'} if r['accepted'] else {'error'})
        require(set(r)==required,'Unknown or missing original transition field')
        for a in s['fixture']['accounts']:
            require(before[a['address']]['balance']==a['balance'],'Original before differs')
            require(before[a['address']]['nonce']==a['nonce'] and before[a['address']]['code']==a['code'] and before[a['address']]['storage']=={x['slot']:x['value'] for x in a['storage']},'Original before state changed')
        if not r['accepted']:
            require(before==after and 'error' in r and 'gas_used' not in r,'Original rejection mutated bank')
            continue
        require(r['failed'] is s['expect_failed'],'Original result differs: '+s['id'])
        require(sum(int(a['balance']) for a in before.values())==sum(int(a['balance']) for a in after.values()),'Original fee balance not conserved')
        require(after[s['caller']]['nonce']==before[s['caller']]['nonce']+1,'Original nonce differs')
        for owner in before:
            require(after[owner]['code']==before[owner]['code'],'Original code changed')
            require(after[owner]['nonce']==before[owner]['nonce']+int(owner==s['caller']),'Original account nonce changed')
            storage=dict(before[owner]['storage'])
            if owner==TARGET and before[owner]['code']=='0x600060005500': storage[word(0)]=word(0)
            require(after[owner]['storage']==storage,'Original poststorage differs')
        if 'clz_zeros' not in s: require(r['output']=='','Original empty output changed')
        gas=r['gas_used']; require(21000<=gas<=s.get('gas_boundary',60000),'Original gas invalid')
        if 'clz_zeros' in s:
            require(r['output']==s['expected_output'],'Original CLZ precision differs')
            require(gas==max(21000+2*4+3*16+24,21000+2*10+3*40),'Original CLZ gas differs')
        if r['failed']:
            require(all(after[a]['storage']==before[a]['storage'] for a in before),'Original failure did not roll back storage')
        # Ordinary zero-value calls may only move native balances among the five
        # explicit fee roles. Validate each alias using independent integer math.
        raw=bytes.fromhex(s['attributes']); scalar=int.from_bytes(raw[164:168],'big'); constant=int.from_bytes(raw[168:176],'big')
        operator=gas*scalar*100+constant
        require(int(r['operator_fee'])==operator,'Original operator units changed')
        base=int(s['fixture']['execution_environment']['base_fee']); cap=int(s['fee_maximum']); price=min(cap,base+int(s['fee_priority'])) if s['fee_kind']=='dynamic' else cap
        credits={a:0 for a in before}
        for a,n in [(s['fixture']['execution_environment']['coinbase'],gas*(price-base)),('0x4200000000000000000000000000000000000019',gas*base),('0x420000000000000000000000000000000000001a',int(r['l1_fee'])),('0x420000000000000000000000000000000000001b',operator)]:credits[a]+=n
        for a in before:
            wanted=int(before[a]['balance'])+credits[a]-(gas*price+int(r['l1_fee'])+operator if a==s['caller'] else 0)
            require(int(after[a]['balance'])==wanted,'Original fee role differs: '+s['id'])


def path_dependencies(module):
    refs=dependencies(module)
    metadata=json.loads((module/'metadata.json').read_bytes())
    archive=BASE/('optimism-60360273aada79572fe2212497de578ad2fa249b.tar.gz')
    with tarfile.open(archive) as tar:
        members={m.name:m for m in tar if m.isfile()}
        for package in metadata['packages']:
            if package['source'] is not None or package['name']=='xone-karst-original-execution':continue
            directory=Path(package['manifest_path']).parent
            require(directory.is_relative_to(OP),'Unpinned original path dependency')
            prefix=str(directory.relative_to(OP.parent))+'/'
            expected={Path(n[len(prefix):]):m for n,m in members.items() if n.startswith(prefix)}
            require(expected,'Original path package absent from archive')
            actual={p.relative_to(directory) for p in directory.rglob('*') if p.is_file()}
            require(actual==set(expected),'Extra or missing original path package operand')
            for relative,member in expected.items():
                p=directory/relative; h=hashlib.sha256(tar.extractfile(member).read()).hexdigest()
                require(digest(p)==h,'Original path package changed');refs[str(p.relative_to(ROOT))]=h
    return refs


def write(output):
    output=Path(output).resolve();output.parent.mkdir(parents=True,exist_ok=False)
    refs=originals();inputs,retained_refs=cases();refs.update(retained_refs);refs[str(RUNNER.relative_to(ROOT))]=digest(RUNNER)
    module=output.parent/'runner';(module/'src').mkdir(parents=True)
    shutil.copyfile(RUNNER,module/'src/main.rs');shutil.copyfile(OP/'rust/Cargo.lock',module/'Cargo.lock')
    (module/'Cargo.toml').write_text('[package]\nname="xone-karst-original-execution"\nversion="0.0.0"\nedition="2024"\n[workspace]\n[dependencies]\nop-revm={path="'+str(OP/'rust/op-revm')+'",default-features=true}\nalloy-consensus={version="=2.5.0",features=["k256"]}\nalloy-eips="=2.5.0"\nserde_json="1.0"\nhex="=0.4.3"\n')
    env=dict(os.environ,PATH=str(Path.home()/'.cargo/bin')+os.pathsep+os.environ['PATH'],CARGO_BUILD_JOBS='2',CARGO_INCREMENTAL='0',CARGO_TARGET_DIR=str(BASE/'target'))
    result=subprocess.run(['cargo','+1.99.0','build','--manifest-path',str(module/'Cargo.toml')],cwd=ROOT,env=env,capture_output=True,text=True,timeout=600)
    (output.parent/'build.log').write_text(result.stdout+result.stderr);require(result.returncode==0,'Original full engine build failed: '+result.stderr[-2500:])
    refs.update(path_dependencies(module));binary=module/'runner';shutil.copy2(BASE/'target/debug/xone-karst-original-execution',binary)
    (output.parent/'inputs.json').write_text(json.dumps(inputs)+'\n')
    result=subprocess.run([str(binary)],input=json.dumps(inputs),capture_output=True,text=True,timeout=300)
    (output.parent/'original.stderr').write_text(result.stderr);require(result.returncode==0,'Original full engine failed: '+result.stderr[-2500:])
    rows=json.loads(result.stdout);audit(inputs,rows)
    for p in [module/'src/main.rs',module/'Cargo.toml',module/'Cargo.lock',binary,output.parent/'build.log',output.parent/'inputs.json',output.parent/'original.stderr']:refs[str(p.relative_to(ROOT))]=digest(p)
    report=dict(passed=True,original_op_commit='60360273aada79572fe2212497de578ad2fa249b',original_revm_version='43.0.3',inputs=inputs,cases=rows,case_count=len(rows),references=refs,current_vm_admitted=False,production_ready=False,all_requested_features_complete=False,scope='FIXED_FULL_ORIGINAL_KARST_USER_VM_LOCAL_BANK_NOT_SCHEDULE_ADMISSION_TRUSTED_BANK_DERIVATION_OR_CONSENSUS')
    output.write_text(json.dumps(report,indent=2)+'\n');return refs


def replay(vector,output):
    vector=Path(vector);v=json.loads(vector.read_bytes())
    for n,h in v['references'].items():require(digest(ROOT/n)==h,'Original full engine operand changed: '+n)
    result=subprocess.run([str(vector.parent/'runner/runner')],input=json.dumps(v['inputs']),capture_output=True,text=True,timeout=300)
    Path(output).with_suffix('.stderr').write_text(result.stderr);require(result.returncode==0,'Original full engine replay failed')
    rows=json.loads(result.stdout);require(rows==v['cases'],'Original full engine replay changed');audit(v['inputs'],rows)
    Path(output).write_text(json.dumps(dict(passed=True,cases=468,vector_sha256=digest(vector),current_vm_admitted=False))+'\n')


if __name__=='__main__':
    import sys
    if len(sys.argv)==4 and sys.argv[1]=='--replay':replay(sys.argv[2],sys.argv[3])
    else:write(sys.argv[1])
