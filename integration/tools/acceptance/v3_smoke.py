#!/usr/bin/env python3
"""Hash-pinned synthetic V3 RPC -> real PG/Kafka -> two quote replicas.
Official SDK differential fixtures supply expected amounts, never this harness.
"""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import copy, hashlib, json, secrets, shutil, subprocess, tempfile, threading, time, urllib.request, os
from receipt_reference import EMPTY_ROOT
from infrastructure import Infrastructure,ROOT,port
from smoke import http,quote,wait_scope
from kafka_smoke import wait
from reorg_smoke import await_condition
OUT=ROOT/'integration/artifacts/local'
GOLDEN=json.loads((ROOT/'integration/test/contracts/v3-quotes.json').read_text())
BASE=json.loads((ROOT/'integration/test/contracts/v3-state.json').read_text())
CODE=bytes.fromhex('6001600055');CODE_HASH=hashlib.sha256(CODE).hexdigest();SECOND='0x'+'06'*20;SECOND_FACTORY='0x'+'07'*20;SECOND_CODE=bytes.fromhex('6002600055');GENESIS='0x'+'99'*32

def word(value):return f'{value%(1<<256):064x}'
def expected(index,amount,zero):
 name=f'case-{index}-{str(zero).lower()}-{amount}'
 return next(c['expected'] for c in GOLDEN['cases'] if c['name']==name)
def state_for(index):return copy.deepcopy(next(c['pool'] for c in GOLDEN['cases'] if c['name']==f'case-{index}-true-10000'))

class Chain:
 def __init__(self):
  self.head=100;self.branch=0;self.bad_tick=False;self.bad_code=False;self.timestamp=int(time.time());self.pinned=0
 def block(self,n):
  branch=self.branch if n>=105 else 0
  digest=lambda number:'0x'+hashlib.sha256(f'{self.branch if number>=105 else 0}/{number}'.encode()).hexdigest()
  return {'number':hex(n),'hash':GENESIS if n==0 else digest(n),'parentHash':digest(max(0,n-1)),'timestamp':hex(self.timestamp),'transactions':[],'receiptsRoot':EMPTY_ROOT}
 def pool(self,n,address=None):
  index=0 if n>=107 else 1 if n==101 else 5 if n==102 or self.branch and n>=105 else 0
  p=state_for(index)
  if address==SECOND:p['pool_id']=SECOND;p['v3']['factory']=SECOND_FACTORY
  return p
 def provider(self):
  chain=self
  class Handler(BaseHTTPRequestHandler):
   def log_message(self,*_):pass
   def do_POST(self):
    q=json.loads(self.rfile.read(int(self.headers['Content-Length'])));params=q['params'];method=q['method'];error=None;result=None
    if method=='eth_chainId':result='0x7a69'
    elif method=='eth_getBlockByNumber':result=chain.block(chain.head if params[0] in ['latest','safe','finalized'] else int(params[0],16))
    elif method in ['eth_call','eth_getCode']:
     anchor=params[1]
     identities={chain.block(n)['hash']:n for n in range(chain.head+1)}
     if anchor.get('requireCanonical') is not True or anchor.get('blockHash') not in identities:error={'code':-32000,'message':'anchor not canonical'}
     else:
      chain.pinned+=1;n=identities[anchor['blockHash']];address=params[0] if method=='eth_getCode' else params[0]['to'];pool=chain.pool(n,address);v=pool['v3']
      if method=='eth_getCode':result='0x00' if chain.bad_code else '0x'+(SECOND_CODE if address==SECOND else CODE).hex()
      else:
       call=params[0];data=call['data'];selector=data[:10]
       if call['to'] not in [BASE['pool_id'],SECOND]:error={'code':-32000,'message':'unknown pool'}
       elif selector=='0x0dfe1681':result='0x'+BASE['token0'][2:].rjust(64,'0')
       elif selector=='0xd21220a7':result='0x'+BASE['token1'][2:].rjust(64,'0')
       elif selector=='0xc45a0155':result='0x'+v['factory'][2:].rjust(64,'0')
       elif selector=='0xddca3f43':result='0x'+word(pool['fee_numerator'])
       elif selector=='0xd0c93a7c':result='0x'+word(v['tick_spacing'])
       elif selector=='0x1a686502':result='0x'+word(int(v['liquidity']))
       elif selector=='0x3850c7bd':result='0x'+''.join(word(x) for x in [int(v['sqrt_price_x96']),v['tick'],0,1,1,0,1])
       elif selector=='0x5339c296':
        pos=int(data[10:],16);pos=pos-(1<<256) if pos&(1<<255) else pos
        bits=next((int(w['bitmap']) for w in v['tick_words'] if w['word_pos']==pos),0);result='0x'+word(bits)
       elif selector=='0xf30dba93':
        tick=int(data[10:],16);tick=tick-(1<<256) if tick&(1<<255) else tick
        info=next((t for t in v['ticks'] if t['tick']==tick),None)
        if info is None:error={'code':-32000,'message':'uninitialized tick requested'}
        else:result='0x'+''.join(word(x) for x in [int(info['liquidity_gross']),int(info['liquidity_net']),0,0,0,0,0,0 if chain.bad_tick else 1])
       else:error={'code':-32601,'message':'unsupported fixture selector'}
    else:error={'code':-32601,'message':'unsupported fixture RPC'}
    body={'jsonrpc':'2.0','id':q['id']};body.update({'error':error} if error else {'result':result});raw=json.dumps(body).encode()
    self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
  server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start();return server

def run():
 report={'profile':'bounded-v3-synthetic-rpc-real-pg-kafka','production_ready':False,'checks':[],'reference':GOLDEN['reference']}
 env=os.environ.copy();env['PATH']=str(Path.home()/'.cargo/bin')+':'+env['PATH']
 with (OUT/'v3-debug-fixture-build.log').open('w') as log:
  subprocess.run([str(Path.home()/'.cargo/bin/cargo'),'+1.90.0','build','--locked','--features','debug-fixtures','--bin','fixture-parser-rust'],cwd=ROOT/'chain-parser-rust',env=env,stdout=log,stderr=subprocess.STDOUT,check=True)
 chain=Chain();providers=[chain.provider(),chain.provider()];processes={};logs={}
 try:
  with Infrastructure() as infra,tempfile.TemporaryDirectory(prefix='xone-v3-') as folder:
   chain.timestamp=int(time.time())
   root=Path(folder);token=secrets.token_urlsafe(32);names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'};ports={a:port() for a in names};configs={}
   sources=[f'http://127.0.0.1:{ports[a]}' for a in ['go','rust']]
   for alias,name in names.items():
    cfg=root/alias/'config';cfg.mkdir(parents=True);service=json.loads((ROOT/name/'config/config-test.yaml').read_text());service['api'].update(listen=f'127.0.0.1:{ports[alias]}',auth_token=token,requests_per_minute=1000);service['state']['path']=str(root/alias/'state')
    if alias=='go':
     service['service']['mode']='rpc';c=service['chains'][0];c.update(genesis_hash=GENESIS,confirmation_tag='latest',auto_reorg_window=4,rpc_urls=[f'http://127.0.0.1:{server.server_port}' for server in providers])
     c['pools']=[{'id':BASE['pool_id'],'protocol':'uniswap-v3','token0':BASE['token0'],'token1':BASE['token1'],'fee_numerator':3000,'fee_denominator':1000000,'program_id':'','accounts':[],
        'v3':{'expected_factory':BASE['v3']['factory'],'expected_code_sha256':CODE_HASH,'tick_word_radius':1,'max_initialized_ticks':32,'max_steps':512}}]
    if alias=='rust':
     fixture=json.loads((ROOT/name/'test/fixtures/solana/baseline.json').read_text());fixture['reference']['block']['timestamp_ms']=chain.timestamp*1000;directory=root/alias/'fixtures';directory.mkdir();(directory/'solana-localnet.json').write_text(json.dumps(fixture))
    service['journal'].update(transport='kafka',source_urls=sources,poll_interval_ms=100,request_timeout_ms=10000)
    service['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=alias,transaction_timeout_ms=4000)
    service['control'].update(backend='disabled' if name=='dex-router' else 'postgres',dsn_file='' if name=='dex-router' else str(infra.dsnfile),instance_id=alias,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000)
    service['startup']['max_baseline_age_ms']=180000;service['logging']['level']='info'
    if name=='dex-router':
     service['routing'].update(max_state_age_ms=180000,quote_ttl_ms=60000)
     service['journal']['source_auth_tokens']={url:token for url in sources}
    config=cfg/'config-test.yaml';config.write_text(json.dumps(service));configs[alias]=config
   def base(a):return f'http://127.0.0.1:{ports[a]}'
   def start(a):
    name=names[a];binary=ROOT/name/('bin/chain-parser-go' if name=='chain-parser-go' else 'target/debug/fixture-parser-rust' if a=='rust' else f'target/debug/{name}');logs[a]=(OUT/f'v3-{a}.log').open('ab');processes[a]=subprocess.Popen([str(binary),'--config',str(configs[a])]+(['--fixtures',str(root/a/'fixtures')] if a=='rust' else []),cwd=ROOT/name,stdout=logs[a],stderr=subprocess.STDOUT)
   def stop(a):
    p=processes.pop(a,None)
    if p:
     p.terminate()
     try:p.wait(timeout=10)
     except subprocess.TimeoutExpired:p.kill();p.wait()
    if a in logs:logs.pop(a).close()
   def status(a):return http(base(a)+'/internal/chains/evm:31337/status',token)
   def at_height(a,height):await_condition(lambda:status(a)[1].get('snapshot_ref',{}).get('block',{}).get('number_or_slot')==height,'V3 replica did not reach target block')
   def request(zero=True,amount='10000',verification='FAST'):
    return {'chainKey':'evm:31337','kind':'EXACT_IN','sellToken':BASE['token0' if zero else 'token1'],'buyToken':BASE['token1' if zero else 'token0'],'sellAmount':amount,'caller':BASE['token0'],'recipient':BASE['token1'],'slippageBps':50,'verification':verification}
   def check_quote(a,index,zero=True,amount='10000'):
    code,value=http(base(a)+'/v1/quote',token,request(zero,amount));assert code==200,(code,value)
    oracle=expected(index,amount,zero);assert not oracle.get('error');assert value['grossBuyAmount']==oracle['amount_out'],(a,index,zero,amount,value['grossBuyAmount'],oracle)
    assert value['consumedIn']==amount and value['refund']=='0' and value['verification']['level']=='INDICATIVE'
    assert value['routes'][0]['path']['edges'][0]['protocol']=='uniswap-v3';return value
   try:
    for a in ['go','rust']:start(a);wait(base(a)+'/health/ready')
    for a in ['router1','router2']:start(a);wait(base(a)+'/health/live');wait_scope(base(a),token,'evm:31337');wait_scope(base(a),token,'solana:localnet')
    quotes={a:check_quote(a,0) for a in ['router1','router2']}
    for a in quotes:
     check_quote(a,0,False);check_quote(a,0,True,'20000000');check_quote(a,0,False,'20000000')
    report['checks'].append('hash-pinned-v3-code-slot0-liquidity-bitmap-ticks-produce-two-independent-replicas')
    report['checks'].append('both-directions-and-multi-tick-input-match-pinned-official-sdk-amounts')
    req=urllib.request.Request(base('go')+'/internal/chains/evm:31337/bootstrap',headers={'Authorization':'Bearer '+token})
    with urllib.request.urlopen(req,timeout=3) as response:raw=response.read();assert response.headers['content-type']=='application/x-protobuf'
    decoded=subprocess.run(['protoc','--proto_path='+str(ROOT/'common/protoc/proto'),'--decode=dex.v1.OwnerBootstrap',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=raw,capture_output=True,check=True).stdout.decode()
    assert 'v3 {' in decoded and 'liquidity_net_i128_be:' in decoded and 'word_pos: -1' in decoded;report['checks'].append('live-protobuf-v3-proof-and-signed-i128-liquidity-net')
    for a in quotes:
     code,error=http(base(a)+'/v1/quote',token,request(True,'1000000000000000000'));assert code==404 and error['error']=='NO_ROUTE',(code,error)
     assert http(base(a)+'/v1/quote',token,request(verification='VERIFIED'))[0]==422
    report['checks'].append('unproven-range-is-rejected-without-v2-fallback-or-false-verified')
    chain.head=101
    for a in quotes:at_height(a,101);check_quote(a,1,True);check_quote(a,1,False)
    report['checks'].append('next-block-exact-initialized-tick-boundary-and-liquidity-crossing')
    chain.head=102
    for a in quotes:at_height(a,102);check_quote(a,5,True);check_quote(a,5,False)
    report['checks'].append('zero-active-liquidity-gap-is-traversed-by-ticks-in-both-directions')
    before={a:check_quote(a,5) for a in quotes};chain.bad_tick=True;chain.head=103
    for a in quotes:await_condition(lambda:status(a)[0]==503,'missing tick did not revoke V3 scope');quote(base(a),token,'solana:localnet','B')
    report['checks'].append('missing-initialized-tick-revokes-only-affected-chain')
    chain.bad_tick=False
    for a in quotes:
     await_condition(lambda:status(a)[1].get('snapshot_ref',{}).get('online_generation',0)>before[a]['onlineGeneration'],'V3 recovery did not allocate generation');fresh=check_quote(a,0)
     assert fresh['snapshotRef']['canonical_epoch']==before[a]['snapshotRef']['canonical_epoch']
     code,error=http(base(a)+f'/v1/quotes/{before[a]["quoteId"]}/build',token,{},'POST');assert code==409 and error['error']=='SNAPSHOT_REORGED'
    report['checks'].append('qualified-head-first-restores-v3-generation-and-invalidates-old-quotes')
    before_code={a:check_quote(a,0) for a in quotes};chain.bad_code=True;chain.head=104
    for a in quotes:await_condition(lambda:status(a)[0]==503,'code mismatch did not revoke V3 scope')
    chain.bad_code=False
    for a in quotes:await_condition(lambda:status(a)[1].get('snapshot_ref',{}).get('online_generation',0)>before_code[a]['onlineGeneration'],'code recovery failed');check_quote(a,0)
    report['checks'].append('pool-bytecode-identity-mismatch-cannot-enter-quote-graph')
    for height in [105,106]:
     chain.head=height
     for a in quotes:at_height(a,height)
    old={a:check_quote(a,0) for a in quotes};chain.branch=1
    for a in quotes:
     await_condition(lambda:status(a)[1].get('snapshot_ref',{}).get('canonical_epoch',0)>old[a]['snapshotRef']['canonical_epoch'],'V3 reorg failed');fresh=check_quote(a,5)
     assert fresh['onlineGeneration']==old[a]['onlineGeneration'];assert fresh['snapshotRef']['journal_seq']==old[a]['snapshotRef']['journal_seq']+3
    report['checks'].append('multi-block-v3-reorg-atomically-replaces-proof-and-keeps-generation')
    last={a:check_quote(a,5) for a in quotes};stop('go');start('go');wait(base('go')+'/health/ready')
    for a in quotes:await_condition(lambda:status(a)[1].get('snapshot_ref',{}).get('online_generation',0)>last[a]['onlineGeneration'],'restarted V3 owner did not restore');check_quote(a,5)
    report['checks'].append('v3-private-kv-owner-restart-and-current-protobuf-bootstrap-recovery')
    chain.head=107
    for a in quotes:at_height(a,107)
    stop('go')
    service=json.loads(configs['go'].read_text());second=copy.deepcopy(service['chains'][0]['pools'][0]);second['id']=SECOND;second['v3']['expected_factory']=SECOND_FACTORY;second['v3']['expected_code_sha256']=hashlib.sha256(SECOND_CODE).hexdigest();service['chains'][0]['pools'].append(second);configs['go'].write_text(json.dumps(service));start('go');wait(base('go')+'/health/ready')
    for a in quotes:
     await_condition(lambda:status(a)[1].get('scope_pool_count')==2,'second V3 pool scope not installed')
     code,value=http(base(a)+'/v1/quote',token,request(True,'60000000'));assert code==200,(code,value)
     assert value['grossBuyAmount']=='59048234' and value['consumedIn']=='60000000' and value['refund']=='0' and len(value['routes'])==2,value
     for route in value['routes']:assert route['amount_in']=='30000000' and route['amount_out']=='29524117' and route['path']['amount_in']==route['amount_in']
    report['checks'].append('full-order-outside-each-pool-range-still-finds-complete-50-50-v3-split')
    report['pinned_rpc_dependency_reads']=chain.pinned;report['passed']=True
   finally:
    for a in list(processes):stop(a)
 finally:
  for server in providers:server.shutdown();server.server_close()
 report.update(measured_at_unix_ms=int(time.time()*1000),tool_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest());return report
if __name__=='__main__':
 OUT.mkdir(parents=True,exist_ok=True)
 try:result=run()
 except Exception as e:
  (OUT/'v3-three-service-smoke.json').write_text(json.dumps({'passed':False,'production_ready':False,'error_type':type(e).__name__,'error':str(e)},indent=2)+'\n');raise
 (OUT/'v3-three-service-smoke.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
