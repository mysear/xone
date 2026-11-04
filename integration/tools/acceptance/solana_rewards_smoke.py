#!/usr/bin/env python3
"""Native reward poststates -> packed RPC -> normal parser -> PG/Kafka replicas."""
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
import argparse,base64,copy,hashlib,importlib.util,json,secrets,subprocess,tempfile,threading,time
from infrastructure import ROOT,Infrastructure,port
from configuration import config_for,save_config,service_command,sample_for
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import http,wait_scope
OUT=ROOT/'integration/artifacts/local';CLOCK='SysvarC1ock11111111111111111111111111111111'
def require(v,message):
 if not v:raise ValueError(message)
def b58(b):
 alpha='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';n=int.from_bytes(b,'big');out=''
 while n:out=alpha[n%58]+out;n//=58
 return'1'*(len(b)-len(b.lstrip(b'\0')))+out
class Chain:
 def __init__(self,family):
  spec=importlib.util.spec_from_file_location('reward_native',ROOT/'integration/tools/ci/solana-advanced-golden.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);self.identity=m.source_identity(m.FAMILIES[family][0]);self.oracle=m.Oracle(m.FAMILIES[family][4]);self.family=family;self.key='orca_whirlpool'if family=='orca'else'raydium_clmm';self.lock=threading.RLock();self.bad=False;self.reads=0
  self.pool=copy.deepcopy(json.loads((ROOT/'integration/test/contracts/solana-advanced'/(family+'-rewards.json')).read_text())['base_pool']);self.pool[self.key]['bank_unix_timestamp']=int(time.time());self.replace(self.pool)
 def replace(self,p):
  with self.lock:self.pool=copy.deepcopy(p);self.fixture=self.oracle.request(action='fixture',pool=self.pool);self.head=p[self.key]['bank_slot'];self.timestamp=p[self.key]['bank_unix_timestamp']
 def next(self,p=None,bad=False):
  new=copy.deepcopy(self.pool if p is None else p);new[self.key]['bank_slot']=self.head+1;new[self.key]['bank_unix_timestamp']=int(time.time());self.replace(new);self.bad=bad
 def quote(self,zero):
  r=self.oracle.request(action='quote',pool=self.pool,input='100000',zero_for_one=zero);require(not r.get('reverted'),r);return r
 def provider(self):
  chain=self
  class Handler(BaseHTTPRequestHandler):
   def log_message(self,*_):pass
   def do_POST(self):
    q=json.loads(self.rfile.read(int(self.headers['Content-Length'])));m=q['method'];args=q['params'];out=None;error=None
    try:
     with chain.lock:
      if m=='getGenesisHash':out='localnet'
      elif m=='getMultipleAccounts':
       require(args[1]['commitment']=='finalized','not finalized');values=[]
       for key in args[0]:
        a=copy.deepcopy(chain.fixture['rpc_accounts'].get(key))
        if a is not None:
         data=bytearray(bytes.fromhex(a['data_hex'])if'data_hex'in a else base64.b64decode(a['data'][0]));a.pop('data_hex',None)
         if key==CLOCK:data[0:8]=chain.head.to_bytes(8,'little');data[32:40]=chain.timestamp.to_bytes(8,'little',signed=True)
         if chain.bad and key==chain.pool['pool_id']:
          if chain.family=='orca':data[269+2*128]=1
          else:data[397]=255
         a['data']=[base64.b64encode(data).decode(),'base64']
        values.append(a)
       out={'context':{'slot':chain.head},'value':values};chain.reads+=1
      elif m=='getBlock':
       n=args[0];out={'blockTime':chain.timestamp,'blockhash':b58(hashlib.sha256(f'reward/{n}'.encode()).digest()),'previousBlockhash':b58(hashlib.sha256(f'reward/{n-1}'.encode()).digest()),'parentSlot':n-1}
      else:raise ValueError('unexpected '+m)
    except Exception as e:error={'code':-32000,'message':str(e)}
    response={'jsonrpc':'2.0','id':q['id']};response.update({'error':error}if error else{'result':out});data=json.dumps(response).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(data)));self.end_headers()
    try:self.wfile.write(data)
    except(BrokenPipeError,ConnectionResetError):pass
  s=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=s.serve_forever,daemon=True).start();return s
CHECKS=['normal-finalized-rpc-initialized-rewards-and-full-protobuf-state','both-directions-match-live-original-native-swap-manager','ordered-native-poststates-reparse-completely-and-update-both-replicas','invalid-reward-account-revokes-only-solana-and-qualified-generation-recovers','memory-replica-restart-retains-all-reward-and-tick-growth-fields']
def run(family):
 chain=Chain(family);servers=[chain.provider(),chain.provider()];processes={};logs={};report={'passed':False,'family':family,'production_ready':False,'sbf_executed':False,'token_program_executed':False,'source_identity':chain.identity,'checks':[]}
 try:
  with Infrastructure()as infra,tempfile.TemporaryDirectory(prefix='xone-reward-')as folder:
   root=Path(folder);token=secrets.token_urlsafe(32);names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'};ports={a:port()for a in names};configs={};sources=[f'http://127.0.0.1:{ports[a]}'for a in['go','rust']]
   for a,name in names.items():
    c=config_for(name,root/a,token,ports[a]);c['journal'].update(transport='kafka',source_urls=sources,poll_interval_ms=100,request_timeout_ms=15000);c['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=a,transaction_timeout_ms=4000);c['control'].update(backend='disabled'if name=='dex-router'else'postgres',dsn_file=''if name=='dex-router'else str(infra.dsnfile),instance_id=a,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000);c['startup']['max_baseline_age_ms']=180000
    if a=='rust':c['chains'][0].update(genesis_hash='localnet',confirmation_tag='finalized',rpc_urls=[f'http://127.0.0.1:{s.server_port}'for s in servers],pools=[chain.fixture['pool_config']])
    elif a=='go':sample_for('evm:31337',root/a/'fixtures',chain.timestamp*1000)
    else:c['routing'].update(max_state_age_ms=180000,quote_ttl_ms=60000)
    configs[a]=save_config(root/a/'config',c)
   def base(a):return f'http://127.0.0.1:{ports[a]}'
   def start(a):logs[a]=(OUT/f'rewards-{family}-{a}.log').open('ab');processes[a]=subprocess.Popen(service_command(names[a],configs[a],root/a/'fixtures'if a=='go'else None),cwd=ROOT/names[a],stdout=logs[a],stderr=subprocess.STDOUT)
   def stop(a):
    p=processes.pop(a,None)
    if p:
     p.terminate()
     try:p.wait(timeout=10)
     except subprocess.TimeoutExpired:p.kill();p.wait()
    if a in logs:logs.pop(a).close()
   def status(a):return http(base(a)+'/internal/chains/solana:localnet/status',token)
   def owner():
    code,s=http(base('rust')+'/internal/chains/solana:localnet/baseline',token);require(code==200,(code,s));require(s['pools']==[chain.pool],'entire native reward pool changed in parse or wire');return s
   def synced():
    await_condition(lambda:status('rust')[1].get('snapshot_ref',{}).get('block',{}).get('number_or_slot')==chain.head,'bank not committed');ref=owner()['reference']
    for a in['router1','router2']:await_condition(lambda:status(a)[1].get('snapshot_ref')==ref,'bank not replicated')
   def quote(a,zero,expected):
    p=chain.pool;req={'chainKey':'solana:localnet','kind':'EXACT_IN','sellToken':p['token0'if zero else'token1'],'buyToken':p['token1'if zero else'token0'],'sellAmount':'100000','caller':p['token0'],'recipient':p['token1'],'slippageBps':50,'verification':'FAST'};code,v=http(base(a)+'/v1/quote',token,req);require(code==200,(code,v));require(v['grossBuyAmount']==expected['amount_out'] and v['consumedIn']=='100000'and v['refund']=='0','native amount mismatch');require(v['verification']['level']=='INDICATIVE'and v['snapshotRef']==owner()['reference'] and v['quality']['candidateScope']==[p['pool_id']],'stale or partial reward quote')
   try:
    for a in['go','rust']:start(a);wait(base(a)+'/health/ready')
    for a in['router1','router2']:start(a);wait(base(a)+'/health/live');wait_scope(base(a),token,'solana:localnet')
    synced();report['checks'].append(CHECKS[0])
    for zero in[True,False]:
     expected=chain.quote(zero)
     for a in['router1','router2']:quote(a,zero,expected)
    report['checks'].append(CHECKS[1]);step_digests=[]
    for zero in[True,False,True]:
     expected=chain.quote(zero)
     for a in['router1','router2']:quote(a,zero,expected)
     step_digests.append(hashlib.sha256(json.dumps(expected,sort_keys=True).encode()).hexdigest());chain.next(expected['next_pool']);synced()
    report['checks'].append(CHECKS[2]);old=owner()['reference']['online_generation'];chain.next(bad=True)
    for a in['router1','router2']:
     await_condition(lambda:status(a)[0]==503,'badreward remained ready');c,v=http(base(a)+'/internal/chains/evm:31337/status',token);require(c==200,'unrelated EVM revoked')
    chain.bad=False;await_condition(lambda:status('rust')[1].get('snapshot_ref',{}).get('online_generation',0)>old,'reward generation didnotrecover');synced();report['checks'].append(CHECKS[3]);stop('router2');start('router2');wait(base('router2')+'/health/live');wait_scope(base('router2'),token,'solana:localnet');synced();quote('router2',True,chain.quote(True));report['checks'].append(CHECKS[4]);require(report['checks']==CHECKS,'missingcheck');report.update(passed=True,steps=3,step_native_sha256=step_digests,account_batches=chain.reads)
   finally:
    for a in list(processes):stop(a)
 finally:
  for s in servers:s.shutdown();s.server_close()
  chain.oracle.close()
 return report
if __name__=='__main__':
 parser=argparse.ArgumentParser();parser.add_argument('--family',choices=['orca','raydium-clmm'],required=True);args=parser.parse_args();OUT.mkdir(parents=True,exist_ok=True);target=OUT/('rewards-'+args.family+'-three-service-smoke.json');target.write_text(json.dumps({'passed':False,'state':'RUNNING'})+'\n')
 try:r=run(args.family);r['measured_at_unix_ms']=int(time.time()*1000);target.write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r,indent=2))
 except BaseException as e:target.write_text(json.dumps({'passed':False,'error':str(e),'measured_at_unix_ms':int(time.time()*1000)})+'\n');raise
