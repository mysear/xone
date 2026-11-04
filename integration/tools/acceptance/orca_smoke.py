#!/usr/bin/env python3
"""Normal Orca RPC owner with fixed/dynamic account bytes, real PG/Kafka and two routers."""
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
import json,hashlib,copy,secrets,subprocess,tempfile,time,threading,base64
from infrastructure import ROOT,Infrastructure,port
from configuration import config_for,save_config,service_command,sample_for
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import http,quote,wait_scope
OUT=ROOT/'integration/artifacts/local';BASE=json.loads((ROOT/'integration/test/solana/orca-base.json').read_text());GOLDEN=json.loads((ROOT/'integration/test/contracts/orca-quotes.json').read_text());ALPHABET='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
PROGRAM=BASE['pool']['orca_whirlpool']['program_id'];TOKEN=BASE['metadata']['token_program'];CLOCK=BASE['metadata']['clock']
def b58(b):
 n=int.from_bytes(b,'big');s=''
 while n:s=ALPHABET[n%58]+s;n//=58
 return '1'*(len(b)-len(b.lstrip(b'\0')))+s
def integer(b,at,n,size=8):b[at:at+size]=n.to_bytes(size,'little')
def keybytes(k):
 n=0
 for c in k:n=n*58+ALPHABET.index(c)
 return b'\0'*(len(k)-len(k.lstrip('1')))+n.to_bytes((n.bit_length()+7)//8,'big')
class Chain:
 def __init__(self):self.head=123456;self.timestamp=int(time.time());self.layout='fixed';self.bad=None;self.reads=0;self.block_reads=0
 def pool(self):
  p=copy.deepcopy(BASE['pool'])
  for a in p['orca_whirlpool']['tick_arrays']:a['layout']=self.layout
  return p
 def accounts(self):
  p=self.pool();s=p['orca_whirlpool'];out={};array_ids={a['address'] for a in s['tick_arrays']}
  for address,a in BASE['rpc_accounts'].items():
   d=bytearray.fromhex(a['data_hex']);owner=a['owner']
   if address==CLOCK:integer(d,0,self.head+(1 if self.bad=='clock-slot' else 0));integer(d,32,self.timestamp)
   if address in array_ids:
    if self.layout=='dynamic':
     fixed=d;d=bytearray(hashlib.sha256(b'account:DynamicTickArray').digest()[:8])+fixed[8:12]+fixed[9956:9988];bitmap=0;items=bytearray()
     for i in range(88):
      t=fixed[12+i*113:12+(i+1)*113]
      if t[0]:bitmap|=1<<i;items+=t
      else:items+=b'\0'
     d+=bitmap.to_bytes(16,'little')+items
    if self.bad=='tick-net':
     if self.layout=='dynamic':
      at=60
      for i in range(88):
       if d[at]:d[at+1:at+17]=((1<<127)-1).to_bytes(16,'little');break
       at+=1
     else:
      for i in range(88):
       at=12+i*113
       if d[at]:d[at+1:at+17]=((1<<127)-1).to_bytes(16,'little');break
   if address==p['pool_id']:
    if self.bad=='adaptive-tier':integer(d,43,s['tick_spacing']+1,2)
    if self.bad=='reward':d[269]=1
    if self.bad=='future-reward-time':integer(d,261,self.timestamp+1)
   if address==s['programdata']:
    if self.bad=='program-code':d[-1]^=1
    if self.bad=='program-slot':integer(d,4,s['deployed_slot']+1)
   if address==TOKEN and self.bad=='token-code':d[-1]^=1
   if address==s['whirlpools_config'] and self.bad=='config-flags':integer(d,106,1,2)
   if address==s['vault0'] and self.bad=='vault-authority':d[32:64]=keybytes(p['token0'])
   if address==s['oracle']:
    if self.bad=='oracle':owner=PROGRAM;d=bytearray(256)
    elif self.bad=='null-oracle':out[address]=None;continue
   out[address]={'owner':owner,'data':[base64.b64encode(d).decode(),'base64'],'executable':a['executable'],'lamports':a['lamports'],'rentEpoch':0}
  if self.bad=='missing-array':out[s['tick_arrays'][0]['address']]=None
  return out
 def block(self,n):return {'blockhash':b58(hashlib.sha256(f'orca/{n}'.encode()).digest()),'previousBlockhash':b58(hashlib.sha256(f'orca/{n-1}'.encode()).digest()),'parentSlot':n-1,'blockTime':self.timestamp,'blockHeight':n}
 def provider(self):
  chain=self;required=set(BASE['pool_config']['accounts'])|{PROGRAM,BASE['pool']['orca_whirlpool']['programdata'],TOKEN,CLOCK,BASE['pool']['orca_whirlpool']['oracle']}|{a['address']for a in BASE['pool']['orca_whirlpool']['tick_arrays']}
  class Handler(BaseHTTPRequestHandler):
   def log_message(self,*args):pass
   def do_POST(self):
    q=json.loads(self.rfile.read(int(self.headers['Content-Length'])));method=q['method'];params=q['params'];result=None;error=None
    if method=='getGenesisHash':result='localnet'
    elif method=='getMultipleAccounts':
     assert len(params[0])==14 and set(params[0])==required;assert params[1]['commitment']=='finalized' and params[1]['encoding']=='base64' and 'minContextSlot' not in params[1];a=chain.accounts();chain.reads+=1;result={'context':{'slot':chain.head},'value':[a[k]for k in params[0]]}
    elif method=='getBlock':assert params[1]['commitment']=='finalized';result=chain.block(params[0]);chain.block_reads+=1
    else:error={'code':-32601,'message':method}
    body={'jsonrpc':'2.0','id':q['id']};body.update({'error':error}if error else{'result':result});raw=json.dumps(body).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
  server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start();return server

def expected(chain,amount,zero):
 direction='a-b'if zero else'b-a';name=f'matrix/{chain.layout}/{direction}/fee-3000/protocol-2500/in-{amount}'
 return next(c['expected']['amount_out']for c in GOLDEN['cases']if c['name']==name)

def run():
 report={'profile':'orca-static-classic-spl-fixed-dynamic-normal-rpc-real-pg-kafka','production_ready':False,'checks':[]};chain=Chain();servers=[chain.provider(),chain.provider()];processes={};logs={}
 try:
  with Infrastructure() as infra,tempfile.TemporaryDirectory(prefix='xone-orca-') as folder:
   chain.timestamp=int(time.time());root=Path(folder);token=secrets.token_urlsafe(32);names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'};ports={a:port() for a in names};configs={};sources=[f'http://127.0.0.1:{ports[a]}' for a in ['go','rust']]
   for a,name in names.items():
    c=config_for(name,root/a,token,ports[a]);c['journal'].update(transport='kafka',source_urls=sources,poll_interval_ms=100,request_timeout_ms=15000);c['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=a,transaction_timeout_ms=4000);c['control'].update(backend='disabled' if name=='dex-router' else 'postgres',dsn_file='' if name=='dex-router' else str(infra.dsnfile),instance_id=a,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000);c['startup']['max_baseline_age_ms']=180000
    if a=='rust':c['chains'][0].update(genesis_hash='localnet',confirmation_tag='finalized',rpc_urls=[f'http://127.0.0.1:{s.server_port}' for s in servers],pools=[copy.deepcopy(BASE['pool_config'])])
    elif a=='go':sample_for('evm:31337',root/a/'fixtures',chain.timestamp*1000)
    else:c['routing'].update(max_state_age_ms=180000,quote_ttl_ms=60000)
    configs[a]=save_config(root/a/'config',c)
   def base(a):return f'http://127.0.0.1:{ports[a]}'
   def start(a):
    name=names[a];logs[a]=(OUT/f'orca-{a}.log').open('ab');processes[a]=subprocess.Popen(service_command(name,configs[a],root/a/'fixtures' if a=='go' else None),cwd=ROOT/name,stdout=logs[a],stderr=subprocess.STDOUT)
   def stop(a):
    p=processes.pop(a,None)
    if p:
     p.terminate()
     try:p.wait(timeout=10)
     except subprocess.TimeoutExpired:p.kill();p.wait()
    if a in logs:logs.pop(a).close()
   def status(a):return http(base(a)+'/internal/chains/solana:localnet/status',token)
   def check(a,zero=True,amount='1000000'):
    p=chain.pool();request={'chainKey':'solana:localnet','kind':'EXACT_IN','sellToken':p['token0' if zero else 'token1'],'buyToken':p['token1' if zero else 'token0'],'sellAmount':amount,'caller':p['token0'],'recipient':p['token1'],'slippageBps':50,'verification':'FAST'}
    code,q=http(base(a)+'/v1/quote',token,request);assert code==200,(code,q);assert q['grossBuyAmount']==expected(chain,amount,zero),(q,expected(chain,amount,zero));assert q['consumedIn']==amount and q['refund']=='0' and q['verification']['level']=='INDICATIVE';return q
   def current(a):return status(a)[1].get('snapshot_ref',{}).get('block',{}).get('number_or_slot')==chain.head
   try:
    for a in ['go','rust']:start(a);wait(base(a)+'/health/ready')
    for a in ['router1','router2']:start(a);wait(base(a)+'/health/live');wait_scope(base(a),token,'solana:localnet')
    for a in ['router1','router2']:
     for amount in ['1000000','10000000000']:check(a,True,amount);check(a,False,amount)
    report['checks'].append('normal-rust-rpc-fourteen-account-finalized-orca-both-direction-cross-tick-amounts-in-two-replicas')
    snap=http(base('rust')+'/internal/chains/solana:localnet/baseline',token)[1];p=snap['pools'][0];s=p['orca_whirlpool'];assert s['sqrt_price_x64']==BASE['pool']['orca_whirlpool']['sqrt_price_x64'] and s['bank_slot']==chain.head and len(s['tick_arrays'])==3 and not p.get('v3')
    report['checks'].append('q64-array-proof-fee-growth-program-pin-and-clock-cross-public-protobuf')
    chain.layout='dynamic';chain.head+=1
    for a in ['router1','router2']:await_condition(lambda:current(a),'dynamic arrays not published');check(a,True,'10000000000');check(a,False,'10000000000')
    report['checks'].append('dynamic-array-borsh-bitmap-and-sparse-payload-have-original-manager-amounts')
    chain.bad='null-oracle';chain.head+=1
    for a in ['router1','router2']:await_condition(lambda:current(a),'null uninitialized oracle not accepted');check(a)
    chain.bad=None
    report['checks'].append('only-declared-uninitialized-oracle-may-use-rpc-null')
    for fault in ['program-code','program-slot','token-code','clock-slot','adaptive-tier','oracle','reward','future-reward-time','vault-authority','config-flags','tick-net','missing-array']:
     old={a:check(a) for a in ['router1','router2']};chain.bad=fault;chain.head+=1
     for a in old:await_condition(lambda:status(a)[0]==503,'invalid Orca dependency not revoked');quote(base(a),token,'evm:31337','B')
     chain.bad=None
     for a in old:await_condition(lambda:status(a)[1].get('snapshot_ref',{}).get('online_generation',0)>old[a]['onlineGeneration'],'Orca qualified scope not restored');check(a)
     report['checks'].append('unsupported-'+fault+'-revokes-solana-only-and-qualified-baseline-restores')
    stop('router1');start('router1');wait(base('router1')+'/health/live');wait_scope(base('router1'),token,'solana:localnet');check('router1');check('router1',False);report['checks'].append('memory-only-router-restores-typed-orca-state-from-owner-checkpoint-tail')
    report['passed']=True;report['atomic_account_batches']=chain.reads;report['finalized_block_reads']=chain.block_reads
   finally:
    for a in list(processes):stop(a)
 finally:
  for s in servers:s.shutdown();s.server_close()
 report.update(measured_at_unix_ms=int(time.time()*1000),tool_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest());return report
if __name__=='__main__':
 OUT.mkdir(parents=True,exist_ok=True)
 try:r=run()
 except Exception as e:(OUT/'orca-three-service-smoke.json').write_text(json.dumps({'passed':False,'production_ready':False,'error':str(e),'error_type':type(e).__name__},indent=2)+'\n');raise
 (OUT/'orca-three-service-smoke.json').write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r,indent=2))
