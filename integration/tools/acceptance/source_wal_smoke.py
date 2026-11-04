#!/usr/bin/env python3
"""Durable raw RPC evidence, capacity failure, fresh recovery and PG/Kafka replicas.
Synthetic finalized accounts test reception semantics, not a Yellowstone stream.
"""
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
import json,hashlib,copy,secrets,subprocess,tempfile,time,threading,base64
from infrastructure import ROOT,Infrastructure,port
from configuration import config_for,save_config,service_command,sample_for
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import http,quote,wait_scope
OUT=ROOT/'integration/artifacts/local';BASE=json.loads((ROOT/'integration/test/solana/rayclmm-dynamic-base.json').read_text());GOLDEN=json.loads((ROOT/'integration/test/contracts/rayclmm-dynamic-quotes.json').read_text());ALPHABET='123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
PROGRAM=BASE['pool']['raydium_clmm']['program_id'];TOKEN=BASE['metadata']['token_program'];CLOCK=BASE['metadata']['clock']
def b58(b):
 n=int.from_bytes(b,'big');s=''
 while n:s=ALPHABET[n%58]+s;n//=58
 return '1'*(len(b)-len(b.lstrip(b'\0')))+s
def integer(b,at,n,size=8):b[at:at+size]=n.to_bytes(size,'little')
def keybytes(s):
 n=0
 for c in s:n=n*58+ALPHABET.index(c)
 return b'\0'*(len(s)-len(s.lstrip('1')))+n.to_bytes((n.bit_length()+7)//8,'big')
class Chain:
 def __init__(self):self.head=123456;self.timestamp=int(time.time());self.bad=None;self.reads=0;self.block_reads=0;self.next_bank=False
 def advance(self):self.head+=1;self.timestamp=int(time.time())
 def fixture(self):return BASE['metadata']['next_bank_fixture'] if self.next_bank else BASE
 def pool(self):return copy.deepcopy(self.fixture()['pool'])
 def accounts(self):
  p=self.pool();s=p['raydium_clmm'];out={};array_ids={a['address']for a in s['tick_arrays']}
  for address,a in self.fixture()['rpc_accounts'].items():
   encoded=a['data'][0] if isinstance(a['data'],list)else a['data_base64'];d=bytearray(base64.b64decode(encoded))
   if address==CLOCK:integer(d,0,self.head+(1 if self.bad=='clock-slot'else 0));integer(d,32,self.timestamp)
   if address==s['programdata']:
    if self.bad=='program-code':d[-1]^=1
    if self.bad=='program-slot':integer(d,4,s['deployed_slot']+1)
   if address==TOKEN and self.bad=='token-code':d[-1]^=1
   if address==p['pool_id']:
    if self.bad=='pool-pause':d[389]|=16
    if self.bad=='invalid-fee-mode':d[390]=3
    if self.bad=='invalid-fourth-mode':d[390]=4
    if self.bad=='unknown-fee-byte':d[390]=255
    if self.bad=='seed-zero':integer(d,391,0,2)
    if self.bad=='seed-endianness':d[391],d[392]=d[392],d[391]
    if self.bad=='seed-other':integer(d,391,65535,2)
    if self.bad=='seed-padding':d[393]=1
    if self.bad=='pool-bump':d[8]^=1
    if self.bad=='dynamic-disabled':d[1096:1176]=bytes(80)
    if self.bad=='dynamic-filter-zero':integer(d,1096,0,2)
    if self.bad=='dynamic-decay':integer(d,1098,0,2)
    if self.bad=='dynamic-reduction':integer(d,1100,10000,2)
    if self.bad=='dynamic-control':integer(d,1102,100000,4)
    if self.bad=='dynamic-reference':integer(d,1114,int.from_bytes(d[1106:1110],'little')+1,4)
    if self.bad=='dynamic-accumulator':integer(d,1118,int.from_bytes(d[1106:1110],'little')+1,4)
    if self.bad=='dynamic-index':integer(d,1110,(1<<31)-1,4)
    if self.bad=='dynamic-timestamp':integer(d,1122,1<<32)
    if self.bad=='dynamic-padding':d[1130]=1
    if self.bad=='not-open':integer(d,1080,self.timestamp)
    if self.bad=='reward':d[397+57]=1
    if self.bad=='bitmap':integer(d,904,1)
   if address==s['vault0']and self.bad=='vault-authority':d[32:64]=keybytes(p['token0'])
   if address in array_ids:
    if self.bad=='limit-order':integer(d,44+116,1)
    if self.bad=='tick-net':
     for i in range(60):
      at=44+i*168
      if int.from_bytes(d[at+20:at+36],'little')>0:d[at+4:at+20]=((1<<127)-1).to_bytes(16,'little');break
   if address==s['bitmap_extension']['address']:
    if self.bad=='extension-discriminator':d[0]^=1
    if self.bad=='extension-pool':d[8:40]=keybytes(p['token0'])
    if self.bad=='extension-truncated':d=d[:-1]
    if self.bad=='extension-bit-mismatch':d[40]^=1
   if address==s['observation']and self.bad=='observation-index':integer(d,17,100,2)
   out[address]={'owner':a['owner'],'data':[base64.b64encode(d).decode(),'base64'],'executable':a['executable'],'lamports':a['lamports'],'rentEpoch':0}
  if self.bad=='extension-owner':out[s['bitmap_extension']['address']]['owner']=TOKEN
  if self.bad=='missing-extension':out[s['bitmap_extension']['address']]=None
  if self.bad=='missing-array':out[s['tick_arrays'][0]['address']]=None
  return out
 def block(self,n):return {'blockhash':b58(hashlib.sha256(f'rayclmm/{n}'.encode()).digest()),'previousBlockhash':b58(hashlib.sha256(f'rayclmm/{n-1}'.encode()).digest()),'parentSlot':n-1,'blockTime':self.timestamp,'blockHeight':n}
 def provider(self):
  chain=self;required=set(BASE['rpc_request_accounts'])
  class Handler(BaseHTTPRequestHandler):
   def log_message(self,*args):pass
   def do_POST(self):
    q=json.loads(self.rfile.read(int(self.headers['Content-Length'])));method=q['method'];params=q['params'];result=None;error=None
    if method=='getGenesisHash':result='localnet'
    elif method=='getMultipleAccounts':
     assert set(params[0])==required and len(params[0])==len(required);assert params[1]['commitment']=='finalized'and params[1]['encoding']=='base64'and'minContextSlot'not in params[1];acc=chain.accounts();chain.reads+=1;result={'context':{'slot':chain.head},'value':[acc[k]for k in params[0]]}
    elif method=='getBlock':assert params[1]['commitment']=='finalized';result=chain.block(params[0]);chain.block_reads+=1
    else:error={'code':-32601,'message':method}
    body={'jsonrpc':'2.0','id':q['id']};body.update({'error':error}if error else{'result':result});raw=json.dumps(body).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
  server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start();return server

def expected(chain,amount,zero):
 if chain.next_bank:
  assert chain.pool()==BASE['metadata']['next_bank_fixture']['pool']
  return next(v['expected']['amount_out']for v in BASE['metadata']['next_bank_vectors']if v['input']==amount and v['zero_for_one']==zero)
 return next(v['expected']['amount_out']for v in BASE['metadata']['base_vectors']if v['input']==amount and v['zero_for_one']==zero)

def advance_evm_fixture(directory,timestamp_ms):
 # The explicit Go debug source only reads its sample during startup. Emit a
 # next block with a new hash before restarting it outside Solana fault probes.
 path=directory/'evm-31337.json';sample=json.loads(path.read_text());block=sample['reference']['block']
 block['parent_hash']=block['hash'];block['number_or_slot']+=1;block['timestamp_ms']=timestamp_ms
 block['hash']='0x'+hashlib.sha256(f"dynamic-control/{block['number_or_slot']}/{timestamp_ms}".encode()).hexdigest()
 temporary=path.with_suffix('.tmp');temporary.write_text(json.dumps(sample));temporary.replace(path)
 return block

def validate_time_policy(chain):
 assert len(GOLDEN['time_invariance_probes'])==8
 for fixture in [BASE,BASE['metadata']['next_bank_fixture']]:
  state=fixture['pool']['raydium_clmm'];dynamic=state['dynamic_fee']
  assert dynamic['last_update_timestamp']==0 and chain.timestamp-dynamic['last_update_timestamp']>=dynamic['decay_period']
  assert 0<chain.timestamp<=(1<<32)-1
  for v in fixture['metadata']['base_vectors'] if fixture is BASE else BASE['metadata']['next_bank_vectors']:
   matching=[p for p in GOLDEN['time_invariance_probes'] if p['pool']==fixture['pool'] and p['input']==v['input'] and p['zero_for_one']==v['zero_for_one']]
   assert len(matching)==1,'time policy requires exact native source pool and input'
   proof=matching[0]
   assert proof['expected']['amount_out']==proof['later_expected']['amount_out']==v['expected']['amount_out']
   early=proof['pool']['raydium_clmm'];late=proof['later_pool']['raydium_clmm']
   assert late['bank_unix_timestamp']>early['bank_unix_timestamp']
   assert early['bank_unix_timestamp']-dynamic['last_update_timestamp']>=dynamic['decay_period']
   assert late['bank_unix_timestamp']-dynamic['last_update_timestamp']>=dynamic['decay_period']

def corrupt_startup_rejected(exit_code,log_bytes,offset):
 if exit_code!=1:return False
 for line in log_bytes[offset:].decode('utf-8',errors='replace').splitlines():
  try:event=json.loads(line)
  except json.JSONDecodeError:continue
  if isinstance(event,dict)and event.get('event')=='service_start_failed'and isinstance(event.get('fields'),dict)and event['fields'].get('error_code')=='SOURCE_WAL_CORRUPT':return True
 return False

def run():
 report={'profile':'solana-rpc-source-evidence-wal-real-pg-kafka','production_ready':False,'checks':[],'evm_clock_refreshes':0};chain=Chain();validate_time_policy(chain);servers=[chain.provider(),chain.provider()];processes={};logs={}
 try:
  with Infrastructure() as infra,tempfile.TemporaryDirectory(prefix='xone-source-wal-') as folder:
   chain.timestamp=int(time.time());root=Path(folder);token=secrets.token_urlsafe(32);names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'};ports={a:port() for a in names};configs={};sources=[f'http://127.0.0.1:{ports[a]}' for a in ['go','rust']]
   for a,name in names.items():
    c=config_for(name,root/a,token,ports[a]);c['journal'].update(transport='kafka',source_urls=sources,poll_interval_ms=100,request_timeout_ms=15000);c['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=a,transaction_timeout_ms=4000);c['control'].update(backend='disabled' if name=='dex-router' else 'postgres',dsn_file='' if name=='dex-router' else str(infra.dsnfile),instance_id=a,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000);c['startup']['max_baseline_age_ms']=180000
    if a=='rust':c['journal']['kafka']['max_tail_records']=4;c['chains'][0].update(genesis_hash='localnet',confirmation_tag='finalized',rpc_urls=[f'http://127.0.0.1:{s.server_port}' for s in servers],pools=[dict(copy.deepcopy(BASE['pool_config']),protocol='raydium-clmm-dynamic')])
    elif a=='go':sample_for('evm:31337',root/a/'fixtures',chain.timestamp*1000)
    else:c['routing'].update(max_state_age_ms=180000,quote_ttl_ms=60000)
    configs[a]=save_config(root/a/'config',c)
   def base(a):return f'http://127.0.0.1:{ports[a]}'
   def start(a):
    name=names[a];logs[a]=(OUT/f'source-wal-{a}.log').open('ab');processes[a]=subprocess.Popen(service_command(name,configs[a],root/a/'fixtures' if a=='go' else None),cwd=ROOT/name,stdout=logs[a],stderr=subprocess.STDOUT)
   def stop(a):
    p=processes.pop(a,None)
    if p:
     p.terminate()
     try:p.wait(timeout=10)
     except subprocess.TimeoutExpired:p.kill();p.wait()
    if a in logs:logs.pop(a).close()
   def status(a):return http(base(a)+'/internal/chains/solana:localnet/status',token)
   def request_quote(a,zero=True,amount='1000000'):
    p=chain.pool();request={'chainKey':'solana:localnet','kind':'EXACT_IN','sellToken':p['token0' if zero else 'token1'],'buyToken':p['token1' if zero else 'token0'],'sellAmount':amount,'caller':p['token0'],'recipient':p['token1'],'slippageBps':50,'verification':'FAST'}
    return http(base(a)+'/v1/quote',token,request)
   def check(a,zero=True,amount='1000000'):
    code,q=request_quote(a,zero,amount);assert code==200,(code,q);assert q['grossBuyAmount']==expected(chain,amount,zero),(q,expected(chain,amount,zero));assert q['consumedIn']==amount and q['refund']=='0' and q['verification']['level']=='INDICATIVE';return q
   def refresh_evm_control():
    directory=root/'go'/'fixtures';current=json.loads((directory/'evm-31337.json').read_text())['reference']['block']
    now=int(time.time()*1000)
    if now-current['timestamp_ms']<60000:return
    stop('go');fresh=advance_evm_fixture(directory,now);start('go');wait(base('go')+'/health/ready')
    for router in ['router1','router2']:
     await_condition(lambda:http(base(router)+'/internal/chains/evm:31337/status',token)[1].get('snapshot_ref',{}).get('block',{}).get('hash')==fresh['hash'],'EVM control clock not refreshed')
     quote(base(router),token,'evm:31337','B')
    report['evm_clock_refreshes']+=1
   def current(a):return status(a)[1].get('snapshot_ref',{}).get('block',{}).get('number_or_slot')==chain.head
   try:
    for a in ['go','rust']:start(a);wait(base(a)+'/health/ready')
    for a in ['router1','router2']:start(a);wait(base(a)+'/health/live');wait_scope(base(a),token,'solana:localnet')
    for a in ['router1','router2']:
     for amount in ['1000000','2000000000']:check(a,True,amount);check(a,False,amount)
    report['checks'].append('normal-rust-rpc-raw-source-evidence-before-typed-baseline-and-both-native-quote-directions')
    url=base('rust')+'/internal/chains/solana:localnet/source-evidence'
    def read_evidence():
     result={}
     def available():
      code,value=http(url,token)
      if code!=200:return False
      result.update(value);return True
     await_condition(available,'source evidence status stayed unavailable')
     return result
    assert http(url)[0]==401,'raw evidence status bypassed internal authentication'
    evidence=read_evidence()
    assert evidence['records']==3 and evidence['session_id']==1 and evidence['evidence_only']is True and evidence['confers_readiness']is False,evidence
    assert 'http'not in json.dumps(evidence)and 'token'not in json.dumps(evidence)
    report['checks'].append('authenticated-non-readiness-evidence-status-redacts-source-credentials-and-deduplicates-unchanged-rpc')
    old={a:check(a)for a in ['router1','router2']};log=OUT/'source-wal-rust.log';offset=log.stat().st_size
    chain.next_bank=True;chain.advance()
    for router in old:await_condition(lambda:status(router)[0]==503,'full WAL failed to revoke stale Solana quotes');quote(base(router),token,'evm:31337','B')
    await_condition(lambda:'SOURCE_WAL_CAPACITY'in log.read_bytes()[offset:].decode(),'capacity path was not exercised')
    evidence=read_evidence();assert evidence['records']==4 and evidence['session_id']==1,evidence
    for router in old:
     assert request_quote(router)[0]!=200,'unrecorded source snapshot became quotable'
    report['checks'].append('source-wal-capacity-retains-raw-prefix-rejects-new-baseline-and-revokes-solana-only')
    stop('rust')
    configuration=json.loads(configs['rust'].read_text());configuration['journal']['kafka']['max_tail_records']=128
    configs['rust']=save_config(root/'rust'/'config',configuration)
    start('rust');wait(base('rust')+'/health/live')
    for router in old:
     await_condition(lambda:status(router)[1].get('snapshot_ref',{}).get('online_generation',0)>old[router]['onlineGeneration'],'retained WAL and new session did not resume qualified baseline',timeout=55)
     for amount in ['1000000','2000000000']:check(router,True,amount);check(router,False,amount)
    evidence=read_evidence();assert evidence['session_id']==2 and evidence['records']>=7 and evidence['records']<=8,evidence
    assert evidence['retained_bytes']>0 and evidence['retained_bytes']<=evidence['max_retained_bytes']
    report['checks'].append('parser-restart-verifies-prior-record-chain-starts-new-session-and-restores-only-from-fresh-recorded-rpc')
    before=evidence['records']
    chain.head+=1;chain.timestamp=int(time.time())
    for router in old:await_condition(lambda:status(router)[1].get('snapshot_ref',{}).get('block',{}).get('number_or_slot')==chain.head,'fresh recorded bank not published');check(router)
    after=read_evidence();assert after['records']>before and after['session_id']==2
    report['checks'].append('new-bank-is-recorded-within-bounds-and-typed-quote-state-keeps-native-math')
    # Force a real durability error after DB sync by making atomic head
    # replacement impossible. This exercises the actual commit path.
    refresh_evm_control();old={a:check(a)for a in ['router1','router2']}
    head=root/'rust'/'state'/'source-evidence'/'solana-localnet'/'SOURCE_HEAD'
    saved_head=head.read_bytes();head.unlink();head.mkdir();chain.advance()
    for router in old:await_condition(lambda:status(router)[0]==503,'failed durable head did not revoke source scope');quote(base(router),token,'evm:31337','B')
    poisoned=read_evidence();assert poisoned['write_poisoned']is True,poisoned
    head.rmdir();head.write_bytes(saved_head)
    for router in old:assert request_quote(router)[0]!=200,'repairing marker bypassed poisoned writer without reopen'
    report['checks'].append('real-marker-commit-failure-poisons-writer-and-cannot-bypass-via-dedup-or-in-place-file-repair')
    stop('rust');start('rust');wait(base('rust')+'/health/live')
    for router in old:
     await_condition(lambda:status(router)[1].get('snapshot_ref',{}).get('online_generation',0)>old[router]['onlineGeneration'],'verified reopen after marker error did not restore scope',timeout=55);check(router)
    after=read_evidence();assert after['session_id']==3 and after['write_poisoned']is False and after['records']>poisoned['records']
    report['checks'].append('reopen-verifies-db-ahead-unacknowledged-suffix-and-new-session-before-fresh-qualification')
    # A revoked source must not resurrect old quote state from a corrupt WAL.
    refresh_evm_control();old={a:check(a)for a in ['router1','router2']};chain.bad='dynamic-control';chain.advance()
    for router in old:await_condition(lambda:status(router)[0]==503,'bad account did not revoke scope before corruption restart');quote(base(router),token,'evm:31337','B')
    stop('rust');saved_head=head.read_bytes();damaged=json.loads(saved_head);damaged['session']=0;head.write_text(json.dumps(damaged))
    corruption_restart_offset=log.stat().st_size
    start('rust');await_condition(lambda:processes['rust'].poll()is not None,'corrupt WAL startup did not stop',timeout=15)
    assert corrupt_startup_rejected(processes['rust'].returncode,log.read_bytes(),corruption_restart_offset)
    for router in old:assert request_quote(router)[0]!=200
    report['checks'].append('corrupt-source-head-startup-is-rejected-and-old-kv-cannot-resurrect-revoked-quotes')
    stop('rust');head.write_bytes(saved_head);chain.bad=None
    start('rust');wait(base('rust')+'/health/live')
    for router in old:await_condition(lambda:status(router)[1].get('snapshot_ref',{}).get('online_generation',0)>old[router]['onlineGeneration'],'repair plus fresh RPC did not restore revoked scope',timeout=55);check(router)
    after=read_evidence();assert after['session_id']==4 and after['write_poisoned']is False
    report['checks'].append('restored-exact-durable-head-and-new-recorded-rpc-start-new-qualified-session-after-corruption')
    stop('router1');start('router1');wait(base('router1')+'/health/live');wait_scope(base('router1'),token,'solana:localnet');check('router1');check('router1',False)
    report['checks'].append('memory-only-router-restores-owner-checkpoint-without-treating-source-wal-as-online-state')
    report['source_evidence_status']=after
    report['passed']=True;report['atomic_account_batches']=chain.reads;report['finalized_block_reads']=chain.block_reads
   finally:
    for a in list(processes):stop(a)
 finally:
  for s in servers:s.shutdown();s.server_close()
 report.update(measured_at_unix_ms=int(time.time()*1000),tool_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest());return report
if __name__=='__main__':
 OUT.mkdir(parents=True,exist_ok=True)
 try:r=run()
 except Exception as e:(OUT/'source-wal-three-service-smoke.json').write_text(json.dumps({'passed':False,'production_ready':False,'error':str(e),'error_type':type(e).__name__},indent=2)+'\n');raise
 (OUT/'source-wal-three-service-smoke.json').write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r,indent=2))
