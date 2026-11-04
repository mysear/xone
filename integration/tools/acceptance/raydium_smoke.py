#!/usr/bin/env python3
"""Actual Rust RPC decoder, synthetic finalized accounts, real PG/Kafka and routers."""
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
import json,hashlib,copy,secrets,subprocess,tempfile,time,threading,base64,signal
from infrastructure import ROOT,Infrastructure,port
from configuration import config_for,save_config,service_command,sample_for
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import http,quote,wait_scope
OUT=ROOT/'integration/artifacts/local';BASE=json.loads((ROOT/'integration/test/solana/raydium-base.json').read_text());GOLDEN=json.loads((ROOT/'integration/test/contracts/raydium-quotes.json').read_text());ALPHABET='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
PROGRAM=BASE['pool']['raydium_cpmm']['program_id'];TOKEN=BASE['metadata']['token_program'];CLOCK=BASE['metadata']['clock'];LOADER=BASE['metadata']['upgradeable_loader'];LOADER2=BASE['metadata']['immutable_loader'];SYSVAR=BASE['metadata']['clock_owner']
def b58decode(v):
 n=0
 for c in v:n=n*58+ALPHABET.index(c)
 return b'\0'*(len(v)-len(v.lstrip('1')))+n.to_bytes((n.bit_length()+7)//8,'big')
def b58encode(b):
 n=int.from_bytes(b,'big');s=''
 while n:s=ALPHABET[n%58]+s;n//=58
 return '1'*(len(b)-len(b.lstrip(b'\0')))+s
def integer(b,at,n,size=8):b[at:at+size]=n.to_bytes(size,'little')
def key(b,at,k):b[at:at+32]=b58decode(k)
def account(owner,data,executable=False):return {'owner':owner,'data':[base64.b64encode(data).decode(),'base64'],'executable':executable,'lamports':10**15,'rentEpoch':0}

class Chain:
 def __init__(self):self.head=123456;self.timestamp=int(time.time());self.mode=0;self.enabled=True;self.bad=None;self.branch=0;self.reads=0;self.block_reads=0
 def pool(self):
  return copy.deepcopy(next(p for p in GOLDEN['base_pools'] if p['raydium_cpmm']['creator_fee_on']==self.mode and p['raydium_cpmm']['creator_fee_enabled']==self.enabled))
 def accounts(self):
  p=self.pool();s=p['raydium_cpmm'];m=BASE['metadata'];out={};state=bytearray(637);state[:8]=bytes([247,237,227,245,215,195,222,70]);cfg=bytearray(236);cfg[:8]=bytes([218,244,33,104,203,203,43,111]);cfg[8]=m['config_bump'];integer(cfg,10,m['config_index'],2)
  for at,k in [(8,s['amm_config']),(40,m['pool_creator']),(72,s['vault0']),(104,s['vault1']),(136,m['lp_mint']),(168,p['token0']),(200,p['token1']),(232,TOKEN),(264,TOKEN),(296,m['observation'])]:key(state,at,k)
  state[328]=s['authority_bump'];state[329]=4 if self.bad=='pool-pause' else 0;state[330]=9;state[331]=s['mint0_decimals'];state[332]=s['mint1_decimals'];state[389]=3 if self.bad=='creator-mode' else self.mode;state[390]=int(self.enabled)
  for at,n in [(333,1000000000),(341,s['protocol_fees0']),(349,s['protocol_fees1']),(357,s['fund_fees0']),(365,s['fund_fees1']),(373,self.timestamp+1 if self.bad=='not-open' else s['open_time']),(381,s['recent_epoch']),(397,s['creator_fees0']),(405,s['creator_fees1'])]:integer(state,at,n)
  if self.bad=='creator-ledger':integer(state,397,s['raw_vault0']+1)
  if self.bad=='padding':state[413]=1
  for at,n in [(12,p['fee_numerator']),(20,s['protocol_fee_rate']),(28,s['fund_fee_rate']),(108,s['creator_fee_rate']),(116,s['creator_fee_share_rate'])]:integer(cfg,at,n)
  key(cfg,44,m['pool_creator']);key(cfg,76,m['pool_creator']);out[p['pool_id']]=account(PROGRAM,state);out[s['amm_config']]=account(PROGRAM,cfg)
  for i in [0,1]:
   vault=bytearray(165);mint=bytearray(82);key(vault,0,p[f'token{i}']);key(vault,32,p[f'token{i}'] if self.bad=='vault-authority' and i==0 else s['authority']);integer(vault,64,s[f'raw_vault{i}']);vault[108]=1;mint[44]=s[f'mint{i}_decimals'];mint[45]=1;out[s[f'vault{i}']]=account(TOKEN,vault);out[p[f'token{i}']]=account(TOKEN,mint)
  program=bytearray(36);integer(program,0,2,4);key(program,4,s['programdata']);out[PROGRAM]=account(LOADER,program,True)
  pd=bytearray(45);integer(pd,0,3,4);integer(pd,4,s['deployed_slot']+(1 if self.bad=='program-slot' else 0));pd[12]=1;key(pd,13,m['pool_creator']);pd.extend(m['program_code_utf8'].encode());
  if self.bad=='program-code':pd[-1]^=1
  out[s['programdata']]=account(LOADER,pd);token_code=m['token_program_code_utf8'].encode();out[TOKEN]=account(LOADER2,token_code+(b'x' if self.bad=='token-code' else b''),True)
  clock=bytearray(40)
  for at,n in [(0,self.head+(1 if self.bad=='clock-slot' else 0)),(8,self.timestamp-100),(16,s['bank_epoch']),(24,s['bank_epoch']+1),(32,self.timestamp)]:integer(clock,at,n)
  out[CLOCK]=account(SYSVAR,clock)
  if self.bad=='missing-account':out[s['vault0']]=None
  return out
 def block(self,n):
  def h(x):return b58encode(hashlib.sha256(f'{self.branch if x>=123457 else 0}/{x}'.encode()).digest())
  return {'blockhash':h(n),'previousBlockhash':h(n-1),'parentSlot':n-1,'blockTime':self.timestamp,'blockHeight':n}
 def provider(self):
  chain=self
  class Handler(BaseHTTPRequestHandler):
   def log_message(self,*args):pass
   def do_POST(self):
    q=json.loads(self.rfile.read(int(self.headers['Content-Length'])));method=q['method'];params=q['params'];error=None;result=None
    if method=='getGenesisHash':result='localnet'
    elif method=='getMultipleAccounts':
     assert len(params[0])==10 and set(params[0])==set(chain.accounts());assert params[1]['commitment']=='finalized' and params[1]['encoding']=='base64';assert 'minContextSlot' not in params[1];chain.reads+=1;acc=chain.accounts();result={'context':{'slot':chain.head},'value':[acc[k] for k in params[0]]}
    elif method=='getBlock':assert params[1]['commitment']=='finalized';result=chain.block(params[0]);chain.block_reads+=1
    else:error={'code':-32601,'message':'unsupported '+method}
    body={'jsonrpc':'2.0','id':q['id']};body.update({'error':error} if error else {'result':result});raw=json.dumps(body).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
  server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start();return server

def expected(chain,amount,zero):
 p=chain.pool()
 return next(c['expected']['amount_out'] for c in GOLDEN['cases'] if c['pool']==p and c['input']==amount and c['zero_for_one']==zero)

def run():
 report={'profile':'raydium-creator-classic-spl-normal-rust-rpc-real-pg-kafka','production_ready':False,'checks':[]};chain=Chain();servers=[chain.provider(),chain.provider()];processes={};logs={}
 try:
  with Infrastructure() as infra,tempfile.TemporaryDirectory(prefix='xone-raydium-') as folder:
   chain.timestamp=int(time.time());root=Path(folder);token=secrets.token_urlsafe(32);names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'};ports={a:port() for a in names};configs={};sources=[f'http://127.0.0.1:{ports[a]}' for a in ['go','rust']]
   for a,name in names.items():
    c=config_for(name,root/a,token,ports[a]);c['journal'].update(transport='kafka',source_urls=sources,poll_interval_ms=100,request_timeout_ms=15000);c['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=a,transaction_timeout_ms=4000);c['control'].update(backend='disabled' if name=='dex-router' else 'postgres',dsn_file='' if name=='dex-router' else str(infra.dsnfile),instance_id=a,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000);c['startup']['max_baseline_age_ms']=180000
    if a=='rust':c['chains'][0].update(genesis_hash='localnet',confirmation_tag='finalized',rpc_urls=[f'http://127.0.0.1:{s.server_port}' for s in servers],pools=[copy.deepcopy(BASE['pool_config'])])
    elif a=='go':sample_for('evm:31337',root/a/'fixtures',chain.timestamp*1000)
    else:c['routing'].update(max_state_age_ms=180000,quote_ttl_ms=60000)
    configs[a]=save_config(root/a/'config',c)
   rust_config=json.loads(configs['rust'].read_text());lease_recovery_timeout=(rust_config['control']['lease_ttl_ms']+rust_config['journal']['request_timeout_ms'])/1000+10
   report['lease_recovery_timeout_seconds']=lease_recovery_timeout
   def base(a):return f'http://127.0.0.1:{ports[a]}'
   def start(a):
    name=names[a];logs[a]=(OUT/f'raydium-{a}.log').open('ab');processes[a]=subprocess.Popen(service_command(name,configs[a],root/a/'fixtures' if a=='go' else None),cwd=ROOT/name,stdout=logs[a],stderr=subprocess.STDOUT)
   def stop(a):
    p=processes.pop(a,None)
    if p:
     p.terminate()
     try:p.wait(timeout=10)
     except subprocess.TimeoutExpired:p.kill();p.wait()
    if a in logs:logs.pop(a).close()
   def status(a):return http(base(a)+'/internal/chains/solana:localnet/status',token)
   def check(a,zero=True,amount='1000003'):
    p=chain.pool();request={'chainKey':'solana:localnet','kind':'EXACT_IN','sellToken':p['token0' if zero else 'token1'],'buyToken':p['token1' if zero else 'token0'],'sellAmount':amount,'caller':p['token0'],'recipient':p['token1'],'slippageBps':50,'verification':'FAST'}
    code,q=http(base(a)+'/v1/quote',token,request);assert code==200,(code,q);assert q['grossBuyAmount']==expected(chain,amount,zero),(q,expected(chain,amount,zero));assert q['consumedIn']==amount and q['refund']=='0' and q['verification']['level']=='INDICATIVE';return q
   def current(a):return status(a)[1].get('snapshot_ref',{}).get('block',{}).get('number_or_slot')==chain.head
   try:
    for a in ['go','rust']:start(a);wait(base(a)+'/health/ready')
    for a in ['router1','router2']:start(a);wait(base(a)+'/health/live');wait_scope(base(a),token,'solana:localnet')
    for a in ['router1','router2']:
     for amount in ['1000003','999']:check(a,True,amount);check(a,False,amount)
    report['checks'].append('normal-rust-rpc-ten-account-finalized-program-clock-and-both-direction-amounts-in-two-replicas')
    snap=http(base('rust')+'/internal/chains/solana:localnet/baseline',token)[1];p=snap['pools'][0];s=p['raydium_cpmm'];assert p['reserve0']==chain.pool()['reserve0'] and s['creator_fees0']==307 and s['bank_slot']==chain.head and s['program_code_sha256']==BASE['metadata']['program_code_sha256'];assert not p.get('v2');report['checks'].append('raw-vault-creator-protocol-fund-ledgers-program-pin-and-bank-cross-protobuf')
    for mode,enabled in [(1,True),(2,True),(0,False),(1,False),(2,False)]:
     chain.mode=mode;chain.enabled=enabled;chain.head+=1
     for a in ['router1','router2']:await_condition(lambda:current(a),'creator mode bank not published');check(a);check(a,False)
    report['checks'].append('all-three-creator-modes-enabled-and-disabled-follow-original-program-both-directions')
    old={a:check(a) for a in ['router1','router2']};logpath=OUT/'raydium-rust.log';before=logpath.read_text().count('scope_revocation_failed')
    infra.kafka.send_signal(signal.SIGSTOP)
    try:
     chain.bad='program-code';chain.head+=1
     await_condition(lambda:http(base('rust')+'/internal/chains/solana:localnet/status',token)[0]==503,'local scope stayed ready while Kafka publication was blocked')
     await_condition(lambda:logpath.read_text().count('scope_revocation_failed')>before,'blocked Kafka did not exercise prepared revocation publication failure')
     process=processes.pop('rust');process.kill();process.wait(timeout=10);logs.pop('rust').close()
    finally:infra.kafka.send_signal(signal.SIGCONT)
    restart_offset=logpath.stat().st_size
    start('rust')
    def restart_log():return logpath.read_bytes()[restart_offset:].decode('utf-8')
    def invalid_source_start_is_closed():
     code=processes['rust'].poll()
     if code is not None:
      events=[json.loads(line)for line in restart_log().splitlines()if line.startswith('{')]
      assert code==1 and any(e.get('event')=='service_start_failed'and e.get('fields',{}).get('error_code')=='PROGRAM_VERSION_UNSUPPORTED'for e in events),(code,events)
      assert any(e.get('event')=='scope_revocation_reconciled'for e in events),'fatal startup did not reconcile prepared revocation first'
      return True
     return http(base('rust')+'/health/live')[0]==200 and http(base('rust')+'/internal/chains/solana:localnet/status',token)[0]==503
    await_condition(invalid_source_start_is_closed,'invalid source restart was neither rejected nor live with revoked scope',timeout=40)
    report['invalid_source_restart_outcome']='REJECTED_PROGRAM_VERSION'if processes['rust'].poll()is not None else 'LIVE_SCOPE_REVOKED'
    for a in old:await_condition(lambda:status(a)[0]==503,'pending revocation was not reconciled after restart with invalid source',timeout=60);quote(base(a),token,'evm:31337','B')
    await_condition(lambda:'scope_revocation_reconciled'in restart_log(),'prepared revocation did not reach the durable journal after restart',timeout=60)
    chain.bad=None
    if processes['rust'].poll()is not None:
     assert invalid_source_start_is_closed()
     stop('rust');start('rust');wait(base('rust')+'/health/live')
    for a in old:await_condition(lambda:status(a)[1].get('snapshot_ref',{}).get('online_generation',0)>old[a]['onlineGeneration'],'qualified source did not restore after durable intent recovery',timeout=lease_recovery_timeout);check(a)
    report['checks'].append('blocked-kafka-revocation-immediately-hides-local-state-and-reconciles-after-parser-crash-with-source-still-invalid')
    for fault in ['program-code','program-slot','token-code','clock-slot','pool-pause','not-open','creator-mode','creator-ledger','vault-authority','padding','missing-account']:
     old={a:check(a) for a in ['router1','router2']};chain.bad=fault;chain.head+=1
     for a in old:await_condition(lambda:status(a)[0]==503,'invalid Raydium dependency not revoked');quote(base(a),token,'evm:31337','B')
     chain.bad=None
     for a in old:await_condition(lambda:status(a)[1].get('snapshot_ref',{}).get('online_generation',0)>old[a]['onlineGeneration'],'Raydium qualified scope not restored');check(a)
     report['checks'].append('unsupported-'+fault+'-revokes-solana-only-and-qualified-baseline-restores')
    stop('router1');start('router1');wait(base('router1')+'/health/live');wait_scope(base('router1'),token,'solana:localnet');check('router1');check('router1',False);report['checks'].append('memory-only-router-restores-typed-raydium-state-from-owner-checkpoint-tail')
    report['passed']=True;report['atomic_account_batches']=chain.reads;report['finalized_block_reads']=chain.block_reads
   finally:
    for a in list(processes):stop(a)
 finally:
  for s in servers:s.shutdown();s.server_close()
 report.update(measured_at_unix_ms=int(time.time()*1000),tool_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest());return report
if __name__=='__main__':
 OUT.mkdir(parents=True,exist_ok=True)
 try:r=run()
 except Exception as e:(OUT/'raydium-three-service-smoke.json').write_text(json.dumps({'passed':False,'production_ready':False,'error':str(e),'error_type':type(e).__name__},indent=2)+'\n');raise
 (OUT/'raydium-three-service-smoke.json').write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r,indent=2))
