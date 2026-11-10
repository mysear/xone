#!/usr/bin/env python3
"""Controlled original LFJ reads -> normal Go -> actual PG/Kafka -> two Routers.

The peer is a controlled independent structural copy. Source/consensus and public
execution qualification remain outside this local calculation gate.
"""
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
import copy,hashlib,json,secrets,subprocess,tempfile,time,threading
from infrastructure import ROOT,Infrastructure,port
from configuration import config_for,save_config,service_command,sample_for
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import http,quote,wait_scope
from receipt_reference import EMPTY_ROOT,SINGLE_EMPTY_ROOT,SINGLE_EMPTY_FIELDS
from weighted_evidence import runtime_fingerprint,write_result

OUT=ROOT/'integration/artifacts/local'
REFERENCE=ROOT/'integration/test/contracts/lfj-v21-state.json'
TARGET=OUT/'core-lfj-normal-process.json'
PEER='0x000000000000000000000000000000000000aacc'
def require(ok,why):
 if not ok:raise ValueError(why)
def sha(path):return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def word(value):return f'{value:064x}'

class Chain:
 def __init__(self,reference):
  self.reference=reference;self.case=reference['cases'][42];self.head=100;self.branch=0;self.bad=None
  self.timestamp=self.case['pool']['lfj']['timestamp'];self.pinned=0;self.receipts=0
  self.dep=reference['deployment'];self.ids=[self.dep['pool'],PEER]
  self.calls={r['to']+':'+r['data']:r['result']for r in self.case['getter_reads']}
  require(len(self.calls)>=30,'LFJ_NATIVE_GETTER_SET')
 def block(self,n):
  h=lambda i:'0x'+hashlib.sha256(f'{self.branch if i>=101 else 0}/{i}'.encode()).hexdigest()
  value={'number':hex(n),'hash':'0x'+'99'*32 if n==0 else h(n),'parentHash':'0x'+'00'*32 if n==0 else h(n-1),'timestamp':hex(self.timestamp),'receiptsRoot':EMPTY_ROOT if n==0 else SINGLE_EMPTY_ROOT,'transactions':[]if n==0 else ['0x'+hashlib.sha256(('tx/'+h(n)).encode()).hexdigest()]}
  return value
 def config(self,pool_id):
  cfg=copy.deepcopy(self.case['config']);cfg['id']=pool_id;return cfg
 def dispatch(self,method,params):
  if method=='eth_chainId':return '0x7a69'
  if method=='eth_blockNumber':return hex(self.head)
  if method=='eth_getBlockByNumber':return self.block(self.head if params[0]in ['latest','safe','finalized']else int(params[0],16))
  if method=='eth_getBlockByHash':
   return next((self.block(n)for n in range(self.head+1)if self.block(n)['hash']==params[0]),None)
  if method in ['eth_getTransactionReceipt','eth_getBlockReceipts']:
   if method=='eth_getBlockReceipts':
    block=self.block(int(params[0],16))if params[0].startswith('0x') and len(params[0])<20 else next((self.block(n)for n in range(self.head+1)if self.block(n)['hash']==params[0]),None)
   else:block=next((self.block(n)for n in range(1,self.head+1)if params[0]in self.block(n)['transactions']),None)
   if not block or not block['transactions']:return []if method=='eth_getBlockReceipts'else None
   self.receipts+=1;receipt={'blockHash':block['hash'],'blockNumber':block['number'],'transactionHash':block['transactions'][0],'transactionIndex':'0x0','logs':[],**SINGLE_EMPTY_FIELDS}
   return [receipt]if method=='eth_getBlockReceipts'else receipt
  require(method in ['eth_call','eth_getCode','eth_getStorageAt'],'LFJ_UNEXPECTED_RPC:'+method)
  index=2 if method=='eth_getStorageAt'else 1;anchor=params[index]
  require(isinstance(anchor,dict)and anchor.get('requireCanonical')is True and any(anchor.get('blockHash')==self.block(n)['hash']for n in range(self.head+1)),'LFJ_UNBOUND_CURRENT_READ')
  self.pinned+=1
  if method=='eth_getCode':
   address=params[0]
   if address in self.ids:return '0x6000'if self.bad else self.dep['pool_code']
   for role in ['factory','implementation','token0','token1']:
    if address==self.dep[role]:return self.dep[role+'_code']
   raise ValueError('LFJ_UNKNOWN_CODE_ACCOUNT')
  if method=='eth_getStorageAt':
   require(params[0]in self.ids,'LFJ_UNKNOWN_STORAGE_ACCOUNT');require(params[1]in self.case['storage_reads'],'LFJ_UNKNOWN_STORAGE_WORD');return self.case['storage_reads'][params[1]]
  call=params[0];require(set(call)=={'to','data'},'LFJ_CALL_OVERRIDES')
  target=call['to'];data=call['data']
  if target in self.ids:target=self.dep['pool']
  elif target in [self.dep['token0'],self.dep['token1']]:
   require(data[10:]in [word(int(i,16))for i in self.ids],'LFJ_TOKEN_BALANCE_ACCOUNT')
   data=data[:10]+word(int(self.dep['pool'],16))
  else:raise ValueError('LFJ_UNKNOWN_GETTER_ACCOUNT')
  key=target+':'+data;require(key in self.calls,'LFJ_UNRECORDED_ORIGINAL_GETTER:'+key);return self.calls[key]
 def provider(self):
  chain=self
  class Handler(BaseHTTPRequestHandler):
   def log_message(self,*_):pass
   def do_POST(self):
    q=json.loads(self.rfile.read(int(self.headers['Content-Length'])));response={'jsonrpc':'2.0','id':q['id']}
    try:response['result']=chain.dispatch(q['method'],q['params'])
    except Exception as error:response['error']={'code':-32000,'message':str(error)}
    raw=json.dumps(response).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
  server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start();return server

def expected(reference,zero,amount,parts=20):
 case=reference['cases'][42 if zero else 90];require(case['zero_for_one']==zero and case['allocation_points'],'LFJ_NATIVE_ORACLE_SCOPE')
 points={int(p['input']):(int(p['amount_out'])if p['amount_in_left']=='0'else None)for p in case['allocation_points']}
 values=[points[amount]]if points[amount]is not None else []
 for part in range(1,parts):
  a=amount*part//parts;b=amount-a
  if points[a]is not None and points[b]is not None:values.append(points[a]+points[b])
 require(values,'LFJ_NO_ORIGINAL_GRID_ALLOCATION');return max(values),points

def canonical_hash(value):
 text=json.dumps(value,sort_keys=True,separators=(',',':'),ensure_ascii=False)
 for a,b in [('<','\\u003c'),('>','\\u003e'),('&','\\u0026'),('\u2028','\\u2028'),('\u2029','\\u2029')]:text=text.replace(a,b)
 return hashlib.sha256(text.encode()).hexdigest()

def rebind(record):
 plan=record['execution_plan'];q=record['quote'];req=record['request']
 plan['plan_hash']=canonical_hash({k:plan[k]for k in ['context','snapshot','allocation','min_buy_amount','economic_policy_hash']})
 q['planHash']=plan['plan_hash']
 typed=copy.deepcopy(req);typed.setdefault('constraints',None)
 q['requestHash']=canonical_hash({'request':typed,'view_id':q['quoteViewId'],'policy':q['economicPolicyHash'],'engine':'xone-router/0.1'})
 q['quoteId']=canonical_hash({'request_hash':q['requestHash'],'plan_hash':q['planHash'],'expires':q['validity']['expiresAtMs']})

def guards(reference_path,captures,projection,projector,tag):
 base={'schema_version':1,'records':[copy.deepcopy(captures[0])]};checks=[]
 def model_reject(name,mutation,reason):
  bad=copy.deepcopy(base);mutation(bad['records'][0]);input_path=OUT/f'core-lfj-guard-{tag}-{name}.json';output=OUT/f'core-lfj-guard-{tag}-{name}-output.json';write_result(input_path,bad)
  run=subprocess.run([str(projector),str(input_path),str(output)],capture_output=True,text=True,timeout=30);text=run.stdout+run.stderr;(OUT/f'core-lfj-guard-{tag}-{name}.log').write_text(text)
  require(run.returncode!=0 and reason in text and not output.exists(),'LFJ_MODEL_GUARD_FAILED:'+name);checks.append(name)
 model_reject('plan-hash',lambda r:r['execution_plan'].__setitem__('plan_hash','00'*32),'PLAN_HASH_MISMATCH')
 model_reject('bank-clock-absent',lambda r:r['snapshot']['pools'][0]['lfj'].pop('timestamp'),'missing field `timestamp`')
 def self_fund(r):
  r['request']['caller']=r['snapshot']['pools'][0]['pool_id'];r['execution_plan']['context']['caller']=r['request']['caller'];rebind(r)
 model_reject('rehashed-self-funding',self_fund,'LFJ_UNMODELED_SELF_FUNDING')
 def partial_input(r):
  r['request']['sellAmount']=str(int(r['request']['sellAmount'])+1);r['quote']['sellAmount']=r['request']['sellAmount'];r['quote']['consumedIn']=r['request']['sellAmount'];rebind(r)
 model_reject('rehashed-partial-input',partial_input,'LFJ_PROJECT_INPUT_CONSERVATION')
 model_reject('raw-reserve-accounting',lambda r:r['snapshot']['pools'][0]['lfj'].__setitem__('raw_reserve0','0'),'LFJ_RESERVE_ACCOUNTING')
 input_path=OUT/f'core-lfj-guard-{tag}-native-base-input.json';write_result(input_path,base)
 good_model=copy.deepcopy(projection);good_model['records']=good_model['records'][:1];good_model['input_sha256']=sha(input_path)
 def native_reject(name,mutation,reason,reference_mutation=None,input_mutation=None,rebind_input_sha=False):
  model=copy.deepcopy(good_model);mutation(model)
  inp=input_path;ref=reference_path
  if reference_mutation:
   document=json.loads(reference_path.read_text());reference_mutation(document);ref=OUT/f'core-lfj-guard-{tag}-{name}-reference.json';write_result(ref,document)
  if input_mutation:
   document=copy.deepcopy(base);input_mutation(document);inp=OUT/f'core-lfj-guard-{tag}-{name}-input.json';write_result(inp,document)
  if rebind_input_sha:model['input_sha256']=sha(inp)
  modeled=OUT/f'core-lfj-guard-{tag}-{name}-model.json';output=OUT/f'core-lfj-guard-{tag}-{name}-native.json';write_result(modeled,model)
  run=subprocess.run(['node',str(ROOT/'integration/tools/ci/lfj-v21-plan-replay.cjs'),str(ref),str(inp),str(modeled),str(output)],capture_output=True,text=True,timeout=90);text=run.stdout+run.stderr;(OUT/f'core-lfj-guard-{tag}-{name}.log').write_text(text)
  require(run.returncode!=0 and reason in text and not output.exists(),'LFJ_NATIVE_GUARD_FAILED:'+name);checks.append(name)
 native_reject('native-poststate',lambda m:m['records'][0]['steps'][0]['after']['lfj'].__setitem__('parameters',str(int(m['records'][0]['steps'][0]['after']['lfj']['parameters'])^1)),'LFJ_NATIVE_COMPLETE_POSTSTATE:parameters')
 native_reject('native-pool-binding',lambda m:m['records'][0]['steps'][0].__setitem__('pool_id','0x000000000000000000000000000000000000dead'),'LFJ_NATIVE_SELECTED_ROUTE_BINDING')
 native_reject('native-code-binding',lambda m:None,'LFJ_NATIVE_ORIGINAL_RUNTIME_BINDING:implementation_code',reference_mutation=lambda r:r['deployment'].__setitem__('implementation_code','0x6000'))
 native_reject('native-input-binding',lambda m:None,'LFJ_NATIVE_ACTUAL_API_INPUT_BINDING',input_mutation=lambda d:d['records'][0]['quote'].__setitem__('grossBuyAmount','1'))
 native_reject('native-u64-precision',lambda m:None,'LFJ_NATIVE_CAPTURE_SOURCE',input_mutation=lambda d:d['records'][0]['snapshot']['pools'][0].__setitem__('fee_denominator',1000000000000000001),rebind_input_sha=True)
 return {'passed':True,'mutations':len(checks),'checks':checks,'authority':'negative controls after successful actual model/native replay; no completion or production upgrade'}

def run():
 require(__debug__,'LFJ_PROCESS_REQUIRES_ASSERTIONS')
 reference=json.loads(REFERENCE.read_text());before=runtime_fingerprint();chain=Chain(reference)
 tools={str(p.relative_to(ROOT)):sha(p)for p in [Path(__file__).resolve(),ROOT/'integration/tools/ci/lfj-v21-plan-replay.cjs',ROOT/'dex-router/target/debug/examples/lfj-plan-projector']}
 original_reference=(ROOT/reference['original_reference']['path']).resolve();require(original_reference.is_relative_to(OUT.resolve()) and original_reference.name.startswith('core-lfj-v21-original-reference'),'LFJ_ORIGINAL_REFERENCE_LOCALITY');require(sha(original_reference)==reference['original_reference']['sha256'],'LFJ_ORIGINAL_REFERENCE_INPUT')
 require(reference['cases'][42]['pool']['lfj']==reference['cases'][90]['pool']['lfj'],'LFJ_DIRECTIONAL_SOURCE_BANK_MISMATCH')
 servers=[chain.provider(),chain.provider()];processes={};logs={};captures=[]
 report={'schema_version':1,'passed':False,'production_ready':False,'all_requested_features_complete':False,'authority':'original getter/storage observations in a controlled fixed-clock RPC envelope; normal Go, actual local PG/Kafka, two normal Routers; unaffected Solana fixture lane','checks':[],'reference_sha256':sha(REFERENCE),'source_tree_sha256':before['source_tree_sha256'],'runtime_fingerprint':before,'peer_source':'independent structural copy of the primary original bank, same code and immutable args; no public deployment qualification'}
 try:
  with Infrastructure()as infra,tempfile.TemporaryDirectory(prefix='xone-lfj-')as folder:
   root=Path(folder);token=secrets.token_urlsafe(32);names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'};ports={a:port()for a in names};configs={};source_urls=[f'http://127.0.0.1:{ports[a]}'for a in ['go','rust']]
   freshness=max(120000,int(time.time()*1000)-chain.timestamp*1000+600000)
   report['offline_freshness']={'original_clock_ms':chain.timestamp*1000,'max_age_ms':freshness,'timestamp_rewritten':False,'production_freshness_claimed':False}
   for alias,name in names.items():
    cfg=config_for(name,root/alias,token,ports[alias]);cfg['journal'].update(transport='kafka',source_urls=source_urls,poll_interval_ms=100,request_timeout_ms=15000)
    cfg['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=alias,transaction_timeout_ms=4000)
    cfg['control'].update(backend='disabled'if name=='dex-router'else'postgres',dsn_file=''if name=='dex-router'else str(infra.dsnfile),instance_id=alias,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000)
    cfg['startup']['max_baseline_age_ms']=freshness
    if alias=='go':cfg['chains'][0].update(genesis_hash='0x'+'99'*32,confirmation_tag='latest',auto_reorg_window=4,rpc_urls=[f'http://127.0.0.1:{s.server_port}'for s in servers],pools=[chain.config(i)for i in chain.ids])
    elif alias=='rust':sample_for('solana:localnet',root/alias/'fixtures',chain.timestamp*1000)
    else:cfg['routing'].update(max_hops=1,max_splits=2,split_parts=20,max_expansions=1000,max_state_age_ms=freshness,quote_ttl_ms=60000);cfg['api']['quote_timeout_ms']=10000
    configs[alias]=save_config(root/alias/'config',cfg)
   base=lambda alias:f'http://127.0.0.1:{ports[alias]}'
   def start(alias):
    logs[alias]=(OUT/f'core-lfj-{alias}.log').open('ab');processes[alias]=subprocess.Popen(service_command(names[alias],configs[alias],root/alias/'fixtures'if alias=='rust'else None),cwd=ROOT/names[alias],stdout=logs[alias],stderr=subprocess.STDOUT)
   def stop(alias):
    process=processes.pop(alias,None)
    if process:
     process.terminate()
     try:process.wait(timeout=10)
     except subprocess.TimeoutExpired:process.kill();process.wait()
    if alias in logs:logs.pop(alias).close()
   def status(alias):return http(base(alias)+'/internal/chains/evm:31337/status',token)
   def check(alias,zero=True,amount=2500000):
    req={'chainKey':'evm:31337','kind':'EXACT_IN','sellToken':chain.dep['token0'if zero else'token1'],'buyToken':chain.dep['token1'if zero else'token0'],'sellAmount':str(amount),'caller':chain.dep['sender'],'recipient':chain.dep['recipient'],'slippageBps':50,'verification':'FAST','constraints':{'maxHops':1}}
    code,result=http(base(alias)+'/v1/quote',token,req);require(code==200,'LFJ_NORMAL_QUOTE:'+str((code,result)))
    optimum,points=expected(reference,zero,amount);require(result['grossBuyAmount']==str(optimum),'LFJ_ORIGINAL_GRID_MISMATCH')
    require(result['consumedIn']==str(amount)and result['refund']=='0'and result['verification']['level']=='INDICATIVE'and result['netBuyAmountEstimate']is None,'LFJ_NORMAL_AMOUNT_VERIFICATION')
    require(result['quality']['allocationSearch']['exhaustiveWithinGrid']is True and result['quality']['budgetExhausted']is False,'LFJ_NORMAL_GRID_COMPLETENESS')
    require(set(result['quality']['candidateScope'])==set(chain.ids),'LFJ_NORMAL_POOL_SCOPE')
    for route in result['routes']:
     require(route['amount_out']==str(points[int(route['amount_in'])]),'LFJ_ROUTE_ORIGINAL_VIEW_AMOUNT')
    code,snapshot=http(base('go')+'/internal/chains/evm:31337/baseline',token);require(code==200,'LFJ_NORMAL_BASELINE')
    require(len(snapshot['pools'])==2,'LFJ_NORMAL_BASELINE_SCOPE')
    for pool in snapshot['pools']:
     original=copy.deepcopy(chain.case['pool']);original['pool_id']=pool['pool_id'];original['lfj'].update(block_number=snapshot['reference']['block']['number_or_slot'],block_hash=snapshot['reference']['block']['hash'])
     require(pool==original,'LFJ_NORMAL_COMPLETE_SOURCE_STATE')
    code,evidence=http(base(alias)+'/internal/quotes/'+result['quoteId']+'/evidence',token);require(code==200 and evidence['snapshot_valid']is True and evidence['expired']is False,'LFJ_ACTUAL_PLAN_EVIDENCE')
    captures.append({'case_index':42,'snapshot':snapshot,'request':req,'quote':result,'execution_plan':evidence['execution_plan']})
    return result
   try:
    for alias in ['go','rust']:start(alias);wait(base(alias)+'/health/ready')
    for alias in ['router1','router2']:start(alias);wait(base(alias)+'/health/live');wait_scope(base(alias),token,'evm:31337')
    for alias in ['router1','router2']:
     for zero in [True,False]:
      for amount in [2500000,5000000]:check(alias,zero,amount)
    report['checks'].append('normal-Go-LFJ-field31-real-Kafka-two-Router-source-and-native-grid-both-directions')
    req=copy.deepcopy(captures[0]['request']);req['verification']='VERIFIED';code,body=http(base('router1')+'/v1/quote',token,req);require(code==422 and 'quoteId'not in body,'LFJ_FALSE_VERIFIED_QUOTE')
    req=copy.deepcopy(captures[0]['request']);req['caller']=chain.ids[0];code,body=http(base('router1')+'/v1/quote',token,req);require(code in [404,422]and'quoteId'not in body,'LFJ_SELF_FUNDED_NORMAL_QUOTE')
    report['checks'].append('normal-API-rejects-unsupported-self-funding-and-false-VERIFIED')
    for height in [101,102]:
     chain.head=height
     for alias in ['router1','router2']:await_condition(lambda:status(alias)[1].get('snapshot_ref',{}).get('block',{}).get('number_or_slot')==height,'LFJ current block not committed')
    old={a:check(a)for a in ['router1','router2']};chain.branch=1
    for alias in old:await_condition(lambda:status(alias)[1].get('snapshot_ref',{}).get('canonical_epoch',0)>old[alias]['snapshotRef']['canonical_epoch'],'LFJ reorg epoch not replaced');check(alias,False)
    report['checks'].append('reorg-preserves-complete-LFJ-scope-and-revokes-old-canonical-epoch')
    old={a:check(a)for a in ['router1','router2']};chain.bad='code';chain.head+=1
    for alias in old:await_condition(lambda:status(alias)[0]==503,'LFJ bad code did not revoke scope');quote(base(alias),token,'solana:localnet','B')
    report['checks'].append('bad-LFJ-clone-code-revokes-only-affected-chain')
    chain.bad=None;chain.head+=1
    for alias in old:await_condition(lambda:status(alias)[1].get('snapshot_ref',{}).get('online_generation',0)>old[alias]['onlineGeneration'],'LFJ qualified scope did not restore');check(alias,False,5000000)
    report['checks'].append('restored-source-publishes-higher-generation-and-native-two-route-output')
    stop('router1');start('router1');wait(base('router1')+'/health/live');wait_scope(base('router1'),token,'evm:31337');check('router1')
    report['checks'].append('memory-only-Router-restart-restores-LFJ-field31-without-old-history-replay')
    old_generation=status('router2')[1]['snapshot_ref']['online_generation'];stop('go');start('go');wait(base('go')+'/health/ready')
    for alias in ['router1','router2']:await_condition(lambda:status(alias)[1].get('snapshot_ref',{}).get('online_generation',0)>old_generation,'LFJ normal Go restart did not advance generation');check(alias)
    report['checks'].append('normal-Go-same-KV-restart-restores-complete-LFJ-scope')
   finally:
    for alias in list(processes):stop(alias)
 finally:
  for server in servers:server.shutdown();server.server_close()
 require(runtime_fingerprint()==before,'LFJ_SOURCE_OR_BINARIES_CHANGED')
 tag=secrets.token_hex(6);captured=OUT/f'core-lfj-api-input-{tag}.json';projected=OUT/f'core-lfj-api-model-{tag}.json';native=OUT/f'core-lfj-api-native-{tag}.json'
 write_result(captured,{'schema_version':1,'records':captures})
 projector=ROOT/'dex-router/target/debug/examples/lfj-plan-projector';require(projector.is_file(),'LFJ_MODEL_PROJECTOR_BINARY_REQUIRED')
 projected_run=subprocess.run([str(projector),str(captured),str(projected)],capture_output=True,text=True,timeout=120);(OUT/f'core-lfj-api-model-{tag}.log').write_text(projected_run.stdout+projected_run.stderr);require(projected_run.returncode==0,'LFJ_ACTUAL_API_MODEL_PROJECTION')
 native_run=subprocess.run(['node',str(ROOT/'integration/tools/ci/lfj-v21-plan-replay.cjs'),str(REFERENCE),str(captured),str(projected),str(native)],capture_output=True,text=True,timeout=180);(OUT/f'core-lfj-api-native-{tag}.log').write_text(native_run.stdout+native_run.stderr);require(native_run.returncode==0,'LFJ_ACTUAL_API_ORIGINAL_RUNTIME_REPLAY')
 actual=json.loads(native.read_text());require(actual.get('passed')is True and len(actual['records'])==len(captures),'LFJ_ACTUAL_REPLAY_RECORD_COVERAGE')
 require(runtime_fingerprint()==before,'LFJ_SOURCE_CHANGED_DURING_REPLAY')
 negative=guards(REFERENCE,captures,json.loads(projected.read_text()),projector,tag)
 require(runtime_fingerprint()==before and all(sha(ROOT/p)==digest for p,digest in tools.items()),'LFJ_FINAL_SOURCE_OR_TOOLS_CHANGED')
 report.update(passed=True,tool_digests=tools,negative_controls=negative,captured_api_records=len(captures),original_native_swaps=actual['native_swaps'],pinned_rpc_reads=chain.pinned,receipt_reads=chain.receipts,actual_api_input={'path':str(captured.relative_to(ROOT)),'sha256':sha(captured)},model_projection={'path':str(projected.relative_to(ROOT)),'sha256':sha(projected)},original_native_replay={'path':str(native.relative_to(ROOT)),'sha256':sha(native)},measured_at_unix_ms=int(time.time()*1000),projector_sha256=sha(projector))
 report['checks'].append('actual-selected-API-routes-match-original-LBPair-token-deltas-and-complete-poststates')
 return report

if __name__=='__main__':
 try:result=run()
 except BaseException as error:write_result(TARGET,{'schema_version':1,'passed':False,'production_ready':False,'all_requested_features_complete':False,'error':str(error),'error_type':type(error).__name__});raise
 write_result(TARGET,result);print(json.dumps(result,indent=2))
