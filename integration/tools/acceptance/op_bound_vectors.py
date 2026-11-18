#!/usr/bin/env python3
"""Independent original-client full Router transitions for signed local OP plans."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
from op_chain_cost_vectors import ROOT, SOURCE, original_inputs, digest, require

RUNNER = ROOT/'integration/tools/acceptance/reference/op_bound.go'
FORKS = {'regolith','canyon','ecotone','fjord','granite','holocene','isthmus'}
VAULTS = ['0x42000000000000000000000000000000000000'+n for n in ['19','1a','1b']]


def envelope(wire):
    """Read the already signed reference envelope without interpreting VM state."""
    raw=bytes.fromhex(wire);kind=raw[0] if raw[0]<0x80 else 0
    if kind:raw=raw[1:]
    def read(offset):
        require(offset<len(raw),'Truncated reference RLP')
        b=raw[offset];offset+=1
        if b<0x80:return bytes([b]),offset
        is_list=b>=0xc0;base=0xc0 if is_list else 0x80;n=b-base
        if n>55:
            width=n-55;require(offset+width<=len(raw),'Truncated RLP length')
            n=int.from_bytes(raw[offset:offset+width],'big');offset+=width
        end=offset+n;require(end<=len(raw),'Truncated RLP payload')
        if not is_list:return raw[offset:end],end
        out=[]
        while offset<end:
            child,offset=read(offset);out.append(child)
        require(offset==end,'RLP list boundary');return out,end
    fields,end=read(0);require(end==len(raw) and isinstance(fields,list),'Reference envelope boundary')
    require(kind in [0,2] and len(fields)==(9 if kind==0 else 12),'Reference transaction type/fields')
    number=lambda x:int.from_bytes(x,'big')
    if kind==0:return dict(kind=kind,nonce=number(fields[0]),cap=number(fields[1]),tip=None,limit=number(fields[2]),data=fields[5])
    return dict(kind=kind,nonce=number(fields[1]),cap=number(fields[3]),tip=number(fields[2]),limit=number(fields[4]),data=fields[7])


def audit(inputs,rows):
    require(len(inputs)==len(rows)==448 and len({r['id'] for r in rows})==448,'BOUND scenario coverage differs')
    coverage={}
    for s,r in zip(inputs,rows):
        require(s['id']==r['id'] and s['transaction_hash']==r['transaction_hash'],'BOUND transaction identity differs')
        rule=s['rule'];tx=envelope(s['signed_wire']);mode=s['id'].rsplit('-',1)[-1]
        key=(rule,tx['kind'],mode);coverage[key]=coverage.get(key,0)+1
        require(rule in FORKS and mode in ['in','out'],'BOUND rules differ')
        e=s['fixture']['execution_environment'];gas=r['gas_used'];base=int(e['base_fee'])
        require(21000<=gas<=tx['limit'] and r['nonce_after']==tx['nonce']+1,'Original gas/nonce differs')
        price=tx['cap'] if tx['tip'] is None else min(tx['cap'],base+tx['tip'])
        require(tx['cap']>=base,'Original invalid fee cap accepted')
        expected_op=0
        if rule=='isthmus':
            l1=next(a for a in s['fixture']['accounts'] if a['address']=='0x4200000000000000000000000000000000000015')
            packed=int(next(v['value'] for v in l1['storage'] if int(v['slot'],16)==8),16)
            expected_op=gas*(packed>>64)//1_000_000+(packed&((1<<64)-1))
        require(int(r['operator_fee'])==expected_op and int(r['l1_fee'])>=0,'Original operator fee differs')
        total=gas*price+int(r['l1_fee'])+expected_op
        credits={}
        for a,n in [(e['coinbase'],gas*(price-base)),(VAULTS[0],gas*base),(VAULTS[1],int(r['l1_fee'])),(VAULTS[2],expected_op)]:credits[a]=credits.get(a,0)+n
        before={a['address']:a for a in s['fixture']['accounts']};after=r['accounts']
        require(before.keys()==after.keys(),'Original declared accounts differ')
        native_before=native_after=0
        for a,b in before.items():
            x=after[a];native_before+=int(b['balance']);native_after+=int(x['balance'])
            require(int(x['balance'])==int(b['balance'])+credits.get(a,0)-(total if a==s['caller'] else 0),'Original native conservation differs')
            require(x['nonce']==b['nonce']+(1 if a==s['caller'] else 0) and x['code']==b['code'],'Original nonce/code differs')
            require(x['storage'].keys()=={v['slot'] for v in b['storage']},'Original storage coverage differs')
            for value in x['storage'].values():require(len(value)==66 and value.startswith('0x') and 0<=int(value,16)<2**256,'Original storage format differs')
        require(native_before==native_after,'OP value burned')
        require(int(r['spent'])>0 and int(r['bought'])>0,'Original real tokens missing')
        data=tx['data'];require(len(data)>=68,'Original Router calldata truncated')
        requested=int.from_bytes(data[4:36],'big');boundary=int.from_bytes(data[36:68],'big')
        require((int(r['spent'])==requested and int(r['bought'])>=boundary) if mode=='in' else (int(r['bought'])==requested and int(r['spent'])<=boundary),'Original Router amount constraints differ')
    require(len(coverage)==28 and set(coverage.values())=={16},'BOUND matrix coverage differs')


def write(output,execution_vectors):
    output=Path(output).resolve();output.parent.mkdir(parents=True,exist_ok=False)
    refs=original_inputs();refs[str(RUNNER.relative_to(ROOT))]=digest(RUNNER)
    execution_vectors=Path(execution_vectors).resolve();refs[str(execution_vectors.relative_to(ROOT))]=digest(execution_vectors)
    inputs_path=output.parent/'inputs.json'
    env=dict(os.environ,PATH=str(Path.home()/'.cargo/bin')+os.pathsep+os.environ['PATH'],XONE_OP_EXECUTION_VECTORS=str(execution_vectors),XONE_OP_BOUND_INPUTS=str(inputs_path))
    env.setdefault('CARGO_BUILD_JOBS','2');env.setdefault('CARGO_TARGET_DIR',str(ROOT/'integration/.runtime/design-review-target'));env.setdefault('RUST_TEST_THREADS','1')
    if sys.platform=='darwin':
        import platform
        triple=('aarch64' if platform.machine()=='arm64' else 'x86_64')+'_APPLE_DARWIN'
        env.setdefault('CARGO_TARGET_'+triple.upper()+'_RUNNER',str(ROOT/'integration/tools/ci/native-test-runner.py'))
    result=subprocess.run(['cargo','+1.90.0','test','--locked','--manifest-path','dex-router/Cargo.toml','-p','xone-simulation','--test','op_bound','export_original_op_bound_inputs','--','--include-ignored'],cwd=ROOT,env=env,text=True,capture_output=True,timeout=180)
    (output.parent/'export.log').write_text(result.stdout+result.stderr)
    require(result.returncode==0 and 'test export_original_op_bound_inputs ... ok' in result.stdout,'BOUND input export failed: '+result.stderr[-1000:])
    inputs=json.loads(inputs_path.read_bytes());module=output.parent/'runner';module.mkdir();shutil.copyfile(RUNNER,module/'main.go')
    (module/'go.mod').write_text('module xone-op-bound-reference\n\ngo 1.24.0\n\nrequire github.com/ethereum/go-ethereum v0.0.0\n\nreplace github.com/ethereum/go-ethereum => '+str(SOURCE)+'\n')
    result=subprocess.run(['go','run','-mod=mod','.'],cwd=module,input=json.dumps(inputs),text=True,capture_output=True,timeout=300)
    (output.parent/'original.stderr').write_text(result.stderr)
    require(result.returncode==0,'Original BOUND transitions failed: '+result.stderr[-2500:]);rows=json.loads(result.stdout);audit(inputs,rows)
    for p in [inputs_path,output.parent/'export.log',output.parent/'original.stderr',module/'main.go',module/'go.mod',module/'go.sum']:refs[str(p.relative_to(ROOT))]=digest(p)
    output.write_text(json.dumps(dict(passed=True,cases=rows,scenario_count=448,references=refs,scope='LOCAL_CONSTRUCTED_HISTORICAL_OP_BOUND_ROUTER_NOT_TRUSTED_BANK_OR_NORMAL_API',production_ready=False,all_requested_features_complete=False),indent=2)+'\n')
    return dict(references=refs,sha256=digest(output))


def replay(vector,output):
    vector=Path(vector).resolve();v=json.loads(vector.read_bytes())
    for name,expected in v['references'].items():require(digest(ROOT/name)==expected,'BOUND original input changed: '+name)
    inputs=json.loads((vector.parent/'inputs.json').read_bytes())
    result=subprocess.run(['go','run','-mod=readonly','.'],cwd=vector.parent/'runner',input=json.dumps(inputs),text=True,capture_output=True,timeout=300)
    Path(output).with_suffix('.stderr').write_text(result.stderr)
    require(result.returncode==0,'Original BOUND replay failed: '+result.stderr[-1000:]);rows=json.loads(result.stdout)
    require(rows==v['cases'],'Original complete state replay differs');audit(inputs,rows)
    Path(output).write_text(json.dumps(dict(passed=True,cases=len(rows),vectors_sha256=digest(vector)),indent=2)+'\n')


if __name__=='__main__':
    if len(sys.argv)==4 and sys.argv[1]=='--replay':replay(sys.argv[2],sys.argv[3])
    else:print(json.dumps(write(sys.argv[1],sys.argv[2])))
