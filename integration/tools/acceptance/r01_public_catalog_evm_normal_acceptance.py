#!/usr/bin/env python3
"""Strict six-case declaration and offline normal-process replay entry point.

Saved flags never make the declared scope qualified. --replay executes MPT
getters and normal binaries again, with no public network or typed baselines.
Expected real rejections remain NOT_QUALIFIED and keep the aggregate false.
"""
import argparse
import json
from pathlib import Path
import sys
sys.path.insert(0,str(Path(__file__).resolve().parent))
from r01_public_catalog import ROOT,load_json,read_blob,require,sha256
from r01_public_catalog_capture import reference
from r01_public_catalog_evm_normal import BASE,IDS,execute,validate_probe,validate_rejection

def scope_errors(required,entries):
    want=[r['id'] for r in required];got=[r['id'] for r in entries];errors=[]
    if len(want)!=6 or set(want)!=IDS:errors.append('NORMAL_EXPECTED_SIX_SCOPE')
    if len(got)!=len(set(got)):errors.append('NORMAL_DUPLICATE_CASE')
    errors.extend('NORMAL_MISSING_CASE:'+x for x in sorted(set(want)-set(got)))
    errors.extend('NORMAL_UNDECLARED_CASE:'+x for x in sorted(set(got)-set(want)))
    return errors

def sealed_check():
    seal=load_json(BASE/'sealed-inputs.json');directory=ROOT/seal['directory'];expected={r['path'] for r in seal['files']};actual={str(p.relative_to(ROOT)) for p in directory.rglob('*') if p.is_file()};require(actual==expected,'OLD_SEALED_INVENTORY_MEMBERSHIP')
    for r in seal['files']+seal['manifests']:read_blob(ROOT,r)
    return len(seal['files'])

def evaluate(required,entries,replay_directory=None,parser_bin=None,router_bin=None):
    errors=scope_errors(required,entries);results=[];binary_pairs=set()
    if errors:return {'structurally_valid':False,'errors':errors,'qualified_declared_scope':False,'entries':[]}
    expected={r['id']:r for r in required}
    for entry in entries:
        id_=entry['id'];row=expected[id_]
        try:
            bound=json.loads(read_blob(ROOT,entry['bound_report']));normal=json.loads(read_blob(ROOT,bound['normal_process_report']));probe=json.loads(read_blob(ROOT,bound['getter_execution']));input_=json.loads(read_blob(ROOT,probe['input']));actual=json.loads(read_blob(ROOT,bound['actual_getter_reexecution']));original=json.loads(read_blob(ROOT,bound['original_VM_reference']));case=next(c for c in original['cases'] if c['catalog_deployment_id']==id_)
            require(input_['row']==row and probe['id']==id_ and probe['anchor']==row['anchor'],'NORMAL_SCOPE_FIXED_H_BINDING');require(bound['id']==id_ and bound['anchor']==row['anchor'],'NORMAL_REPORT_CASE');require(actual['calls']==probe['calls'] and actual['missing']==probe['missing'],'NORMAL_ACTUAL_GETTER_REPLAY')
            require(original['anchor']['height']==row['anchor']['height'] and original['anchor']['hash']==row['anchor']['hash'] and case['pool']==row['pool'],'NORMAL_REFERENCE_ANCHOR')
            reject=bool(entry.get('record_getter_rejection',False));(validate_rejection if reject else validate_probe)(probe)
            for binary in normal['binaries'].values():require(sha256((ROOT/binary['pinned']).read_bytes())==binary['sha256'],'NORMAL_PINNED_BINARY_HASH')
            pair=tuple(normal['binaries'][name]['sha256'] for name in ['parser','router']);binary_pairs.add(pair)
            cfg=load_json((ROOT/bound['normal_process_report']['path']).parent/'parser-config.json');require(cfg['service']['name']=='chain-parser-go' and cfg['service']['mode']=='rpc' and len(cfg['chains'])==1 and len(cfg['chains'][0]['pools'])==1,'NORMAL_PRODUCTION_SOURCE_MODE');require(cfg['chains'][0]['pools'][0]['id']==row['pool'],'NORMAL_CONFIG_POOL')
            require(all(not (key in cfg['chains'][0]['pools'][0]) for key in ['reserve0','reserve1','sqrt_price_x96','liquidity']),'NORMAL_TYPED_STATE_INJECTION')
            if replay_directory:
                out=replay_directory/(id_+'-replay');pbin=parser_bin or ROOT/normal['binaries']['parser']['pinned'];rbin=router_bin or ROOT/normal['binaries']['router']['pinned'];new=execute(ROOT/entry['case_directory'],ROOT/bound['getter_execution']['path'],out,pbin,rbin,reject);normal=json.loads(read_blob(ROOT,new['normal_process_report']));bound=new;cfg=load_json((ROOT/new['normal_process_report']['path']).parent/'parser-config.json')
            quote=normal.get('quote',{});q=quote.get('body',{});amount=case['amount_in'];delta=int(case['recipient_after'])-int(case['recipient_before']);require(delta==int(case['actual_net_recipient'])>0,'NORMAL_ORIGINAL_TOKEN_DELTA')
            positive=False;reason=None
            if quote.get('status')==200:
                require(not reject,'NORMAL_UNPROVEN_GETTER_QUOTE');require(normal['parser_baseline']['status']==200,'NORMAL_PARSER_BASELINE_REQUIRED');request=normal['quote_request'];require(request['sellAmount']==amount and request['sellToken']==case['token_in'] and request['buyToken']==case['token_out'],'NORMAL_AMOUNT_INPUT_REFERENCE');ref=q['snapshotRef']['block'];require(ref['number_or_slot']==row['anchor']['height'] and ref['hash']==row['anchor']['hash'] and q['quality']['candidateScope']==[row['pool']],'NORMAL_QUOTE_SOURCE_SCOPE');positive=int(q['grossBuyAmount'])==delta
                if not positive:reason='NORMAL_GROSS_RECIPIENT_DELTA_MISMATCH'
            elif reject:
                require(any(r['method']=='eth_call' and r.get('error') and 'READONLY_MPT_STORAGE_MISSING' in r['error']['message'] for r in normal['rpc_trace']),'NORMAL_REAL_GETTER_REJECTION_REQUIRED');reason='FIXED_H_ADDITIONAL_GETTER_MPT_UNAVAILABLE'
            elif probe.get('normal_admission_rejection'):
                reason=probe['normal_admission_rejection'];require(reason in normal.get('normal_process_errors',{}).get('parser',''),'NORMAL_REAL_PRODUCTION_REJECTION_REQUIRED')
            else:reason='NORMAL_CONSUMER_FAILURE_REQUIRES_REVIEW'
            freshness=bound['offline_freshness_policy'];require(isinstance(freshness,dict) and not freshness['original_header_timestamp_rewritten'] and not freshness['production_freshness_claimed'],'NORMAL_TEST_FRESHNESS_DISCLOSURE');require(cfg['startup']['max_baseline_age_ms']<=max(60000,freshness['test_wall_clock_ms']-freshness['original_header_timestamp_ms']+120000),'NORMAL_TEST_FRESHNESS_BOUND')
            results.append({'id':id_,'chain':row['chain'],'anchor':row['anchor'],'status':'QUALIFIED_FINITE_NORMAL_REFERENCE_DIFFERENTIAL' if positive else 'NOT_QUALIFIED','reason':reason,'amount_in':amount,'router_pool_gross':q.get('grossBuyAmount'),'original_VM_recipient_token_delta':str(delta),'additional_getter_MPT_bound':not reject,'getter_calls':len(probe['calls']),'after_chain_fees':bound['quantities']['after_chain_fees'],'normal_report':bound['normal_process_report'],'bound_report':entry['bound_report'],'runtime_reexecuted_in_this_acceptance':bool(replay_directory),'production_Router_build_execution_verified':False})
        except (ValueError,KeyError,TypeError,OSError,StopIteration) as e:errors.append(id_+':'+str(e))
    require(len(binary_pairs)<=1,'NORMAL_STAGE_BINARY_SET_CHANGED')
    return {'schema_version':4,'structurally_valid':not errors,'errors':errors,'declared_cases':6,'observed_positive':sum(r['status']=='QUALIFIED_FINITE_NORMAL_REFERENCE_DIFFERENTIAL' for r in results),'observed_rejected':sum(r['status']=='NOT_QUALIFIED' for r in results),'qualified_declared_scope':bool(replay_directory) and not errors and len(results)==6 and all(r['status']=='QUALIFIED_FINITE_NORMAL_REFERENCE_DIFFERENTIAL' for r in results),'runtime_reexecuted_in_this_acceptance':bool(replay_directory),'qualification_boundary':'six exact-input original-runtime getter MPT -> normal Go -> normal Rust quote comparisons; HTTP development journal; no current production freshness, native SPEC3, production Router build, after-chain-fees or full R01 claim','full_R01_qualified':False,'entries':results}

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--index',type=Path,default=BASE/'normal-results-index.json');p.add_argument('--replay-directory',type=Path);p.add_argument('--parser-bin',type=Path);p.add_argument('--router-bin',type=Path);p.add_argument('--out',type=Path,required=True);p.add_argument('--require-qualified',action='store_true');a=p.parse_args()
    if a.replay_directory:require(a.replay_directory.resolve().is_relative_to((ROOT/'integration/artifacts/local').resolve()) and a.replay_directory.name.startswith('r01-public-'),'OWNED_STAGE_REPLAY');a.replay_directory.mkdir(parents=True)
    require(a.out.resolve().is_relative_to((ROOT/'integration/artifacts/local').resolve()) and a.out.name.startswith('r01-public-'),'OWNED_STAGE_ACCEPTANCE_OUTPUT');index=load_json(a.index);result=evaluate(load_json(BASE/'stage-scope.json')['required'],index['entries'],a.replay_directory,a.parser_bin,a.router_bin);result['old_sealed_files_unchanged']=sealed_check();a.out.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2));raise SystemExit(0 if result['structurally_valid'] and (not a.require_qualified or result['qualified_declared_scope']) else 1)
if __name__=='__main__':main()
