#!/usr/bin/env python3
"""Original Crypto exchange replay through ordinary RPC/PG/Kafka and two routers.

Source addresses/envelopes are synthetic local banks. This is not a live-chain
or transaction execution verification. All standard-profile poststate is checked.
"""
from pathlib import Path
import copy,hashlib,importlib.util,json,secrets,subprocess,tempfile,threading,time,urllib.request
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
ROOT=Path(__file__).resolve().parents[3];OUT=ROOT/'integration/artifacts/local';TARGET=OUT/'curve-crypto-three-service-smoke.json'
def require(ok,message):
 if not ok:raise ValueError(message)
def write(value):
 TARGET.parent.mkdir(parents=True,exist_ok=True);tmp=TARGET.with_name(TARGET.name+'.'+secrets.token_hex(6)+'.tmp')
 try:tmp.write_text(json.dumps(value,indent=2)+'\n');tmp.replace(TARGET)
 finally:tmp.unlink(missing_ok=True)
def initial():return {'passed':False,'state':'RUNNING','verified':False,'production_ready':False,'all_requested_features_complete':False,'trust':'SYNTHETIC_RPC_NOT_DEPLOYMENT_PROOF','checks':[]}
if __name__=='__main__':write(initial())
from infrastructure import Infrastructure,port
from configuration import config_for,save_config,service_command,sample_for
from receipt_reference import EMPTY_ROOT,SINGLE_EMPTY_ROOT,SINGLE_EMPTY_FIELDS
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import http,wait_scope
from curve_ng_smoke import bootstrap_snapshot
from weighted_evidence import runtime_fingerprint,snapshot_inputs
spec=importlib.util.spec_from_file_location('crypto_wire',ROOT/'integration/tools/ci/curve-crypto-wire-golden.py');wire=importlib.util.module_from_spec(spec);spec.loader.exec_module(wire)
GENESIS='0x'+'99'*32;FRESHNESS=315360000000;FAULTS=['lp-code','minter','raw-balance','template','ramp']
def sha(data):return hashlib.sha256(data).hexdigest()
def word(v):
 v=int(v);require(0<=v<1<<256,'bad uint256');return f'{v:064x}'
def address(i):return '0x'+f'{i:040x}'
class Chain:
 def __init__(self,fixture="curve-crypto-service.json",phases=15):
  self.template=json.loads((ROOT/'integration/test/contracts/curve-crypto-runtime-template.json').read_text());self.corpus=json.loads((ROOT/'integration/test/contracts'/fixture).read_text());self.lock=threading.RLock();self.head=99;self.banks={};self.reads={};self.receipts=0
  self.groups=[[c for c in self.corpus['cases']if c['name'].endswith('-'+suffix)]for suffix in ['18-18','18-6']];require(len(self.groups[0])==len(self.groups[1])==phases,'Crypto phase inventory changed');self.advance(0)
 def advance(self,phase=None,fault=None):
  with self.lock:
   phase=getattr(self,'phase',0)if phase is None else phase;items=[];self.head+=1;blockhash='0x'+sha(f'CRYPTO/service/{self.head}'.encode())
   for index,group in enumerate(self.groups):
    c=copy.deepcopy(group[phase]);p=c['pool'];s=p['multi_asset'];crypto=s['curve_crypto'];n=100*index;assets=s['assets'];p.update(pool_id=address(50+n),token0=address(1+n),token1=address(2+n));assets[0]['token']=p['token0'];assets[1]['token']=p['token1'];crypto.update(lp_token=address(99+n),admin_receiver=address(98+n)if crypto['admin_receiver']!=address(0)else address(0),block_number=self.head,block_hash=blockhash)
    tail=b''.join(int(v,16).to_bytes(32,'big')for v in [crypto['lp_token'],p['token0'],p['token1']])+b''.join((int(a['scaling'])//10**18).to_bytes(32,'big')for a in assets);c['runtime']=bytes.fromhex(self.template['runtime_template_hex'])+tail;s['code_sha256']=sha(c['runtime']);c['config'].update(id=p['pool_id'],token0=p['token0'],token1=p['token1']);c['config']['multi_asset'].update(expected_code_sha256=s['code_sha256'],expected_lp_token=crypto['lp_token'],tokens=[{'token':a['token'],'expected_code_sha256':a['code_sha256']}for a in assets]);items.append(c)
   require(len({i['pool']['multi_asset']['curve_crypto']['timestamp']for i in items})==1,'phase timestamp differs');self.timestamp=items[0]['pool']['multi_asset']['curve_crypto']['timestamp'];self.phase=phase;self.items=items;self.banks[self.head]={'items':items,'timestamp':self.timestamp,'hash':blockhash,'fault':fault}
 def block(self,number):
  if number==0:return {'number':'0x0','hash':GENESIS,'parentHash':'0x'+'00'*32,'timestamp':hex(self.timestamp),'receiptsRoot':EMPTY_ROOT,'transactions':[]}
  require(0<number<=self.head,'unknown block');h='0x'+sha(f'CRYPTO/service/{number}'.encode());bank=self.banks.get(number)
  return {'number':hex(number),'hash':h,'parentHash':GENESIS if number==1 else '0x'+sha(f'CRYPTO/service/{number-1}'.encode()),'timestamp':hex(bank['timestamp']if bank else self.timestamp),'receiptsRoot':SINGLE_EMPTY_ROOT,'transactions':['0x'+sha(('tx/'+h).encode())]}
 def rpc(self,method,args):
  with self.lock:
   if method=='eth_chainId':return '0x7a69'
   if method=='eth_getBlockByNumber':return self.block(self.head if args[0]in ['latest','safe','finalized']else int(args[0],16))
   if method=='eth_getTransactionReceipt':
    for i in range(1,self.head+1):
     b=self.block(i)
     if args[0]in b['transactions']:self.receipts+=1;return {'blockHash':b['hash'],'transactionHash':args[0],'transactionIndex':'0x0','logs':[],**SINGLE_EMPTY_FIELDS}
    raise ValueError('unknown receipt')
   require(method in ['eth_getCode','eth_getStorageAt','eth_call'],'unexpected RPC '+method);anchor=args[2]if method=='eth_getStorageAt'else args[1];require(isinstance(anchor,dict)and anchor.get('requireCanonical')is True,'dependency missing canonical anchor');bank=next((b for b in self.banks.values()if b['hash']==anchor.get('blockHash')),None);require(bank is not None,'unknown bank');self.reads[method]=self.reads.get(method,0)+1;target=(args[0]['to']if method=='eth_call'else args[0]).lower();item=next((i for i in bank['items']if target in [i['pool']['pool_id'],i['pool']['multi_asset']['curve_crypto']['lp_token'],*[a['token']for a in i['pool']['multi_asset']['assets']]]),None);require(item is not None,'unexpected target');p=item['pool'];s=p['multi_asset'];c=s['curve_crypto'];fault=bank['fault']
   if method=='eth_getCode':
    b=item['runtime']if target==p['pool_id']else bytes.fromhex(self.template['lp_runtime_hex'])if target==c['lp_token']else bytes.fromhex('600255')
    if fault=='template'and target==p['pool_id']or fault=='lp-code'and target==c['lp_token']:b=bytes([b[0]^1])+b[1:]
    return '0x'+b.hex()
   if method=='eth_getStorageAt':
    require(target==p['pool_id'],'foreign storage');slots={1:c['price_scale'],2:c['price_oracle'],3:c['last_prices'],4:c['last_prices_timestamp'],6:(int(s['amplification'])<<128)|int(c['gamma']),8:2000000000 if fault=='ramp'else c['future_a_gamma_time'],9:c['allowed_extra_profit'],11:c['fee_gamma'],13:c['adjustment_step'],15:c['ma_half_time'],17:p['fee_numerator'],18:c['out_fee'],19:s['admin_fee'],23:s['assets'][0]['balance'],24:s['assets'][1]['balance'],25:c['invariant'],28:c['xcp_profit'],29:c['xcp_profit_a'],30:c['virtual_price'],31:int(c['not_adjusted']),32:0,36:int(c['admin_receiver'],16)};return '0x'+word(slots[int(args[1],16)])
   data=args[0]['data'];sel=data[:10]
   if sel=='0xfc0c546a':require(target==p['pool_id']and len(data)==10,'bad token getter');v=int(c['lp_token'],16)
   elif sel=='0x07546172':require(target==c['lp_token']and len(data)==10,'bad minter getter');v=int(p['pool_id'],16)+(1 if fault=='minter'else 0)
   elif sel=='0xc6610657':require(target==p['pool_id']and len(data)==74,'bad coins');v=int(s['assets'][int(data[10:],16)]['token'],16)
   elif sel=='0x313ce567':require(len(data)==10,'bad decimals');asset=next(a for a in s['assets']if a['token']==target);v=36-(len(asset['scaling'])-1)
   elif sel=='0x18160ddd':require(target==c['lp_token']and len(data)==10,'bad supply');v=c['total_supply']
   elif sel=='0x70a08231':
    require(len(data)==74,'bad balanceOf');account='0x'+data[-40:]
    if target==c['lp_token']:require(account==c['admin_receiver'],'wrong LP receiver');v=c['admin_receiver_balance']
    else:require(account==p['pool_id'],'wrong raw balance owner');index=next(i for i,a in enumerate(s['assets'])if a['token']==target);v=0 if fault=='raw-balance'else c['raw_balances'][index]
   else:raise ValueError('unknown selector '+sel)
   return '0x'+word(v)
 def provider(self):
  chain=self
  class Handler(BaseHTTPRequestHandler):
   def log_message(self,*args):pass
   def do_POST(self):
    q=json.loads(self.rfile.read(int(self.headers['Content-Length'])));response={'jsonrpc':'2.0','id':q['id']}
    try:response['result']=chain.rpc(q['method'],q['params'])
    except(ValueError,KeyError,TypeError,IndexError,StopIteration)as error:response['error']={'code':-32000,'message':str(error)}
    data=json.dumps(response).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers()
    try:self.wfile.write(data)
    except(BrokenPipeError,ConnectionResetError):pass
  server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start();return server

def run(chain_factory=Chain,faults=FAULTS,log_prefix="curve-crypto",recovery_phase=0):
 started=int(time.time()*1000);inputs=snapshot_inputs();report=initial();chain=chain_factory();servers=[chain.provider(),chain.provider()];processes={};logs={};fingerprint=runtime_fingerprint();report['source_binding']=fingerprint
 try:
  with Infrastructure()as infra,tempfile.TemporaryDirectory(prefix='xone-crypto-')as folder:
   root=Path(folder);token=secrets.token_urlsafe(32);names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'};ports={a:port()for a in names};configs={};sources=[f'http://127.0.0.1:{ports[a]}'for a in ['go','rust']]
   for actor,name in names.items():
    config=config_for(name,root/actor,token,ports[actor]);config['journal'].update(transport='kafka',source_urls=sources,poll_interval_ms=100,request_timeout_ms=15000);config['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=actor,transaction_timeout_ms=4000);config['control'].update(backend='disabled'if name=='dex-router'else 'postgres',dsn_file=''if name=='dex-router'else str(infra.dsnfile),instance_id=actor,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000);config['startup']['max_baseline_age_ms']=FRESHNESS
    if actor=='go':config['chains'][0].update(genesis_hash=GENESIS,confirmation_tag='latest',auto_reorg_window=4,rpc_urls=[f'http://127.0.0.1:{s.server_port}'for s in servers],pools=[item['config']for item in chain.items])
    elif actor=='rust':sample_for('solana:localnet',root/actor/'fixtures',chain.timestamp*1000)
    else:config['routing'].update(max_state_age_ms=FRESHNESS,quote_ttl_ms=60000)
    configs[actor]=save_config(root/actor/'config',config)
   def base(actor):return f'http://127.0.0.1:{ports[actor]}'
   def start(actor):logs[actor]=(OUT/f'{log_prefix}-{actor}.log').open('ab');processes[actor]=subprocess.Popen(service_command(names[actor],configs[actor],root/actor/'fixtures'if actor=='rust'else None),cwd=ROOT/names[actor],stdout=logs[actor],stderr=subprocess.STDOUT)
   def stop(actor):
    process=processes.pop(actor,None)
    if process:
     process.terminate()
     try:process.wait(timeout=10)
     except subprocess.TimeoutExpired:process.kill();process.wait()
    if actor in logs:logs.pop(actor).close()
   def status(actor,key='evm:31337'):return http(base(actor)+'/internal/chains/'+key+'/status',token)
   def owner():
    code,s=http(base('go')+'/internal/chains/evm:31337/baseline',token);require(code==200,('Crypto baseline unavailable',code,s));return s
   def synced():
    await_condition(lambda:status('go')[1].get('snapshot_ref',{}).get('block',{}).get('number_or_slot')==chain.head,'Crypto owner did not reach source bank');s=owner()
    for actor in ['router1','router2']:await_condition(lambda:status(actor)[1].get('snapshot_ref')==s['reference'],'Crypto replica source bank mismatch')
    return s
   def complete_bank():
    s=synced();require({p['pool_id']:p for p in s['pools']}=={i['pool']['pool_id']:i['pool']for i in chain.items},'complete Crypto pricing/raw/LP bank differs from original');request=urllib.request.Request(base('go')+'/internal/chains/evm:31337/bootstrap',headers={'Authorization':'Bearer '+token})
    with urllib.request.urlopen(request,timeout=10)as response:require(response.headers['content-type']=='application/x-protobuf','bootstrap not protobuf');raw=response.read()
    encoded=subprocess.run(['protoc','-I',str(ROOT/'common/protoc/proto'),'--encode=dex.v1.OnlineSnapshot',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=wire.base.encode(s).encode(),capture_output=True,check=True).stdout;require(bootstrap_snapshot(raw)==encoded,'live Crypto protobuf changed complete state');return s
   def quote(actor,item):
    p=item['pool'];assets=p['multi_asset']['assets'];request={'chainKey':'evm:31337','kind':'EXACT_IN','sellToken':assets[item['index_in']]['token'],'buyToken':assets[item['index_out']]['token'],'sellAmount':item['input'],'caller':assets[0]['token'],'recipient':assets[1]['token'],'slippageBps':50,'verification':'FAST'};code,q=http(base(actor)+'/v1/quote',token,request)
    if item['expected'].get('reverted'):require(code in [400,404]and 'quoteId'not in q,('native revert returned quote',item['name'],code,q));return False
    require(code==200,('Crypto quote failed',item['name'],code,q));require(q['grossBuyAmount']==item['expected']['amount_out']and q['consumedIn']==item['input']and q['refund']=='0','original exchange amount differs');require(q['verification']['level']=='INDICATIVE'and q['snapshotRef']==owner()['reference'],'Crypto source or verification differs');require(set(q['quality']['candidateScope'])=={i['pool']['pool_id']for i in chain.items},'incomplete candidate scope');require(all(len(r['path']['edges'])==1 and r['path']['edges'][0]['pool_id']==p['pool_id']for r in q['routes']),'Crypto physical pool differs');return True
   try:
    for actor in ['go','rust']:start(actor);wait(base(actor)+'/health/ready')
    for actor in ['router1','router2']:start(actor);wait(base(actor)+'/health/live');wait_scope(base(actor),token,'evm:31337');wait_scope(base(actor),token,'solana:localnet')
    positives=reverts=0
    for phase in range(len(chain.groups[0])):
     if phase:chain.advance(phase)
     complete_bank()
     for actor in ['router1','router2']:
      for item in chain.items:
       if quote(actor,item):positives+=1
       else:reverts+=1
    report['checks']+=['normal-go-rpc-postgres-kafka-and-two-routers',f'{2*len(chain.groups[0])}-original-exchanges-complete-live-pricing-raw-lp-state','independent-live-protobuf-and-canonical-hash-pinning','native-reverts-never-return-quotes' if reverts else 'native-positive-only-corpus-no-execution-claim']
    chain.advance(recovery_phase);before=complete_bank()['reference']['online_generation']
    for fault in faults:
     chain.advance(fault=fault)
     for actor in ['router1','router2']:
      await_condition(lambda:status(actor)[0]==503,'invalid Crypto source stayed ready');require(status(actor,'solana:localnet')[0]==200,'unrelated Solana scope withdrawn');code,sol=http(base(actor)+'/v1/quote',token,{'chainKey':'solana:localnet','kind':'EXACT_IN','sellToken':'A','buyToken':'B','sellAmount':'10003','caller':'A','recipient':'B','slippageBps':50,'verification':'FAST'});require(code==200 and sol['verification']['level']=='INDICATIVE' and int(sol['grossBuyAmount'])>0,'unrelated Solana quote failed')
     chain.advance(fault=None);s=complete_bank();require(s['reference']['online_generation']>before,'recovery reused old generation');before=s['reference']['online_generation'];quote('router1',chain.items[0]);report['checks'].append('scope-withdraw-and-new-generation:'+fault)
    report['checks'].append('unrelated-solana-quotes-survive-crypto-withdrawal')
    stop('router2');start('router2');wait(base('router2')+'/health/live');wait_scope(base('router2'),token,'evm:31337');complete_bank();quote('router2',chain.items[0]);report['checks'].append('router-restart-restores-complete-crypto-bank')
    for actor in ['router1','router2']:
     item=chain.items[0];p=item['pool'];assets=p['multi_asset']['assets'];request={'chainKey':'evm:31337','kind':'EXACT_IN','sellToken':assets[item['index_in']]['token'],'buyToken':assets[item['index_out']]['token'],'sellAmount':item['input'],'caller':assets[0]['token'],'recipient':assets[1]['token'],'slippageBps':50,'verification':'FAST'};code,quoted=http(base(actor)+'/v1/quote',token,request);require(code==200,'missing quote for execution guard');request['verification']='VERIFIED';code,value=http(base(actor)+'/v1/quote',token,request);require(code==422 and 'INDEPENDENT_SIMULATION_UNAVAILABLE'in json.dumps(value),'Crypto VERIFIED enabled');code,_=http(base(actor)+'/v1/quotes/'+quoted['quoteId']+'/build',token,{});require(code==422,'Crypto build enabled')
    report['checks'].append('verified-and-build-remain-disabled')
    require(fingerprint==runtime_fingerprint(),'source or binaries changed during process run');require(inputs==snapshot_inputs(),'execution inputs changed during process run');report.update(input_hashes=inputs,inputs_unchanged=True,started_at_unix_ms=started,completed_at_unix_ms=int(time.time()*1000),passed=True,state='PASSED',quote_checks=positives+reverts,successful_quotes=positives,native_revert_rejections=reverts,phase_count=len(chain.groups[0]),pool_count=2,read_counts=chain.reads,receipt_reads=chain.receipts,owner_reference=owner()['reference']);return report
   finally:
    for actor in list(processes):stop(actor)
 finally:
  for server in servers:server.shutdown();server.server_close()

def main():
 write(initial())
 try:report=run();write(report);print(json.dumps({'passed':report['passed'],'checks':len(report['checks']),'quotes':report['quote_checks']}))
 except BaseException as error:report=initial();report.update(state='FAILED',error=str(error),error_type=type(error).__name__);write(report);raise
if __name__=='__main__':main()
