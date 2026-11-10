#!/usr/bin/env python3
"""Authenticate the whole native shared bank, then project exact getter values.

No swap/fee/interpolation output is calculated here. Only integer encoding and
observed supply bookkeeping are reused from the reviewed single-pool projector.
Profile selection is explicit: a requested legacy pool never receives field40.
"""
import argparse
import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile

import managed_v2_projection as base
import managed_v2_scheduled_projection as single

ROOT=Path(__file__).resolve().parents[3]
CLI=ROOT/'integration/tools/ci/managed-v2-scheduled-shared-golden.cjs'
FILENAME='managed-v2-scheduled-shared-pools.json'
require=base.require


class _Bank(dict):
    """Whole-bank qualification receipt; ordinary parsed dictionaries lack it."""


class _ObservationView(dict):
    """Read-only extraction context; this type conveys no independent authority."""


def load(path, original_input=None):
    path=Path(path).resolve()
    raw=path.read_bytes()
    result=subprocess.run(['node',str(CLI),'--validate','--input',str(path)],capture_output=True,text=True,check=False)
    require(result.returncode==0,'shared scheduled original qualification failed: '+result.stderr[-4000:])
    require(path.read_bytes()==raw,'shared original changed during qualification')
    bank=_Bank(json.loads(raw,object_pairs_hook=base._pairs))
    prefix_path=Path(original_input) if original_input else path.with_name('managed-v2-aum.json')
    prefix=base.load(prefix_path)
    templates={i['label']:copy.deepcopy(i) for i in prefix._identities}
    pools=[i for i in prefix._identities if i['artifact']['name']=='ManagedPool']
    require(len({i['template_sha256'] for i in pools})==1,'original compiler pool template ambiguity')
    templates['ManagedPool']=copy.deepcopy(pools[0])
    original_ids={i['address']:i['code_sha256'] for i in prefix._identities}
    identities={mode:{**original_ids,**{i['address']:i['runtime_sha256'] for i in branch['new_identities']}}
                for mode,branch in bank['branches'].items()}
    bank._templates=templates
    bank._identities=identities
    bank._seal=base._digest(bank)
    bank._metadata_seal=base._digest([templates,identities])
    bank._raw={'filename':path.name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),
               'original_aum_sha256':base.BANK_SHA256}
    bank._raw_seal=base._digest(bank._raw)
    return bank


def _qualified(bank):
    require(isinstance(bank,_Bank),'qualified native shared bank required')
    require(base._digest(bank)==bank._seal,'shared original changed after seal')
    require(base._digest([bank._templates,bank._identities])==bank._metadata_seal,'shared identity metadata changed after seal')
    require(base._digest(bank._raw)==bank._raw_seal,'shared raw identity changed after seal')


def _view(bank, branch, config):
    key=branch['mode']+'/'+config['name']
    view=_ObservationView(branches={key:{'configuration':{**config,'factory':branch['factory'],
        'vault':branch['vault'],'tokens':branch['tokens'],'roles':branch['roles']}}})
    view._templates=bank._templates
    view._runtime_ids={key:bank._identities[branch['mode']]}
    return key,view


def _derive(before, after):
    result=copy.deepcopy(after)
    m=result['multi_asset']['managed_v2']
    m['execution_context']={k:m[k] for k in ('block_number','timestamp')}
    for k in ('block_number','timestamp','block_hash'):
        m[k]=before['multi_asset']['managed_v2'][k]
    return result


def _project_observation_pools(bank, namespace, row):
    """Map retained observation fields only; caller establishes their authority."""
    branch=bank['branches'][namespace]
    records=[]
    for raw in row['pools']:
        config=next(p for p in branch['pools'] if p['name']==raw['name'])
        key,view=_view(bank,branch,config)
        state={**raw['state'],'id':row['id']+'/'+raw['name'],'accounts':row['accounts']}
        pool=single._pool(view,key,state)
        requested=config['requested_protocol']
        require(requested in (single.PROTOCOL,base.PROTOCOL),'unknown requested shared source profile')
        if requested==base.PROTOCOL:
            m=pool['multi_asset']['managed_v2']
            require(m['gradual_weight_end']<=m['timestamp'] and m['gradual_fee_end']<=m['timestamp'],
                    'legacy source cannot promote active schedule')
            pool['protocol'],pool['adapter_version']=base.PROTOCOL,base.ADAPTER
            del m['schedule']
        public_config=base._config(pool)
        public_config['protocol']=pool['protocol']
        records.append({'name':config['name'],'pool':pool,'config':public_config})
    return records


def project(bank):
    _qualified(bank)
    projected={}
    for row in bank['banks']:
        records=_project_observation_pools(bank,row['namespace'],row)
        projected[row['id']]={'namespace':row['namespace'],'header':copy.deepcopy(row['header']),'pools':records}
    cases=[]
    for raw in bank['cases']:
        before=projected[raw['before']]['pools']
        observed=projected[raw['after']]['pools']
        after_header=projected[raw['after']]['header']
        params={'execution_block':int(after_header['number'],16),'execution_timestamp':int(after_header['timestamp'],16)}
        if raw['operation']=='swap':
            params.update(amount=raw['amount_in'],index_in=raw['index_in'],index_out=raw['index_out'])
        cases.append({'id':raw['id'],'namespace':raw['namespace'],'operation':raw['operation'],
            'pool_name':raw['pool_name'],'before':raw['before'],'after':raw['after'],
            'transaction':raw['transaction'],'params':params,'expected_native':copy.deepcopy(raw['expected_native']),
            'expected_pools_after':[_derive(a['pool'],b['pool']) for a,b in zip(before,observed)],
            'authority':'complete native observed common-H after bank; original before source retained with explicit execution context'})
    producers=[Path(__file__).resolve(),Path(single.__file__).resolve(),Path(base.__file__).resolve()]
    return {'schema_version':1,'authority':'authenticated original multi-pool getter/raw projection; no pricing outputs computed',
            'original_raw':copy.deepcopy(bank._raw),
            'projection_inputs':[{'path':str(p.relative_to(ROOT)),'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} for p in producers],
            'banks':projected,'cases':cases,'checkpoints':copy.deepcopy(bank['checkpoints']),'orders':copy.deepcopy(bank['orders'])}


def project_selected_report(bank, report):
    """Map controlled original-VM observations; never compute expected prices.

    The smoke obtains this report directly from its hash-bound Node executor.
    This mapper is not a substitute for executing or authenticating that program.
    """
    _qualified(bank)
    require(report.get('passed') is True and report.get('vm_started') is True,
            'actual selected original VM did not pass')
    require(report.get('original_shared_sha256')==bank._raw['sha256'],'selected VM original identity differs')
    require(report.get('production_ready') is False and report.get('all_requested_features_complete') is False,
            'selected VM scope promoted')
    executions=report.get('executions')
    require(type(executions) is list and 0<len(executions)<=64,'bounded actual VM executions required')
    sources={b['id']:b for b in bank['banks']}
    vectors=[]
    for execution in executions:
        require(execution.get('passed') is True and execution.get('source_restored') is True,'selected source was not restored')
        namespace=execution['namespace']
        source=sources[execution['source_bank']]
        require(source['namespace']==namespace,'selected source namespace differs')
        observation=execution['source']
        for key in ('header','header_anchor_after','pools','global_wallets','global_wallet_getters','accounts','storage'):
            require(observation[key]==source[key],'selected VM source differs: '+key)
        origin=_project_observation_pools(bank,namespace,observation)
        require(len(execution['routes'])==2,'actual selected two-route execution required')
        steps=[]
        current=observation
        for index,route in enumerate(execution['routes']):
            require(route['index']==index and route['before']==current,'selected route ordering/source changed')
            after=route['after']
            observed=_project_observation_pools(bank,namespace,after)
            request=execution['request']
            selected=next(row['pool'] for row in origin if row['name']==route['pool_name'])
            assets=[a['token'] for a in selected['multi_asset']['assets']]
            require(route['actual_amount_in']==route['quoted_amount_in'] and route['actual_amount_out']==route['quoted_amount_out'],
                    'actual selected route price/input mismatch')
            steps.append({'pool_name':route['pool_name'],'before':execution['source_bank'],'operation':'swap',
                'params':{'amount':route['actual_amount_in'],'index_in':assets.index(request['sellToken']),
                    'index_out':assets.index(request['buyToken']),'execution_block':int(after['header']['number'],16),
                    'execution_timestamp':int(after['header']['timestamp'],16)},
                'expected_native':{'actual_amount_in':route['actual_amount_in'],'actual_amount_out':route['actual_amount_out']},
                'expected_pools_after':[_derive(a['pool'],b['pool']) for a,b in zip(origin,observed)]})
            current=after
        require(execution['complete_native_after']==current,'selected final native bank differs')
        vectors.append({'id':execution['id'],'namespace':namespace,'source_bank':execution['source_bank'],
            'source_pools':[copy.deepcopy(r['pool']) for r in origin],'steps':steps,
            'actual_amount_in':execution['actual_amount_in'],'actual_amount_out':execution['actual_amount_out']})
    require(sum(len(v['steps']) for v in vectors)==report['selected_transactions_executed'],'selected transaction inventory differs')
    return {'schema_version':1,'authority':'controlled actual original-VM observations projected without pricing math',
        'original_shared_sha256':bank._raw['sha256'],'native_report_canonical_sha256':base._digest(report),
        'executions':vectors,'selected_transactions':report['selected_transactions_executed']}


def output_path(path, source, original_input=None):
    path=Path(path).absolute()
    require(not path.is_symlink(),'projection output symlink refused')
    if path.exists():
        require(path.is_file() and path.stat().st_nlink==1,'projection output hardlink/nonfile refused')
    canonical=path.resolve()
    protected={Path(source).resolve(),Path(original_input).resolve() if original_input else Path(source).with_name('managed-v2-aum.json').resolve()}
    require(canonical not in protected,'source/original input overwrite refused')
    projects=('integration','chain-parser-go','chain-parser-rust','dex-router')
    registered={ROOT/p/'test/contracts'/FILENAME for p in projects}
    protected_names={p.name for project in projects for p in (ROOT/project/'test/contracts').iterdir() if p.name!=FILENAME}
    temporary=canonical.is_relative_to(Path(tempfile.gettempdir()).resolve()) or canonical.is_relative_to(Path('/tmp').resolve())
    require(canonical in registered or temporary and not canonical.is_relative_to(ROOT)
            and canonical.suffix=='.json' and canonical.name not in protected_names,'shared projection output boundary')
    return canonical


def main():
    parser=argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--input',required=True,type=Path)
    parser.add_argument('--original-input',type=Path)
    group=parser.add_mutually_exclusive_group(required=True)
    group.add_argument('--output',type=Path)
    group.add_argument('--check',type=Path)
    args=parser.parse_args()
    destination=output_path(args.output,args.input,args.original_input) if args.output else None
    value=project(load(args.input,args.original_input))
    data=(json.dumps(value,indent=2,ensure_ascii=False)+'\n').encode()
    if args.check:
        require(args.check.read_bytes()==data,'shared scheduled Pool projection differs')
    else:
        output_path(args.output,args.input,args.original_input)
        fd,temp=tempfile.mkstemp(prefix=destination.name+'.',suffix='.tmp',dir=destination.parent)
        try:
            with os.fdopen(fd,'wb') as stream:stream.write(data)
            Path(temp).replace(destination)
        finally:Path(temp).unlink(missing_ok=True)
    print(json.dumps({'passed':True,'banks':len(value['banks']),'physical_pools':sum(len(b['pools']) for b in value['banks'].values()),
        'cases':len(value['cases']),'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest(),'original_raw_sha256':value['original_raw']['sha256']}))


if __name__=='__main__':
    main()
