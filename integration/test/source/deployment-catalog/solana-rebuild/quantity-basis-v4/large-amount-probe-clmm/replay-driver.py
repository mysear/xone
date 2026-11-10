"""Observe an extra real HTTP quote while the unchanged normal process runs."""
import copy
import json
from pathlib import Path
import sys
sys.path.insert(0,str(Path.cwd()/'integration/tools/acceptance'))
import r01_public_catalog_solana_process as process

original_read=process.read_http
probes=[]
def read_and_probe(url,token,body=None,timeout=3):
    response=original_read(url,token,body,timeout)
    if url.endswith('/v1/quote') and body is not None and body.get('sellAmount')=='1000000':
        request=copy.deepcopy(body);request['sellAmount']='1000000000000000'
        probes.append({'request':request,'response':original_read(url,token,request,15),
            'authority':'additional actual HTTP request to normal Router using unchanged Parser baseline',
            'quantity_basis':'pool_gross','transaction_submitted':False})
    return response
process.read_http=read_and_probe
out=process.ROOT/'integration/artifacts/local/r01-solana-process-boundary-probe-v4'
result=process.run_case('clmm',out/'clmm',process.ROOT/'chain-parser-rust/target/debug/chain-parser-rust',process.ROOT/'dex-router/target/debug/dex-router',20,clmm_window='nearest-three')
report={'tick_window':result['tick_window'],'window_scope_observation':result.get('window_scope_observation'),'binaries':result['binaries'],
    'control_amount':'1000000','control_quote_http_status':result['normal_quote_response']['http_status'],
    'probes':probes,'original_account_bytes_and_clock_unchanged':result['original_account_bytes_and_clock_unchanged'],
    'native_large_amount_replayed':False,'whole_pool_or_all_inputs_claimed':False}
(out/'boundary-probe.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({'control':report['control_quote_http_status'],'probes':probes},indent=2))
