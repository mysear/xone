#!/usr/bin/env python3
"""Actual fixed-H public ABI bytes -> normal Go parser -> normal Rust Router.

This is a state-decode/quote differential over the existing HTTP development
journal. Native SPEC3, PG/Kafka, production build execution and proof authority
for additional public eth_call results are separate qualifications.
"""
import argparse
import copy
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import secrets
import socket
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
sys.path.insert(0,str(Path(__file__).resolve().parent))
from r01_public_catalog import ROOT,load_json,read_blob,require,public_rpc_endpoint,sha256
from r01_public_catalog_capture import write_new,reference
from r01_public_catalog_evm_history import CHAINS
from r01_public_catalog_evm_seed import curl_read
from configuration import config_for
BASE=ROOT/'integration/test/source/deployment-catalog/evm-continuation-v2'
FRESHNESS=315360000000
PROFILES={'uniswap-v3':'uniswap-v3','pancake-v3':'pancake-v3','pancake-v2':'pancake-v2','aerodrome-slipstream':'aerodrome-slipstream','aerodrome-v2':'aerodrome-v2-volatile'}

def normal_profile(protocol):return PROFILES.get(protocol)
def call_key(to,data):return to.lower()+':'+data.lower()
def words(value):
    require(isinstance(value,str) and value.startswith('0x') and (len(value)-2)%64==0,'PUBLIC_ABI_WORD_BYTES')
    return [int(value[i:i+64],16) for i in range(2,len(value),64)]
def signed(value):return value-(1<<256) if value&(1<<255) else value
def arg(value):return format(value%(1<<256),'064x')

def public_batch(endpoint,calls,height,path):
    public_rpc_endpoint(endpoint)
    requests=[{'jsonrpc':'2.0','id':i+1,'method':'eth_call','params':[{'to':a,'data':d},hex(height)]} for i,(a,d) in enumerate(calls)]
    require(0<len(requests)<=256,'NORMAL_PUBLIC_BATCH_BUDGET')
    p=subprocess.run(['curl','--compressed','-sS','--fail-with-body','--max-time','18','--max-filesize',str(16*1024*1024),'-A','XONE-source-evidence/1.0','-H','Content-Type: application/json','--data-binary',json.dumps(requests),endpoint],capture_output=True,timeout=22)
    require(len(p.stdout)<=16*1024*1024,'NORMAL_RESPONSE_BUDGET')
    try:response=json.loads(p.stdout)
    except ValueError:response=None
    record={'endpoint':endpoint,'requests':requests,'response':response,'exit_code':p.returncode,'response_sha256':sha256(p.stdout),'stderr':p.stderr.decode(errors='replace'),'source_kind':'actual-public-fixed-H-read-only-JSON-RPC-batch'}
    write_new(path,(json.dumps(record,indent=2)+'\n').encode())
    if not isinstance(response,list):return {},reference(path)
    by_id={r['id']:r for r in response};results={}
    for q,(a,d) in zip(requests,calls):
        r=by_id.get(q['id'],{})
        if isinstance(r.get('result'),str):results[call_key(a,d)]=r['result']
    return results,reference(path)

def capture(row,directory,resume=False):
    require(not directory.exists() or (resume and not (directory/'capture.json').exists()),'NEW_NORMAL_PUBLIC_CAPTURE_REQUIRED');directory.mkdir(parents=True,exist_ok=resume)
    source=ROOT/row['directory'];execution=load_json(source/'fork-execution.json');case=next(c for c in execution['cases'] if c['catalog_deployment_id']==row['id'])
    profile=normal_profile(row['protocol']);require(profile is not None,'NORMAL_LITERAL_PROFILE_UNSUPPORTED')
    endpoints=list(dict.fromkeys(load_json(source/'anchor.json')['endpoints']+CHAINS[row['chain']][1]));cache={};refs=[];stage=0
    if resume:
        for f in sorted(directory.glob('public-abi-*.json')):
            r=load_json(f);refs.append(reference(f));stage=max(stage,int(f.name.split('-')[2]))
            if not isinstance(r.get('response'),list):continue
            by_id={v['id']:v for v in r['response']}
            for q in r['requests']:
                require(q['params'][1]==hex(row['anchor']['height']),'RESUME_ABI_FIXED_H')
                v=by_id.get(q['id'],{})
                if isinstance(v.get('result'),str):cache[call_key(q['params'][0]['to'],q['params'][0]['data'])]=v['result'];endpoints=[r['endpoint']]+[e for e in endpoints if e!=r['endpoint']]
    def fetch(calls):
        nonlocal stage,endpoints
        calls=[c for c in dict.fromkeys(calls) if call_key(*c) not in cache]
        if not calls:return
        stage+=1
        for i,endpoint in enumerate(endpoints[:3]):
            missing=[c for c in calls if call_key(*c) not in cache]
            if not missing:break
            chunk_size=3 if 'drpc.org' in endpoint else 32
            for part,offset in enumerate(range(0,len(missing),chunk_size)):
                result,ref=public_batch(endpoint,missing[offset:offset+chunk_size],row['anchor']['height'],directory/f'public-abi-{stage}-{i}-{part}.json');cache.update(result);refs.append(ref)
                if result:endpoints=[endpoint]+[e for e in endpoints if e!=endpoint]
        require(all(call_key(*c) in cache for c in calls),'PUBLIC_FIXED_H_ABI_MISSING:'+str([c for c in calls if call_key(*c) not in cache][:4]))
    def get(a,d):return cache[call_key(a,d)]
    pool=row['pool'];factory=row['factory'];common=['0x0dfe1681','0xd21220a7','0xc45a0155']
    extra=['0xddca3f43','0xd0c93a7c','0x3850c7bd','0x1a686502'] if 'v3' in profile or 'slipstream' in profile else ['0x0902f1ac'] if profile=='pancake-v2' else ['0x392f37e9']
    if profile=='pancake-v3':extra.append('0x540d4918')
    fetch([(pool,d) for d in common+extra])
    token0='0x'+get(pool,'0x0dfe1681')[-40:];token1='0x'+get(pool,'0xd21220a7')[-40:]
    require(set([token0,token1])==set([case['token_in'],case['token_out']]),'NORMAL_PUBLIC_POOL_TOKENS')
    cfg={'id':pool,'protocol':profile,'token0':token0,'token1':token1,'program_id':'','accounts':[]}
    identity={'expected_factory':factory,'expected_code_sha256':row['pool_runtime_sha256']}
    if profile=='pancake-v2':cfg.update(fee_numerator=25,fee_denominator=10000,v2=identity)
    elif profile.startswith('aerodrome-v2'):
        metadata=words(get(pool,'0x392f37e9'));require(len(metadata)==7,'AERO_METADATA_ABI');stable=metadata[4]
        require(stable in [0,1],'AERO_STABLE_FLAG');cfg['protocol']='aerodrome-v2-stable' if stable else 'aerodrome-v2-volatile'
        fee_call='0xcc56b2c5'+pool[2:].rjust(64,'0')+arg(stable)
        fetch([(factory,'0xb187bd26'),(factory,fee_call)])
        cfg.update(fee_numerator=words(get(factory,fee_call))[0],fee_denominator=10000,stable=identity|{'expected_factory_code_sha256':row['factory_runtime_sha256'],'expected_implementation':row['implementation_address'],'expected_implementation_code_sha256':row['implementation_runtime_sha256']})
    else:
        fee=words(get(pool,'0xddca3f43'))[0];spacing=signed(words(get(pool,'0xd0c93a7c'))[0]);tick=signed(words(get(pool,'0x3850c7bd'))[1]);require(0<spacing<16384,'PUBLIC_V3_SPACING')
        cfg.update(fee_numerator=fee,fee_denominator=1000000,v3=identity|{'tick_word_radius':0,'max_initialized_ticks':256,'max_steps':8192})
        if profile=='aerodrome-slipstream':
            cfg['v3'].update(expected_factory_code_sha256=row['factory_runtime_sha256'],expected_implementation=row['implementation_address'],expected_implementation_code_sha256=row['implementation_runtime_sha256'])
            fetch([(factory,'0x23c43a51'),(factory,'0x7693bc11')])
        center=(tick//spacing)//256;bitmap_call='0x5339c296'+arg(center);fetch([(pool,bitmap_call)])
        bitmap=words(get(pool,bitmap_call))[0];ticks=[(center*256+bit)*spacing for bit in range(256) if bitmap>>bit&1]
        if ticks:fetch([(pool,'0xf30dba93'+arg(t)) for t in ticks])
    value={'schema_version':1,'catalog_row':row,'original_execution':reference(source/'fork-execution.json'),'profile':cfg['protocol'],'pool_config':cfg,'public_batches':refs,'call_count':len(cache),'tick_window_policy':'V3 only: actual complete current bitmap word, radius 0; every initialized tick in that declared word fetched; no out-of-window zero-liquidity assumption','additional_eth_call_results_MPT_proven':False,'authority':'actual raw public ABI return bytes at the same explicit H; source/runtime and original VM reference independently retained'}
    write_new(directory/'capture.json',(json.dumps(value,indent=2)+'\n').encode());return value

class FixedRPC:
    def __init__(self,source,calls,refs):
        self.source=source;self.anchor=load_json(source/'anchor.json');self.header=json.loads(read_blob(ROOT,self.anchor['header']))['response']['result'];self.height=int(self.header['number'],16);self.calls=calls;self.refs=refs;self.trace=[]
        evidence=load_json(source/('prestate-evidence-v2.json' if (source/'prestate-evidence-v2.json').exists() else 'prestate-evidence.json'));self.code={d['address']:d['code'] for d in evidence['deployments']};self.receipts={};self.auxiliary_headers={}
        genesis_wire=json.loads(read_blob(ROOT,self.anchor['genesis']))
        if not isinstance(genesis_wire.get('response',{}).get('result'),dict):genesis_wire=load_json(source/'auxiliary-genesis.json')
        require(genesis_wire['method']=='eth_getBlockByNumber' and genesis_wire['params']==['0x0',False],'NORMAL_GENESIS_WIRE');self.genesis=genesis_wire['response']['result'];require(self.genesis['number']=='0x0','NORMAL_GENESIS_HEIGHT')
        for header_path in sorted(source.glob('auxiliary-header-*.json')):
            wire=load_json(header_path);block=wire.get('response',{}).get('result');require(wire['method']=='eth_getBlockByNumber' and isinstance(block,dict) and wire['params']==[block['number'],False],'NORMAL_AUXILIARY_HEADER_WIRE');require(block['number'] not in ['0x0',hex(self.height)],'NORMAL_AUXILIARY_HEADER_REPLACEMENT');self.auxiliary_headers[block['number']]=block
        p=source/'canonical-native-receipts.json'
        if p.exists():self.receipts={r['transactionHash']:r for r in load_json(p).get('response',{}).get('result',[]) or []}
    def check_anchor(self,anchor):
        if isinstance(anchor,dict):require(anchor=={'blockHash':self.header['hash'],'requireCanonical':True},'NORMAL_WRONG_FIXED_HASH')
        else:require(anchor==hex(self.height),'NORMAL_WRONG_FIXED_H')
    def dispatch(self,method,params):
        if method=='eth_chainId':return hex(self.anchor['anchor']['chain_id'])
        if method=='eth_blockNumber':return hex(self.height)
        if method=='eth_getBlockByNumber':
            tag=params[0];require(tag in ['latest','safe','finalized',hex(self.height),'0x0'] or tag in self.auxiliary_headers,'NORMAL_UNRECORDED_HEADER')
            header=copy.deepcopy(self.auxiliary_headers[tag] if tag in self.auxiliary_headers else self.genesis if tag=='0x0' else self.header)
            if len(params)>1 and not params[1]:header['transactions']=[x['hash'] if isinstance(x,dict) else x for x in header['transactions']]
            return header
        if method=='eth_getBlockByHash':
            require(params[0]==self.header['hash'],'NORMAL_UNRECORDED_HEADER');return self.dispatch('eth_getBlockByNumber',[hex(self.height),params[1]])
        if method=='eth_getCode':
            self.check_anchor(params[1]);require(params[0].lower() in self.code,'NORMAL_UNPROVEN_CODE');return self.code[params[0].lower()]
        if method=='eth_call':
            self.check_anchor(params[1]);require(set(params[0])<=set(['to','data']),'NORMAL_CALL_ENVIRONMENT_OVERRIDE');key=call_key(params[0]['to'],params[0]['data']);require(key in self.calls,'NORMAL_UNRECORDED_PUBLIC_ABI_CALL:'+key);return self.calls[key]
        if method=='eth_getBlockReceipts':
            require(params[0] in [hex(self.height),self.header['hash']],'NORMAL_RECEIPT_H');require(len(self.receipts)==len(self.header['transactions']),'NORMAL_RECEIPT_INVENTORY');return list(self.receipts.values())
        if method=='eth_getTransactionReceipt':require(params[0] in self.receipts,'NORMAL_UNRECORDED_RECEIPT');return self.receipts[params[0]]
        raise ValueError('NORMAL_METHOD_NOT_CAPTURED:'+method)
    def serve(self):
        owner=self
        class Handler(BaseHTTPRequestHandler):
            def log_message(self,*_):pass
            def do_POST(self):
                q=json.loads(self.rfile.read(int(self.headers['Content-Length'])));result=None;error=None
                try:result=owner.dispatch(q['method'],q.get('params',[]))
                except (ValueError,KeyError,TypeError) as e:error={'code':-32000,'message':str(e)}
                owner.trace.append({'method':q['method'],'params':q.get('params',[]),'result_sha256':sha256(json.dumps(result,sort_keys=True).encode()) if not error else None,'error':error})
                body=json.dumps({'jsonrpc':'2.0','id':q['id'],**({'error':error} if error else {'result':result})}).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
        server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start();return server

def captured_calls(capture):
    calls={}
    for ref in capture['public_batches']:
        record=json.loads(read_blob(ROOT,ref))
        if not isinstance(record['response'],list):continue
        by_id={r['id']:r for r in record['response']}
        for q in record['requests']:
            require(q['method']=='eth_call' and q['params'][1]==hex(capture['catalog_row']['anchor']['height']),'NORMAL_CAPTURE_FIXED_H');r=by_id.get(q['id'],{})
            if 'result' not in r:continue
            key=call_key(q['params'][0]['to'],q['params'][0]['data'])
            if key in calls:require(calls[key]==r['result'],'NORMAL_CAPTURE_RESPONSE_CHANGED')
            calls[key]=r['result']
    require(len(calls)==capture['call_count'],'NORMAL_CAPTURE_CALL_INVENTORY')
    return calls

def free_port():
    with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
def read_http(url,token,body=None):
    try:
        request=urllib.request.Request(url,data=None if body is None else json.dumps(body).encode(),headers={'Authorization':'Bearer '+token,'Content-Type':'application/json'})
        with urllib.request.urlopen(request,timeout=10) as r:return {'status':r.status,'body':json.load(r)}
    except urllib.error.HTTPError as e:
        try:value=json.loads(e.read())
        except ValueError:value={}
        return {'status':e.code,'body':value}
    except (OSError,ValueError) as e:return {'status':None,'error':str(e)}

def run(directory,output,timeout=35,parser_bin=None,router_bin=None,rpc_factory=None,freshness_ms=FRESHNESS):
    capture_=load_json(directory/'capture.json');row=capture_['catalog_row'];source=ROOT/row['directory'];native=load_json(source/'fork-execution.json');case=next(c for c in native['cases'] if c['catalog_deployment_id']==row['id']);require(not output.exists(),'NEW_NORMAL_PROCESS_OUTPUT_REQUIRED');output.mkdir(parents=True)
    fixture=(rpc_factory or FixedRPC)(source,captured_calls(capture_),capture_['public_batches']);require(fixture.genesis['hash']==row['genesis_hash'],'NORMAL_GENESIS_CHAIN_FINGERPRINT');server=fixture.serve();processes=[];streams=[];result={'schema_version':1,'id':row['id'],'chain':row['chain'],'profile':capture_['profile'],'fixed_H':row['anchor'],'capture':reference(directory/'capture.json'),'normal_parser_router_quote_observed':False,'pool_gross_equals_original_VM_recipient_delta':False,'original_VM_recipient_delta':case['actual_net_recipient'],'original_VM_reference':reference(source/'fork-execution.json'),'network_requests_during_process_replay':0,'additional_public_ABI_results_MPT_proven':False,'production_build_executed':False,'Go_SPEC3_claimed':False,'PG_Kafka_claimed':False,'current_quote_freshness_claimed':False,'authority':'actual public ABI bytes -> normal Go state decoder -> existing HTTP development journal -> normal Rust Router; finite one-pool exact-input differential'}
    try:
        token=secrets.token_urlsafe(32);ports={'parser':free_port(),'router':free_port()};chain='evm:'+str(row['chain_id']);result['binaries']={}
        chain_cfg={'key':chain,'owner_service':'chain-parser-go','assignment_version':1,'genesis_hash':row['genesis_hash'],'rpc_urls':[f'http://127.0.0.1:{server.server_port}'],'pools':[capture_['pool_config']],'confirmation_tag':'finalized','auto_reorg_window':128}
        for alias,service,binary in [('parser','chain-parser-go',Path(parser_bin or ROOT/'chain-parser-go/bin/chain-parser-go').resolve()),('router','dex-router',Path(router_bin or ROOT/'dex-router/target/debug/dex-router').resolve())]:
            cfg=config_for(service,output/alias,token,ports[alias]);cfg['service']['mode']='rpc';cfg['chains']=[copy.deepcopy(chain_cfg)];cfg['startup']['max_baseline_age_ms']=freshness_ms;cfg['logging']['level']='info';cfg['control'].update(backend='disabled',dsn_file='',instance_id='r01-evm-'+alias);cfg['journal'].update(transport='http-development',source_urls=[f"http://127.0.0.1:{ports['parser']}"],poll_interval_ms=250,request_timeout_ms=10000,max_payload_bytes=16*1024*1024)
            if alias=='router':
                cfg['chains'][0].update(rpc_urls=[],pools=[]);cfg['journal']['source_auth_tokens']={f"http://127.0.0.1:{ports['parser']}":token};cfg['routing'].update(max_hops=1,max_splits=1,max_state_age_ms=freshness_ms,quote_ttl_ms=60000,route_activation_cost='0');cfg['api'].update(quote_timeout_ms=10000,quote_concurrency=1)
            config_path=output/(alias+'-config.json');config_path.write_text(json.dumps(cfg,indent=2)+'\n');config_path.chmod(0o600)
            data=binary.read_bytes();pinned=output/(alias+'-binary');pinned.write_bytes(data);pinned.chmod(0o700);result['binaries'][alias]={'source':str(binary.relative_to(ROOT)),'sha256':sha256(data),'pinned':str(pinned.relative_to(ROOT))}
            stream=(output/(alias+'.log')).open('w');streams.append(stream);processes.append(subprocess.Popen([str(pinned),'--config',str(config_path)],cwd=ROOT/service,stdout=stream,stderr=subprocess.STDOUT))
        parser=f"http://127.0.0.1:{ports['parser']}";router=f"http://127.0.0.1:{ports['router']}";deadline=time.monotonic()+timeout
        while time.monotonic()<deadline:
            baseline=read_http(parser+'/internal/chains/'+chain+'/baseline',token);result['parser_baseline']=baseline
            status=read_http(router+'/internal/chains/'+chain+'/status',token);result['router_status']=status
            if baseline['status']==200 and status.get('body',{}).get('ready'):break
            if any(p.poll() is not None for p in processes):break
            time.sleep(.2)
        result['normal_process_errors']={alias:(output/(alias+'.log')).read_text()[-5000:] for alias in ['parser','router']}
        if result['parser_baseline']['status']==200:
            ref=result['parser_baseline']['body']['reference']['block'];require(ref['number_or_slot']==row['anchor']['height'] and ref['hash']==row['anchor']['hash'],'NORMAL_PARSER_FIXED_H_BINDING');require(len(result['parser_baseline']['body']['pools'])==1 and result['parser_baseline']['body']['pools'][0]['pool_id']==row['pool'],'NORMAL_PARSER_POOL_SCOPE')
        request={'chainKey':chain,'kind':'EXACT_IN','sellToken':case['token_in'],'buyToken':case['token_out'],'sellAmount':case['amount_in'],'caller':native['sender'],'recipient':native['recipient'],'slippageBps':50,'verification':'FAST'};result['quote_request']=request;quote=read_http(router+'/v1/quote',token,request);result['quote']=quote
        if quote['status']==200:
            q=quote['body'];ref=q['snapshotRef']['block'];require(ref['number_or_slot']==row['anchor']['height'] and ref['hash']==row['anchor']['hash'],'NORMAL_QUOTE_FIXED_H_BINDING');require(q['quality']['candidateScope']==[row['pool']],'NORMAL_QUOTE_POOL_SCOPE');result['normal_parser_router_quote_observed']=True;result['pool_gross_equals_original_VM_recipient_delta']=int(q['grossBuyAmount'])==int(case['actual_net_recipient']);result['router_plan_evidence']=read_http(router+'/internal/quotes/'+q['quoteId']+'/evidence',token)
        result['parser_status']=read_http(parser+'/internal/chains/'+chain+'/status',token)
    except Exception as e:result.update(error_type=type(e).__name__,error=str(e))
    finally:
        for p in processes:
            if p.poll() is None:p.terminate()
        for p in processes:
            try:p.wait(timeout=10)
            except subprocess.TimeoutExpired:p.kill();p.wait()
        for s in streams:s.close()
        server.shutdown();server.server_close();result['rpc_trace']=fixture.trace;result['unrecorded_rpc_requests']=[r for r in fixture.trace if r['error']];result['process_exit_codes']=[p.returncode for p in processes];result['binary_source_changed']={a:sha256((ROOT/r['source']).read_bytes())!=r['sha256'] for a,r in result.get('binaries',{}).items()}
        write_new(output/'report.json',(json.dumps(result,indent=2)+'\n').encode())
    print(json.dumps({k:result.get(k) for k in ['id','normal_parser_router_quote_observed','pool_gross_equals_original_VM_recipient_delta','error','unrecorded_rpc_requests']},indent=2));return result

def main():
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('command',choices=['capture','run']);p.add_argument('--id');p.add_argument('--directory',type=Path,required=True);p.add_argument('--output',type=Path);p.add_argument('--timeout',type=int,default=35);p.add_argument('--resume',action='store_true');p.add_argument('--parser-bin',type=Path);p.add_argument('--router-bin',type=Path);a=p.parse_args()
    require(a.directory.resolve().is_relative_to(BASE.resolve()),'OWNED_NORMAL_CAPTURE_DIRECTORY')
    if a.command=='capture':
        row=next(r for r in load_json(BASE/'catalog-v3.json')['entries'] if r['id']==a.id);r=capture(row,a.directory.resolve(),a.resume);print(json.dumps({'id':a.id,'call_count':r['call_count'],'profile':r['profile']}))
    else:
        require(a.output and a.output.resolve().is_relative_to((ROOT/'integration/artifacts/local').resolve()) and a.output.name.startswith('r01-public-'),'OWNED_NORMAL_OUTPUT');run(a.directory.resolve(),a.output.resolve(),a.timeout,a.parser_bin,a.router_bin)
if __name__=='__main__':main()
