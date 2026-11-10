#!/usr/bin/env python3
"""Strict finite EVM original-pool reference acceptance, independent of report flags.

This scope proves the retained public state/source/bytecode/net-recipient reference.
It does not claim Go SPEC3, Parser/Router differential, consensus or current native
hardfork fee equivalence; those consumers receive raw native replay inputs.
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import tempfile
sys.path.insert(0,str(Path(__file__).resolve().parent))
from r01_public_catalog import ROOT,CatalogError,require,load_json,read_blob,json_blob,sha256
from r01_public_catalog_evm_history import replay

BASE=ROOT/'integration/test/source/deployment-catalog/evm-continuation-v2'

def row_key(row):
    return tuple(row[k] for k in ('id','chain','protocol','factory','pool','factory_runtime_sha256','pool_runtime_sha256','implementation_runtime_sha256'))+(row['anchor']['height'],row['anchor']['hash'])

def scope_errors(rows,required):
    got=[row_key(r) for r in rows];want=[row_key(r) for r in required];errors=[]
    if not rows or not required:errors.append('EMPTY_EVM_REFERENCE_SCOPE')
    if len(got)!=len(set(got)):errors.append('DUPLICATE_EVM_REFERENCE')
    if len(want)!=len(set(want)):errors.append('DUPLICATE_EVM_REQUIRED_REFERENCE')
    errors.extend('MISSING_EVM_REFERENCE:'+str(k) for k in set(want)-set(got));errors.extend('UNDECLARED_EVM_REFERENCE:'+str(k) for k in set(got)-set(want));return errors

def validate_execution(row,execution):
    require(execution['chain']==row['chain'] and execution['chain_id']==row['chain_id'],'EXECUTION_CHAIN_ID')
    require(execution['anchor']['height']==row['anchor']['height'] and execution['anchor']['hash']==row['anchor']['hash'],'EXECUTION_FIXED_H')
    require(execution['public_transactions_sent']==0 and execution['original_chain_mined_transaction'] is False,'EXECUTION_SCOPE_AUTHORITY')
    require(execution['current_native_VM_equivalence_claimed'] is False and execution['local_hardfork']=='shanghai','EXECUTION_NATIVE_VM_EQUIVALENCE_NOT_PROVEN')
    for key in ['balance_overrides','storage_overrides','code_overrides']:require(execution[key]==[],'EXECUTION_STATE_OVERRIDE:'+key)
    cases=[c for c in execution['cases'] if c['catalog_deployment_id']==row['id']];require(len(cases)==1,'EXECUTION_CASE_INVENTORY');case=cases[0]
    require(case['factory']==row['factory'] and case['pool']==row['pool'] and case['pool_runtime_sha256']==row['pool_runtime_sha256'],'EXECUTION_ORIGINAL_POOL_BINDING')
    require(case['passed'] is True and case['original_chain_mined_transaction'] is False,'ORIGINAL_POOL_EXECUTION_FAILED')
    require(int(case['recipient_after'])-int(case['recipient_before'])==int(case['actual_net_recipient'])>0,'ACTUAL_NET_RECIPIENT')
    require(case['sender_input_before']==case['sender_input_after'],'ORIGINAL_WRAPPED_INPUT_CONSUMPTION')
    require(case['recipient']==execution['recipient'],'EXECUTION_RECIPIENT_BINDING')
    transactions=case['transactions'];require(len(transactions)==4,'EXECUTION_LOCAL_SETUP_INVENTORY')
    nonce=int(transactions[0]['transaction']['nonce'],16);previous=None
    for i,item in enumerate(transactions):
        tx,receipt,header,before=item['transaction'],item['receipt'],item['header'],item['before_header']
        require(tx['from'].lower()==execution['sender'],'EXECUTION_PUBLIC_SENDER')
        require(int(tx['nonce'],16)==nonce+i,'EXECUTION_NONCE_CONTINUITY')
        require(int(receipt['status'],16)==1 and item['status']==1,'EXECUTION_RECEIPT_STATUS')
        require(tx['hash']==receipt['transactionHash'] and tx['blockHash']==receipt['blockHash']==header['hash'],'EXECUTION_TRANSACTION_RECEIPT_HASH')
        require(header['parentHash']==before['hash'] and int(header['number'],16)==int(before['number'],16)+1,'EXECUTION_LOCAL_HEADER_CONTINUITY')
        if previous:require(before['hash']==previous['hash'],'EXECUTION_LOCAL_SETUP_CONTINUITY')
        previous=header
    require(transactions[0]['receipt']['contractAddress'].lower()==case['local_helper'],'LOCAL_HELPER_DEPLOYMENT')
    require(transactions[1]['transaction']['to']==case['token_in'] and transactions[1]['transaction']['input']=='0xd0e30db0' and int(transactions[1]['transaction']['value'],16)==int(case['amount_in']),'ORIGINAL_WRAPPED_DEPOSIT')
    require(transactions[2]['transaction']['to']==case['token_in'] and transactions[2]['transaction']['input'].startswith('0x095ea7b3'),'ORIGINAL_TOKEN_APPROVAL')
    require(transactions[3]['transaction']['to']==case['local_helper'],'LOCAL_CALLBACK_TARGET')
    return case

def environment_differences(directory):
    anchor=load_json(directory/'anchor.json');original=json_blob(ROOT,anchor['header'])['response']['result'];execution=load_json(directory/'fork-execution.json');cases=[]
    require(not execution['environment_opcode_counts'].get('BLOCKHASH'),'UNBOUND_ORIGINAL_BLOCKHASH_ENVIRONMENT')
    for c in execution['cases']:
        if not c['transactions']:continue
        local=c['transactions'][0]['before_header'];diff={k:{'original':original.get(k),'local':local.get(k)} for k in sorted(set(original)|set(local)) if k not in ['transactions','withdrawals','uncles'] and original.get(k)!=local.get(k)}
        cases.append({'id':c['catalog_deployment_id'],'first_local_preparation_header_differences':diff,'local_helper':c.get('local_helper'),'original_pool':c['pool'],'actual_net_recipient':c.get('actual_net_recipient')})
    return {'chain':execution['chain'],'anchor':execution['anchor'],'local_hardfork':'shanghai','local_coinbase':'0x000000000000000000000000000000000000cafe','local_block_gas_limit':30000000,'gas_price':execution['gas_price'],'local_helper_newly_deployed':True,'state_overrides':{'balance':[],'code':[],'storage':[]},'environment_opcode_counts':execution['environment_opcode_counts'],'cases':cases,'original_native_block_transactions_replayed':False,'local_world_state_commitments':'Ganache sparse overlay; local hashes/state roots are not original chain full-state commitments','native_fee_rules_equivalence_claimed':False,'go_spec3_or_parser_router_differential_claimed':False,'public_transactions_sent':0}

def verify_inventory(manifest):
    directory=ROOT/manifest['data_directory'];require(directory.resolve()==BASE.resolve(),'EVM_MANIFEST_DIRECTORY')
    require(manifest['excluded_inventory_files']==['manifest-v2.json'],'EVM_INVENTORY_EXCLUSION_POLICY')
    expected={r['path'] for r in manifest['files']};require(expected and len(expected)==len(manifest['files']),'EVM_INVENTORY_DUPLICATE_OR_EMPTY');actual={str(p.relative_to(ROOT)) for p in directory.rglob('*') if p.is_file() and str(p.relative_to(directory)) not in manifest['excluded_inventory_files']};require(expected==actual,'EVM_INVENTORY_MEMBERSHIP')
    for ref in manifest['files']+manifest['tools']+manifest['external_inputs']:read_blob(ROOT,ref)
    return True

def node_result(script,directory,evidence=None):
    with tempfile.TemporaryDirectory(prefix='r01-public-evm-verify-',dir=ROOT/'integration/artifacts/local') as tmp:
        output=Path(tmp)/'output.json';command=['node',str(ROOT/'integration/tools/acceptance'/script),str(directory),str(output)]
        if evidence:command.append(evidence)
        p=subprocess.run(command,capture_output=True,text=True,timeout=300);require(p.returncode==0,'EVM_INDEPENDENT_VERIFIER_FAILED:'+script+':'+p.stderr[-1800:]);return load_json(output)

def observed_finality(row):
    anchor=json_blob(ROOT,row['inputs']['anchor']);header=json_blob(ROOT,anchor['header']);height=row['anchor']['height'];block_hash=row['anchor']['hash']
    require(header['response']['result']['number']==hex(height) and header['response']['result']['hash']==block_hash,'ANCHOR_RAW_HEADER')
    if row['inputs'].get('subsequent_finality'):
        finality=json_blob(ROOT,row['inputs']['subsequent_finality']);ref=finality['subsequent_finalized_header'];f=json_blob(ROOT,ref);require(f['params'][0]=='finalized','FINALITY_ACTUAL_RPC_TAG')
        c=json_blob(ROOT,finality.get('independent_canonical_witness',finality.get('canonical_header')));require(c['params'][0]==hex(height) and c['response']['result']['hash']==block_hash,'SUBSEQUENT_CANONICAL_FIXED_H')
        return int(f['response']['result']['number'],16)>=height
    require(header['params'][0]=='finalized','FINALIZED_ANCHOR_OBSERVATION_REQUIRED');return True

def verify_catalog(catalog,scope,run_replay):
    require(catalog.get('schema_version')==2 and scope.get('schema_version')==2,'EVM_REFERENCE_SCHEMA');rows=catalog['entries'];errors=scope_errors(rows,scope['required']);results=[];bundles={}
    for row in rows:
        result={k:row[k] for k in ['id','chain','protocol','factory','pool','anchor']};missing=[]
        try:
            require(row['chain_key']=='evm:'+str(row['chain_id']),'REFERENCE_CHAIN_KEY')
            for ref in row['inputs'].values():
                if isinstance(ref,dict) and 'path' in ref:read_blob(ROOT,ref)
            identity=json_blob(ROOT,row['genesis_observation']);require(identity['method']=='eth_getBlockByNumber' and identity['params'][0]=='0x0' and identity['response']['result']['hash']==row['genesis_hash'],'PUBLIC_GENESIS_FINGERPRINT')
            execution=json_blob(ROOT,row['inputs']['execution']);case=validate_execution(row,execution)
            directory=(ROOT/row['directory']).resolve();require(directory.is_relative_to(BASE.resolve()),'OWNED_EVM_BUNDLE')
            for key,name in [('execution','fork-execution.json'),('anchor','anchor.json'),('plan','execution-plan.json'),('rpc_index','rpc-index.json'),('local_helper_compilation','local-helper-compiler.json')]:
                require((ROOT/row['inputs'][key]['path']).resolve()==directory/name,'BUNDLE_INPUT_PATH_BINDING:'+key)
            for key in ['source_runtime','environment','prestate']:
                if row['inputs'].get(key):require((ROOT/row['inputs'][key]['path']).resolve().parent==directory,'BUNDLE_INPUT_DIRECTORY_BINDING:'+key)
            if str(directory) not in bundles:
                actual_source=node_result('r01_public_catalog_evm_sources.cjs',directory) if run_replay else None
                proof=None;proof_error=None
                if row['inputs'].get('prestate') and run_replay:
                    try:proof=node_result('r01_public_catalog_evm_commitments.cjs',directory,Path(row['inputs']['prestate']['path']).name)
                    except CatalogError as e:proof_error=str(e)
                actual_execution=replay(directory) if run_replay else None
                bundles[str(directory)]={'source':actual_source,'proof':proof,'proof_error':proof_error,'execution':actual_execution}
            bundle=bundles[str(directory)];source=next((e for e in (bundle['source'] or {}).get('entries',[]) if e['id']==row['id']),None)
            if not run_replay:missing.append('ACTUAL_OFFLINE_COMPILER_MPT_EXECUTION_REPLAY_REQUIRED')
            else:
                require(source is not None,'SOURCE_REPLAY_ENTRY');require(source.get('source_code_execution_bound') and source.get('factory_pool_identity_bound'),'EXACT_SOURCE_FACTORY_POOL_EXECUTION_BINDING:'+str(source.get('error')))
                require(source['factory_rebuild']['runtime_sha256']==row['factory_runtime_sha256'],'DECLARED_FACTORY_RUNTIME_VERSION')
                require(source.get('proxy',{}).get('runtime_sha256')==row['implementation_runtime_sha256'],'DECLARED_IMPLEMENTATION_RUNTIME_VERSION')
                if row['inputs'].get('prestate'):
                    public=json_blob(ROOT,row['inputs']['prestate']);sender=next(d for d in public['deployments'] if d['address'].lower()==execution['sender'])
                    require(int(case['transactions'][0]['transaction']['nonce'],16)==int(sender['proof']['nonce'],16),'PROVEN_ORIGINAL_SENDER_NONCE')
                    require(int(case['sender_native_before'],16)==int(sender['proof']['balance'],16),'PROVEN_ORIGINAL_NATIVE_BALANCE')
                recorded=json_blob(ROOT,row['inputs']['source_runtime']);record=next(e for e in recorded['entries'] if e['id']==row['id'])
                for section in ['factory_rebuild','pool_rebuild']:
                    for field in ['compiler_sha256','source_files','settings','metadata','runtime_sha256','immutable_bindings','fully_qualified_name','creation_bytecode']:
                        require(source[section].get(field)==record[section].get(field),'SOURCE_REPLAY_REPORT_DIFFERENCE:'+section+':'+field)
                require(bundle['execution']['public_transactions_sent']==0 and bundle['execution']['network_requests']==0,'OFFLINE_EXECUTION_READ_ONLY')
                actual_case=next(c for c in bundle['execution']['cases'] if c['id']==row['id']);require(actual_case['passed'] is True and actual_case['actual_net_recipient']==case['actual_net_recipient'],'REPLAYED_ACTUAL_NET_RECIPIENT_BINDING')
                proof=bundle['proof']
                if proof:
                    require(proof['height']==row['anchor']['height'] and proof['chain_id']==str(row['chain_id']) and proof['header']['hash']==row['anchor']['hash'],'STATE_NATIVE_HEADER_FIXED_H')
                    require(proof['native_body_transactions_verified'],'NATIVE_TRANSACTION_BODY_REQUIRED');result.update(state_commitments_verified=True,accounts=proof['accounts'],storage_slots=proof['storage_slots'])
                else:missing.append('FIXED_H_COMPLETE_STATE_PROOF_MISSING'+(':'+bundle['proof_error'] if bundle['proof_error'] else ''))
                result.update(exact_source_runtime_verified=True,factory_pool_identity_bound=True,actual_net_recipient=case['actual_net_recipient'],token_out=case['token_out'],original_pool_execution_replayed=True,network_requests=0)
            if not observed_finality(row):missing.append('SUBSEQUENT_FINALIZED_HEIGHT_NOT_YET_COVERING_H')
            expected_env=json_blob(ROOT,row['inputs']['environment']);require(expected_env==environment_differences(directory),'EXPLICIT_ENVIRONMENT_DIFFERENCES')
        except (CatalogError,ValueError,TypeError,KeyError,OSError,subprocess.SubprocessError) as error:errors.append(row['id']+':'+str(error));missing.append('INVALID_EVM_REFERENCE_BINDING:'+str(error))
        result.update(status='QUALIFIED_FIXED_H_POOL_REFERENCE' if not missing else 'NOT_QUALIFIED',missing=missing,original_chain_mined_transaction=False,current_native_hardfork_equivalence=False,go_spec3_independent_replay_verified=False,normal_parser_router_differential_verified=False);results.append(result)
    return {'schema_version':2,'structurally_valid':not errors,'qualified_declared_reference_scope':not errors and bool(results) and all(r['status']=='QUALIFIED_FIXED_H_POOL_REFERENCE' for r in results),'errors':errors,'entries':results,'full_R01_qualified':False,'qualification_boundary':'finite original-pool reference only; Go SPEC3, normal Parser/Router differential and consensus remain separate consumers'}

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--manifest',type=Path,default=BASE/'manifest-v2.json');p.add_argument('--catalog',type=Path,default=BASE/'catalog-v2.json');p.add_argument('--scope',type=Path,default=BASE/'scope-v2.json');p.add_argument('--replay',action='store_true');p.add_argument('--require-qualified',action='store_true');p.add_argument('--out',type=Path);a=p.parse_args()
    try:
        manifest=load_json(a.manifest);verify_inventory(manifest);require(str(a.catalog.resolve().relative_to(ROOT)) in {r['path'] for r in manifest['files']} and str(a.scope.resolve().relative_to(ROOT)) in {r['path'] for r in manifest['files']},'EVM_INVENTORY_SCOPE');result=verify_catalog(load_json(a.catalog),load_json(a.scope),a.replay);result['byte_inventory_verified']=True
    except (CatalogError,ValueError,KeyError,OSError) as e:result={'schema_version':2,'structurally_valid':False,'errors':[str(e)],'qualified_declared_reference_scope':False}
    if a.out:a.out.write_text(json.dumps(result,indent=2)+'\n')
    print(json.dumps(result,indent=2));raise SystemExit(0 if result['structurally_valid'] and (not a.require_qualified or result['qualified_declared_reference_scope']) else 1)
if __name__=='__main__':main()
