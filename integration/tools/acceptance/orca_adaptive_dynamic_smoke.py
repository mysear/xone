#!/usr/bin/env python3
"""Frozen original-program adaptive banks through ordinary RPC/PG/Kafka routers.

Clock.slot is relabeled for local bank progression; original pricing timestamps
and math states are preserved. Sources are synthetic, no SBF/SPL CPI claim.
"""
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
import base64,copy,hashlib,importlib.util,json,secrets,subprocess,tempfile,threading,time,urllib.request,urllib.error
ROOT=Path(__file__).resolve().parents[3];OUT=ROOT/'integration/artifacts/local';TARGET=OUT/'orca-adaptive-dynamic-three-service-smoke.json';FRESHNESS=315360000000
FAULTS=['oracle-owner','oracle-binding','oracle-length','trade-gate','future-reference','tier-seed','program-code','token-code','dynamic-bitmap','dynamic-truncated','dynamic-trailing','array-discriminator','reward-control','reward-vault','uninitialized-reward-growth','clock-slot']
CHECKS=['normal-finalized-rpc-postgres-kafka-two-routers','complete-original-packed-oracle-pool-and-tick-bank','independent-live-protobuf-preserves-all-reserved-bytes','ten-original-banks-twenty-dual-router-requests','native-proof-exhaustion-rejected-without-quote',*['scope-withdraw-and-new-generation:'+f for f in FAULTS],'unrelated-evm-quotes-remain-ready','router-restart-restores-complete-adaptive-bank','verified-and-build-remain-disabled']
def require(ok,message):
 if not ok:raise ValueError(message)
def sha(b):return hashlib.sha256(b).hexdigest()
def b58(raw):
 alphabet='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';n=int.from_bytes(raw,'big');out=''
 while n:out=alphabet[n%58]+out;n//=58
 return '1'*(len(raw)-len(raw.lstrip(b'\0')))+out
def initial():return {'passed':False,'state':'RUNNING','verified':False,'production_ready':False,'all_requested_features_complete':False,'trust':'SYNTHETIC_RPC_ORIGINAL_PACK_NOT_SBF_TRANSACTION','checks':[]}
def write(v):
 OUT.mkdir(parents=True,exist_ok=True);tmp=TARGET.with_name(TARGET.name+'.'+secrets.token_hex(6)+'.tmp')
 try:tmp.write_text(json.dumps(v,indent=2)+'\n');tmp.replace(TARGET)
 finally:tmp.unlink(missing_ok=True)
if __name__=='__main__':write(initial())
from infrastructure import Infrastructure,port
from configuration import config_for,save_config,service_command,sample_for
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import wait_scope
def http(url,token,body=None):
 request=urllib.request.Request(url,data=None if body is None else json.dumps(body).encode(),headers={'Authorization':'Bearer '+token,'Content-Type':'application/json'})
 try:
  with urllib.request.urlopen(request,timeout=30)as response:return response.status,json.load(response)
 except urllib.error.HTTPError as error:return error.code,json.load(error)
from curve_ng_smoke import bootstrap_snapshot
from weighted_evidence import runtime_fingerprint,snapshot_inputs
from orca_adaptive_dynamic_fixture import corpus,clean_config,pool,CLOCK
spec=importlib.util.spec_from_file_location('adaptive_wire',ROOT/'integration/tools/ci/orca-adaptive-dynamic-wire-golden.py');wire=importlib.util.module_from_spec(spec);spec.loader.exec_module(wire)
class Chain:
 def __init__(self):
  self.document=corpus();self.scenarios=[self.document['scenarios'][0],self.document['scenarios'][3]];self.lock=threading.RLock();self.head=900000;self.phase=0;self.bad=None;self.reads=0;self.blocks=0;self.advance(0)
 def advance(self,phase=None,fault=None):
  with self.lock:
   phase=self.phase if phase is None else phase;require(0<=phase<5,'unknown replay phase');require(fault is None or fault in FAULTS,'unknown fault');self.head+=1;self.phase=phase;self.bad=fault;self.items=[];self.accounts={}
   for scenario in self.scenarios:
    bank=copy.deepcopy(scenario['banks'][phase]);native=bank['native_state'];native['bank_slot']=self.head;native['clock']['slot']=self.head;config=clean_config(scenario['config']);self.items.append({'pool':pool(scenario['config'],native),'config':config,'trade':bank['next_trade'],'original_slot':bank['slot'],'original_config':copy.deepcopy(scenario['config']),'original_bank':copy.deepcopy(scenario['banks'][phase])})
    for key,a in bank['accounts'].items():
     a=copy.deepcopy(a)
     if key==CLOCK:
      data=bytearray(base64.b64decode(a['data'][0]));data[:8]=self.head.to_bytes(8,'little');a['data'][0]=base64.b64encode(data).decode()
     if key in self.accounts:require(self.accounts[key]==a,'shared account conflicts between pools')
     self.accounts[key]=a
   times={i['pool']['orca_whirlpool']['bank_unix_timestamp']for i in self.items};require(len(times)==1,'bank timestamps differ');self.timestamp=times.pop()
 def advance_recovery(self):
  self.advance(4)
  for item in self.items:
   params=copy.deepcopy(item['trade']['params']);params.update(amount='200',sqrt_price_limit='0')
   item['trade']={'params':params,'expected':{'status':'fulfilled'}}
 def block(self,slot):return {'blockTime':self.timestamp,'blockhash':b58(hashlib.sha256(f'adaptive/{slot}'.encode()).digest()),'previousBlockhash':b58(hashlib.sha256(f'adaptive/{slot-1}'.encode()).digest()),'parentSlot':slot-1}
 def rpc(self,method,args):
  with self.lock:
   if method=='getGenesisHash':return 'localnet'
   if method=='getBlock':require(args[0]==self.head and args[1]['commitment']=='finalized','unfinalized/wrong slot block');self.blocks+=1;return self.block(self.head)
   require(method=='getMultipleAccounts','unexpected RPC '+method);require(args[1]['commitment']=='finalized'and args[1]['encoding']=='base64'and 'minContextSlot'not in args[1],'account read is not original finalized current bank');require(set(args[0])==set().union(*[self.required(i)for i in self.items]),'account source set differs');values=[]
   for key in args[0]:
    a=copy.deepcopy(self.accounts.get(key));require(a is not None,'missing required account');d=bytearray(base64.b64decode(a['data'][0]));s=self.items[0]['pool']['orca_whirlpool'];p=self.items[0]['pool'];bad=self.bad
    if key==s['oracle']:
     if bad=='oracle-owner':a['owner']='11111111111111111111111111111111'
     elif bad=='oracle-binding':d[8:40]=wire.pubkey(p['token0'])
     elif bad=='oracle-length':d.pop()
     elif bad=='trade-gate':d[40:48]=(self.timestamp+1).to_bytes(8,'little')
     elif bad=='future-reference':d[82:90]=(self.timestamp+1).to_bytes(8,'little')
    if key==p['pool_id']:
     if bad=='tier-seed':d[43:45]=s['tick_spacing'].to_bytes(2,'little')
     elif bad=='reward-control':d[269+128+64]=1
     elif bad=='reward-vault':d[301:333]=bytes(32)
     elif bad=='uninitialized-reward-growth':d[269+256+112:269+256+128]=(1).to_bytes(16,'little')
    if key==s['programdata']and bad=='program-code':d[-1]^=1
    if key=='TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'and bad=='token-code':d[-1]^=1
    if key==s['tick_arrays'][0]['address']:
     if bad=='dynamic-bitmap':d[44]^=1
     elif bad=='dynamic-truncated':d.pop()
     elif bad=='dynamic-trailing':d.append(1)
     elif bad=='array-discriminator':d[:8]=hashlib.sha256(b'account:unknown-array').digest()[:8]
    if key==CLOCK and bad=='clock-slot':d[:8]=(self.head+1).to_bytes(8,'little')
    a['data'][0]=base64.b64encode(d).decode();values.append(a)
   self.reads+=1;return {'context':{'slot':self.head},'value':values}
 def required(self,item):
  p=item['pool'];s=p['orca_whirlpool'];return set(item['config']['accounts'])|{s['program_id'],s['programdata'],s['oracle'],CLOCK,'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'}|{a['address']for a in s['tick_arrays']}
 def provider(self):
  chain=self
  class Handler(BaseHTTPRequestHandler):
   def log_message(self,*_):pass
   def do_POST(self):
    q=json.loads(self.rfile.read(int(self.headers['Content-Length'])));response={'jsonrpc':'2.0','id':q['id']}
    try:response['result']=chain.rpc(q['method'],q['params'])
    except(ValueError,KeyError,TypeError,IndexError)as e:response['error']={'code':-32000,'message':str(e)}
    data=json.dumps(response).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers()
    try:self.wfile.write(data)
    except(BrokenPipeError,ConnectionResetError):pass
  server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start();return server

def replay_plan(quoted,request,items,native):
 banks={i['pool']['pool_id']:copy.deepcopy(i['original_bank'])for i in items};configs={i['pool']['pool_id']:i['original_config']for i in items};total_in=total_out=0;steps=0
 require(bool(quoted.get('routes')),'adaptive empty plan')
 for route in quoted['routes']:
  edges=route['path']['edges'];require(len(edges)==1,'adaptive reference expected direct token pair');edge=edges[0];pid=edge['pool_id'];require(pid in banks and edge['protocol']=='orca-whirlpools-adaptive-dynamic','adaptive path pool/protocol differs');config=configs[pid];left=request['sellToken']==config['token0'];require(request['buyToken']==config['token1'if left else'token0']and edge['zero_for_one']==left,'adaptive route direction differs');amount=route['amount_in'];params={'amount':amount,'a_to_b':left,'sqrt_price_limit':'0','timestamp':banks[pid]['timestamp'],'max_steps':8192,'amount_specified_is_input':True};result=native.request({'action':'trade','config':config,'bank':banks[pid],'params':params});require(result.get('status')=='fulfilled'and result.get('amount_in')==amount and result.get('amount_remaining')=='0','adaptive chosen route is not natively fulfilled');require(route['amount_out']==result['amount_out']and route['path']['amount_out']==result['amount_out'],'adaptive selected route differs from original native');banks[pid]['accounts']=result['post_accounts'];banks[pid]['native_state']=result['next_state'];total_in+=int(amount);total_out+=int(result['amount_out']);steps+=1
 require(str(total_in)==request['sellAmount']==quoted['consumedIn']and str(total_out)==quoted['grossBuyAmount']and quoted['refund']=='0','adaptive selected allocation does not conserve original amounts')
 return steps

def run():
 started=int(time.time()*1000);inputs=snapshot_inputs();report=initial();chain=Chain();servers=[chain.provider(),chain.provider()];processes={};logs={};fingerprint=runtime_fingerprint();report['source_binding']=fingerprint
 refspec=importlib.util.spec_from_file_location('adaptive_reference',ROOT/'integration/tools/ci/orca-adaptive-dynamic-service-golden.py');ref=importlib.util.module_from_spec(refspec);refspec.loader.exec_module(ref);native=ref.Native(ref.BINARY);native_steps=0
 try:
  with Infrastructure()as infra,tempfile.TemporaryDirectory(prefix='xone-adaptive-')as folder:
   root=Path(folder);token=secrets.token_urlsafe(32);names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'};ports={a:port()for a in names};sources=[f'http://127.0.0.1:{ports[a]}'for a in ['go','rust']];configs={}
   for actor,name in names.items():
    c=config_for(name,root/actor,token,ports[actor]);c['journal'].update(transport='kafka',source_urls=sources,poll_interval_ms=100,request_timeout_ms=15000);c['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=actor,transaction_timeout_ms=4000);c['control'].update(backend='disabled'if name=='dex-router'else 'postgres',dsn_file=''if name=='dex-router'else str(infra.dsnfile),instance_id=actor,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000);c['startup']['max_baseline_age_ms']=FRESHNESS
    if actor=='rust':c['chains'][0].update(genesis_hash='localnet',confirmation_tag='finalized',auto_reorg_window=4,rpc_urls=[f'http://127.0.0.1:{s.server_port}'for s in servers],pools=[i['config']for i in chain.items])
    elif actor=='go':sample_for('evm:31337',root/actor/'fixtures',chain.timestamp*1000)
    else:c['routing'].update(max_state_age_ms=FRESHNESS,quote_ttl_ms=60000,max_hops=1,max_candidates=8,max_expansions=64,split_parts=4,max_splits=2);c['api']['quote_timeout_ms']=10000
    configs[actor]=save_config(root/actor/'config',c)
   def base(actor):return f'http://127.0.0.1:{ports[actor]}'
   def start(actor):logs[actor]=(OUT/f'orca-adaptive-dynamic-{actor}.log').open('ab');processes[actor]=subprocess.Popen(service_command(names[actor],configs[actor],root/actor/'fixtures'if actor=='go'else None),cwd=ROOT/names[actor],stdout=logs[actor],stderr=subprocess.STDOUT)
   def stop(actor):
    p=processes.pop(actor,None)
    if p:
     p.terminate()
     try:p.wait(timeout=10)
     except subprocess.TimeoutExpired:p.kill();p.wait()
    if actor in logs:logs.pop(actor).close()
   def status(actor,key='solana:localnet'):return http(base(actor)+'/internal/chains/'+key+'/status',token)
   def owner():
    code,s=http(base('rust')+'/internal/chains/solana:localnet/baseline',token);require(code==200,('adaptive owner unavailable',code,s));return s
   def synced():
    await_condition(lambda:status('rust')[1].get('snapshot_ref',{}).get('block',{}).get('number_or_slot')==chain.head,'adaptive owner bank not reached');s=owner()
    for actor in ['router1','router2']:await_condition(lambda:status(actor)[1].get('snapshot_ref')==s['reference'],'adaptive router source differs')
    return s
   def complete():
    s=synced();require({p['pool_id']:p for p in s['pools']}=={i['pool']['pool_id']:i['pool']for i in chain.items},'adaptive full Oracle/raw/tick bank differs');request=urllib.request.Request(base('rust')+'/internal/chains/solana:localnet/bootstrap',headers={'Authorization':'Bearer '+token})
    with urllib.request.urlopen(request,timeout=10)as response:require(response.headers['content-type']=='application/x-protobuf','adaptive bootstrap not protobuf');raw=response.read()
    expected=subprocess.run(['protoc','-I',str(ROOT/'common/protoc/proto'),'--encode=dex.v1.OnlineSnapshot',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=wire.encode(s).encode(),capture_output=True,check=True).stdout;require(bootstrap_snapshot(raw)==expected,'live independent protobuf changed Oracle/reserved/ticks');return s
   def quote(actor,item):
    nonlocal native_steps
    p=item['pool'];t=item['trade'];left=t['params']['a_to_b'];request={'chainKey':'solana:localnet','kind':'EXACT_IN','sellToken':p['token0'if left else'token1'],'buyToken':p['token1'if left else'token0'],'sellAmount':t['params']['amount'],'caller':p['token0'],'recipient':p['token1'],'slippageBps':50,'verification':'FAST'};code,q=http(base(actor)+'/v1/quote',token,request)
    if t['expected']['status']=='native_rejected':require(code in [400,404]and 'quoteId'not in q,('native failure produced quote',code,q));return None,request
    require(code==200,('adaptive quote failed',code,q,item['original_slot']));native_steps+=replay_plan(q,request,chain.items,native);require(q['snapshotRef']==owner()['reference']and q['verification']['level']=='INDICATIVE','adaptive stale or overstated');require(set(q['quality']['candidateScope'])=={i['pool']['pool_id']for i in chain.items},'adaptive candidate scope incomplete');return q,request
   try:
    for actor in ['go','rust']:start(actor);wait(base(actor)+'/health/ready')
    for actor in ['router1','router2']:start(actor);wait(base(actor)+'/health/live');wait_scope(base(actor),token,'solana:localnet');wait_scope(base(actor),token,'evm:31337')
    success=rejected=0
    for phase in range(5):
     if phase:chain.advance(phase)
     complete()
     for actor in ['router1','router2']:
      for item in chain.items:
       q,_=quote(actor,item)
       if q is None:rejected+=1
       else:success+=1
    report['checks']+=CHECKS[:5];chain.advance_recovery();old=complete()['reference']['online_generation']
    for fault in FAULTS:
     chain.advance_recovery();chain.bad=fault
     for actor in ['router1','router2']:
      await_condition(lambda:status(actor)[0]==503,'bad adaptive scope stayed ready');code,q=http(base(actor)+'/v1/quote',token,{'chainKey':'evm:31337','kind':'EXACT_IN','sellToken':'A','buyToken':'B','sellAmount':'10003','caller':'A','recipient':'B','slippageBps':50,'verification':'FAST'});require(code==200 and q['verification']['level']=='INDICATIVE'and int(q['grossBuyAmount'])>0,'unrelated EVM quote failed')
     chain.advance_recovery();s=complete();require(s['reference']['online_generation']>old,'adaptive recovery reused generation');old=s['reference']['online_generation'];quote('router1',chain.items[0]);report['checks'].append('scope-withdraw-and-new-generation:'+fault)
    report['checks'].append('unrelated-evm-quotes-remain-ready');stop('router2');start('router2');wait(base('router2')+'/health/live');wait_scope(base('router2'),token,'solana:localnet');complete();quote('router2',chain.items[0]);report['checks'].append('router-restart-restores-complete-adaptive-bank')
    for actor in ['router1','router2']:
     q,request=quote(actor,chain.items[0]);request['verification']='VERIFIED';code,v=http(base(actor)+'/v1/quote',token,request);require(code==422 and 'INDEPENDENT_SIMULATION_UNAVAILABLE'in json.dumps(v),'adaptive VERIFIED enabled');code,_=http(base(actor)+'/v1/quotes/'+q['quoteId']+'/build',token,{});require(code==422,'adaptive build enabled')
    report['checks'].append('verified-and-build-remain-disabled');require(report['checks']==CHECKS,'missing adaptive process coverage');require(fingerprint==runtime_fingerprint(),'adaptive source or binaries changed');require(inputs==snapshot_inputs(),'adaptive rewards execution inputs changed');report.update(input_hashes=inputs,inputs_unchanged=True,started_at_unix_ms=started,completed_at_unix_ms=int(time.time()*1000),passed=True,state='PASSED',quote_checks=success+rejected,successful_quotes=success,native_rejections=rejected,pool_count=2,phase_count=5,account_batches=chain.reads,block_reads=chain.blocks,owner_reference=owner()['reference'],native_plan_steps=native_steps,test_quote_budget={'timeout_ms':10000,'max_hops':1,'max_candidates':8,'max_expansions':64,'split_parts':4,'max_splits':2});return report
   finally:
    for actor in list(processes):stop(actor)
 finally:
  for s in servers:s.shutdown();s.server_close()
  native.close()
def main():
 write(initial())
 try:r=run();write(r);print(json.dumps({'passed':True,'checks':len(r['checks']),'requests':r['quote_checks']}))
 except BaseException as e:r=initial();r.update(state='FAILED',error=str(e),error_type=type(e).__name__);write(r);raise
if __name__=='__main__':main()
