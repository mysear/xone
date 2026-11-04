#!/usr/bin/env python3
"""Weighted fixed-block RPC -> normal Go parser -> PG/Kafka -> two replicas.
All RPC data is synthetic. The oracle executes original WeightedPool.onSwap;
this process test proves local parsing/transport/readiness, not a deployment.
"""
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
import copy,hashlib,json,secrets,subprocess,tempfile,threading,time
from infrastructure import ROOT,Infrastructure,port
from configuration import config_for,save_config,service_command,sample_for
from receipt_reference import EMPTY_ROOT,SINGLE_EMPTY_ROOT,SINGLE_EMPTY_FIELDS
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import http,wait_scope
OUT=ROOT/'integration/artifacts/local';E=10**18;GENESIS='0x'+'99'*32
NAMES=['weighted-18-18','weighted-18-6','weighted-6-18']
DECIMALS=[(18,18),(18,6),(6,18)]
def require(value,message):
 if not value:raise ValueError(message)
def load_golden():return json.loads((ROOT/'integration/test/contracts/weighted-quotes.json').read_text())
def address(n):return f'0x{n:040x}'
def word(n):return f'{n%(1<<256):064x}'
def code(n):return bytes([0x60,n,0x60,0,0x55])
def digest(n):return hashlib.sha256(code(n)).hexdigest()
def profile(name):
 i=NAMES.index(name);d=DECIMALS[i];g=load_golden();c=next(c for c in g['cases'] if c['pool']['fee_numerator']==10**15 and c['pool']['weighted']['scaling0']==str(10**(36-d[0])) and c['pool']['weighted']['scaling1']==str(10**(36-d[1])) and c['pool']['reserve0']==str(1000000*10**d[0]) and c['pool']['reserve1']==str(1000000*10**d[1]));p=copy.deepcopy(c['pool']);p.update(pool_id=address(10+i),token0=address(100+i*2),token1=address(101+i*2));s=p['weighted'];s.update(code_sha256=digest(1+i),vault=address(20+i),vault_code_sha256=digest(4+i),pool_id=p['pool_id']+'0002'+'0'*19+'1',token0_code_sha256=digest(7+i*2),token1_code_sha256=digest(8+i*2))
 cfg={'id':p['pool_id'],'protocol':p['protocol'],'token0':p['token0'],'token1':p['token1'],'fee_numerator':p['fee_numerator'],'fee_denominator':p['fee_denominator'],'program_id':'','accounts':[],'weighted':{'expected_code_sha256':s['code_sha256'],'expected_vault':s['vault'],'expected_vault_code_sha256':s['vault_code_sha256'],'pool_id':s['pool_id'],'expected_token0_code_sha256':s['token0_code_sha256'],'expected_token1_code_sha256':s['token1_code_sha256'],'token_policy':'standard-no-bpt'}}
 return p,cfg

def reference_case(index,zero):
 p,_=profile(NAMES[index]);amount=str(10**DECIMALS[index][0 if zero else 1])
 c=next(c for c in load_golden()['cases'] if c['pool']['reserve0']==p['reserve0'] and c['pool']['reserve1']==p['reserve1'] and c['pool']['weighted']['scaling0']==p['weighted']['scaling0'] and c['pool']['weighted']['scaling1']==p['weighted']['scaling1'] and c['pool']['fee_numerator']==p['fee_numerator'] and c['input']==amount and c['zero_for_one']==zero)
 return amount,c['expected']['amount_out']

class Chain:
 def __init__(self):
  self.head=100;self.timestamp=int(time.time());self.branch=0;self.fork_at=101;self.bad=None;self.pinned=0;self.receipts=0;self.catalog=[profile(n)[0] for n in NAMES];self.original=copy.deepcopy(self.catalog)
 def advance(self,bad=None):self.bad=bad;self.head+=1;self.timestamp=int(time.time())
 def block(self,n):
  def h(v):return '0x'+hashlib.sha256(f'weighted/{self.branch if v>=self.fork_at else 0}/{v}'.encode()).hexdigest()
  return {'number':hex(n),'hash':GENESIS if n==0 else h(n),'parentHash':h(max(0,n-1)),'timestamp':hex(self.timestamp),'receiptsRoot':SINGLE_EMPTY_ROOT if n else EMPTY_ROOT,'transactions':[] if n==0 else ['0x'+hashlib.sha256(('tx/'+h(n)).encode()).hexdigest()]}
 def provider(self):
  chain=self
  class Handler(BaseHTTPRequestHandler):
   def log_message(self,*_):pass
   def do_POST(self):
    q=json.loads(self.rfile.read(int(self.headers['Content-Length'])));m=q['method'];args=q['params'];out=None;error=None
    try:
     if m=='eth_chainId':out='0x7a69'
     elif m=='eth_getBlockByNumber':out=chain.block(chain.head if args[0] in ['latest','safe','finalized'] else int(args[0],16))
     elif m=='eth_getTransactionReceipt':
      b=next((chain.block(n) for n in range(1,chain.head+1) if args[0] in chain.block(n)['transactions']),None)
      if b:out={'blockHash':b['hash'],'transactionHash':args[0],'transactionIndex':'0x0','logs':[],**SINGLE_EMPTY_FIELDS};chain.receipts+=1
     else:
      require(args[1].get('requireCanonical') is True and args[1].get('blockHash') in {chain.block(n)['hash'] for n in range(chain.head+1)},'not fixed canonical block')
      chain.pinned+=1;target=args[0] if m=='eth_getCode' else args[0]['to'];idx=next(i for i,p in enumerate(chain.catalog) if target in [p['pool_id'],p['weighted']['vault'],p['token0'],p['token1']]);p=chain.catalog[idx];s=p['weighted'];bad=chain.bad if idx==0 else None;pad=lambda a:a[2:].rjust(64,'0')
      if m=='eth_getCode':
       n=(1+idx if target==p['pool_id'] else 4+idx if target==s['vault'] else 7+idx*2 if target==p['token0'] else 8+idx*2);out='0x'+code(n).hex()
       if bad=='token-code' and target==p['token0']:out='0x6001'
      elif m=='eth_call':
       data=args[0]['data'];sel=data[:10]
       if sel=='0x8d928af8':out='0x'+pad(s['vault'])
       elif sel=='0x38fff2d0':out=s['pool_id']
       elif sel=='0xf6c00927':out='0x'+pad(address(99) if bad=='registration' else p['pool_id'])+word(1 if bad=='specialization' else 2)
       elif sel=='0x1c0de051':out='0x'+word(int(bad=='pool-pause' and target==p['pool_id'] or bad=='vault-pause' and target==s['vault']))+word(0)+word(0)
       elif sel=='0xf89f27ed':out='0x'+word(32)+word(2)+word(int(s['weight0'])-int(bad=='weight'))+word(int(s['weight1']))
       elif sel=='0x1dd746ea':out='0x'+word(32)+word(2)+word(int(s['scaling0'])+int(bad=='scale'))+word(int(s['scaling1']))
       elif sel=='0x313ce567':out='0x'+word(19 if bad=='decimals' else DECIMALS[idx][0 if target==p['token0'] else 1])
       elif sel=='0xf94d4668':out='0x'+word(96)+word(192)+word(chain.head)+word(2)+pad(p['token0'])+pad(p['token1'])+word(2)+word(int(p['reserve0']))+word(int(p['reserve1']))
       elif sel=='0xb05f8e48':
        token='0x'+data[-40:];i=0 if token==p['token0'] else 1;out='0x'+word(int(p['reserve'+str(i)])-int(bad=='cash'))+word(int(bad=='managed'))+word(chain.head)+word(1 if bad=='manager' else 0)
       elif sel=='0x55c67628':out='0x'+word(int(p['fee_numerator']))
       else:raise ValueError('unexpected selector '+sel)
      else:raise ValueError('unexpected RPC '+m)
    except Exception as e:error={'code':-32000,'message':str(e)}
    body={'jsonrpc':'2.0','id':q['id']};body.update({'error':error} if error else {'result':out});raw=json.dumps(body).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(raw)));self.end_headers()
    try:self.wfile.write(raw)
    except (BrokenPipeError,ConnectionResetError):pass
  s=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=s.serve_forever,daemon=True).start();return s

def validate_quote(status,value,p,amount,expected,reference,ids):
 require(status==200,(status,value));require(value['grossBuyAmount']==expected and value['consumedIn']==amount and value['refund']=='0','amount mismatch')
 require(value['verification']['level']=='INDICATIVE','verification overstated')
 require(value['snapshotRef']==reference and value['onlineGeneration']==reference['online_generation'],'bank/generation mismatch')
 scope=value['quality']['candidateScope'];require(len(scope)==len(ids) and set(scope)==ids,'partial/duplicate pool scope')
 return value

PROCESS_CHECKS=['three-weighted-pools-mixed-decimals-both-directions-in-two-replicas','typed-weight-scale-vault-token-identity-cross-public-wire','sequential-shared-pool-both-direction-complete-postbalances-match-original-onSwap','reorg-republishes-whole-weighted-scope','memory-router-restart-restores-exact-full-owner-snapshot']+['unsupported-'+b+'-revokes-whole-evm-and-restores-new-generation' for b in ['weight','scale','decimals','manager','managed','cash','specialization','registration','pool-pause','vault-pause','token-code']]+['large-head-gap-invalid-weighted-dependency-revokes-old-scope-before-reanchor']

def run():
 chain=Chain();servers=[chain.provider(),chain.provider()];processes={};logs={};report={'passed':False,'production_ready':False,'protocol':'balancer-v2-weighted2-equal','trust':'SYNTHETIC_RPC_NOT_DEPLOYMENT_PROOF','checks':[]}
 try:
  with Infrastructure() as infra,tempfile.TemporaryDirectory(prefix='xone-weighted-') as folder:
   root=Path(folder);token=secrets.token_urlsafe(32);names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'};ports={a:port() for a in names};configs={};sources=[f'http://127.0.0.1:{ports[a]}' for a in ['go','rust']]
   chain.timestamp=int(time.time())
   for a,name in names.items():
    c=config_for(name,root/a,token,ports[a]);c['journal'].update(transport='kafka',source_urls=sources,poll_interval_ms=100,request_timeout_ms=15000);c['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=a,transaction_timeout_ms=4000);c['control'].update(backend='disabled' if name=='dex-router' else 'postgres',dsn_file='' if name=='dex-router' else str(infra.dsnfile),instance_id=a,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000);c['startup']['max_baseline_age_ms']=180000
    if a=='go':c['chains'][0].update(genesis_hash=GENESIS,confirmation_tag='latest',auto_reorg_window=4,rpc_urls=[f'http://127.0.0.1:{s.server_port}' for s in servers],pools=[profile(n)[1] for n in NAMES])
    elif a=='rust':sample_for('solana:localnet',root/a/'fixtures',chain.timestamp*1000)
    else:c['routing'].update(max_state_age_ms=180000,quote_ttl_ms=60000)
    configs[a]=save_config(root/a/'config',c)
   def base(a):return f'http://127.0.0.1:{ports[a]}'
   def start(a):
    logs[a]=(OUT/f'weighted-{a}.log').open('ab');processes[a]=subprocess.Popen(service_command(names[a],configs[a],root/a/'fixtures' if a=='rust' else None),cwd=ROOT/names[a],stdout=logs[a],stderr=subprocess.STDOUT)
   def stop(a):
    p=processes.pop(a,None)
    if p:
     p.terminate()
     try:p.wait(timeout=10)
     except subprocess.TimeoutExpired:p.kill();p.wait()
    if a in logs:logs.pop(a).close()
   def status(a):return http(base(a)+'/internal/chains/evm:31337/status',token)
   def owner():
    code,s=http(base('go')+'/internal/chains/evm:31337/baseline',token);require(code==200,(code,s));require(len(s['pools'])==3 and {p['pool_id'] for p in s['pools']}=={p['pool_id'] for p in chain.catalog},'owner scope');return s
   def check(a,index,zero=True,step=None):
    p=chain.catalog[index];amount,expected=reference_case(index,zero) if step is None else (step['input'],step['expected']['amount_out']);ref=owner()['reference']
    request={'chainKey':'evm:31337','kind':'EXACT_IN','sellToken':p['token0' if zero else 'token1'],'buyToken':p['token1' if zero else 'token0'],'sellAmount':amount,'caller':p['token0'],'recipient':p['token1'],'slippageBps':50,'verification':'FAST'}
    code,value=http(base(a)+'/v1/quote',token,request);return validate_quote(code,value,p,amount,expected,ref,{p['pool_id'] for p in chain.catalog})
   def synced():
    ref=owner()['reference']
    for a in ['router1','router2']:await_condition(lambda:status(a)[1].get('snapshot_ref')==ref,'replica bank not synchronized')
   def solana_healthy(a):
    c,v=http(base(a)+'/v1/quote',token,{'chainKey':'solana:localnet','kind':'EXACT_IN','sellToken':'A','buyToken':'B','sellAmount':'10003','caller':'A','recipient':'B','slippageBps':50,'verification':'FAST'})
    require(c==200 and v['verification']['level']=='INDICATIVE' and v['consumedIn']=='10003' and int(v['grossBuyAmount'])>0,'unaffected solana scope unhealthy')
   try:
    for a in ['go','rust']:start(a);wait(base(a)+'/health/ready')
    for a in ['router1','router2']:start(a);wait(base(a)+'/health/live');wait_scope(base(a),token,'evm:31337');wait_scope(base(a),token,'solana:localnet')
    synced()
    for a in ['router1','router2']:
     for i in range(3):check(a,i);check(a,i,False)
    report['checks'].append(PROCESS_CHECKS[0]);snap=owner()
    for actual,expected in zip(sorted(snap['pools'],key=lambda p:p['pool_id']),chain.catalog):require(actual==expected,'complete parsed weighted pool mismatch')
    report['checks'].append(PROCESS_CHECKS[1])
    sequence=next(s for s in load_golden()['sequences'] if s['pool']['weighted']['scaling0']==str(E) and s['pool']['weighted']['scaling1']==str(E) and s['pool']['fee_numerator']==10**15)
    for step in sequence['steps']:
     for a in ['router1','router2']:check(a,0,step['zero_for_one'],step)
     chain.catalog[0]['reserve0'],chain.catalog[0]['reserve1']=step['expected']['next_balances'];chain.advance();await_condition(lambda:status('go')[1].get('snapshot_ref',{}).get('block',{}).get('number_or_slot')==chain.head,'poststate block not committed');synced();actual=next(p for p in owner()['pools'] if p['pool_id']==chain.catalog[0]['pool_id']);require(actual==chain.catalog[0],'complete sequential raw poststate mismatch')
    report['checks'].append(PROCESS_CHECKS[2]);chain.catalog=copy.deepcopy(chain.original);chain.advance();await_condition(lambda:status('go')[1].get('snapshot_ref',{}).get('block',{}).get('number_or_slot')==chain.head,'reset block');synced();old=check('router1',0);chain.fork_at=chain.head;chain.branch=1
    await_condition(lambda:status('go')[1].get('snapshot_ref',{}).get('canonical_epoch',0)>old['snapshotRef']['canonical_epoch'],'reorg epoch');synced()
    for a in ['router1','router2']:check(a,0)
    report['checks'].append(PROCESS_CHECKS[3]);stop('router2');start('router2');wait(base('router2')+'/health/live');wait_scope(base('router2'),token,'evm:31337');synced();check('router2',0);report['checks'].append(PROCESS_CHECKS[4])
    for fault in ['weight','scale','decimals','manager','managed','cash','specialization','registration','pool-pause','vault-pause','token-code']:
     old=check('router1',0);chain.advance(fault)
     for a in ['router1','router2']:await_condition(lambda:status(a)[0]==503,'bad dependency scope remained ready');solana_healthy(a)
     chain.bad=None
     await_condition(lambda:status('go')[1].get('snapshot_ref',{}).get('online_generation',0)>old['onlineGeneration'],'qualified recovery generation');synced()
     for a in ['router1','router2']:check(a,0)
     report['checks'].append('unsupported-'+fault+'-revokes-whole-evm-and-restores-new-generation')
    old=check('router1',0);chain.head+=200;chain.bad='manager';chain.timestamp=int(time.time())
    for a in ['router1','router2']:await_condition(lambda:status(a)[0]==503,'large-gap failed reanchor left remote scope ready');solana_healthy(a)
    chain.bad=None
    await_condition(lambda:status('go')[1].get('snapshot_ref',{}).get('online_generation',0)>old['onlineGeneration'],'large-gap qualified generation');synced()
    for a in ['router1','router2']:check(a,0)
    report['checks'].append(PROCESS_CHECKS[-1])
    require(report['checks']==PROCESS_CHECKS,'missing process coverage');report.update(passed=True,pinned_rpc_reads=chain.pinned,receipt_reads=chain.receipts,owner_reference=owner()['reference'],pool_count=3)
   finally:
    for a in list(processes):stop(a)
 finally:
  for server in servers:server.shutdown();server.server_close()
 return report

if __name__=='__main__':
 OUT.mkdir(parents=True,exist_ok=True);target=OUT/'weighted-three-service-smoke.json';target.write_text(json.dumps({'passed':False,'state':'RUNNING'})+'\n');started=int(time.time()*1000)
 try:
  from weighted_evidence import snapshot_inputs,runtime_fingerprint,source_hash,write_result
  before=snapshot_inputs();runtime=runtime_fingerprint();r=run();require(snapshot_inputs()==before and runtime_fingerprint()==runtime,'inputs/runtime changed during process execution');r.update(started_at_unix_ms=started,measured_at_unix_ms=int(time.time()*1000),input_hashes=before,runtime=runtime);write_result(target,r)
 except BaseException as e:
  target.write_text(json.dumps({'passed':False,'error':str(e),'error_type':type(e).__name__,'measured_at_unix_ms':int(time.time()*1000)})+'\n');raise
 print(json.dumps(r,indent=2))
