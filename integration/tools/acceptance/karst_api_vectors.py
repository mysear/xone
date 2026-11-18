#!/usr/bin/env python3
"""Complete original Karst Router transitions for normal local signed APIs."""
import json
import os
from pathlib import Path
import subprocess
from op_bound_vectors import envelope
from karst_execution_vectors import ROOT,PRIOR,write as execution_write,replay as execution_replay,digest,require


def inputs_from(sources):
    require(len(sources)==4 and len({r['id'] for r in sources})==4,'Karst normal source coverage changed')
    inputs=[]
    for row in sources:
        q=row['request'];b=q['bound'] if q.get('bound') is not None else q['exact_out_bound'];c=b['plan']['context'];u=q['op_user']
        require(q['fixture']['execution_environment']['fork']=='op-karst','Karst source rule changed')
        inputs.append(dict(id=row['id'],rule='karst',fixture=q['fixture'],signed_wire=u['signed_wire'],transaction_hash=u['transaction_hash'],caller=c['caller'],token_in=c['sell_token'],token_out=c['buy_token'],recipient=c['recipient']))
    return inputs


def audit(inputs,rows,sources):
    require(inputs==inputs_from(sources) and len(rows)==4,'Original Karst Router input changed')
    require({r['id'] for r in rows}=={s['id'] for s in inputs},'Original Karst Router identity coverage changed')
    for s,r in zip(inputs,rows):
        require(r['id']==s['id'] and r['transaction_hash'].lower()==s['transaction_hash'].lower() and r['caller']==s['caller'].lower(),'Original Karst Router signature identity changed')
        require(r['accepted'] is True and r['failed'] is False and r['output']!='','Original Karst Router did not execute')
        require(set(r)=={'id','transaction_hash','caller','before','accepted','failed','gas_used','output','status','l1_fee','operator_fee','spent','bought','nonce_after','accounts'},'Unknown original Router result field')
        before=r['before'];after=r['accounts'];require(set(before)==set(after)=={a['address'].lower() for a in s['fixture']['accounts']},'Original Router account set changed')
        for a in s['fixture']['accounts']:
            b=before[a['address'].lower()]
            require(b['balance']==a['balance'] and b['nonce']==a['nonce'] and b['code'].lower()==a['code'].lower() and b['storage']=={x['slot']:x['value'] for x in a['storage']},'Original Router source before changed')
        require(sum(int(a['balance']) for a in before.values())==sum(int(a['balance']) for a in after.values()),'Original Router native fees not conserved')
        require(r['nonce_after']==before[s['caller'].lower()]['nonce']+1,'Original Router nonce changed')
        require(int(r['spent'])>0 and int(r['bought'])>0 and 21000<=r['gas_used']<=15_000_000,'Original Router amounts/gas invalid')
        tx=envelope(s['signed_wire'].removeprefix('0x'));gas=r['gas_used'];base=int(s['fixture']['execution_environment']['base_fee']);price=tx['cap'] if tx['tip'] is None else min(tx['cap'],base+tx['tip']);charge=gas*price+int(r['l1_fee'])+int(r['operator_fee'])
        require(gas<=tx['limit'],'Original Router exceeded signed gas')
        data=tx['data'];require(len(data)>=68,'Original Router calldata truncated');requested=int.from_bytes(data[4:36],'big');boundary=int.from_bytes(data[36:68],'big')
        require((int(r['spent'])==requested and int(r['bought'])>=boundary) if s['id'].endswith('-in') else (int(r['bought'])==requested and int(r['spent'])<=boundary),'Original Router amount constraints differ')
        attributes=bytes.fromhex(sources[inputs.index(s)]['request']['op_user']['attributes_calldata'].removeprefix('0x'))
        require(int(r['operator_fee'])==gas*int.from_bytes(attributes[164:168],'big')*100+int.from_bytes(attributes[168:176],'big'),'Original Router operator units changed')
        credits={a:0 for a in before}
        for owner,n in [(s['fixture']['execution_environment']['coinbase'].lower(),gas*(price-base)),('0x4200000000000000000000000000000000000019',gas*base),('0x420000000000000000000000000000000000001a',int(r['l1_fee'])),('0x420000000000000000000000000000000000001b',int(r['operator_fee']))]:credits[owner]+=n
        for owner in before:
            require(set(after[owner]['storage'])==set(before[owner]['storage']) and all(isinstance(v,str) and len(v)==66 and v.startswith('0x') and 0<=int(v,16)<2**256 for v in after[owner]['storage'].values()),'Original Router storage coverage differs')
            require(after[owner]['code']==before[owner]['code'],'Original Router code changed')
            require(after[owner]['nonce']==before[owner]['nonce']+int(owner==s['caller'].lower()),'Original Router account nonce differs')
            require(int(after[owner]['balance'])==int(before[owner]['balance'])+credits[owner]-(charge if owner==s['caller'].lower() else 0),'Original Router fee role differs')


def prepare(directory):
    directory=Path(directory).resolve();execution=directory/'execution/vectors.json';refs=execution_write(execution);refs[str(execution.relative_to(ROOT))]=digest(execution)
    old=json.loads(PRIOR.read_bytes());jovian=ROOT/next(n for n in old['reference_inputs'] if '/execution/vectors.json' in n and '/karst-precompiles-check/original-runs/' in n)
    sources=directory/'karst-api-sources.json'
    env=dict(os.environ,PATH=str(Path.home()/'.cargo/bin')+os.pathsep+os.environ['PATH'],XONE_OP_JOVIAN_VECTORS=str(jovian),XONE_OP_KARST_API_INPUTS=str(sources),RUST_TEST_THREADS='1')
    env.setdefault('CARGO_TARGET_DIR',str(ROOT/'integration/.runtime/design-review-target'));env.setdefault('CARGO_BUILD_JOBS','2')
    if sys_platform_darwin():
        import platform
        env.setdefault('CARGO_TARGET_'+('AARCH64' if platform.machine()=='arm64' else 'X86_64')+'_APPLE_DARWIN_RUNNER',str(ROOT/'integration/tools/ci/native-test-runner.py'))
    result=subprocess.run(['cargo','+1.90.0','test','--locked','--manifest-path','dex-router/Cargo.toml','-p','xone-simulation','--test','op_bound','export_original_op_bound_inputs_for_karst_normal_api_sources','--','--include-ignored','--exact'],cwd=ROOT,env=env,capture_output=True,text=True,timeout=180)
    log=directory/'export.log';log.write_text(result.stdout+result.stderr);require(result.returncode==0 and 'test export_original_op_bound_inputs_for_karst_normal_api_sources ... ok' in result.stdout,'Karst source export failed: '+result.stderr[-1500:])
    source_rows=json.loads(sources.read_bytes());inputs=inputs_from(source_rows);binary=execution.parent/'runner/runner'
    result=subprocess.run([str(binary)],input=json.dumps(inputs),capture_output=True,text=True,timeout=120)
    (directory/'bound.stderr').write_text(result.stderr);require(result.returncode==0,'Original full Karst Router failed: '+result.stderr[-2000:]);(directory/'bound.raw.json').write_text(result.stdout);rows=json.loads(result.stdout);audit(inputs,rows,source_rows)
    bound=directory/'bound-vectors.json';bound.write_text(json.dumps(dict(passed=True,inputs=inputs,cases=rows,source_sha256=digest(sources),execution_vector_sha256=digest(execution),scope='FIXED_FULL_ORIGINAL_KARST_ROUTER_LOCAL_BANK_NOT_TRUSTED_BANK_OR_CONSENSUS'),indent=2)+'\n')
    for p in [sources,log,bound,directory/'bound.stderr',directory/'bound.raw.json']:refs[str(p.relative_to(ROOT))]=digest(p)
    return dict(references=refs,execution=execution,sources=sources,bound=bound,binary=binary,jovian=jovian)


def sys_platform_darwin():
    import sys
    return sys.platform=='darwin'


def replay(directory,output):
    directory=Path(directory);v=json.loads((directory/'bound-vectors.json').read_bytes());sources=json.loads((directory/'karst-api-sources.json').read_bytes())
    require(digest(directory/'karst-api-sources.json')==v['source_sha256'] and digest(directory/'execution/vectors.json')==v['execution_vector_sha256'],'Original Karst bound source changed')
    result=subprocess.run([str(directory/'execution/runner/runner')],input=json.dumps(v['inputs']),capture_output=True,text=True,timeout=120);Path(output).with_suffix('.stderr').write_text(result.stderr);require(result.returncode==0,'Original Karst Router replay failed');rows=json.loads(result.stdout);require(rows==v['cases'],'Original Karst Router replay changed');audit(v['inputs'],rows,sources)
    Path(output).write_text(json.dumps(dict(passed=True,cases=4,bound_sha256=digest(directory/'bound-vectors.json')))+'\n')


if __name__=='__main__':
    import sys
    if len(sys.argv)==4 and sys.argv[1]=='--replay':replay(sys.argv[2],sys.argv[3])
    else:prepare(sys.argv[1])
