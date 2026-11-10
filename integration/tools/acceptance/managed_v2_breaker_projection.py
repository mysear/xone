#!/usr/bin/env python3
"""Project qualified original breaker observations. No local pricing oracle."""
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
import managed_v2_scheduled_shared_projection as shared
ROOT=Path(__file__).resolve().parents[3]
CLI=ROOT/'integration/tools/ci/managed-v2-breaker-golden.cjs'
PROTOCOL='balancer-v2-managed-breaker'
ADAPTER='balancer-v2/20230411-managed-breaker-1'
FILENAME='managed-v2-breaker-pools.json'
GETTER_FIELDS=('reference_bpt_price','reference_weight','lower_percentage','upper_percentage','lower_bpt_price_bound','upper_bpt_price_bound')
require=base.require
class _Bank(dict):
    """Complete qualification receipt plus sealed compiler/runtime identities."""
def load(path,original_input=None):
    path=Path(path).resolve();raw=path.read_bytes()
    result=subprocess.run(['node',str(CLI),'--validate','--input',str(path)],capture_output=True,text=True,check=False)
    require(result.returncode==0,'breaker original qualification failed: '+result.stderr[-4000:])
    require(path.read_bytes()==raw,'breaker original changed during qualification')
    bank=_Bank(json.loads(raw,object_pairs_hook=base._pairs))
    prefix=base.load(Path(original_input) if original_input else path.with_name('managed-v2-aum.json'))
    templates={i['label']:copy.deepcopy(i) for i in prefix._identities}
    pools=[i for i in prefix._identities if i['artifact']['name']=='ManagedPool']
    require(len({i['template_sha256'] for i in pools})==1,'original compiler pool template ambiguity')
    templates['ManagedPool']=copy.deepcopy(pools[0])
    original_ids={i['address']:i['code_sha256'] for i in prefix._identities}
    identities={ns:{**original_ids,**{i['address']:i['runtime_sha256'] for i in branch['new_identities']}} for ns,branch in bank['branches'].items()}
    bank._templates=templates;bank._identities=identities
    bank._seal=base._digest(bank);bank._metadata_seal=base._digest([templates,identities])
    bank._raw={'filename':path.name,'bytes':len(raw),'sha256':hashlib.sha256(raw).hexdigest(),'original_aum_sha256':base.BANK_SHA256}
    bank._raw_seal=base._digest(bank._raw)
    return bank

def _qualified(bank):
    require(isinstance(bank,_Bank),'qualified breaker bank required')
    require(base._digest(bank)==bank._seal,'breaker original changed after seal')
    require(base._digest([bank._templates,bank._identities])==bank._metadata_seal,'breaker source metadata changed after seal')
    require(base._digest(bank._raw)==bank._raw_seal,'breaker raw identity changed after seal')

def _project_rows(bank, source_rows):
    rows=[];unsupported=[]
    for row in source_rows:
        ns=row['namespace'];branch=bank['branches'][ns];records=[]
        for raw in row['pools']:
            config=next(p for p in branch['pools'] if p['name']==raw['name'])
            key,view=shared._view(bank,branch,config)
            state={**raw['state'],'id':row['id']+'/'+raw['name'],'accounts':row['accounts']}
            # Reuse unchanged identity/AUM/schedule extraction with the exact
            # authenticated breaker getter values supplied as separate token
            # metadata. Only this old helper's zero-only admission is bypassed
            # on a private extraction view, never on the qualified raw bank.
            extraction=copy.deepcopy(state)
            for g in extraction['getters']:
                if g['to']==config['pool'] and g['method']=='getCircuitBreakerState':g['decoded']=['0']*6
            pool=single._pool(view,key,extraction)
            pool['protocol'],pool['adapter_version']=PROTOCOL,ADAPTER
            m=pool['multi_asset']['managed_v2'];pid=config['pool']
            getters=state['getters']
            def get(method,args=(),address=pid):
                found=[g['decoded'] for g in getters if g['to']==address and g['method']==method and g['args']==list(args)]
                require(found and all(g==found[0] for g in found),'missing/conflicting breaker getter '+method)
                return copy.deepcopy(found[0])
            members=next(a for a in row['actor_allowlists'] if a['pool_name']==raw['name'])['members']
            m['bpt']={'join_exit_enabled':get('getJoinExitEnabled')[0],
                      'minimum_bpt_locked':get('balanceOf',['0x'+'0'*40])[0],
                      'bpt_last_change_block':int(get('getPoolTokenInfo',[pool['multi_asset']['pool_id'],pid],branch['vault'])[2]),
                      'allowlist_members':[{'address':a,'allowed':get('isAddressOnAllowlist',[a])[0]} for a in sorted(members)]}
            circuit=[]
            for index,token in enumerate(branch['tokens']):
                records_raw=[r for r in row['control_storage'] if r['address']==pid and r.get('kind')=='circuit' and r['token']==token]
                require(len(records_raw)==1,'one exact original circuit raw observation')
                word=int(records_raw[0]['value'],16) if records_raw[0]['value']!='0x' else 0
                values=get('getCircuitBreakerState',[token])
                require(len(values)==6,'six original circuit getters')
                circuit.append({'token':token,'state_word':str(word),**dict(zip(GETTER_FIELDS,values))})
                m['token_info'][index]['circuit_breaker_enabled']=word!=0
            m['circuit_breakers']={'tokens':circuit}
            records.append({'name':raw['name'],'pool':pool})
        rows.append({'id':row['id'],'namespace':ns,'header':copy.deepcopy(row['header']),'pools':records})
    return {'schema_version':1,'authority':'authenticated-original-managed-v2-breaker-observations',
            'original_raw':copy.deepcopy(bank._raw),'banks':rows,'cases':copy.deepcopy(bank['cases']),
            'unsupported_pools':unsupported,'production_ready':False,'all_requested_features_complete':False}

def project(bank):
    _qualified(bank)
    return _project_rows(bank, bank['banks'])

def project_selected_report(bank, report_path, selected_input):
    """Revalidate signed original executions before projecting their getters."""
    _qualified(bank)
    require(type(selected_input) is dict,'independently supplied actual API input required')
    selected_seal=base._digest(selected_input)
    path=Path(report_path).resolve();data=path.read_bytes()
    require(len(data)<=512*1024*1024,'selected breaker report budget for the declared larger scenario inventory')
    script="""
const fs=require('node:fs'),P=require('./integration/tools/ci/managed-v2-breaker-plan-replay.cjs'),I=require('./integration/tools/reference/balancer-managed-v2-breaker/inputs.cjs'),C=require('./integration/tools/reference/balancer-managed-v2-breaker/checker.cjs');
const raw=fs.readFileSync('integration/test/contracts/managed-v2-breaker.json'),bank=JSON.parse(raw);C.check(bank);
const report=JSON.parse(fs.readFileSync(process.argv[1])),input=JSON.parse(fs.readFileSync(0));process.stdout.write(JSON.stringify(P.validateReport(report,bank,I.sha(raw),input)));
"""
    qualified=subprocess.run(['node','-e',script,str(path)],input=json.dumps(selected_input,sort_keys=True,separators=(',',':')),capture_output=True,text=True,check=False,cwd=ROOT)
    require(qualified.returncode==0,'selected original breaker report failed qualification: '+qualified.stderr[-4000:])
    require(path.read_bytes()==data,'selected breaker report changed during qualification')
    require(base._digest(selected_input)==selected_seal,'actual API input changed during qualification')
    report=json.loads(data,object_pairs_hook=base._pairs)
    require(report['original_breaker_sha256']==bank._raw['sha256'],'selected original raw identity differs')
    executions=[];count=0
    for execution in report['executions']:
        source=_project_rows(bank,[execution['source']])
        require(not source['unsupported_pools'],'selected source contains unsupported breaker')
        steps=[]
        for route in execution['routes']:
            for step in route['steps']:
                observed=_project_rows(bank,[step['after']]);require(not observed['unsupported_pools'],'selected after contains unsupported breaker')
                steps.append({'pool_id':next(p['pool'] for p in bank['branches'][execution['namespace']]['pools'] if p['name']==step['pool_name']),
                    'token_in':step['token_in'],'token_out':step['token_out'],'amount_in':step['amount_in'],'amount_out':step['amount_out'],
                    'execution_block':int(step['after']['header']['number'],16),'execution_timestamp':int(step['after']['header']['timestamp'],16),
                    'after_pools':[p['pool'] for p in observed['banks'][0]['pools']]})
                count+=1
        executions.append({'id':execution['id'],'request':copy.deepcopy(execution['request']),
            'source_pools':[p['pool'] for p in source['banks'][0]['pools']],'steps':steps})
    require(count==report['selected_transactions_executed'],'selected breaker step inventory')
    return {'schema_version':1,'authority':'authenticated-actual-selected-breaker-API-original-execution','original_raw':copy.deepcopy(bank._raw),
        'native_report_sha256':hashlib.sha256(data).hexdigest(),'selected_transactions':count,'executions':executions,
        'production_ready':False,'all_requested_features_complete':False}

def output_path(path,source,original_input=None):
    # Reuse the output boundary policy with this projector's registered name.
    path=Path(path).absolute()
    require(not path.is_symlink(),'breaker output symlink refused')
    if path.exists():require(path.is_file() and path.stat().st_nlink==1,'breaker output hardlink/nonfile refused')
    canonical=path.resolve();projects=('integration','chain-parser-go','chain-parser-rust','dex-router')
    registered={ROOT/p/'test/contracts'/FILENAME for p in projects}
    protected={Path(source).resolve(),Path(original_input).resolve() if original_input else Path(source).with_name('managed-v2-aum.json').resolve()}
    names={p.name for project in projects for p in (ROOT/project/'test/contracts').iterdir() if p.name!=FILENAME}
    temporary=canonical.is_relative_to(Path(tempfile.gettempdir()).resolve()) or canonical.is_relative_to(Path('/tmp').resolve())
    require(canonical not in protected and (canonical in registered or temporary and not canonical.is_relative_to(ROOT) and canonical.suffix=='.json' and canonical.name not in names),'breaker projection output boundary')
    return canonical

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--input',required=True,type=Path);p.add_argument('--original-input',type=Path)
    group=p.add_mutually_exclusive_group(required=True);group.add_argument('--output',type=Path);group.add_argument('--check',type=Path);args=p.parse_args()
    destination=output_path(args.output,args.input,args.original_input) if args.output else None
    v=project(load(args.input,args.original_input));data=(json.dumps(v,indent=2)+'\n').encode()
    if args.check:require(args.check.read_bytes()==data,'breaker projection differs')
    else:
        output_path(args.output,args.input,args.original_input)
        fd,temp=tempfile.mkstemp(prefix=destination.name+'.',suffix='.tmp',dir=destination.parent)
        try:
            with os.fdopen(fd,'wb') as stream:stream.write(data)
            Path(temp).replace(destination)
        finally:Path(temp).unlink(missing_ok=True)
    print(json.dumps({'passed':True,'banks':len(v['banks']),'cases':len(v['cases']),'bytes':len(data),'sha256':hashlib.sha256(data).hexdigest(),'unsupported_pools':len(v['unsupported_pools'])}))

if __name__=='__main__':main()
