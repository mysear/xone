#!/usr/bin/env python3
"""Frozen original Bin manager getter banks through RPC/PG/Kafka and routers.

Original BinPool swaps are a separate EVM reference. No Vault lock/transient
storage, caller settlement, real deployment or production verification claim.
"""
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
import copy,hashlib,importlib.util,json,secrets,subprocess,tempfile,threading,time,urllib.request,urllib.error,select
ROOT=Path(__file__).resolve().parents[3];OUT=ROOT/'integration/artifacts/local';TARGET=OUT/'infinity-bin-three-service-smoke.json';FRESHNESS=315360000000;GENESIS='0x'+'99'*32
FAULTS=['manager-code','vault-code','key','pause','registration','tree-child','bin-share','next-link','token-code','slot0']
CHECKS=['normal-bin-rpc-postgres-kafka-two-routers','five-complete-original-manager-tree-banks','independent-live-protobuf-full-tree-and-shares','eight-quotes-actual-plan-original-evm-replay','two-large-input-rejections-without-quote',*['scope-withdraw-and-new-generation:'+f for f in FAULTS],'unrelated-solana-quote-remains-ready','router-restart-restores-bin-tree-bank','verified-and-build-remain-disabled']
def require(ok,message):
 if not ok:raise ValueError(message)
def digest(raw):return hashlib.sha256(raw).hexdigest()
def initial():return {'passed':False,'state':'RUNNING','verified':False,'production_ready':False,'all_requested_features_complete':False,'trust':'ORIGINAL_MANAGER_GETTERS_SYNTHETIC_BANK_BINPOOL_REFERENCE_NOT_SETTLEMENT','checks':[]}
def write(v):
 OUT.mkdir(parents=True,exist_ok=True);tmp=TARGET.with_name(TARGET.name+'.'+secrets.token_hex(6)+'.tmp')
 try:tmp.write_text(json.dumps(v,indent=2)+'\n');tmp.replace(TARGET)
 finally:tmp.unlink(missing_ok=True)
if __name__=='__main__':write(initial())
from infrastructure import Infrastructure,port
from configuration import config_for,save_config,service_command,sample_for
from receipt_reference import EMPTY_ROOT,SINGLE_EMPTY_ROOT,SINGLE_EMPTY_FIELDS
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import wait_scope
from curve_ng_smoke import bootstrap_snapshot
from weighted_evidence import runtime_fingerprint,snapshot_inputs
from infinity_bin_fixture import corpus,project
spec=importlib.util.spec_from_file_location('bin_wire',ROOT/'integration/tools/ci/infinity-bin-wire-golden.py');wire=importlib.util.module_from_spec(spec);spec.loader.exec_module(wire)
def http(url,token,body=None):
 r=urllib.request.Request(url,data=None if body is None else json.dumps(body).encode(),headers={'Authorization':'Bearer '+token,'Content-Type':'application/json'})
 try:
  with urllib.request.urlopen(r,timeout=30)as response:return response.status,json.load(response)
 except urllib.error.HTTPError as e:return e.code,json.load(e)
class Native:
 def __init__(self):
  self.process=subprocess.Popen(['node',str(ROOT/'integration/tools/ci/infinity-bin-service-replay.cjs')],cwd=ROOT,stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True);observed=self.request({'action':'identity'});require(observed.get('ok')is True,'native original reference unavailable');self.identity=observed['identity'];require(self.identity['revision']=='3c5ef1be1f71442d8bc60cc6fd11f9c677b714df'and self.identity['full_manager_swap_settlement_verified']is False,'native reference scope differs')
 def request(self,value):
  self.process.stdin.write(json.dumps(value)+'\n');self.process.stdin.flush();ready=select.select([self.process.stdout],[],[],30)[0];
  if not ready:self.process.terminate();self.process.wait(timeout=10);raise ValueError('original reference request timed out')
  line=self.process.stdout.readline();require(bool(line),'original reference stopped');return json.loads(line)
 def close(self):self.process.stdin.close();require(self.process.wait(timeout=30)==0,'native cleanup failed')
class Chain:
 def __init__(self):self.doc=corpus();self.lock=threading.RLock();self.head=99;self.phase=0;self.banks={};self.read_counts={};self.receipts=0;self.advance(0)
 def advance(self,phase=None,fault=None):
  with self.lock:
   phase=self.phase if phase is None else phase;require(0<=phase<5,'unknown Bin bank phase');require(fault is None or fault in FAULTS,'unknown Bin fault');self.head+=1;self.phase=phase;self.current=copy.deepcopy(self.doc['RPCbanks'][phase]);self.timestamp=self.current['block']['timestamp'];hash='0x'+digest(f'BIN/service/{self.head}'.encode());self.current['block'].update(num=self.head,hash=hash);self.items=[project(self.doc,self.current,i)for i in range(2)];self.banks[self.head]={'source':self.current,'hash':hash,'timestamp':self.timestamp,'fault':fault}
 def block(self,n):
  if n==0:return {'number':'0x0','hash':GENESIS,'parentHash':'0x'+'00'*32,'timestamp':hex(self.timestamp),'receiptsRoot':EMPTY_ROOT,'transactions':[]}
  require(0<n<=self.head,'unknown local Bin block');h='0x'+digest(f'BIN/service/{n}'.encode());bank=self.banks.get(n)
  return {'number':hex(n),'hash':h,'parentHash':GENESIS if n==1 else '0x'+digest(f'BIN/service/{n-1}'.encode()),'timestamp':hex(bank['timestamp']if bank else self.timestamp),'receiptsRoot':SINGLE_EMPTY_ROOT,'transactions':['0x'+digest(('tx/'+h).encode())]}
 def rpc(self,method,args):
  with self.lock:
   if method=='eth_chainId':return '0x7a69'
   if method=='eth_getBlockByNumber':return self.block(self.head if args[0]in ['latest','safe','finalized']else int(args[0],16))
   if method=='eth_getTransactionReceipt':
    for n in range(1,self.head+1):
     b=self.block(n)
     if args[0]in b['transactions']:self.receipts+=1;return {'blockHash':b['hash'],'transactionHash':args[0],'transactionIndex':'0x0','logs':[],**SINGLE_EMPTY_FIELDS}
    raise ValueError('unknown Bin receipt')
   require(method in ['eth_getCode','eth_call','eth_getStorageAt'],'unexpected Bin RPC '+method);anchor=args[2]if method=='eth_getStorageAt'else args[1];require(isinstance(anchor,dict)and anchor.get('requireCanonical')is True,'Bin dependency not canonical pinned');bank=next((b for b in self.banks.values()if b['hash']==anchor.get('blockHash')),None);require(bank is not None,'unknown Bin hash');self.read_counts[method]=self.read_counts.get(method,0)+1;source=bank['source'];fault=bank['fault'];manager,vault=self.doc['manager'],self.doc['vault'];to=(args[0]['to']if method=='eth_call'else args[0]).lower()
   if method=='eth_getCode':
    raw=source['code'][to]
    if fault=='manager-code'and to==manager or fault=='vault-code'and to==vault or fault=='token-code'and to==self.items[0]['pool']['token0']:raw='0x'+('00'if raw[2:4]!='00'else '01')+raw[4:]
    return raw
   if method=='eth_getStorageAt':return source['storage'].get(to,{}).get('0x'+f'{int(args[1],16):064x}','0x'+'0'*64)
   data=args[0]['data'].lower();require(data in source['callmap'].get(to,{}),'unrecorded Bin getter '+to+'/'+data);raw=source['callmap'][to][data];selector=data[:10]
   if fault=='key'and selector=='0x0e2d484a':raw=raw[:64]+('0'if raw[64]!='0'else '1')+raw[65:]
   elif fault=='pause'and selector=='0x5c975abb':raw='0x'+f'{1:064x}'
   elif fault=='registration'and selector=='0x8403be91':raw='0x'+'0'*64
   elif fault=='slot0'and selector=='0xc815641c':raw='0x'+'0'*64+raw[66:]
   elif fault=='next-link'and selector=='0xbf7b7286':raw='0x'+f'{8388608:064x}'
   elif fault=='bin-share'and selector=='0x4580c3c5':raw=raw[:194]+'0'*64
   elif fault=='tree-child'and selector=='0x1e2eaeaf':
    roots={int(p['pool_base'],16)+4 for p in self.doc['pools']}
    if int(data[10:],16)not in roots:raw='0x'+'0'*64
   return raw
 def provider(self):
  chain=self
  class Handler(BaseHTTPRequestHandler):
   def log_message(self,*_):pass
   def do_POST(self):
    q=json.loads(self.rfile.read(int(self.headers['Content-Length'])));v={'jsonrpc':'2.0','id':q['id']}
    try:v['result']=chain.rpc(q['method'],q['params'])
    except(ValueError,KeyError,TypeError,IndexError)as e:v['error']={'code':-32000,'message':str(e)}
    data=json.dumps(v).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers()
    try:self.wfile.write(data)
    except(BrokenPipeError,ConnectionResetError):pass
  s=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=s.serve_forever,daemon=True).start();return s

def replay_plan(quote,request,items,native):
 states={i['pool']['pool_id']:copy.deepcopy(i['pool']['infinity_bin'])for i in items};currencies={};tokens={i['pool']['pool_id']:(i['pool']['token0'],i['pool']['token1'])for i in items}
 for i in items:
  p=i['pool'];s=p['infinity_bin']
  for k in [0,1]:currencies[(s['manager'],p['token'+str(k)])]=[int(s['app_reserve'+str(k)]),int(s['protocol_fees'+str(k)])]
 total_in=total_out=steps=0
 for route in quote['routes']:
  edges=route['path']['edges'];require(len(edges)==1,'Bin replay requires direct pair scope');edge=edges[0];pid=edge['pool_id'];require(pid in states and edge['protocol']=='pancake-infinity-bin','Bin unknown pool/protocol');s=states[pid];t0,t1=tokens[pid];left=request['sellToken']==t0;require(request['buyToken']==(t1 if left else t0)and edge['zero_for_one']==left,'Bin route direction differs');n=int(route['amount_in']);native_state={'active_id':s['active_id'],'bin_step':s['bin_step'],'lp_fee':s['lp_fee'],'protocol_fee':s['protocol_fee0']+(s['protocol_fee1']<<12),'bins':s['bins']};expected=native.request({'action':'quote','state':native_state,'amount':str(n),'zero':left});require(expected.get('ok')is True and expected['amount_in']==str(n),'selected Bin route reverts in original EVM');out=int(expected['amount_out']);require(out>0 and route['amount_out']==str(out)and route['path']['amount_out']==str(out),'Bin selected route native output differs');post=expected['nextstate'];s['active_id']=post['active_id'];s['bins']=[{'id':int(i),'reserve0':b['reserve0'],'reserve1':b['reserve1'],'total_shares':b['total_shares']}for i,b in sorted(post['bins'].items(),key=lambda e:int(e[0]))];input_token,output_token=(t0,t1)if left else(t1,t0);input_ledger=currencies[(s['manager'],input_token)];output_ledger=currencies[(s['manager'],output_token)];require(input_ledger[0]+n<1<<256 and output_ledger[0]>=out,'Bin native plan app-reserve infeasible');input_ledger[0]+=n;output_ledger[0]-=out;input_ledger[1]=(input_ledger[1]+int(expected['protocol_amount']))%(1<<256);total_in+=n;total_out+=out;steps+=1
 require(str(total_in)==request['sellAmount']==quote['consumedIn']and str(total_out)==quote['grossBuyAmount']and quote['refund']=='0','Bin allocation input/output does not conserve original EVM amounts');return steps

def run():
 started_at=int(time.time()*1000);input_hashes=snapshot_inputs();fingerprint=runtime_fingerprint();report=initial();chain=Chain();servers=[chain.provider(),chain.provider()];native=Native();report['source_binding']=fingerprint;processes={};logs={};native_steps=0
 try:
  with Infrastructure()as infra,tempfile.TemporaryDirectory(prefix='xone-bin-')as folder:
   root=Path(folder);token=secrets.token_urlsafe(32);names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'};ports={a:port()for a in names};sources=[f'http://127.0.0.1:{ports[a]}'for a in ['go','rust']];configs={}
   for actor,name in names.items():
    c=config_for(name,root/actor,token,ports[actor]);c['journal'].update(transport='kafka',source_urls=sources,poll_interval_ms=100,request_timeout_ms=15000);c['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=actor,transaction_timeout_ms=4000);c['control'].update(backend='disabled'if name=='dex-router'else 'postgres',dsn_file=''if name=='dex-router'else str(infra.dsnfile),instance_id=actor,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000);c['startup']['max_baseline_age_ms']=FRESHNESS
    if actor=='go':c['chains'][0].update(genesis_hash=GENESIS,confirmation_tag='latest',auto_reorg_window=4,rpc_urls=[f'http://127.0.0.1:{s.server_port}'for s in servers],pools=[i['config']for i in chain.items])
    elif actor=='rust':sample_for('solana:localnet',root/actor/'fixtures',chain.timestamp*1000)
    else:c['routing'].update(max_state_age_ms=FRESHNESS,quote_ttl_ms=60000,max_hops=1,max_candidates=8,max_expansions=64,split_parts=4,max_splits=2);c['api']['quote_timeout_ms']=10000
    configs[actor]=save_config(root/actor/'config',c)
   def base(a):return f'http://127.0.0.1:{ports[a]}'
   def start(a):logs[a]=(OUT/f'infinity-bin-{a}.log').open('ab');processes[a]=subprocess.Popen(service_command(names[a],configs[a],root/a/'fixtures'if a=='rust'else None),cwd=ROOT/names[a],stdout=logs[a],stderr=subprocess.STDOUT)
   def stop(a):
    p=processes.pop(a,None)
    if p:
     p.terminate()
     try:p.wait(timeout=10)
     except subprocess.TimeoutExpired:p.kill();p.wait()
    if a in logs:logs.pop(a).close()
   def status(a,key='evm:31337'):return http(base(a)+'/internal/chains/'+key+'/status',token)
   def owner():
    code,s=http(base('go')+'/internal/chains/evm:31337/baseline',token);require(code==200,('Bin owner unavailable',code,s));return s
   def complete():
    await_condition(lambda:status('go')[1].get('snapshot_ref',{}).get('block',{}).get('number_or_slot')==chain.head,'Bin owner source bank missing');s=owner()
    for actor in ['router1','router2']:await_condition(lambda:status(actor)[1].get('snapshot_ref')==s['reference'],'Bin replica source bank differs')
    require({p['pool_id']:p for p in s['pools']}=={i['pool']['pool_id']:i['pool']for i in chain.items},'Bin complete parsed tree/raw/shared ledger differs');r=urllib.request.Request(base('go')+'/internal/chains/evm:31337/bootstrap',headers={'Authorization':'Bearer '+token})
    with urllib.request.urlopen(r,timeout=15)as response:require(response.headers['content-type']=='application/x-protobuf','Bin bootstrap not protobuf');raw=response.read()
    expected=subprocess.run(['protoc','-I',str(ROOT/'common/protoc/proto'),'--encode=dex.v1.OnlineSnapshot',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=wire.encode(s).encode(),capture_output=True,check=True).stdout;require(bootstrap_snapshot(raw)==expected,'Bin live protobuf loses tree/share/source');return s
   def quote(actor,amount,left,reject=False):
    nonlocal native_steps
    p=chain.items[0]['pool'];r={'chainKey':'evm:31337','kind':'EXACT_IN','sellToken':p['token0'if left else'token1'],'buyToken':p['token1'if left else'token0'],'sellAmount':amount,'caller':p['token0'],'recipient':p['token1'],'slippageBps':50,'verification':'FAST'};code,q=http(base(actor)+'/v1/quote',token,r)
    if reject:require(code in [400,404]and 'quoteId'not in q,('huge Bin input produced quote',code,q));return None,r
    require(code==200,('Bin quote failed',code,q));native_steps+=replay_plan(q,r,chain.items,native);require(q['snapshotRef']==owner()['reference']and q['verification']['level']=='INDICATIVE','Bin quote source/verification differs');require(set(q['quality']['candidateScope'])=={i['pool']['pool_id']for i in chain.items},'Bin scope incomplete');return q,r
   try:
    for actor in ['go','rust']:start(actor);wait(base(actor)+'/health/ready')
    for actor in ['router1','router2']:start(actor);wait(base(actor)+'/health/live');wait_scope(base(actor),token,'evm:31337');wait_scope(base(actor),token,'solana:localnet')
    for phase in range(5):
     if phase:chain.advance(phase)
     complete()
     for actor in ['router1','router2']:
      if phase<4:t=chain.doc['trades'][phase];quote(actor,t['amount_in'],t['zero_for_one'])
      else:quote(actor,str((1<<126)-1),True,True)
    report['checks']+=CHECKS[:5];old=owner()['reference']['online_generation']
    for fault in FAULTS:
     chain.advance(fault=fault)
     for actor in ['router1','router2']:
      await_condition(lambda:status(actor)[0]==503,'bad Bin source remained ready');code,v=http(base(actor)+'/v1/quote',token,{'chainKey':'solana:localnet','kind':'EXACT_IN','sellToken':'A','buyToken':'B','sellAmount':'10003','caller':'A','recipient':'B','slippageBps':50,'verification':'FAST'});require(code==200 and v['verification']['level']=='INDICATIVE'and int(v['grossBuyAmount'])>0,'unrelated Solana quote failed')
     chain.advance(fault=None);s=complete();require(s['reference']['online_generation']>old,'Bin recovery reused generation');old=s['reference']['online_generation'];quote('router1','10000',True);report['checks'].append('scope-withdraw-and-new-generation:'+fault)
    report['checks'].append('unrelated-solana-quote-remains-ready');stop('router2');start('router2');wait(base('router2')+'/health/live');wait_scope(base('router2'),token,'evm:31337');complete();quote('router2','10000',False);report['checks'].append('router-restart-restores-bin-tree-bank')
    for actor in ['router1','router2']:
     q,r=quote(actor,'10000',True);r['verification']='VERIFIED';code,v=http(base(actor)+'/v1/quote',token,r);require(code==422 and 'INDEPENDENT_SIMULATION_UNAVAILABLE'in json.dumps(v),'Bin VERIFIED enabled');code,_=http(base(actor)+'/v1/quotes/'+q['quoteId']+'/build',token,{});require(code==422,'Bin build enabled')
    report['checks'].append('verified-and-build-remain-disabled');require(report['checks']==CHECKS,'Bin process coverage incomplete');require(runtime_fingerprint()==fingerprint,'Bin source or binaries changed');require(snapshot_inputs()==input_hashes,'Bin reference or fixture inputs changed during process run');report.update(input_hashes=input_hashes,inputs_unchanged=True,started_at_unix_ms=started_at,completed_at_unix_ms=int(time.time()*1000),passed=True,state='PASSED',pool_count=2,banks=5,quote_checks=10,successful_quotes=8,large_input_rejections=2,native_plan_steps=native_steps,read_counts=chain.read_counts,receipt_reads=chain.receipts,owner_reference=owner()['reference'],test_quote_budget={'max_hops':1,'max_candidates':8,'max_expansions':64,'split_parts':4,'max_splits':2,'timeout_ms':10000});return report
   finally:
    for actor in list(processes):stop(actor)
 finally:
  for server in servers:server.shutdown();server.server_close()
  native.close()
def main():
 write(initial())
 try:r=run();write(r);print(json.dumps({'passed':True,'checks':len(r['checks']),'native_steps':r['native_plan_steps']}))
 except BaseException as e:r=initial();r.update(state='FAILED',error=str(e),error_type=type(e).__name__);write(r);raise
if __name__=='__main__':main()
