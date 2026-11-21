"""Normal processes over bounded live public reads or authenticated retained bytes.

The HTTP development journal, provider-declared finality and arithmetic oracle
have separate scopes. Nothing here submits transactions or grants VERIFIED.
"""
import base64
import copy
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import re
import secrets
import shutil
import signal
import socket
import subprocess
import threading
import time
import urllib.error
import urllib.request
from urllib.parse import urlsplit
from configuration import config_for

ROOT=Path(__file__).resolve().parents[3]
ENDPOINTS=('https://eth.drpc.org','https://ethereum-rpc.publicnode.com')
GENESIS='0xd4e56740f876aef8c010b86a40d5f56745a118d0906a34e69aec8c0db1cb8fa3'
POOL={'id':'0xa478c2975ab1ea89e8196811f51a7b7ade33eb11','protocol':'uniswap-v2-complete','token0':'0x6b175474e89094c44da98b954eedeac495271d0f','token1':'0xc02aaa39b223fe8d0a0e5c4f27ead9083c756cc2','fee_numerator':3,'fee_denominator':1000,'v2':{'expected_factory':'0x5c69bee701ef814a2b6a3edd4b1652cb9cc5aa6f','expected_code_sha256':'8b5db55fa9ab3b9527508d4abe0b39eb588bf310270c8e04b3f38214e8ba63b4'}}
METHODS=frozenset(['eth_chainId','eth_getBlockByNumber','eth_getBlockReceipts','eth_getTransactionReceipt','eth_getCode','eth_call','eth_getStorageAt','eth_getProof'])
MAX_CAPTURE_BYTES=128<<20

def require(value,code):
    if not value:raise ValueError(code)
def sha(data):return hashlib.sha256(data).hexdigest()
def provider_id(url):return 'rpc-'+sha(url.encode())[:24]
def canonical(value):return json.dumps(value,sort_keys=True,separators=(',',':'))
def pairs(items):
    value={}
    for key,item in items:
        require(key not in value,'DUPLICATE_JSON_KEY');value[key]=item
    return value

def sealed_payload(path, file_budget, expected_chain='evm:1'):
    path=Path(path);require(not path.is_symlink() and path.is_file(),'REGULAR_CAPTURE_REQUIRED')
    require(path.stat().st_size <= file_budget, 'CAPTURE_FILE_BUDGET')
    with path.open('rb') as stream:
        raw = stream.read(file_budget + 1)
    require(len(raw) <= file_budget, 'CAPTURE_FILE_BUDGET')
    value=json.loads(raw,object_pairs_hook=pairs);require(set(value)=={'Digest','Payload'},'CAPTURE_ENVELOPE')
    text=raw.decode();match=re.search(r'"Payload"\s*:',text);require(match,'CAPTURE_PAYLOAD')
    start=match.end()
    while text[start].isspace():start+=1
    _,end=json.JSONDecoder().raw_decode(text,start)
    require(sha(text[start:end].encode())==value['Digest'],'CAPTURE_ENVELOPE_HASH')
    payload=value['Payload'];require(set(payload)=={'Ref','Data','CapturedAt'},'CAPTURE_PAYLOAD_FIELDS')
    ref=payload['Ref'];owner=ref['Owner'];require(set(ref)=={'ID','Digest','Owner','Bytes'} and set(owner)=={'Chain','Height','Hash','ProviderID'},'CAPTURE_OWNER_FIELDS')
    require(owner['Chain'] == expected_chain, 'CAPTURE_OWNER')
    data=base64.b64decode(payload['Data'],validate=True)
    require(len(data)==ref['Bytes'] and sha(data)==ref['Digest'],'CAPTURE_DATA_HASH')
    owner_bytes=json.dumps({k:owner[k] for k in ['Chain','Height','Hash','ProviderID']},separators=(',',':')).encode()
    require(ref['ID']==sha(owner_bytes) and path.name==ref['ID']+'.raw','CAPTURE_ID')
    identity = {'file':path.name,'sha256':sha(raw),'data_sha256':ref['Digest'],'bytes':len(raw),'provider_id':owner['ProviderID']}
    return owner, data, identity


def rpc_observation(owner, data, identity, methods):
    require(owner['Height'] == 0 and owner['Hash'] == identity['data_sha256']
            and re.fullmatch(r'rpc-[0-9a-f]{24}', owner['ProviderID']), 'CAPTURE_OWNER')
    observation=json.loads(data,object_pairs_hook=pairs)
    require(set(observation)=={'source_kind','method','params','response'} and observation['source_kind']=='raw-json-rpc-response' and observation['method'] in methods,'CAPTURE_METHOD')
    body=base64.b64decode(observation['response'],validate=True)
    return observation['method'], observation['params'], body


def sealed_record(path):
    owner, data, identity = sealed_payload(path, 32 << 20)
    method, params, body = rpc_observation(owner, data, identity, METHODS)
    response=json.loads(body,object_pairs_hook=pairs)
    require(response.get('jsonrpc')=='2.0' and response.get('id')==1 and 'result' in response and 'error' not in response,'CAPTURE_SUCCESS_REQUIRED')
    return owner['ProviderID'],method,params,body,identity

class RecordedRPC:
    def __init__(self,records,fault=None):
        self.responses={};self.variants=0;self.fault=fault;self.trace=[];self.lock=threading.Lock();self.server=None
        for method,params,body in records:
            key=(method,canonical(params))
            if key in self.responses:
                require(json.loads(self.responses[key])==json.loads(body),'AMBIGUOUS_CAPTURE_RESULT');self.variants+=int(self.responses[key]!=body)
            else:self.responses[key]=body
    def response(self,method,params):
        require(method in METHODS,'READ_ONLY_METHOD_REQUIRED')
        key=(method,canonical(params));require(key in self.responses,'UNRECORDED_RPC_REQUEST')
        original=self.responses[key];value=json.loads(original);mutated=False
        if self.fault=='pair-proof' and method=='eth_getProof' and params[0]==POOL['id']:
            value['result']['accountProof']=['0xc0'];mutated=True
        elif self.fault=='proof-unavailable' and method=='eth_getProof':
            value={'jsonrpc':'2.0','id':1,'error':{'code':-32601,'message':'retained control: proof unavailable'}};mutated=True
        elif self.fault=='pair-getter' and method=='eth_call' and params[0]['to']==POOL['id'] and params[0]['data']=='0x0902f1ac':
            result=value['result'];value['result']='0x'+format(int(result[2:66],16)^1,'064x')+result[66:];mutated=True
        elif self.fault=='token-getter' and method=='eth_call' and params[0]['to']==POOL['token0'] and params[0]['data'].startswith('0x70a08231'):
            value['result']='0x'+format(int(value['result'],16)^1,'064x');mutated=True
        elif self.fault=='header' and method=='eth_getBlockByNumber' and params[0]!='0x0':
            value['result']['stateRoot']='0x'+'11'*32;mutated=True
        with self.lock:self.trace.append({'method':method,'params':params,'mutated':mutated,'original_response_sha256':sha(original)})
        return json.dumps(value,separators=(',',':')).encode() if mutated else original
    def start(self):
        owner=self
        class Handler(BaseHTTPRequestHandler):
            def log_message(self,*args):pass
            def do_POST(self):
                try:
                    size=int(self.headers.get('Content-Length','0'));require(0<size<=65536,'REQUEST_BUDGET')
                    request=json.loads(self.rfile.read(size),object_pairs_hook=pairs);require(request.get('id')==1 and request.get('jsonrpc')=='2.0','REQUEST_ID')
                    body=owner.response(request['method'],request.get('params',[]))
                except (ValueError,KeyError,TypeError) as error:
                    with owner.lock:owner.trace.append({'error':str(error)})
                    body=json.dumps({'jsonrpc':'2.0','id':1,'error':{'code':-32000,'message':str(error)}}).encode()
                self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(body)));self.end_headers();self.wfile.write(body)
        self.server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=self.server.serve_forever,daemon=True).start()
        return 'http://127.0.0.1:'+str(self.server.server_port)
    def close(self):
        if self.server:self.server.shutdown();self.server.server_close();self.server=None

def capture_index(directory):
    directory=Path(directory);paths=sorted(directory.glob('*.raw'));require(0<len(paths)<=1024,'CAPTURE_COUNT_BUDGET')
    require(sum(p.stat().st_size for p in paths)<=MAX_CAPTURE_BYTES,'CAPTURE_TOTAL_BUDGET')
    expected={provider_id(url) for url in ENDPOINTS};records={key:[] for key in expected};manifest=[]
    for path in paths:
        provider,method,params,body,identity=sealed_record(path);require(provider in expected,'CAPTURE_PROVIDER')
        records[provider].append((method,params,body));manifest.append(identity)
    require(all(records.values()),'CAPTURE_PROVIDER_COVERAGE')
    proofs=[(params,json.loads(body)['result']) for rows in records.values() for method,params,body in rows if method=='eth_getProof']
    require(len(proofs)>=6,'CAPTURE_PROOF_COVERAGE')
    require({params[0] for params,_ in proofs}=={POOL['id'],POOL['v2']['expected_factory'],POOL['token0'],POOL['token1']},'CAPTURE_ACCOUNT_COVERAGE')
    return records,manifest

def free_port():
    with socket.socket() as sock:sock.bind(('127.0.0.1',0));return sock.getsockname()[1]
def stop(child):
    if child.poll() is None:
        os.killpg(child.pid,signal.SIGTERM)
        try:child.wait(timeout=6)
        except subprocess.TimeoutExpired:os.killpg(child.pid,signal.SIGKILL);child.wait()

def http(url,token,body=None):
    request=urllib.request.Request(url,data=None if body is None else json.dumps(body).encode(),headers={'Authorization':'Bearer '+token,'Content-Type':'application/json'})
    try:
        with urllib.request.urlopen(request,timeout=3) as response:return {'status':response.status,'body':json.load(response)}
    except urllib.error.HTTPError as error:
        return {'status':error.code,'body':json.loads(error.read())}
    except (OSError,ValueError) as error:return {'status':None,'error':type(error).__name__}

def requests_for(pool):
    for zero,amount,target in [(True,10**18,10**15),(False,10**16,10**18)]:
        sell,buy=(pool['token0'],pool['token1']) if zero else (pool['token1'],pool['token0'])
        common={'chainKey':'evm:1','sellToken':sell,'buyToken':buy,'caller':'0x'+'11'*20,'recipient':'0x'+'22'*20,'slippageBps':50,'verification':'FAST'}
        yield '/v1/quote',dict(common,kind='EXACT_IN',sellAmount=str(amount))
        yield '/v1/quote-exact-out',dict(common,kind='EXACT_OUT',buyAmount=str(target),maxSellAmount=str(10**30))

def check_quote(pool,reference,request,response):
    require(response['status']==200,'NORMAL_QUOTE_FAILED');quote=response['body'];require(quote['snapshotRef']==reference,'QUOTE_REFERENCE_CHANGED')
    require(quote['verification']['level']=='INDICATIVE' and quote['verification']['success'] is False,'FALSE_VERIFIED_CLAIM')
    require(quote['costs']['unavailableReason']=='CHAIN_FEE_MODEL_NOT_IMPLEMENTED','UNEXPECTED_COST_SCOPE')
    zero=request['sellToken']==pool['token0'];rin,rout=(int(pool['reserve0']),int(pool['reserve1'])) if zero else (int(pool['reserve1']),int(pool['reserve0']))
    amount_out=lambda value:value*997*rout//(rin*1000+value*997)
    if request['kind']=='EXACT_IN':
        expected=amount_out(int(request['sellAmount']));require(int(quote['grossBuyAmount'])==expected and quote['consumedIn']==request['sellAmount'] and quote['refund']=='0','INTEGER_EXACT_IN_ORACLE')
    else:
        target=int(request['buyAmount']);require(0<target<rout,'ORACLE_TARGET_SCOPE');denom=(rout-target)*997;expected=(rin*target*1000+denom-1)//denom
        require(int(quote['requiredSellAmount'])==expected and amount_out(expected)>=target and amount_out(expected-1)<target,'INTEGER_EXACT_OUT_ORACLE')
    return {'kind':request['kind'],'sell_token':request['sellToken'],'independent_integer_expected':str(expected),'matches':True}

def source_configuration(endpoints,live,signed_finality=None,proof_endpoint=None):
    """Keep legacy selection fixed; separate signed live and retained inputs."""
    signed=signed_finality is not None
    require(proof_endpoint is None or signed,'PROOF_ENDPOINT_REQUIRES_SIGNED_FINALITY')
    if signed:
        require(len(endpoints)==2 and len(set(endpoints))==2,'SIGNED_SOURCE_ENDPOINT_SCOPE')
        for url in [*endpoints,*([proof_endpoint] if proof_endpoint is not None else [])]:
            require(isinstance(url,str) and not any(c.isspace() for c in url),'SIGNED_SOURCE_URL')
            if live:
                parsed=urlsplit(url)
                require(parsed.scheme=='https' and parsed.hostname and parsed.username is None and parsed.password is None and not parsed.fragment,'SIGNED_SOURCE_URL')
            else:
                match=re.fullmatch(r'http://127\.0\.0\.1:([1-9][0-9]{0,4})',url)
                require(match is not None and int(match.group(1))<=65535,'SIGNED_RETAINED_SOURCE_URL')
        require(isinstance(signed_finality,dict) and set(signed_finality)=={'checkpoint_root','proof_file','max_checkpoint_age_ms'},'SIGNED_FINALITY_FIELDS')
        require(isinstance(signed_finality['checkpoint_root'],str) and re.fullmatch(r'0x[0-9a-f]{64}',signed_finality['checkpoint_root']),'SIGNED_CHECKPOINT_ROOT')
        age=signed_finality['max_checkpoint_age_ms'];require(type(age) is int and 0<age<=7*86400000,'SIGNED_CHECKPOINT_AGE')
        path=Path(signed_finality['proof_file']);require(path.is_file() and not path.is_symlink() and path.stat().st_size<=16<<20,'SIGNED_PROOF_FILE')
        with path.open('rb') as stream:raw=stream.read((16<<20)+1)
        require(len(raw)<=16<<20,'SIGNED_PROOF_FILE')
        proof=json.loads(raw,object_pairs_hook=pairs)
        execution=proof['finality']['data']['finalized_header']['execution']
        require(isinstance(execution['block_number'],str) and re.fullmatch(r'[0-9]+',execution['block_number']),'SIGNED_EXECUTION_NUMBER')
        require(isinstance(execution['block_hash'],str) and re.fullmatch(r'0x[0-9a-f]{64}',execution['block_hash']),'SIGNED_EXECUTION_HASH')
        binding={'checkpoint_root':signed_finality['checkpoint_root'],'proof_sha256':sha(raw),'execution_number':int(execution['block_number']),'execution_hash':execution['block_hash']}
    else:
        require(len(endpoints)==2 and (tuple(endpoints)==ENDPOINTS if live else all(re.fullmatch(r'http://127\.0\.0\.1:\d+',url) for url in endpoints)),'SOURCE_ENDPOINT_SCOPE')
        raw=None;binding=None
    chain={'key':'evm:1','owner_service':'chain-parser-go','assignment_version':1,'genesis_hash':GENESIS,'rpc_urls':list(endpoints),'pools':[copy.deepcopy(POOL)],'confirmation_tag':'finalized','auto_reorg_window':128,'block_integrity':'known-evm-native-commitments-v2','receipts_mode':'bulk-auto','native_proof':{'max_bytes':64<<20,'max_headers':64,'max_window_blocks':16,'max_transactions':4096,'max_lookahead':32}}
    if signed:
        chain['beacon_finality']=copy.deepcopy(signed_finality)
        chain['rpc_retry']={'max_attempts':3,'base_backoff_ms':150,'max_backoff_ms':1500}
        if proof_endpoint is not None:chain['proof_rpc_url']=proof_endpoint
    return chain,binding,raw

def check_source_binding(evidence,reference,binding=None):
    require(evidence['confirmation_tag']=='finalized' and evidence['consensus_verified'] is False,'NORMAL_SOURCE_DISCLOSURE')
    if binding is not None:
        require(evidence['confirmation_policy']=='PINNED_BEACON_FINALITY','SIGNED_CONFIRMATION_POLICY')
        block=reference['block']
        require(block['hash']==binding['execution_hash'] and block['number_or_slot']==binding['execution_number'],'SIGNED_EXECUTION_BINDING')

def run_processes(output,owner_binary,router_binary,endpoints,live=False,owner_state=None,reject=False,replica=False,signed_finality=None,proof_endpoint=None,expected_startup_failure=None):
    require(expected_startup_failure is None or reject,'EXPECTED_FAILURE_REQUIRES_REJECTION')
    chain,binding,proof_bytes=source_configuration(endpoints,live,signed_finality,proof_endpoint)
    output=Path(output);output.mkdir(parents=True,exist_ok=False);output.chmod(0o700)
    if binding is not None:
        proof_path=output/'signed-proof.json';proof_path.touch(mode=0o600,exist_ok=False);proof_path.write_bytes(proof_bytes)
        chain['beacon_finality']['proof_file']=str(proof_path.resolve())
    token=secrets.token_urlsafe(32);ports={'owner':free_port(),'router':free_port(),'replica':free_port()};owner_url='http://127.0.0.1:'+str(ports['owner']);router_url='http://127.0.0.1:'+str(ports['router'])
    recent=live or binding is not None
    source_mode=('LIVE_EXPLICIT_SIGNED_RPC' if live else 'RETAINED_RECENT_SIGNED_RPC_BYTES') if binding is not None else ('LIVE_PUBLIC_RPC' if live else 'RETAINED_PUBLIC_BYTES_LOOPBACK_ONLY')
    children=[];streams=[];result={'passed':False,'source_mode':source_mode,'history_enabled':False,'production_ready':False,'consensus_verified':False,'public_transactions_submitted':False,'started_ms':int(time.time()*1000)};start=time.monotonic()
    try:
        selected=[('owner','chain-parser-go',owner_binary),('router','dex-router',router_binary)]
        if replica and not reject:selected.append(('replica','dex-router',router_binary))
        for alias,service,binary in selected:
            cfg=config_for(service,output/alias,token,ports[alias]);cfg['chains']=[copy.deepcopy(chain)];cfg['history']['enabled']=False;cfg['startup']['max_baseline_age_ms']=3600000 if recent else 315360000000;cfg['journal'].update(transport='http-development',source_urls=[owner_url],poll_interval_ms=60000 if alias=='owner' else 250,request_timeout_ms=60000,max_payload_bytes=16<<20)
            if alias=='owner':
                cfg['journal']['raw_evidence']={'max_bytes':512<<20,'max_records':10000,'max_record_bytes':128<<20}
                if owner_state is not None:cfg['state']['path']=str(Path(owner_state).resolve())
            else:
                cfg['chains'][0]={key:value for key,value in chain.items() if key not in ['block_integrity','receipts_mode','native_proof','beacon_finality','proof_rpc_url','rpc_retry']};cfg['chains'][0].update(rpc_urls=[],pools=[]);cfg['journal']['source_auth_tokens']={owner_url:token};cfg['routing'].update(max_hops=1,max_splits=1,max_state_age_ms=3600000 if recent else 315360000000,quote_ttl_ms=60000,route_activation_cost='0');cfg['api'].update(quote_timeout_ms=10000,quote_concurrency=1)
            path=output/(alias+'-config.json');path.write_text(json.dumps(cfg,indent=2)+'\n');path.chmod(0o600)
            checked=subprocess.run([str(binary),'--config',str(path),'--check-config'],cwd=ROOT,capture_output=True,text=True,timeout=10)
            require(checked.returncode==0,'NORMAL_CONFIG_REJECTED:'+alias)
            result[alias+'_binary_sha256']=sha(Path(binary).read_bytes());stream=(output/(alias+'.log')).open('w');streams.append(stream)
            children.append(subprocess.Popen([str(binary),'--config',str(path)],cwd=ROOT/service,stdout=stream,stderr=subprocess.STDOUT,start_new_session=True))
        deadline=time.monotonic()+(120 if recent else 30)
        while time.monotonic()<deadline:
            baseline=http(owner_url+'/internal/chains/evm:1/baseline',token);status=http(router_url+'/internal/chains/evm:1/status',token);result.update(baseline=baseline,router_status=status)
            if baseline['status']==200 and 'first_snapshot_ms' not in result:result['first_snapshot_ms']=round((time.monotonic()-start)*1000)
            if baseline['status']==200 and status.get('body',{}).get('ready'):break
            if children[0].poll() is not None:break
            require(children[1].poll() is None,'NORMAL_ROUTER_EXITED');time.sleep(.2)
        ready=result['baseline']['status']==200 and result['router_status'].get('body',{}).get('ready',False)
        if reject:
            require(not ready and children[0].poll() is not None and children[0].returncode!=0,'FAULT_DID_NOT_REJECT_OWNER')
            logs=[json.loads(line) for line in (output/'owner.log').read_text().splitlines() if line.strip()]
            failures=[line['fields']['error_code'] for line in logs if line.get('event')=='service_start_failed']
            require(len(failures)==1,'FAULT_STARTUP_FAILURE_NOT_OBSERVED');result['startup_failure_code']=failures[0]
            if expected_startup_failure is not None:
                require(failures[0]==expected_startup_failure,'UNEXPECTED_STARTUP_FAILURE')
            rejection_deadline=time.monotonic()+10
            while True:
                attempted=http(router_url+'/v1/quote',token,next(requests_for(POOL))[1])
                if attempted['status'] is not None or time.monotonic()>=rejection_deadline:break
                require(children[1].poll() is None,'FAULT_ROUTER_EXITED');time.sleep(.1)
            require(attempted['status']==503 and attempted.get('body',{}).get('error')=='CHAIN_NOT_READY','FAULT_DID_NOT_REJECT_HTTP_QUOTE');result['rejected_quote']=attempted
        else:
            require(ready,'NORMAL_SCOPE_NOT_READY');snapshot=result['baseline']['body'];require(len(snapshot['pools'])==1 and snapshot['pools'][0]['pool_id']==POOL['id'] and snapshot['pools'][0]['protocol']==POOL['protocol'],'NORMAL_POOL_SCOPE')
            pool=snapshot['pools'][0];ref=snapshot['reference'];require(pool['v2']['swap']['bank_block_hash']==ref['block']['hash'],'NORMAL_BANK_BINDING')
            result['quotes']=[];result['integer_oracles']=[]
            for route,request in requests_for(pool):
                response=http(router_url+route,token,request);result['integer_oracles'].append(check_quote(pool,ref,request,response));result['quotes'].append({'request':request,'response':response})
                if 'first_quote_ms' not in result:result['first_quote_ms']=round((time.monotonic()-start)*1000)
            result['source_evidence']=http(owner_url+'/internal/chains/evm:1/source-evidence',token);result['source_status']=http(owner_url+'/internal/chains/evm:1/source-status',token)
            require(result['source_evidence']['status']==200,'NORMAL_SOURCE_EVIDENCE_UNAVAILABLE')
            evidence=result['source_evidence']['body'];check_source_binding(evidence,ref,binding)
            if binding is not None:result['signed_finality']={**binding,'owner_admitted_signed_execution':True}
            history=result['source_status']['body']['history'];require(history.get('Enabled',history.get('enabled')) is False,'HISTORY_BLOCKED_CURRENT_SCOPE')
            require(history['Queue']['Declared'] and not history['Queue']['Complete'] and history['Queue']['Gaps'] and history['Queue']['FactBlocks']==0 and all(job['Attempts']==0 for job in history['Queue']['Jobs']),'HISTORY_GAP_NOT_EXPLICIT_OR_READINESS_DEPENDENT')
            require(not (output/'router/state').exists(),'ROUTER_UNEXPECTED_LOCAL_KV')
            if replica:
                peer_url='http://127.0.0.1:'+str(ports['replica']);peer_deadline=time.monotonic()+10
                while True:
                    peer_status=http(peer_url+'/internal/chains/evm:1/status',token)
                    if peer_status.get('body',{}).get('ready'):break
                    require(children[2].poll() is None and time.monotonic()<peer_deadline,'INDEPENDENT_REPLICA_NOT_READY');time.sleep(.1)
                route,request=next(requests_for(pool));stop(children[1])
                healthy=http(peer_url+route,token,request);check_quote(pool,ref,request,healthy)
                stream=(output/'router-restart.log').open('w');streams.append(stream)
                resumed=subprocess.Popen([str(router_binary),'--config',str(output/'router-config.json')],cwd=ROOT/'dex-router',stdout=stream,stderr=subprocess.STDOUT,start_new_session=True);children.append(resumed)
                cold_deadline=time.monotonic()+10
                while True:
                    cold=http(router_url+route,token,request)
                    if cold['status']==200:break
                    require(resumed.poll() is None and time.monotonic()<cold_deadline,'COLD_ROUTER_RESTART_NOT_READY');time.sleep(.1)
                check_quote(pool,ref,request,cold);require(not (output/'replica/state').exists() and not (output/'router/state').exists(),'REPLICA_RESTART_USED_LOCAL_KV')
                result['replica_recovery']={'healthy_peer_quote':healthy,'cold_restarted_quote':cold,'same_owner_reference':True,'both_routers_without_local_kv':True,'external_RPC_requests_by_router':0}
        result['passed']=True
    except BaseException as error:
        result['error']=str(error);raise
    finally:
        for child in children:stop(child)
        for stream in streams:stream.close()
        result['process_exit_codes']=[p.returncode for p in children];result['finished_ms']=int(time.time()*1000);(output/'report.json').write_text(json.dumps(result,indent=2)+'\n')
    return result

def unpack_fixture(directory,destination):
    """Materialize only authenticated regular records; never extract an archive."""
    import gzip
    directory=Path(directory);destination=Path(destination)
    manifest_path=directory/'manifest.json';require(manifest_path.is_file() and not manifest_path.is_symlink() and manifest_path.stat().st_size<=1<<20,'FIXTURE_MANIFEST_BUDGET')
    manifest=json.loads(manifest_path.read_bytes(),object_pairs_hook=pairs)
    require(manifest['schema_version']==1 and manifest['bundle_file']=='responses.json.gz' and manifest['record_count']==len(manifest['records']) and 0<manifest['record_count']<=1024,'FIXTURE_SHAPE')
    bundle=directory/manifest['bundle_file'];require(bundle.is_file() and not bundle.is_symlink() and bundle.stat().st_size<=32<<20,'FIXTURE_COMPRESSED_BUDGET')
    compressed=bundle.read_bytes();require(len(compressed)==manifest['bundle_bytes'] and sha(compressed)==manifest['bundle_sha256'],'FIXTURE_COMPRESSED_HASH')
    with gzip.GzipFile(fileobj=__import__('io').BytesIO(compressed)) as stream:decoded=stream.read((32<<20)+1)
    require(len(decoded)<=32<<20 and len(decoded)==manifest['decoded_bytes'] and sha(decoded)==manifest['decoded_sha256'],'FIXTURE_DECODED_HASH_OR_BUDGET')
    value=json.loads(decoded,object_pairs_hook=pairs);require(set(value)=={'schema_version','records'} and value['schema_version']==1 and len(value['records'])==manifest['record_count'],'FIXTURE_RECORDS')
    declared={row['file']:row for row in manifest['records']};require(len(declared)==len(manifest['records']),'FIXTURE_DUPLICATE_MEMBER')
    staged={}
    for row in value['records']:
        require(set(row)=={'name','data'} and re.fullmatch(r'[0-9a-f]{64}\.raw',row['name']) and row['name'] in declared and row['name'] not in staged,'FIXTURE_MEMBER_NAME')
        raw=base64.b64decode(row['data'],validate=True);entry=declared[row['name']]
        require(len(raw)<=32<<20 and len(raw)==entry['bytes'] and sha(raw)==entry['sha256'],'FIXTURE_MEMBER_HASH')
        staged[row['name']]=raw
    require(set(staged)==set(declared) and sum(map(len,staged.values()))==manifest['raw_bytes']<=MAX_CAPTURE_BYTES,'FIXTURE_MEMBERSHIP_OR_TOTAL')
    # Validate every nested envelope before publishing even the first record.
    import tempfile
    with tempfile.TemporaryDirectory(prefix='xone-fixture-validation-') as temporary:
        candidate=Path(temporary)
        for name,raw in staged.items():(candidate/name).write_bytes(raw)
        capture_index(candidate)
    require(not destination.exists() and not destination.is_symlink(),'FIXTURE_DESTINATION_EXISTS');destination.mkdir(parents=True)
    for name,raw in staged.items():
        with (destination/name).open('xb') as stream:stream.write(raw)
    return manifest


def signed_main():
    """Run the signed mode with already built ordinary service executables."""
    import argparse
    parser=argparse.ArgumentParser(description=signed_main.__doc__)
    parser.add_argument('owner',type=Path);parser.add_argument('router',type=Path);parser.add_argument('output',type=Path)
    parser.add_argument('--beacon-proof',type=Path,required=True)
    parser.add_argument('--checkpoint-root',required=True)
    parser.add_argument('--rpc-urls',nargs=2,required=True)
    parser.add_argument('--proof-rpc-url')
    parser.add_argument('--retained-signed',action='store_true',help='use only loopback retained recent RPC bytes; keep signed proof and one-hour freshness requirements')
    parser.add_argument('--max-checkpoint-age-ms',type=int,default=7*86400000)
    parser.add_argument('--replica',action='store_true')
    parser.add_argument('--expect-startup-failure',help='require this exact owner error and HTTP quote 503')
    args=parser.parse_args()
    result=run_processes(args.output.resolve(),args.owner.resolve(),args.router.resolve(),args.rpc_urls,
                         live=not args.retained_signed,replica=args.replica,reject=args.expect_startup_failure is not None,
                         signed_finality={'checkpoint_root':args.checkpoint_root,'proof_file':str(args.beacon_proof.resolve()),
                                          'max_checkpoint_age_ms':args.max_checkpoint_age_ms},
                         proof_endpoint=args.proof_rpc_url,expected_startup_failure=args.expect_startup_failure)
    print(json.dumps({key:result[key] for key in ('passed','source_mode','signed_finality','first_quote_ms',
                                                 'startup_failure_code','consensus_verified','production_ready') if key in result},indent=2))


if __name__=='__main__':signed_main()
