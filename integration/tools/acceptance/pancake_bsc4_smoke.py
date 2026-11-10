#!/usr/bin/env python3
"""Normal field34 source/PG/Kafka/two Routers and unchanged original runtime replay."""
from pathlib import Path
import copy,json,secrets,subprocess,tempfile,time
import infrastructure as infra_module
from infrastructure import ROOT,Infrastructure,port
# Keep PostgreSQL Unix socket names within the macOS sockaddr limit.
infra_module.BASE=Path("/tmp/xone-bsc4-infrastructure")
from configuration import config_for,save_config,service_command,sample_for
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import http,wait_scope,quote
from lfj_smoke import Chain as HeaderChain,require,sha,rebind
from weighted_evidence import runtime_fingerprint,write_result
OUT=ROOT/'integration/artifacts/local'
REFERENCE=ROOT/'integration/test/contracts/pancake-bsc4-state.json'
TARGET=OUT/'core-pancake-bsc4-normal-process.json'

class Chain(HeaderChain):
 def __init__(self,reference):
  self.reference=reference;self.dep=reference['deployment'];self.head=100;self.branch=0;self.bad=None;self.pinned=0;self.receipts=0;self.stage=0
  self.sequence=reference['sequences'][0];self.timestamp=self.sequence['pool']['pancake_bsc_lm']['ledger']['timestamp'];self.ids=[self.dep['pool']['address']]
  self.case=next(c for c in reference['cases']if c['mode']=='first'and c['input']=='1000000000'and c['zero_for_one'])
 def bank(self):return self.reference['source_banks'][self.sequence['source_bank_ids'][self.stage]]
 def config(self,pool_id):
  cfg=copy.deepcopy(self.case['config']);cfg['id']=pool_id;return cfg
 def dispatch(self,method,params):
  if method not in ['eth_getCode','eth_call','eth_getStorageAt']:return super().dispatch(method,params)
  require(method!='eth_getStorageAt','PANCAKE_UNEXPECTED_DIRECT_STORAGE')
  require(isinstance(params[1],dict)and params[1].get('requireCanonical')is True and any(params[1].get('blockHash')==self.block(n)['hash']for n in range(self.head+1)),'PANCAKE_UNBOUND_SOURCE_READ');self.pinned+=1
  if method=='eth_getCode':
   for role,entry in self.dep.items():
    if params[0]==entry['address']:return '0x6000'if self.bad and role=='masterChef'else entry['code']
   raise ValueError('PANCAKE_UNRECORDED_CODE')
  call=params[0];require(set(call)=={'to','data'},'PANCAKE_CALL_OVERRIDES');values={r['to']+':'+r['data']:r['result']for r in self.bank()['original_bank']['reads']};key=call['to']+':'+call['data'];require(key in values,'PANCAKE_UNRECORDED_ORIGINAL_GETTER:'+key);return values[key]

def guards(captures,projection,projector,tag):
 source={'schema_version':1,'records':[copy.deepcopy(captures[0])]};checks=[]
 def model_reject(name,mutation,reason):
  document=copy.deepcopy(source);mutation(document['records'][0]);inputs=OUT/f'core-bsc4-guard-{tag}-{name}.json';output=OUT/f'core-bsc4-guard-{tag}-{name}-output.json';write_result(inputs,document)
  result=subprocess.run([str(projector),str(inputs),str(output)],capture_output=True,text=True,timeout=90);text=result.stdout+result.stderr;(OUT/f'core-bsc4-guard-{tag}-{name}.log').write_text(text)
  require(result.returncode!=0 and reason in text and not output.exists(),'PANCAKE_MODEL_GUARD:'+name);checks.append(name)
 model_reject('missing-migration-flag',lambda r:r['snapshot']['pools'][0]['pancake_bsc_lm']['flags'][0].pop('migrated'),'migrated')
 model_reject('missing-predecessor-tick',lambda r:r['snapshot']['pools'][0]['pancake_bsc_lm']['predecessors'][2]['ticks'].pop(),'BSC4_PREDECESSOR_TICK_SCOPE')
 model_reject('plan-hash',lambda r:r['execution_plan'].__setitem__('plan_hash','00'*32),'PLAN_HASH_MISMATCH')
 model_reject('derived-source',lambda r:r['snapshot']['pools'][0]['pancake_bsc_lm']['ledger'].__setitem__('derived',True),'BSC4_CANONICAL_BANK')
 model_reject('reward-view',lambda r:r['snapshot']['pools'][0]['pancake_bsc_lm']['ledger']['master_chef'].__setitem__('view_reward_per_second','1'),'PANCAKE_LM_REWARD_VIEW_MISMATCH')
 def self_recipient(r):r['request']['recipient']=r['snapshot']['pools'][0]['pancake_bsc_lm']['ledger']['lm']['address'];r['execution_plan']['context']['recipient']=r['request']['recipient'];rebind(r)
 model_reject('rehashed-lm-self-recipient',self_recipient,'PANCAKE_LM_UNMODELED_SELF_FUNDING')
 model_reject('missing-observed-time',lambda r:r['snapshot']['pools'][0]['pancake_bsc_lm']['ledger']['lm'].pop('last_reward_timestamp'),'last_reward_timestamp')
 base_input=OUT/f'core-bsc4-guard-{tag}-native-input.json';write_result(base_input,source);base_model=copy.deepcopy(projection);base_model['records']=base_model['records'][:1];base_model['input_sha256']=sha(base_input)
 def native_reject(name,mutation,reason,mutate_reference=None):
  body=copy.deepcopy(base_model);mutation(body);reference=REFERENCE
  if mutate_reference:
   document=json.loads(REFERENCE.read_text());mutate_reference(document);reference=OUT/f'core-bsc4-guard-{tag}-{name}-reference.json';write_result(reference,document)
  model=OUT/f'core-bsc4-guard-{tag}-{name}-model.json';output=OUT/f'core-bsc4-guard-{tag}-{name}-output.json';write_result(model,body)
  result=subprocess.run(['node',str(ROOT/'integration/tools/ci/pancake-bsc4-plan-replay.cjs'),str(reference),str(base_input),str(model),str(output)],capture_output=True,text=True,timeout=150);text=result.stdout+result.stderr;(OUT/f'core-bsc4-guard-{tag}-{name}.log').write_text(text)
  require(result.returncode!=0 and reason in text and not output.exists(),'PANCAKE_NATIVE_GUARD:'+name);checks.append(name)
 native_reject('native-migration-after',lambda d:next(f for f in d['records'][0]['steps'][0]['after']['pancake_bsc_lm']['flags']if f['migrated']).__setitem__('migrated',False),'PANCAKE_NATIVE_FULL_AFTER_STATE')
 native_reject('native-predecessor-readonly',lambda d:d['records'][0]['steps'][0]['after']['pancake_bsc_lm']['predecessors'][0]['ticks'][0].__setitem__('reward_growth_outside','999'),'BSC4_PREDECESSOR_READONLY')
 native_reject('native-full-after',lambda d:d['records'][0]['steps'][0]['after']['pancake_bsc_lm']['ledger']['lm'].__setitem__('reward_growth_global','1'),'PANCAKE_NATIVE_FULL_AFTER_STATE')
 native_reject('native-model-window',lambda d:d['records'][0]['steps'][0]['after']['pancake_bsc_lm']['ledger']['core']['tick_words'].append({'word_pos':2,'bitmap':'0'}),'PANCAKE_NATIVE_CORE_MARKER:tick_words')
 native_reject('original-source-bank',lambda d:None,'PANCAKE_NATIVE_ORIGINAL_BANK',mutate_reference=lambda d:d['source_banks'][captures[0]['case_index']]['original_bank']['state']['lm'].__setitem__('liquidity','1'))
 native_reject('runtime-source',lambda d:None,'PANCAKE_NATIVE_RUNTIME_BINDING:lm',mutate_reference=lambda d:d['deployment']['lm'].__setitem__('code','0x6000'))
 result={'passed':True,'checks':checks,'mutations':len(checks),'authority':'explicit local API-model and original-runtime corruption controls; not a general adversarial proof'};write_result(OUT/'core-bsc4-lm-negative-controls.json',result);return result

def run():
 require(__debug__,'PANCAKE_PROCESS_REQUIRES_ASSERTIONS');reference=json.loads(REFERENCE.read_text());chain=Chain(reference);before=runtime_fingerprint();servers=[chain.provider(),chain.provider()];processes={};logs={};captures=[]
 projector=ROOT/'dex-router/target/debug/examples/pancake-bsc4-plan-projector';require(projector.is_file(),'PANCAKE_PROJECTOR_REQUIRED')
 tool_paths=[projector,Path(__file__).resolve(),ROOT/'integration/tools/ci/pancake-bsc4-plan-replay.cjs']
 tool_inputs={str(path.relative_to(ROOT)):sha(path)for path in tool_paths}
 reference_digest=sha(REFERENCE)
 report={'schema_version':1,'passed':False,'source_tree_sha256':before['source_tree_sha256'],'runtime_fingerprint':before,'authority':'original getters and controlled source banks with a synthetic fixed-clock RPC header; normal Go, dedicated actual PG/Kafka and two normal Routers','reference_sha256':reference_digest,'tool_inputs':tool_inputs,'all_requested_features_complete':False,'production_ready':False,'checks':[]}
 try:
  with Infrastructure()as infra,tempfile.TemporaryDirectory(prefix='xone-pancake-lm-')as directory:
   directory=Path(directory);token=secrets.token_urlsafe(32);paths={};configs={};names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'};ports={actor:port()for actor in names};source_urls=[f'http://127.0.0.1:{ports[actor]}'for actor in ['go','rust']];freshness=max(120000,int(time.time()*1000)-chain.timestamp*1000+600000)
   for actor,name in names.items():
    config=config_for(name,directory/actor,token,ports[actor]);config['journal'].update(transport='kafka',source_urls=source_urls,poll_interval_ms=100,request_timeout_ms=15000);config['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=actor,transaction_timeout_ms=4000)
    config['control'].update(backend='disabled'if name=='dex-router'else'postgres',dsn_file=''if name=='dex-router'else str(infra.dsnfile),instance_id=actor,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000);config['startup']['max_baseline_age_ms']=freshness
    if actor=='go':config['chains'][0].update(genesis_hash='0x'+'99'*32,confirmation_tag='latest',auto_reorg_window=4,rpc_urls=[f'http://127.0.0.1:{server.server_port}'for server in servers],pools=[chain.case['config']])
    elif actor=='rust':sample_for('solana:localnet',directory/actor/'fixtures',chain.timestamp*1000)
    else:config['routing'].update(max_hops=1,max_splits=1,max_state_age_ms=freshness,quote_ttl_ms=60000);config['api']['quote_timeout_ms']=10000
    configs[actor]=config;paths[actor]=save_config(directory/actor/'config',config)
   def start(actor):
    name='chain-parser-go'if actor=='go'else'chain-parser-rust'if actor=='rust'else'dex-router';log=OUT/f'core-bsc4-lm-{actor}-{secrets.token_hex(4)}.log';logs[actor]=log;stream=log.open('w');command=service_command(name,paths[actor],directory/actor/'fixtures'if actor=='rust'else None);processes[actor]=subprocess.Popen(command,cwd=ROOT/name,stdout=stream,stderr=subprocess.STDOUT);stream.close()
   def stop(actor):
    process=processes.pop(actor,None)
    if process is not None:
     process.terminate()
     try:process.wait(timeout=10)
     except subprocess.TimeoutExpired:process.kill();process.wait()
   def base(actor):return 'http://127.0.0.1:'+str(ports[actor])
   def status(actor):return http(base(actor)+'/internal/chains/evm:31337/status',token)
   def check(actor,zero=True):
    bank=chain.bank();dep=chain.dep;request={'chainKey':'evm:31337','kind':'EXACT_IN','sellToken':dep['token0'if zero else'token1']['address'],'buyToken':dep['token1'if zero else'token0']['address'],'sellAmount':'13000000000','caller':bank['pool']['pancake_bsc_lm']['ledger']['master_chef']['position_manager'],'recipient':'0x000000000000000000000000000000000000beef','slippageBps':50,'verification':'FAST','constraints':{'maxHops':1}}
    code,result=http(base(actor)+'/v1/quote',token,request);require(code==200,'PANCAKE_ACTUAL_QUOTE:'+str((code,result)))
    code,evidence=http(base(actor)+'/internal/quotes/'+result['quoteId']+'/evidence',token);require(code==200 and evidence['snapshot_valid']is True and evidence['expired']is False,'PANCAKE_QUOTE_EVIDENCE')
    code,snapshot=http(base('go')+'/internal/chains/evm:31337/baseline',token);require(code==200,'PANCAKE_SOURCE_SNAPSHOT')
    require(result['consumedIn']==request['sellAmount']and result['refund']=='0'and result['snapshotRef']==snapshot['reference'],'PANCAKE_ACTUAL_PLAN_SOURCE_SCOPE')
    expected=copy.deepcopy(bank['pool']);expected['pancake_bsc_lm']['ledger'].update(block_hash=snapshot['reference']['block']['hash'],block_number=snapshot['reference']['block']['number_or_slot']);require(snapshot['pools']==[expected],'PANCAKE_COMPLETE_PARSED_SOURCE_BANK')
    require(result['verification']['level']=='INDICATIVE'and result.get('netBuyAmountEstimate')is None,'PANCAKE_FALSE_EXECUTION_UPGRADE')
    captures.append({'case_index':bank['id'],'snapshot':snapshot,'request':request,'quote':result,'execution_plan':evidence['execution_plan']});return result
   try:
    for actor in ['go','rust']:start(actor);wait(base(actor)+'/health/ready')
    for actor in ['router1','router2']:start(actor);wait(base(actor)+'/health/live');wait_scope(base(actor),token,'evm:31337')
    for actor in ['router1','router2']:check(actor,True);check(actor,False)
    report['checks'].append('normal-field34-source-and-both-quote-directions-on-two-replicas')
    for stage in [1,2]:
     chain.stage=stage;chain.head+=1
     for actor in ['router1','router2']:await_condition(lambda:status(actor)[1].get('snapshot_ref',{}).get('block',{}).get('number_or_slot')==chain.head,'LM source stage did not publish');check(actor)
    report['checks'].append('actual-original-LM-post-banks-with-recentered-observed-bitmap-scope')
    req=copy.deepcopy(captures[0]['request']);req['verification']='VERIFIED';code,value=http(base('router1')+'/v1/quote',token,req);require(code==422 and'quoteId'not in value,'PANCAKE_FALSE_VERIFIED')
    old={actor:check(actor)for actor in ['router1','router2']};chain.bad='chef-code';chain.head+=1
    for actor in old:await_condition(lambda:status(actor)[0]==503,'bad LM source did not revoke');quote(base(actor),token,'solana:localnet','B')
    chain.bad=None;chain.head+=1
    for actor in old:await_condition(lambda:status(actor)[1].get('snapshot_ref',{}).get('online_generation',0)>old[actor]['onlineGeneration'],'LM source recovery did not advance');check(actor)
    report['checks'].append('unknown-masterChef-code-revokes-only-EVM-and-qualified-source-recovers')
    stop('router1');start('router1');wait(base('router1')+'/health/live');wait_scope(base('router1'),token,'evm:31337');check('router1');report['checks'].append('Router-restart-restores-complete-field34-and-preserves-other-chain')
   finally:
    for actor in list(processes):stop(actor)
 finally:
  for server in servers:server.shutdown();server.server_close()
 require(runtime_fingerprint()==before and sha(REFERENCE)==reference_digest and all(sha(ROOT/path)==digest for path,digest in tool_inputs.items()),'PANCAKE_SOURCE_OR_BINARY_CHANGED');tag=secrets.token_hex(6);inputs=OUT/f'core-bsc4-api-input-{tag}.json';projection=OUT/f'core-bsc4-api-model-{tag}.json';native=OUT/f'core-bsc4-api-native-{tag}.json';write_result(inputs,{'schema_version':1,'records':captures})
 projector=ROOT/'dex-router/target/debug/examples/pancake-bsc4-plan-projector';require(projector.is_file(),'PANCAKE_PROJECTOR_REQUIRED')
 for name,command in [('model',[str(projector),str(inputs),str(projection)]),('native',['node',str(ROOT/'integration/tools/ci/pancake-bsc4-plan-replay.cjs'),str(REFERENCE),str(inputs),str(projection),str(native)])]:
  code=subprocess.run(command,capture_output=True,text=True,timeout=300);(OUT/f'core-bsc4-api-{name}-{tag}.log').write_text(code.stdout+code.stderr);require(code.returncode==0,'PANCAKE_ACTUAL_API_'+name.upper())
 actual=json.loads(native.read_text());require(actual.get('passed')is True and len(actual['records'])==len(captures),'PANCAKE_NATIVE_COVERAGE');negative=guards(captures,json.loads(projection.read_text()),projector,tag);require(runtime_fingerprint()==before and sha(REFERENCE)==reference_digest and all(sha(ROOT/path)==digest for path,digest in tool_inputs.items()),'PANCAKE_FINAL_SOURCE_CHANGED')
 migrations=sum(sum(1 for before,after in zip(step['before']['pancake_bsc_lm']['flags'],step['after']['pancake_bsc_lm']['flags'])if not before['migrated']and after['migrated'])for record in json.loads(projection.read_text())['records']for step in record['steps']);require(migrations>0,'BSC4_ACTUAL_API_MIGRATION_REQUIRED')
 report.update(passed=True,actual_tick_migrations=migrations,actual_api_records=len(captures),original_swaps=actual['original_swaps'],negative_controls=negative,pinned_reads=chain.pinned,receipt_reads=chain.receipts,actual_api_input={'path':str(inputs.relative_to(ROOT)),'sha256':sha(inputs)},model_projection={'path':str(projection.relative_to(ROOT)),'sha256':sha(projection)},original_replay={'path':str(native.relative_to(ROOT)),'sha256':sha(native)},offline_freshness={'original_clock_ms':chain.timestamp*1000,'max_age_ms':freshness,'timestamp_rewritten':False,'production_claimed':False},measured_at_unix_ms=int(time.time()*1000));return report

if __name__=='__main__':
 try:result=run()
 except BaseException as error:write_result(TARGET,{'schema_version':1,'passed':False,'production_ready':False,'all_requested_features_complete':False,'error':str(error),'error_type':type(error).__name__});raise
 write_result(TARGET,result);print(json.dumps(result,indent=2))
