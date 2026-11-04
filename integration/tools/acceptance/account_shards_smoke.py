#!/usr/bin/env python3
"""Same-context current bootstrap; synthetic RPC, real PG/Kafka/two routers."""
import base64
import copy
import hashlib
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import secrets
import subprocess
import threading
import time

ROOT=Path(__file__).resolve().parents[3]
OUT=ROOT/'integration/artifacts/local'
CLOCK='SysvarC1ock11111111111111111111111111111111'
BUDGET=1_000_000
CHECKS=[
 'sixteen-pools-134-accounts-in-two-shards-and-both-quote-directions-in-two-replicas',
 'new-current-bank-publishes-complete-scope-with-provider-context-source-label',
 'slot-mismatch-revokes-only-solana-and-restores-whole-new-generation',
 'clock-change-revokes-only-solana-and-restores-whole-new-generation',
 'short-response-revokes-only-solana-and-restores-whole-new-generation',
 'missing-dependency-revokes-only-solana-and-restores-whole-new-generation',
 'anchor-change-revokes-only-solana-and-restores-whole-new-generation',
 'aggregate-response-budget-revokes-only-solana-and-restores-whole-new-generation',
 'memory-only-router-restores-full-scope-from-owner-checkpoint',
 'parser-restart-verifies-wal-and-new-session-before-fresh-whole-scope',
]

def require(condition,message):
 if not condition:raise AssertionError(message)

def b58(data):
 alphabet='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';n=int.from_bytes(data,'big');result=''
 while n:result=alphabet[n%58]+result;n//=58
 return '1'*(len(data)-len(data.lstrip(b'\0')))+result

def expected(amount,zero):
 # Independent integer constant-liquidity/no-initialized-tick reference.
 liquidity=1_000_000;q=1<<64;net=amount-(amount*3000+999999)//1_000_000
 if zero:
  after=(liquidity*q+liquidity+net-1)//(liquidity+net)
  return liquidity*(q-after)//q
 after=q+net*q//liquidity
 return liquidity*(after-q)*q//(after*q)

def validate_quote_response(code,value,zero,expected_ids,expected_reference=None):
 require(expected_reference is not None,'expected owner reference required')
 require(code==200,(code,value));require(value['grossBuyAmount']==str(expected(10000,zero)),value)
 require(value['verification']['level']=='INDICATIVE'and value['quality']['source']=='RPC_SAME_FINALIZED_CONTEXT_SHARDS',value)
 require(value['consumedIn']=='10000'and value['refund']=='0',value)
 require(value['snapshotRef']==expected_reference and value['onlineGeneration']==expected_reference['online_generation'],'stale quote bank or generation')
 require(len(value['quality']['candidateScope'])==len(expected_ids) and set(value['quality']['candidateScope'])==expected_ids,'partial or duplicate replica pool scope')
 return value

def validate_evm_response(code,value):
 require(code==200,(code,value))
 require(value['chainKey']=='evm:31337'and value['verification']['level']=='INDICATIVE'and value['consumedIn']=='10003'and int(value['grossBuyAmount'])>0,'EVM quote scope unhealthy')
 return value

class Chain:
 def __init__(self,fixture):
  self.fixture=fixture;self.head=100;self.timestamp=int(time.time());self.bad=None
  self.lock=threading.RLock();self.responses=[];self.reads=0
  self.first=tuple(sorted(k for k in fixture['accounts'] if k!=CLOCK)[:99])
 def advance(self,bad=None):
  with self.lock:self.head+=1;self.timestamp=int(time.time());self.bad=bad
 def provider(self):
  chain=self;blocks=0
  class Handler(BaseHTTPRequestHandler):
   def log_message(self,*_):pass
   def do_POST(self):
    nonlocal blocks
    request=json.loads(self.rfile.read(int(self.headers['Content-Length'])));method=request['method'];params=request['params']
    with chain.lock:
     if method=='getGenesisHash':result='localnet'
     elif method=='getMultipleAccounts':
      keys=params[0];require(0<len(keys)<=100 and keys[-1]==CLOCK,'invalid shard request')
      require(params[1]=={'encoding':'base64','commitment':'finalized'},'request must not claim minContextSlot pins history')
      first=tuple(keys[:-1])==chain.first;values=[]
      for key in keys:
       value=copy.deepcopy(chain.fixture['accounts'][key])
       if key==CLOCK:
        data=bytearray(base64.b64decode(value['data'][0]));data[0:8]=chain.head.to_bytes(8,'little');data[32:40]=chain.timestamp.to_bytes(8,'little',signed=True)
        if not first and chain.bad=='clock':data[16]^=1
        value['data'][0]=base64.b64encode(data).decode()
       values.append(value)
      if not first and chain.bad=='short':values.pop()
      if not first and chain.bad=='missing':
       index=next(i for i,v in enumerate(values[:-1]) if v['data'][0]);values[index]=None
      slot=chain.head+(1 if not first and chain.bad=='slot'else 0)
      result={'context':{'slot':slot},'value':values}
      if chain.bad=='budget':result['padding']='p'*300_000
      chain.reads+=1
     elif method=='getBlock':
      blocks+=1;slot=params[0]
      result={'blockTime':chain.timestamp,'blockhash':b58(hashlib.sha256(f'bank/{slot}'.encode()).digest()),'previousBlockhash':b58(hashlib.sha256(f'bank/{slot-1}'.encode()).digest()),'parentSlot':slot-1}
      if chain.bad=='anchor' and blocks%2==0:result['blockhash']=b58(hashlib.sha256(b'wrong-bank').digest())
     else:raise ValueError('unexpected RPC '+method)
     raw=json.dumps({'jsonrpc':'2.0','id':request['id'],'result':result}).encode()
     if method=='getMultipleAccounts':chain.responses.append((chain.bad,first,len(raw)))
    self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(raw)));self.end_headers()
    try:self.wfile.write(raw)
    except (BrokenPipeError,ConnectionResetError):pass
  server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start();return server

def run(fixture):
 from infrastructure import Infrastructure,port
 from configuration import config_for,save_config,service_command,sample_for
 from kafka_smoke import wait
 from reorg_smoke import await_condition
 from smoke import http,wait_scope
 report={'passed':False,'production_ready':False,'checks':[]};chain=Chain(fixture)
 servers=[chain.provider(),chain.provider()];processes={};logs={}
 try:
  with Infrastructure() as infra:
   import tempfile
   with tempfile.TemporaryDirectory(prefix='xone-account-shards-') as folder:
    root=Path(folder);token=secrets.token_urlsafe(32);names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'};ports={a:port()for a in names};configs={}
    chain.timestamp=int(time.time());sources=[f'http://127.0.0.1:{ports[a]}'for a in ('go','rust')]
    for alias,name in names.items():
     cfg=config_for(name,root/alias,token,ports[alias]);cfg['journal'].update(transport='kafka',source_urls=sources,poll_interval_ms=250,request_timeout_ms=15000,max_payload_bytes=BUDGET)
     cfg['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=alias,transaction_timeout_ms=4000)
     cfg['control'].update(backend='disabled'if name=='dex-router'else'postgres',dsn_file=''if name=='dex-router'else str(infra.dsnfile),instance_id=alias,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000);cfg['startup']['max_baseline_age_ms']=180000
     if alias=='rust':cfg['chains'][0].update(genesis_hash='localnet',confirmation_tag='finalized',rpc_urls=[f'http://127.0.0.1:{s.server_port}'for s in servers],pools=fixture['chain']['pools'])
     elif alias=='go':sample_for('evm:31337',root/alias/'fixtures',chain.timestamp*1000)
     else:cfg['routing'].update(max_state_age_ms=180000,quote_ttl_ms=60000,max_splits=1,max_hops=1)
     configs[alias]=save_config(root/alias/'config',cfg)
    def base(alias):return f'http://127.0.0.1:{ports[alias]}'
    def start(alias):
     logs[alias]=(OUT/f'account-shards-{alias}.log').open('ab');name=names[alias]
     processes[alias]=subprocess.Popen(service_command(name,configs[alias],root/alias/'fixtures'if alias=='go'else None),cwd=ROOT/name,stdout=logs[alias],stderr=subprocess.STDOUT)
    def stop(alias):
     process=processes.pop(alias,None)
     if process:
      process.terminate()
      try:process.wait(timeout=10)
      except subprocess.TimeoutExpired:process.kill();process.wait()
     if alias in logs:logs.pop(alias).close()
    def status(alias):return http(base(alias)+'/internal/chains/solana:localnet/status',token)
    def check(alias,zero=True):
     pool=fixture['chain']['pools'][0]
     body={'chainKey':'solana:localnet','kind':'EXACT_IN','sellToken':pool['token0'if zero else'token1'],'buyToken':pool['token1'if zero else'token0'],'sellAmount':'10000','caller':pool['token0'],'recipient':pool['token1'],'slippageBps':50,'verification':'FAST'}
     owner_code,owner=http(base('rust')+'/internal/chains/solana:localnet/baseline',token);require(owner_code==200,'owner unavailable for quote-bank assertion')
     code,value=http(base(alias)+'/v1/quote',token,body)
     return validate_quote_response(code,value,zero,{p['id']for p in fixture['chain']['pools']},owner['reference'])
    def check_evm(alias):
     body={'chainKey':'evm:31337','kind':'EXACT_IN','sellToken':'A','buyToken':'B','sellAmount':'10003','caller':'0x0000000000000000000000000000000000000001','recipient':'0x0000000000000000000000000000000000000002','slippageBps':50,'verification':'FAST'}
     code,value=http(base(alias)+'/v1/quote',token,body);return validate_evm_response(code,value)
    def evidence():
     value={}
     def fetch():
      code,body=http(base('rust')+'/internal/chains/solana:localnet/source-evidence',token)
      if code==200:value.update(body)
      return code==200
     await_condition(fetch,'source evidence status remained busy');return value
    def ready():
     wait_scope(base('rust'),token,'solana:localnet')
     await_condition(lambda:http(base('rust')+'/internal/chains/solana:localnet/status',token)[1].get('snapshot_ref',{}).get('block',{}).get('number_or_slot')==chain.head,'owner current bank not applied')
     owner_code,owner=http(base('rust')+'/internal/chains/solana:localnet/baseline',token)
     require(owner_code==200,'owner baseline not ready')
     expected_ids={p['id']for p in fixture['chain']['pools']}
     require(len(owner['pools'])==16 and {p['pool_id']for p in owner['pools']}==expected_ids,'partial owner pool scope')
     require(all(p['orca_whirlpool']['bank_slot']==chain.head for p in owner['pools']),'mixed-bank owner pools')
     for alias in ('router1','router2'):
      wait_scope(base(alias),token,'solana:localnet');await_condition(lambda:status(alias)[1].get('snapshot_ref')==owner['reference'],'new complete owner snapshot not applied')
      code,state=status(alias);require(code==200 and state['ready']and state['scope_pool_count']==16,'partial replica scope')
      require(state['snapshot_ref']['scope_hash']==owner['reference']['scope_hash'],'replica scope hash differs from complete owner')
      for zero in (True,False):require(check(alias,zero)['scopeHash']==owner['reference']['scope_hash'],'quote scope hash differs')
    try:
     for alias in ('go','rust'):start(alias);wait(base(alias)+'/health/ready')
     for alias in ('router1','router2'):start(alias);wait(base(alias)+'/health/live')
     ready()
     baseline=http(base('rust')+'/internal/chains/solana:localnet/baseline',token)[1]
     require(len(baseline['pools'])==16 and len(fixture['accounts'])==134,'fixture/snapshot scope differs')
     report['checks'].append(CHECKS[0]);chain.advance();ready();report['checks'].append(CHECKS[1])
     for index,(fault,error) in enumerate([('slot','ACCOUNT_SHARD_CONTEXT_MISMATCH'),('clock','ACCOUNT_SHARD_CLOCK_CHANGED'),('short','ACCOUNT_BATCH_INCOMPLETE'),('missing','ACCOUNT_MISSING'),('anchor','ACCOUNT_BANK_ANCHOR_CHANGED'),('budget','ACCOUNT_SHARD_RESPONSE_BUDGET')],2):
      previous={a:check(a)for a in ('router1','router2')};log=OUT/'account-shards-rust.log';offset=log.stat().st_size;chain.advance(fault)
      await_condition(lambda:error in log.read_bytes()[offset:].decode(),'expected shard rejection absent: '+error,timeout=35)
      for alias,old in previous.items():
       await_condition(lambda:status(alias)[0]==503,'partial account scope remained ready')
       check_evm(alias)
      chain.advance();ready()
      for alias,old in previous.items():
       fresh=check(alias);require(fresh['onlineGeneration']>old['onlineGeneration'],'generation not renewed')
       code,value=http(base(alias)+f'/v1/quotes/{old["quoteId"]}/build',token,{},'POST');require(code==409 and value['error']=='SNAPSHOT_REORGED',value)
      report['checks'].append(CHECKS[index])
     budget=[size for fault,_,size in chain.responses if fault=='budget'];require(budget and max(budget)<=BUDGET and sum(budget[:2])>BUDGET,'did not exercise aggregate-only budget rejection')
     old=check('router1');stop('router1');start('router1');wait(base('router1')+'/health/live');ready();require(check('router1')['onlineGeneration']==old['onlineGeneration'],'router restart changed owner generation');report['checks'].append(CHECKS[8])
     before=evidence();stop('rust');start('rust');wait(base('rust')+'/health/ready');ready()
     after=evidence()
     require(after['session_id']>before['session_id']and after['records']>before['records'],'fresh evidence not captured on reopen');report['checks'].append(CHECKS[9])
     report.update(passed=True,pool_count=16,account_count=134,max_account_response_bytes=BUDGET,source_evidence_status=after,account_reads=chain.reads,header_trust='PROVIDER_DECLARED_FINALIZED_CONTEXT_NOT_INDEPENDENT_BANK_PROOF')
    finally:
     for alias in list(processes):stop(alias)
 finally:
  for server in servers:server.shutdown();server.server_close()
 return report

def write_result(path,value):
 temporary=path.with_name(path.name+'.'+secrets.token_hex(6)+'.tmp')
 try:temporary.write_text(json.dumps(value,indent=2)+'\n');temporary.replace(path)
 finally:temporary.unlink(missing_ok=True)

def main():
 OUT.mkdir(parents=True,exist_ok=True);output=OUT/'account-shards-three-service-smoke.json'
 pending={'passed':False,'production_ready':False,'status':'RUNNING','attempt_id':secrets.token_hex(12),'measured_at_unix_ms':int(time.time()*1000)}
 try:
  write_result(output,pending)
  from account_shards_evidence import snapshot_inputs,runtime_fingerprint,current_source_hash
  inputs=snapshot_inputs(ROOT);runtime=runtime_fingerprint(ROOT,current_source_hash())
  fixture=json.loads((ROOT/'integration/test/solana/account-shards.json').read_text())
  report=run(fixture);require(report['checks']==CHECKS,'process coverage changed');require(snapshot_inputs(ROOT)==inputs,'process inputs changed')
  require(runtime_fingerprint(ROOT,current_source_hash())==runtime,'service sources/binaries changed during execution')
  report.update(input_hashes=inputs,runtime=runtime,started_at_unix_ms=pending['measured_at_unix_ms'],attempt_id=pending['attempt_id'],measured_at_unix_ms=int(time.time()*1000))
  write_result(output,report);print(json.dumps(report,indent=2))
 except BaseException as exc:
  write_result(output,{**pending,'status':'FAILED','error_type':type(exc).__name__,'error':str(exc),'measured_at_unix_ms':int(time.time()*1000)});raise

if __name__=='__main__':main()
