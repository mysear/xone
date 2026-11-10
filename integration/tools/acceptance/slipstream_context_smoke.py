#!/usr/bin/env python3
"""Original source stages -> normal Go/PG/Kafka -> actual contextual API plans.
Only controlled program banks and a synthetic header envelope are used.
"""
from pathlib import Path
import copy,json,secrets,subprocess,tempfile,time,hashlib
from infrastructure import ROOT,Infrastructure,port
from configuration import config_for,save_config,service_command,sample_for
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import http,wait_scope,quote
from lfj_smoke import Chain as HeaderChain,require,sha,rebind
from weighted_evidence import runtime_fingerprint,write_result
OUT=ROOT/'integration/artifacts/local'
REFERENCE=ROOT/'integration/test/contracts/slipstream-context-state.json'
TARGET=OUT/'core-slipstream-context-normal-process.json'

class Chain(HeaderChain):
 def __init__(self,reference):
  self.reference=reference;self.dep=reference['deployment'];self.head=100;self.branch=0;self.bad=None;self.pinned=0;self.receipts=0
  sequence=next(s for s in reference['sequences']if reference['cases'][s['initial_case_index']]['zero_for_one']and s['steps'][0]['input']=='6000000000')
  self.sequence=sequence;self.case=reference['cases'][sequence['initial_case_index']];self.timestamp=self.case['pool']['slipstream_context']['timestamp'];self.ids=[self.dep['pool']];self.stage=0
 def current(self):
  if self.stage==0:return self.case['pool'],self.case['getter_reads']
  step=self.sequence['steps'][self.stage-1];return step['canonical_pool'],step['getter_reads']
 def dispatch(self,method,params):
  if method not in ['eth_getCode','eth_call','eth_getStorageAt']:return super().dispatch(method,params)
  require(method!='eth_getStorageAt','SLIPSTREAM_UNEXPECTED_DIRECT_STORAGE')
  require(isinstance(params[1],dict)and params[1].get('requireCanonical')is True and any(params[1].get('blockHash')==self.block(n)['hash']for n in range(self.head+1)),'SLIPSTREAM_UNBOUND_SOURCE_READ')
  self.pinned+=1
  if method=='eth_getCode':
   address=params[0]
   if address==self.dep['sender']:return '0x'
   for key in ['pool','factory','implementation','voter','token0','token1','swap_custom','unstaked_custom','swap_dynamic']:
    if address==self.dep[key]:return '0x6000'if self.bad and key=='swap_dynamic'else self.dep[key+'_code']
   raise ValueError('SLIPSTREAM_UNRECORDED_CODE')
  _,records=self.current();call=params[0];require(set(call)<=set(['to','data','from']),'SLIPSTREAM_CALL_OVERRIDES');origin=call.get('from','0x'+'0'*40)
  values={r['to']+':'+r['data']+':'+r['from']:r['result']for r in records};key=call['to']+':'+call['data']+':'+origin
  require(key in values,'SLIPSTREAM_UNRECORDED_ORIGINAL_GETTER:'+key);return values[key]

def guards(captures,projection,projector,tag):
 source={'schema_version':1,'records':[copy.deepcopy(captures[0])]};checks=[]
 def model_reject(name,mutation,reason):
  document=copy.deepcopy(source);mutation(document['records'][0]);inputs=OUT/f'core-slipstream-guard-{tag}-{name}.json';output=OUT/f'core-slipstream-guard-{tag}-{name}-output.json';write_result(inputs,document)
  result=subprocess.run([str(projector),str(inputs),str(output)],capture_output=True,text=True,timeout=45);text=result.stdout+result.stderr;(OUT/f'core-slipstream-guard-{tag}-{name}.log').write_text(text)
  require(result.returncode!=0 and reason in text and not output.exists(),'SLIPSTREAM_MODEL_GUARD:'+name);checks.append(name)
 model_reject('plan-hash',lambda r:r['execution_plan'].__setitem__('plan_hash','00'*32),'PLAN_HASH_MISMATCH')
 model_reject('derived-source',lambda r:r['snapshot']['pools'][0]['slipstream_context'].__setitem__('derived',True),'SLIPSTREAM_DERIVED_STATE_NOT_CANONICAL')
 model_reject('current-fee-view',lambda r:r['snapshot']['pools'][0]['slipstream_context'].__setitem__('view_swap_fee',101),'SLIPSTREAM_CURRENT_FEE_VIEW_MISMATCH')
 def origin(r):r['request']['caller']='0x0000000000000000000000000000000000000001';r['execution_plan']['context']['caller']=r['request']['caller'];rebind(r)
 model_reject('rehashed-unknown-origin',origin,'SLIPSTREAM_ORIGIN_SCOPE_UNAVAILABLE')
 def recipient(r):r['request']['recipient']=r['snapshot']['pools'][0]['pool_id'];r['execution_plan']['context']['recipient']=r['request']['recipient'];rebind(r)
 model_reject('rehashed-self-recipient',recipient,'SLIPSTREAM_UNMODELED_SELF_FUNDING')
 base_input=OUT/f'core-slipstream-guard-{tag}-native-base-input.json';write_result(base_input,source);base_model=copy.deepcopy(projection);base_model['records']=base_model['records'][:1];base_model['input_sha256']=sha(base_input)
 def native_reject(name,mutation,reason,mutate_input=None,mutate_reference=None):
  body=copy.deepcopy(base_model);mutation(body);inputs=base_input;reference=REFERENCE
  if mutate_input:
   document=copy.deepcopy(source);mutate_input(document);inputs=OUT/f'core-slipstream-guard-{tag}-{name}-input.json';write_result(inputs,document);body['input_sha256']=sha(inputs)
  if mutate_reference:
   document=json.loads(REFERENCE.read_text());mutate_reference(document);reference=OUT/f'core-slipstream-guard-{tag}-{name}-reference.json';write_result(reference,document)
  model=OUT/f'core-slipstream-guard-{tag}-{name}-model.json';output=OUT/f'core-slipstream-guard-{tag}-{name}-native.json';write_result(model,body)
  result=subprocess.run(['node',str(ROOT/'integration/tools/ci/slipstream-context-plan-replay.cjs'),str(reference),str(inputs),str(model),str(output)],capture_output=True,text=True,timeout=120);text=result.stdout+result.stderr;(OUT/f'core-slipstream-guard-{tag}-{name}.log').write_text(text)
  require(result.returncode!=0 and reason in text and not output.exists(),'SLIPSTREAM_NATIVE_GUARD:'+name);checks.append(name)
 native_reject('native-poststate',lambda d:d['records'][0]['steps'][0]['after']['slipstream_context'].__setitem__('fee_growth_global0','1'),'SLIPSTREAM_NATIVE_COMPLETE_AFTER_STATE')
 def source_input(d):d['records'][0]['snapshot']['pools'][0]['slipstream_context']['staked_liquidity']='1'
 def source_model(d):
  d['records'][0]['snapshot']['pools'][0]['slipstream_context']['staked_liquidity']='1';d['records'][0]['steps'][0]['before']['slipstream_context']['staked_liquidity']='1'
 native_reject('native-original-source-bank',source_model,'SLIPSTREAM_NATIVE_ORIGINAL_BANK_MUTATION_BINDING',mutate_input=source_input)
 native_reject('native-runtime-source',lambda d:None,'SLIPSTREAM_NATIVE_RUNTIME_BINDING:implementation_code',mutate_reference=lambda d:d['deployment'].__setitem__('implementation_code','0x6000'))
 return {'passed':True,'mutations':len(checks),'checks':checks,'authority':'post-positive model/native negative controls; all require specific rejection and no successful output'}

def run():
 require(__debug__,'SLIPSTREAM_PROCESS_REQUIRES_ASSERTIONS');reference=json.loads(REFERENCE.read_text());chain=Chain(reference);before=runtime_fingerprint();servers=[chain.provider(),chain.provider()];processes={};logs={};captures=[]
 report={'schema_version':1,'passed':False,'source_tree_sha256':before['source_tree_sha256'],'runtime_fingerprint':before,'authority':'original byte getter results and physical original post banks in a synthetic fixed-clock RPC envelope; normal Go, actual local PG/Kafka and two normal Routers','reference_sha256':sha(REFERENCE),'all_requested_features_complete':False,'production_ready':False,'checks':[]}
 try:
  with Infrastructure()as infra,tempfile.TemporaryDirectory(prefix='xone-slipstream-context-')as directory:
   root=Path(directory);token=secrets.token_urlsafe(32);names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'};ports={a:port()for a in names};configs={};source_urls=[f'http://127.0.0.1:{ports[a]}'for a in ['go','rust']];freshness=max(120000,int(time.time()*1000)-chain.timestamp*1000+600000)
   for alias,name in names.items():
    cfg=config_for(name,root/alias,token,ports[alias]);cfg['journal'].update(transport='kafka',source_urls=source_urls,poll_interval_ms=100,request_timeout_ms=15000);cfg['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=alias,transaction_timeout_ms=4000)
    cfg['control'].update(backend='disabled'if name=='dex-router'else'postgres',dsn_file=''if name=='dex-router'else str(infra.dsnfile),instance_id=alias,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000);cfg['startup']['max_baseline_age_ms']=freshness
    if alias=='go':cfg['chains'][0].update(genesis_hash='0x'+'99'*32,confirmation_tag='latest',auto_reorg_window=4,rpc_urls=[f'http://127.0.0.1:{s.server_port}'for s in servers],pools=[chain.case['config']])
    elif alias=='rust':sample_for('solana:localnet',root/alias/'fixtures',chain.timestamp*1000)
    else:cfg['routing'].update(max_hops=1,max_splits=1,max_state_age_ms=freshness,quote_ttl_ms=60000);cfg['api']['quote_timeout_ms']=10000
    configs[alias]=save_config(root/alias/'config',cfg)
   base=lambda a:f'http://127.0.0.1:{ports[a]}'
   def start(alias):
    logs[alias]=(OUT/f'core-slipstream-context-{alias}.log').open('ab');processes[alias]=subprocess.Popen(service_command(names[alias],configs[alias],root/alias/'fixtures'if alias=='rust'else None),cwd=ROOT/names[alias],stdout=logs[alias],stderr=subprocess.STDOUT)
   def stop(alias):
    p=processes.pop(alias,None)
    if p:
     p.terminate()
     try:p.wait(timeout=10)
     except subprocess.TimeoutExpired:p.kill();p.wait()
    if alias in logs:logs.pop(alias).close()
   def status(alias):return http(base(alias)+'/internal/chains/evm:31337/status',token)
   def check(alias,zero=True,amount=None):
    step=chain.sequence['steps'][chain.stage];amount=amount or step['input'];require(chain.stage==0 or zero,'SLIPSTREAM_STAGE_DIRECTION')
    if chain.stage==0:
     expected=next(c for c in reference['cases']if c['mode']=='dynamic'and c['zero_for_one']==zero and c['input']==amount and c['pool']['slipstream_context']['staked_liquidity']=='500000000000')['original_expected']['amount_out']
    else:expected=step['amount_out']
    request={'chainKey':'evm:31337','kind':'EXACT_IN','sellToken':chain.dep['token0'if zero else'token1'],'buyToken':chain.dep['token1'if zero else'token0'],'sellAmount':amount,'caller':chain.dep['sender'],'recipient':chain.dep['recipient'],'slippageBps':50,'verification':'FAST','constraints':{'maxHops':1}}
    code,result=http(base(alias)+'/v1/quote',token,request);require(code==200,'SLIPSTREAM_CONTEXT_QUOTE:'+str((code,result)))
    require(result['grossBuyAmount']==expected and result['consumedIn']==amount and result['refund']=='0','SLIPSTREAM_CONTEXT_NATIVE_AMOUNT');require(result['verification']['level']=='INDICATIVE'and result['netBuyAmountEstimate']is None,'SLIPSTREAM_CONTEXT_VERIFICATION_BOUNDARY')
    code,snapshot=http(base('go')+'/internal/chains/evm:31337/baseline',token);require(code==200 and len(snapshot['pools'])==1,'SLIPSTREAM_CONTEXT_SOURCE_BASELINE')
    original=copy.deepcopy(chain.current()[0]);original['slipstream_context'].update(block_hash=snapshot['reference']['block']['hash'],block_number=snapshot['reference']['block']['number_or_slot'],derived=False);require(snapshot['pools']==[original],'SLIPSTREAM_CONTEXT_COMPLETE_PARSED_BANK')
    code,evidence=http(base(alias)+'/internal/quotes/'+result['quoteId']+'/evidence',token);require(code==200 and evidence['snapshot_valid']is True and evidence['expired']is False,'SLIPSTREAM_CONTEXT_ACTUAL_PLAN')
    bank=next(b for b in reference['source_banks']if b['original_case_index']==chain.case['index']and b['source_step']==chain.stage-1)
    captures.append({'case_index':bank['id'],'snapshot':snapshot,'request':request,'quote':result,'execution_plan':evidence['execution_plan']});return result
   try:
    for a in ['go','rust']:start(a);wait(base(a)+'/health/ready')
    for a in ['router1','router2']:start(a);wait(base(a)+'/health/live');wait_scope(base(a),token,'evm:31337')
    for a in ['router1','router2']:
     check(a,True);check(a,False)
    report['checks'].append('normal-source-field32-two-replicas-both-directions-match-original')
    for stage in [1,2]:
     chain.stage=stage;chain.head+=1
     for a in ['router1','router2']:await_condition(lambda:status(a)[1].get('snapshot_ref',{}).get('block',{}).get('number_or_slot')==chain.head,'contextual post-bank did not publish');check(a)
    report['checks'].append('original-carried-fee-growth-staking-reward-and-Oracle-source-banks-remain-exact')
    request=copy.deepcopy(captures[0]['request']);request['caller']='0x000000000000000000000000000000000000beef';code,value=http(base('router1')+'/v1/quote',token,request);require(code in [404,422]and'quoteId'not in value,'SLIPSTREAM_UNKNOWN_ORIGIN_QUOTED')
    request=copy.deepcopy(captures[0]['request']);request['verification']='VERIFIED';code,value=http(base('router1')+'/v1/quote',token,request);require(code==422 and'quoteId'not in value,'SLIPSTREAM_FALSE_VERIFIED')
    report['checks'].append('unknown-origins-and-false-VERIFIED-remain-closed')
    old={a:check(a)for a in ['router1','router2']};chain.bad='module-code';chain.head+=1
    for a in old:await_condition(lambda:status(a)[0]==503,'bad contextual module did not revoke');quote(base(a),token,'solana:localnet','B')
    chain.bad=None;chain.head+=1
    for a in old:await_condition(lambda:status(a)[1].get('snapshot_ref',{}).get('online_generation',0)>old[a]['onlineGeneration'],'contextual source did not recover');check(a)
    report['checks'].append('bad-module-revokes-only-affected-scope-and-qualified-context-restores')
    stop('router1');start('router1');wait(base('router1')+'/health/live');wait_scope(base('router1'),token,'evm:31337');check('router1')
    report['checks'].append('memory-only-Router-restart-retains-complete-context-model')
   finally:
    for a in list(processes):stop(a)
 finally:
  for server in servers:server.shutdown();server.server_close()
 require(runtime_fingerprint()==before,'SLIPSTREAM_SOURCE_OR_BINARY_CHANGED');tag=secrets.token_hex(6);inputs=OUT/f'core-slipstream-api-input-{tag}.json';projection=OUT/f'core-slipstream-api-model-{tag}.json';native=OUT/f'core-slipstream-api-native-{tag}.json';write_result(inputs,{'schema_version':1,'records':captures})
 projector=ROOT/'dex-router/target/debug/examples/slipstream-plan-projector';require(projector.is_file(),'SLIPSTREAM_PROJECTOR_REQUIRED')
 code=subprocess.run([str(projector),str(inputs),str(projection)],capture_output=True,text=True,timeout=120);(OUT/f'core-slipstream-api-model-{tag}.log').write_text(code.stdout+code.stderr);require(code.returncode==0,'SLIPSTREAM_ACTUAL_API_MODEL_PROJECTION')
 code=subprocess.run(['node',str(ROOT/'integration/tools/ci/slipstream-context-plan-replay.cjs'),str(REFERENCE),str(inputs),str(projection),str(native)],capture_output=True,text=True,timeout=240);(OUT/f'core-slipstream-api-native-{tag}.log').write_text(code.stdout+code.stderr);require(code.returncode==0,'SLIPSTREAM_ACTUAL_API_ORIGINAL_RUNTIME')
 actual=json.loads(native.read_text());require(actual.get('passed')is True and len(actual['records'])==len(captures),'SLIPSTREAM_NATIVE_COVERAGE');require(runtime_fingerprint()==before,'SLIPSTREAM_FINAL_SOURCE_CHANGED')
 negative=guards(captures,json.loads(projection.read_text()),projector,tag)
 require(runtime_fingerprint()==before,'SLIPSTREAM_SOURCE_CHANGED_DURING_NEGATIVE_CONTROLS')
 report.update(passed=True,negative_controls=negative,actual_api_records=len(captures),original_swaps=actual['original_swaps'],pinned_reads=chain.pinned,receipt_reads=chain.receipts,actual_api_input={'path':str(inputs.relative_to(ROOT)),'sha256':sha(inputs)},model_projection={'path':str(projection.relative_to(ROOT)),'sha256':sha(projection)},original_replay={'path':str(native.relative_to(ROOT)),'sha256':sha(native)},offline_freshness={'original_clock_ms':chain.timestamp*1000,'max_age_ms':freshness,'timestamp_rewritten':False,'production_claimed':False},measured_at_unix_ms=int(time.time()*1000))
 report['checks'].append('actual-selected-API-plans-match-original-runtime-token-deltas-and-full-poststates');return report

if __name__=='__main__':
 try:result=run()
 except BaseException as error:write_result(TARGET,{'schema_version':1,'passed':False,'production_ready':False,'all_requested_features_complete':False,'error':str(error),'error_type':type(error).__name__});raise
 write_result(TARGET,result);print(json.dumps(result,indent=2))
