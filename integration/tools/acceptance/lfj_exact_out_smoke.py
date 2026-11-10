#!/usr/bin/env python3
"""Original source stages -> normal Go/PG/Kafka -> actual contextual API plans.
Only controlled program banks and a synthetic header envelope are used.
"""
from pathlib import Path
import copy,json,secrets,subprocess,tempfile,time,hashlib
import infrastructure as infra_module
from infrastructure import ROOT,Infrastructure,port
infra_module.BASE=Path("/tmp/xone-lfj-xo-infra")
from configuration import config_for,save_config,service_command,sample_for
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import http,wait_scope,quote
from lfj_smoke import Chain as HeaderChain,require,sha
from exact_out_smoke import rebind
from lfj_exact_out_reference import validate as validate_reference
from weighted_evidence import runtime_fingerprint,write_result
OUT=ROOT/'integration/artifacts/local'
REFERENCE=ROOT/'integration/test/contracts/lfj-exact-out.json'
TARGET=OUT/'core-lfj-exact-out-normal-process-20261110.json'

class Chain(HeaderChain):
 def __init__(self,reference):
  self.reference=reference;self.dep=reference['deployment'];self.head=100;self.branch=0;self.bad=None;self.pinned=0;self.receipts=0;self.stage=0;self.bank_override=None;self.ids=[self.dep['pool']]
  self.sequence=next(seq for seq in reference['sequences']if reference['cases'][seq['initial_case_index']]['original']['active_offset']==0 and reference['cases'][seq['initial_case_index']]['zero_for_one'])
  self.base_case=reference['cases'][self.sequence['initial_case_index']];self.timestamp=self.base_case['pool']['lfj']['timestamp'];self.case=self.base_case;self.calls={}
 def bank(self):return self.reference['source_banks'][self.bank_override if self.bank_override is not None else self.sequence['source_bank_ids'][self.stage]]
 def current(self):return self.bank()['pool'],self.bank()['getter_reads']
 def dispatch(self,method,params):
  bank=self.bank();self.case=dict(self.base_case,pool=bank['pool'],getter_reads=bank['getter_reads'],storage_reads=bank['storage_reads']);self.calls={r['to']+':'+r['data']:r['result']for r in bank['getter_reads']};return super().dispatch(method,params)

def guards(captures,projection,projector,tag):
 source={'schema_version':1,'records':[copy.deepcopy(captures[0])]};checks=[];receipts=[]
 def model_reject(name,mutation,reason):
  document=copy.deepcopy(source);mutation(document['records'][0]);inputs=OUT/f'core-lfj-xo-guard-{tag}-{name}.json';output=OUT/f'core-lfj-xo-guard-{tag}-{name}-output.json';write_result(inputs,document)
  result=subprocess.run([str(projector),str(inputs),str(output)],capture_output=True,text=True,timeout=90);text=result.stdout+result.stderr;log=OUT/f'core-lfj-xo-guard-{tag}-{name}.log';log.write_text(text);receipts.append({'name':name,'observed_exit_code':result.returncode,'log':{'path':str(log.relative_to(ROOT)),'sha256':sha(log)}})
  require(result.returncode!=0 and reason in text and not output.exists(),'SLIPSTREAM_MODEL_GUARD:'+name);checks.append(name)
 model_reject('plan-hash',lambda r:r['execution_plan'].__setitem__('plan_hash','00'*32),'PLAN_HASH_MISMATCH')
 model_reject('reserve-accounting',lambda r:r['snapshot']['pools'][0]['lfj'].__setitem__('raw_reserve0','1'),'LFJ_RESERVE_ACCOUNTING')
 model_reject('step-budget',lambda r:r['snapshot']['pools'][0]['lfj'].__setitem__('max_steps',0),'LFJ_STATE_IDENTITY')
 def recipient(r):r['request']['recipient']=r['snapshot']['pools'][0]['pool_id'];r['execution_plan']['context']['recipient']=r['request']['recipient'];rebind(r)
 model_reject('rehashed-self-recipient',recipient,'EXACT_OUT_UNMODELED_SELF_FUNDING')
 model_reject('false-surplus',lambda r:r['quote'].__setitem__('outputSurplus','1'),'EXACT_OUT_PROJECT_PUBLIC_ALLOCATION')
 base_input=OUT/f'core-lfj-xo-guard-{tag}-native-base-input.json';write_result(base_input,source);base_model=copy.deepcopy(projection);base_model['records']=base_model['records'][:1];base_model['input_sha256']=sha(base_input)
 def native_reject(name,mutation,reason,mutate_input=None,mutate_reference=None):
  body=copy.deepcopy(base_model);mutation(body);inputs=base_input;reference=REFERENCE
  if mutate_input:
   document=copy.deepcopy(source);mutate_input(document);inputs=OUT/f'core-lfj-xo-guard-{tag}-{name}-input.json';write_result(inputs,document);body['input_sha256']=sha(inputs)
  if mutate_reference:
   document=json.loads(REFERENCE.read_text());mutate_reference(document);reference=OUT/f'core-lfj-xo-guard-{tag}-{name}-reference.json';write_result(reference,document)
  model=OUT/f'core-lfj-xo-guard-{tag}-{name}-model.json';output=OUT/f'core-lfj-xo-guard-{tag}-{name}-native.json';write_result(model,body)
  result=subprocess.run(['node',str(ROOT/'integration/tools/ci/lfj-v21-exact-out-plan-replay.cjs'),str(reference),str(inputs),str(model),str(output)],capture_output=True,text=True,timeout=180);text=result.stdout+result.stderr;log=OUT/f'core-lfj-xo-guard-{tag}-{name}.log';log.write_text(text);receipts.append({'name':name,'observed_exit_code':result.returncode,'log':{'path':str(log.relative_to(ROOT)),'sha256':sha(log)}})
  require(result.returncode!=0 and reason in text and not output.exists(),'SLIPSTREAM_NATIVE_GUARD:'+name);checks.append(name)
 native_reject('native-poststate',lambda d:d['records'][0]['steps'][0]['after']['lfj'].__setitem__('parameters','1'),'LFJ_NATIVE_COMPLETE_POSTSTATE:parameters')
 def source_input(d):d['records'][0]['snapshot']['pools'][0]['lfj']['raw_reserve0']='1'
 def source_model(d):
  d['records'][0]['snapshot']['pools'][0]['lfj']['raw_reserve0']='1';d['records'][0]['steps'][0]['before']['lfj']['raw_reserve0']='1'
 native_reject('native-original-source-bank',source_model,'LFJ_NATIVE_ORIGINAL_SOURCE_BANK',mutate_input=source_input)
 native_reject('native-runtime-source',lambda d:None,'LFJ_NATIVE_ORIGINAL_RUNTIME_BINDING:implementation_code',mutate_reference=lambda d:d['deployment'].__setitem__('implementation_code','0x6000'))
 native_reject('native-model-marker',lambda d:d['records'][0]['steps'][0]['after']['lfj'].__setitem__('max_steps',63),'LFJ_NATIVE_MODEL_MARKER_CHANGED:max_steps')
 return {'passed':True,'mutations':len(checks),'checks':checks,'receipts':receipts,'authority':'post-positive model/native negative controls; all require specific rejection and no successful output'}

def run():
 require(__debug__,'SLIPSTREAM_PROCESS_REQUIRES_ASSERTIONS');validate_reference();reference=json.loads(REFERENCE.read_text());chain=Chain(reference);before=runtime_fingerprint();servers=[chain.provider(),chain.provider()];processes={};logs={};captures=[];latest={}
 tool_paths=[Path(__file__).resolve(),ROOT/'integration/tools/ci/lfj-v21-exact-out-plan-replay.cjs',ROOT/'integration/tools/ci/lfj-v21-exact-out-getters.cjs',ROOT/'integration/tools/ci/lfj-v21-exact-out-state.py',ROOT/'integration/tools/acceptance/lfj_exact_out_reference.py',ROOT/'dex-router/target/debug/examples/lfj-exact-out-plan-projector'];tool_inputs={str(p.relative_to(ROOT)):sha(p)for p in tool_paths};reference_digest=sha(REFERENCE)
 report={'tool_inputs':tool_inputs,'schema_version':1,'passed':False,'source_tree_sha256':before['source_tree_sha256'],'runtime_fingerprint':before,'authority':'original byte getter results and physical original post banks in a synthetic fixed-clock RPC envelope; normal Go, actual local PG/Kafka and two normal Routers','reference_sha256':sha(REFERENCE),'all_requested_features_complete':False,'production_ready':False,'checks':[]}
 try:
  with Infrastructure()as infra,tempfile.TemporaryDirectory(prefix='xone-lfj-target-')as directory:
   root=Path(directory);token=secrets.token_urlsafe(32);names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'};ports={a:port()for a in names};configs={};config_values={};source_urls=[f'http://127.0.0.1:{ports[a]}'for a in ['go','rust']];freshness=max(120000,int(time.time()*1000)-chain.timestamp*1000+600000)
   for alias,name in names.items():
    cfg=config_for(name,root/alias,token,ports[alias]);cfg['journal'].update(transport='kafka',source_urls=source_urls,poll_interval_ms=100,request_timeout_ms=15000);cfg['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=alias,transaction_timeout_ms=4000)
    cfg['control'].update(backend='disabled'if name=='dex-router'else'postgres',dsn_file=''if name=='dex-router'else str(infra.dsnfile),instance_id=alias,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000);cfg['startup']['max_baseline_age_ms']=freshness
    if alias=='go':cfg['chains'][0].update(genesis_hash='0x'+'99'*32,confirmation_tag='latest',auto_reorg_window=4,rpc_urls=[f'http://127.0.0.1:{s.server_port}'for s in servers],pools=[chain.base_case['config']])
    elif alias=='rust':sample_for('solana:localnet',root/alias/'fixtures',chain.timestamp*1000)
    else:cfg['routing'].update(max_hops=1,max_splits=1,max_state_age_ms=freshness,quote_ttl_ms=60000);cfg['api']['quote_timeout_ms']=10000
    config_values[alias]=cfg;configs[alias]=save_config(root/alias/'config',cfg)
   base=lambda a:f'http://127.0.0.1:{ports[a]}'
   def start(alias):
    logs[alias]=(OUT/f'core-lfj-xo-{alias}.log').open('ab');processes[alias]=subprocess.Popen(service_command(names[alias],configs[alias],root/alias/'fixtures'if alias=='rust'else None),cwd=ROOT/names[alias],stdout=logs[alias],stderr=subprocess.STDOUT)
   def stop(alias):
    p=processes.pop(alias,None)
    if p:
     p.terminate()
     try:p.wait(timeout=10)
     except subprocess.TimeoutExpired:p.kill();p.wait()
    if alias in logs:logs.pop(alias).close()
   def status(alias):return http(base(alias)+'/internal/chains/evm:31337/status',token)
   def check(alias,zero=True,amount=None):
    bank=chain.bank();step=chain.sequence['steps'][chain.stage];amount=amount or (step['target_output'] if chain.bank_override is None else '2');dep=chain.dep
    request={'chainKey':'evm:31337','kind':'EXACT_OUT','sellToken':dep['token0'if zero else'token1'],'buyToken':dep['token1'if zero else'token0'],'buyAmount':amount,'maxSellAmount':'1000000000000000000000000','caller':dep['sender'],'recipient':dep['recipient'],'slippageBps':50,'verification':'FAST','constraints':{'maxHops':1}}
    code,result=http(base(alias)+'/v1/quote-exact-out',token,request);require(code==200,'LFJ_TARGET_QUOTE:'+str((code,result)))
    require(int(result['grossBuyAmount'])>=int(amount) and result['minBuyAmount']==amount and str(int(result['grossBuyAmount'])-int(amount))==result['outputSurplus'] and result['refund']=='0','LFJ_TARGET_ACTUAL_OUTPUT')
    known=step if chain.bank_override is None and chain.stage>0 else next((c for c in reference['cases']if c['pool']['lfj']['parameters']==bank['pool']['lfj']['parameters'] and c['target_output']==amount and c['zero_for_one']==zero),None)
    if known is not None:require(result['requiredSellAmount']==known['required_input'] and result['grossBuyAmount']==known['actual_output'],'LFJ_TARGET_ORIGINAL_AMOUNTS')
    require(result['verification']['level']=='INDICATIVE'and result['netSellAmountEstimate']is None,'LFJ_TARGET_VERIFICATION_BOUNDARY')
    code,snapshot=http(base('go')+'/internal/chains/evm:31337/baseline',token);require(code==200 and len(snapshot['pools'])==1,'LFJ_TARGET_BASELINE')
    original=copy.deepcopy(bank['pool']);original['lfj'].update(block_hash=snapshot['reference']['block']['hash'],block_number=snapshot['reference']['block']['number_or_slot']);require(snapshot['pools']==[original],'LFJ_TARGET_COMPLETE_PARSED_BANK')
    code,evidence=http(base(alias)+'/internal/quotes/'+result['quoteId']+'/evidence',token);require(code==200 and evidence['snapshot_valid']is True and evidence['expired']is False,'LFJ_TARGET_ACTUAL_PLAN')
    captures.append({'case_index':bank['id'],'snapshot':snapshot,'request':request,'quote':result,'execution_plan':evidence['execution_plan']});latest[alias]=result;return result
   try:
    for a in ['go','rust']:start(a);wait(base(a)+'/health/ready')
    for a in ['router1','router2']:start(a);wait(base(a)+'/health/live');wait_scope(base(a),token,'evm:31337')
    for a in ['router1','router2']:
     check(a,True);check(a,False)
    report['checks'].append('normal-source-field31-two-replicas-both-directions-match-original')
    for stage in [1,2]:
     chain.stage=stage;chain.head+=1
     for a in ['router1','router2']:await_condition(lambda:status(a)[1].get('snapshot_ref',{}).get('block',{}).get('number_or_slot')==chain.head,'contextual post-bank did not publish');check(a)
    report['checks'].append('original-carried-fee-growth-staking-reward-and-Oracle-source-banks-remain-exact')
    for offset in [-500,0,500]:
     for dynamic in [False,True]:
      for oracle in [False,True]:
       for elapsed in [0,10,1000]:
        selected=next(c for c in reference['cases']if c['original']['active_offset']==offset and c['original']['dynamic']==dynamic and c['original']['oracle']==oracle and c['original']['elapsed']==elapsed and c['zero_for_one'] and c['target_output']=='1');chain.bank_override=selected['index'];chain.head+=1
        for a in ['router1','router2']:
         await_condition(lambda:status(a)[1].get('snapshot_ref',{}).get('block',{}).get('number_or_slot')==chain.head,'LFJ target bank did not publish');check(a,True);check(a,False)
        if offset!=0:check('router1',offset>0,'1')
    report['checks'].append('three-price-ranges-dynamic-fee-decay-and-oracle-banks-with-real-surplus')
    request=copy.deepcopy(captures[0]['request']);request['verification']='VERIFIED';code,value=http(base('router1')+'/v1/quote-exact-out',token,request);require(code==422 and'quoteId'not in value,'SLIPSTREAM_FALSE_VERIFIED')
    report['checks'].append('false-VERIFIED-remains-closed')
    partial=copy.deepcopy(captures[-1]['request']);partial['buyAmount']='100000000';code,value=http(base('router1')+'/v1/quote-exact-out',token,partial);require(code==404 and value.get('error')=='NO_ROUTE','SLIPSTREAM_EXACT_OUT_PARTIAL_TARGET')
    for operation,reason in [('build','EXECUTION_ADAPTER_UNSUPPORTED'),('simulate','INDEPENDENT_SIMULATION_UNAVAILABLE')]:
     code,value=http(base('router1')+'/v1/quotes/'+latest['router1']['quoteId']+'/'+operation,token,{});require(code==422 and value.get('error')==reason,'SLIPSTREAM_EXACT_OUT_EXECUTION_BOUNDARY:'+str((operation,code,value)))
    old={a:check(a)for a in ['router1','router2']};chain.bad='module-code';chain.head+=1
    for a in old:await_condition(lambda:status(a)[0]==503,'bad LFJ pool code did not revoke');quote(base(a),token,'solana:localnet','B')
    chain.bad=None;chain.head+=1
    for a in old:await_condition(lambda:status(a)[1].get('snapshot_ref',{}).get('online_generation',0)>old[a]['onlineGeneration'],'contextual source did not recover');check(a)
    report['checks'].append('bad-pool-code-revokes-only-affected-scope-and-qualified-context-restores')
    stop('router1');start('router1');wait(base('router1')+'/health/live');wait_scope(base('router1'),token,'evm:31337');check('router1')
    report['checks'].append('memory-only-Router-restart-retains-complete-context-model')
   finally:
    for a in list(processes):stop(a)
 finally:
  for server in servers:server.shutdown();server.server_close()
 require(runtime_fingerprint()==before and sha(REFERENCE)==reference_digest and all(sha(ROOT/p)==h for p,h in tool_inputs.items()),'SLIPSTREAM_SOURCE_OR_BINARY_CHANGED');tag=secrets.token_hex(6);inputs=OUT/f'core-lfj-xo-api-input-{tag}.json';projection=OUT/f'core-lfj-xo-api-model-{tag}.json';native=OUT/f'core-lfj-xo-api-native-{tag}.json';write_result(inputs,{'schema_version':1,'records':captures})
 projector=ROOT/'dex-router/target/debug/examples/lfj-exact-out-plan-projector';require(projector.is_file(),'SLIPSTREAM_PROJECTOR_REQUIRED')
 code=subprocess.run([str(projector),str(inputs),str(projection)],capture_output=True,text=True,timeout=120);(OUT/f'core-lfj-xo-api-model-{tag}.log').write_text(code.stdout+code.stderr);require(code.returncode==0,'SLIPSTREAM_ACTUAL_API_MODEL_PROJECTION')
 code=subprocess.run(['node',str(ROOT/'integration/tools/ci/lfj-v21-exact-out-plan-replay.cjs'),str(REFERENCE),str(inputs),str(projection),str(native)],capture_output=True,text=True,timeout=240);(OUT/f'core-lfj-xo-api-native-{tag}.log').write_text(code.stdout+code.stderr);require(code.returncode==0,'SLIPSTREAM_ACTUAL_API_ORIGINAL_RUNTIME')
 actual=json.loads(native.read_text());require(actual.get('passed')is True and len(actual['records'])==len(captures),'SLIPSTREAM_NATIVE_COVERAGE');require(runtime_fingerprint()==before,'SLIPSTREAM_FINAL_SOURCE_CHANGED')
 negative=guards(captures,json.loads(projection.read_text()),projector,tag)
 require(runtime_fingerprint()==before and sha(REFERENCE)==reference_digest and all(sha(ROOT/p)==h for p,h in tool_inputs.items()),'SLIPSTREAM_SOURCE_CHANGED_DURING_NEGATIVE_CONTROLS')
 report.update(passed=True,negative_controls=negative,actual_api_records=len(captures),original_swaps=actual['native_swaps'],pinned_reads=chain.pinned,receipt_reads=chain.receipts,actual_api_input={'path':str(inputs.relative_to(ROOT)),'sha256':sha(inputs)},model_projection={'path':str(projection.relative_to(ROOT)),'sha256':sha(projection)},original_replay={'path':str(native.relative_to(ROOT)),'sha256':sha(native)},offline_freshness={'original_clock_ms':chain.timestamp*1000,'max_age_ms':freshness,'timestamp_rewritten':False,'production_claimed':False},measured_at_unix_ms=int(time.time()*1000))
 report['checks'].append('actual-selected-API-plans-match-original-runtime-token-deltas-and-full-poststates');return report

if __name__=='__main__':
 try:result=run()
 except BaseException as error:write_result(TARGET,{'schema_version':1,'passed':False,'production_ready':False,'all_requested_features_complete':False,'error':str(error),'error_type':type(error).__name__});raise
 write_result(TARGET,result);print(json.dumps(result,indent=2))
