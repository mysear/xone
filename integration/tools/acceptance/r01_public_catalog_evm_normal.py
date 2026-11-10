#!/usr/bin/env python3
"""Six literal EVM profiles: MPT original getter VM -> normal Go -> normal Router.

Configuration carries identities and limits only. No typed baseline/state enters
normal binaries. Original sealed v2/v3 inputs remain unchanged.
"""
import argparse
import json
from pathlib import Path
import subprocess
import sys
import time
sys.path.insert(0,str(Path(__file__).resolve().parent))
from r01_public_catalog import ROOT,load_json,read_blob,require
from r01_public_catalog_capture import reference,write_new
from r01_public_catalog_evm_process import normal_profile,words,call_key,captured_calls,FixedRPC,run as normal_run
BASE=ROOT/'integration/test/source/deployment-catalog/evm-normal-v4'
IDS={'base-uniswap-v3','base-aerodrome-v2','base-aerodrome-slipstream','optimism-uniswap-v3','polygon-uniswap-v3','arbitrum-uniswap-v3'}

def validate_probe(probe):
    require(probe['passed'] is True and not probe['missing'] and probe['calls'],'NORMAL_GETTER_PROOF_INCOMPLETE')
    for c in probe['calls']:
        require(c.get('error') is None and c['all_code_and_storage_MPT_bound'] is True and c['steps']>0,'NORMAL_GETTER_MPT_EXECUTION_REQUIRED')
        require(not c.get('forbidden',[]),'NORMAL_GETTER_PERSISTENT_WRITE')

def pool_config(row,probe):
    pool=row['pool'];successful=[c for c in probe['calls'] if c['error'] is None];require(successful and all(c['all_code_and_storage_MPT_bound'] and c['steps']>0 for c in successful),'NORMAL_CONFIG_GETTERS_UNPROVEN');calls={call_key(c['to'],c['data']):c['value'] for c in successful}
    def value(a,d):return calls[call_key(a,d)]
    token0='0x'+value(pool,'0x0dfe1681')[-40:];token1='0x'+value(pool,'0xd21220a7')[-40:]
    cfg={'id':pool,'protocol':normal_profile(row['protocol']),'token0':token0,'token1':token1,'program_id':'','accounts':[]};identity={'expected_factory':row['factory'],'expected_code_sha256':row['pool_runtime_sha256']}
    if row['protocol']=='aerodrome-v2':
        meta=words(value(pool,'0x392f37e9'));stable=meta[4];require(stable in [0,1],'NORMAL_AERO_STABLE_FLAG');cfg['protocol']='aerodrome-v2-stable' if stable else 'aerodrome-v2-volatile'
        fee='0xcc56b2c5'+pool[2:].rjust(64,'0')+format(stable,'064x');cfg.update(fee_numerator=words(value(row['factory'],fee))[0],fee_denominator=10000,stable=identity|{'expected_factory_code_sha256':row['factory_runtime_sha256'],'expected_implementation':row['implementation_address'],'expected_implementation_code_sha256':row['implementation_runtime_sha256']})
    else:
        cfg.update(fee_numerator=words(value(pool,'0xddca3f43'))[0],fee_denominator=1000000,v3=identity|{'tick_word_radius':0,'max_initialized_ticks':256,'max_steps':8192})
        if row['protocol']=='aerodrome-slipstream':cfg['v3'].update(expected_factory_code_sha256=row['factory_runtime_sha256'],expected_implementation=row['implementation_address'],expected_implementation_code_sha256=row['implementation_runtime_sha256'])
    return cfg

def validate_cache(capture,probe,values=None):
    input_=json.loads(read_blob(ROOT,probe['input']));row=input_['row']
    require(capture['catalog_row']==row and capture['original_execution']==row['inputs']['execution'],'NORMAL_CACHE_REFERENCE_BINDING')
    require(capture['pool_config']==pool_config(row,probe),'NORMAL_CACHE_CONFIG_NOT_FROM_GETTERS')
    expected={call_key(c['to'],c['data']):c['value'] for c in probe['calls'] if c['error'] is None}
    require((captured_calls(capture) if values is None else values)==expected,'NORMAL_CACHE_ABI_NOT_FROM_GETTERS')
    return True

def validate_rejection(probe):
    require(probe['passed'] is False and probe['missing'] and probe['error'] and any(c['error'] for c in probe['calls']),'NORMAL_EXPECTED_REAL_GETTER_REJECTION')
    for c in probe['calls']:
        if c['error'] is None:require(c['all_code_and_storage_MPT_bound'] and c['steps']>0,'NORMAL_PARTIAL_GETTER_UNPROVEN')

def prepare(directory,probe_path,permit_getter_rejection=False):
    probe=load_json(probe_path);(validate_rejection if permit_getter_rejection else validate_probe)(probe);input_=json.loads(read_blob(ROOT,probe['input']));row=input_['row'];require(row['id'] in IDS,'BOUNDED_NORMAL_SIX_SCOPE');require(probe['id']==row['id'] and probe['anchor']==row['anchor'],'NORMAL_GETTER_FIXED_H')
    source=ROOT/row['directory'];ex=json.loads(read_blob(ROOT,row['inputs']['execution']));require(ex['anchor']['height']==row['anchor']['height'] and ex['anchor']['hash']==row['anchor']['hash'],'NORMAL_ORIGINAL_VM_ANCHOR');case=next(c for c in ex['cases'] if c['catalog_deployment_id']==row['id']);require(case['passed'] and case['pool']==row['pool'],'NORMAL_ORIGINAL_REFERENCE_POOL')
    cfg=pool_config(row,probe);require(set([cfg['token0'],cfg['token1']])==set([case['token_in'],case['token_out']]),'NORMAL_REFERENCE_TOKENS')
    raw={'schema_version':4,'source_kind':'derived-original-runtime-getter-execution-from-MPT-state; not public eth_call wire','getter_execution':reference(probe_path),'requests':[{'jsonrpc':'2.0','id':i+1,'method':'eth_call','params':[{'to':c['to'],'data':c['data']},hex(row['anchor']['height'])]} for i,c in enumerate(probe['calls'])],'response':[{'jsonrpc':'2.0','id':i+1,**({'error':{'code':-32099,'message':c['error']}} if c['error'] else {'result':c['value']})} for i,c in enumerate(probe['calls'])],'original_state_overrides':{'balance':[],'code':[],'storage':[]},'caller':probe['caller'],'caller_policy':probe['caller_policy'],'public_network_requests':0,'authority':'executed original code, every code hash/account/storage read authenticated; environment explicit'}
    batch=directory/'getter-derived-rpc.json';write_new(batch,(json.dumps(raw,indent=2)+'\n').encode())
    capture={'schema_version':4,'catalog_row':row,'original_execution':row['inputs']['execution'],'profile':cfg['protocol'],'pool_config':cfg,'public_batches':[reference(batch)],'call_count':len({call_key(c['to'],c['data']) for c in probe['calls'] if c['error'] is None}),'additional_eth_call_results_MPT_proven':not permit_getter_rejection,'getter_execution':reference(probe_path),'tick_window_policy':'one complete current bitmap word; every initialized tick in that word, no inferred zero liquidity outside window','normal_parser_typed_baseline_injected':False,'qualification_authority':'must rerun getter original VM and normal binaries; saved flags alone do not qualify'}
    write_new(directory/'capture.json',(json.dumps(capture,indent=2)+'\n').encode());return capture

class StageFixedRPC(FixedRPC):
    def __init__(self,source,calls,refs,directory):
        super().__init__(source,calls,refs);self.extra_receipts={};self.failure_calls={}
        capture=load_json(directory/'capture.json');probe=json.loads(read_blob(ROOT,capture['getter_execution']));self.failure_calls={call_key(c['to'],c['data']):c['error'] for c in probe['calls'] if c['error']}
        for path in sorted(directory.glob('normal-auxiliary-header-*.json')):
            wire=load_json(path);block=wire.get('response',{}).get('result')
            if not block:continue
            require(wire['method']=='eth_getBlockByNumber' and wire['params']==[block['number'],False],'NORMAL_STAGE_AUX_HEADER_WIRE');require(block['number'] not in ['0x0',hex(self.height)],'NORMAL_STAGE_AUX_REPLACEMENT');self.auxiliary_headers[block['number']]=block
        for path in sorted(directory.glob('normal-auxiliary-receipt-*.json')):
            wire=load_json(path);receipt=wire.get('response',{}).get('result')
            if not receipt:continue
            require(wire['method']=='eth_getTransactionReceipt' and wire['params']==[receipt['transactionHash']],'NORMAL_STAGE_AUX_RECEIPT_WIRE');header=self.auxiliary_headers.get(receipt['blockNumber']);require(header and header['hash']==receipt['blockHash'] and receipt['transactionHash'] in header['transactions'],'NORMAL_STAGE_AUX_RECEIPT_BLOCK');self.extra_receipts[receipt['transactionHash']]=receipt
    def dispatch(self,method,params):
        if method=='eth_call':
            self.check_anchor(params[1]);failure=self.failure_calls.get(call_key(params[0]['to'],params[0]['data']))
            if failure:raise ValueError(failure)
        if method=='eth_getTransactionReceipt' and params[0] in self.extra_receipts:return self.extra_receipts[params[0]]
        return super().dispatch(method,params)

def execute(directory,probe_path,output,parser_bin=None,router_bin=None,permit_getter_rejection=False,timeout_seconds=35):
    require(type(timeout_seconds) is int and 1 <= timeout_seconds <= 180,'NORMAL_PROCESS_TIMEOUT_BUDGET')
    require(directory.resolve().is_relative_to(BASE.resolve()),'OWNED_NORMAL_CASE')
    capture=prepare(directory,probe_path,permit_getter_rejection) if not (directory/'capture.json').exists() else load_json(directory/'capture.json');probe=load_json(probe_path);(validate_rejection if permit_getter_rejection else validate_probe)(probe);require(capture['getter_execution']==reference(probe_path),'NORMAL_GETTER_CACHE_BINDING')
    # Re-execute getters into a new local artifact before trusting the normal cache.
    checked=output.parent/(output.name+'-getter-recheck.json');command=['node',str(ROOT/'integration/tools/acceptance/r01_public_catalog_evm_readonly.cjs'),str(ROOT/probe['input']['path']),str(checked)];p=subprocess.run(command,capture_output=True,text=True,timeout=180);write_new(output.parent/(output.name+'-getter-recheck.log'),(p.stdout+p.stderr).encode());require(p.returncode==0,'NORMAL_GETTER_REEXECUTION_FAILED');actual=load_json(checked);(validate_rejection if permit_getter_rejection else validate_probe)(actual)
    require(actual['calls']==probe['calls'] and actual['anchor']==probe['anchor'],'NORMAL_GETTER_REPLAY_BYTES_CHANGED');validate_cache(capture,actual)
    row=capture['catalog_row'];original=json.loads(read_blob(ROOT,capture['original_execution']));case=next(c for c in original['cases'] if c['catalog_deployment_id']==row['id']);wall_ms=int(time.time()*1000);header_ms=original['anchor']['timestamp']*1000;require(header_ms<=wall_ms+30000,'NORMAL_FUTURE_ORIGINAL_HEADER');freshness=max(60000,wall_ms-header_ms+120000)
    result=normal_run(directory,output,timeout_seconds,parser_bin,router_bin,rpc_factory=lambda source,calls,refs:StageFixedRPC(source,calls,refs,directory),freshness_ms=freshness);q=result.get('quote',{}).get('body',{})
    gross=q.get('grossBuyAmount');quoted=result['normal_parser_router_quote_observed'];delta=int(case['recipient_after'])-int(case['recipient_before']);exact=not permit_getter_rejection and quoted and int(gross)==delta==int(case['actual_net_recipient'])
    request=result.get('quote_request',{});require(not quoted or (request['sellAmount']==case['amount_in'] and request['sellToken']==case['token_in'] and request['buyToken']==case['token_out']),'NORMAL_EXACT_INPUT_REFERENCE_BINDING')
    bound={'schema_version':4,'id':row['id'],'anchor':row['anchor'],'normal_process_report':reference(output/'report.json'),'test_process_startup_timeout_seconds':timeout_seconds,'getter_execution':reference(probe_path),'actual_getter_reexecution':reference(checked),'original_VM_reference':capture['original_execution'],'normal_quote_observed':quoted,'additional_getter_code_account_storage_MPT_bound':not permit_getter_rejection,'recorded_getter_rejection':probe['error'] if permit_getter_rejection else None,'gross_matches_original_VM_token_delta':bool(exact),'quantities':{'amount_in':case['amount_in'],'router_pool_gross':gross,'reference_recipient_token_delta':str(delta),'reference_token_out':case['token_out'],'after_chain_fees':{'available':False,'compared_with_token_delta':False,'router_estimate':q.get('netBuyAmountEstimate'),'reason':q.get('costs',{}).get('unavailableReason','native chain fees and production execution not established')}},'normal_parser_typed_baseline_injected':False,'normal_journal':'http-development','offline_freshness_policy':{'test_wall_clock_ms':wall_ms,'original_header_timestamp_ms':header_ms,'startup_and_router_max_age_ms':freshness,'age_slack_ms':120000,'original_header_timestamp_rewritten':False,'production_freshness_claimed':False},'public_writes':0,'normal_replay_public_network_requests':0,'production_Router_build_execution_verified':False,'native_SPEC3_claimed':False,'full_R01_qualified':False,'status':'QUALIFIED_FINITE_NORMAL_REFERENCE_DIFFERENTIAL' if exact else 'NOT_QUALIFIED','remaining':[] if exact else ['normal process rejected or amount mismatch; inspect original logs and RPC trace']}
    write_new(output/'bound-report.json',(json.dumps(bound,indent=2)+'\n').encode());print(json.dumps(bound,indent=2));return bound

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--case-directory',type=Path,required=True);p.add_argument('--probe',type=Path,required=True);p.add_argument('--out',type=Path,required=True);p.add_argument('--parser-bin',type=Path);p.add_argument('--router-bin',type=Path);p.add_argument('--record-getter-rejection',action='store_true');p.add_argument('--timeout-seconds',type=int,default=35);a=p.parse_args();require(a.out.resolve().is_relative_to((ROOT/'integration/artifacts/local').resolve()) and a.out.name.startswith('r01-public-'),'OWNED_NORMAL_OUTPUT');execute(a.case_directory.resolve(),a.probe.resolve(),a.out.resolve(),a.parser_bin,a.router_bin,a.record_getter_rejection,a.timeout_seconds)
if __name__=='__main__':main()
