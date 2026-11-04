#!/usr/bin/env python3
"""Complete-receipt witness disagreement, real PG/Kafka and two independent replicas.
RPC sources are synthetic; distinct loopback URLs do not attest provider independence.
"""
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
from pathlib import Path
import copy,hashlib,json,os,secrets,subprocess,tempfile,threading,time,urllib.request
from receipt_reference import EMPTY_ROOT, SINGLE_EMPTY_ROOT, SINGLE_EMPTY_FIELDS, seal_receipts
from infrastructure import ROOT,Infrastructure,port
from configuration import config_for,save_config,service_command,sample_for
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import http,quote,wait_scope
OUT=ROOT/'integration/artifacts/local'
BASE=json.loads((ROOT/'integration/test/contracts/v3-state.json').read_text())
GOLDEN=json.loads((ROOT/'integration/test/contracts/dex-versions.json').read_text())
V3GOLDEN=json.loads((ROOT/'integration/test/contracts/v3-quotes.json').read_text())
NAMES=['uniswap-v2','uniswap-v3','pancake-v1','pancake-v2','pancake-v3','sushiswap-v2']
GENESIS='0x'+'99'*32

def require(condition, message='acceptance condition failed'):
 if not condition:raise AssertionError(message)

def address(n):return '0x'+f'{n:040x}'
def word(n):return f'{n%(1<<256):064x}'
def profile(name):
 index=NAMES.index(name)+1;clmm=name.endswith('v3');fee,den=(3000,1000000) if clmm else (2,1000) if name=='pancake-v1' else (25,10000) if name=='pancake-v2' else (3,1000)
 code=bytes([0x60,index,0x60,0,0x55]);identity={'expected_factory':address(200+index),'expected_code_sha256':hashlib.sha256(code).hexdigest()}
 config={'id':address(index),'protocol':name,'token0':address(index*10+1),'token1':address(index*10+2),'fee_numerator':fee,'fee_denominator':den,'program_id':'','accounts':[]}
 config['v3' if clmm else 'v2']=dict(identity,**({'tick_word_radius':1,'max_initialized_ticks':32,'max_steps':512} if clmm else {}))
 return config,code

class VersionChain:
 def __init__(self):self.head=100;self.branch=0;self.timestamp=int(time.time());self.bad=None;self.pinned=0;self.receipts=0
 def block(self,n):
  def h(x):return '0x'+hashlib.sha256(f'{self.branch if x>=101 else 0}/{x}'.encode()).hexdigest()
  block={'number':hex(n),'hash':GENESIS if n==0 else h(n),'parentHash':h(max(n-1,0)),'timestamp':hex(self.timestamp),'transactions':[]}
  if n:block['transactions']=['0x'+hashlib.sha256(('tx/'+block['hash']).encode()).hexdigest()]
  block['receiptsRoot']=SINGLE_EMPTY_ROOT if n else EMPTY_ROOT
  return block
 def provider(self):
  chain=self;catalog={profile(name)[0]['id']:profile(name) for name in NAMES}
  class Handler(BaseHTTPRequestHandler):
   def log_message(self,*_):pass
   def do_POST(self):
    q=json.loads(self.rfile.read(int(self.headers['Content-Length'])));params=q['params'];method=q['method'];out=None;error=None
    if method=='eth_chainId':out='0x7a69'
    elif method=='eth_getBlockByNumber':out=chain.block(chain.head if params[0] in ['latest','safe','finalized'] else int(params[0],16))
    elif method=='eth_getTransactionReceipt':
     block=next((chain.block(n) for n in range(1,chain.head+1) if params[0] in chain.block(n)['transactions']),None)
     if block:out={'blockHash':block['hash'],'transactionHash':params[0],'transactionIndex':'0x0','logs':[],**SINGLE_EMPTY_FIELDS};chain.receipts+=1
    elif method in ['eth_call','eth_getCode']:
     anchor=params[1];identities={chain.block(n)['hash']:n for n in range(chain.head+1)}
     if anchor.get('requireCanonical') is not True or anchor.get('blockHash') not in identities:error={'code':-32000,'message':'noncanonical'}
     else:
      chain.pinned+=1;call=params[0];target=call if method=='eth_getCode' else call['to'];p,code=catalog[target];is3=p['protocol'].endswith('v3');name=p['protocol'];v=BASE['v3'];identity=p['v3' if is3 else 'v2']
      if method=='eth_getCode':out='0x'+(b'\x00' if chain.bad=='code' and name=='pancake-v2' else code).hex()
      else:
       data=call['data'];selector=data[:10]
       if selector=='0x0dfe1681':out='0x'+p['token0'][2:].rjust(64,'0')
       elif selector=='0xd21220a7':out='0x'+p['token1'][2:].rjust(64,'0')
       elif selector=='0xc45a0155':out='0x'+identity['expected_factory'][2:].rjust(64,'0')
       elif selector=='0x0902f1ac':out='0x'+word(1000000)+word(1000000)+word(chain.timestamp)
       elif selector=='0xddca3f43':out='0x'+word(p['fee_numerator'])
       elif selector=='0xd0c93a7c':out='0x'+word(v['tick_spacing'])
       elif selector=='0x1a686502':out='0x'+word(int(v['liquidity']))
       elif selector=='0x540d4918':out='0x'+word(1 if chain.bad=='lm' and name=='pancake-v3' else 0)
       elif selector=='0x3850c7bd':out='0x'+''.join(word(x) for x in [int(v['sqrt_price_x96']),v['tick'],0,1,1,209718400 if name=='pancake-v3' else 0,1])
       elif selector=='0x5339c296':
        pos=int(data[10:],16);pos=pos-(1<<256) if pos&(1<<255) else pos;out='0x'+word(next((int(w['bitmap']) for w in v['tick_words'] if w['word_pos']==pos),0))
       elif selector=='0xf30dba93':
        tick=int(data[10:],16);tick=tick-(1<<256) if tick&(1<<255) else tick;info=next(t for t in v['ticks'] if t['tick']==tick);out='0x'+''.join(word(x) for x in [int(info['liquidity_gross']),int(info['liquidity_net']),0,0,0,0,0,1])
       else:error={'code':-32601,'message':'unconfigured selector'}
    else:error={'code':-32601,'message':'unconfigured RPC'}
    body={'jsonrpc':'2.0','id':q['id']};body.update({'error':error} if error else {'result':out});raw=json.dumps(body).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
  server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start();return server

def oracle(name,zero):
 if name=='uniswap-v2':return '9871'
 if name=='uniswap-v3':return next(c['expected']['amount_out'] for c in V3GOLDEN['cases'] if c['name']==f'case-0-{str(zero).lower()}-10000')
 return next(c['expected']['amount_out'] for c in GOLDEN['cases'] if c['pool']['protocol']==name and c['input']=='10000' and c['zero_for_one']==zero and (c['pool'].get('v3',{}).get('tick',0)==0) and (c['pool']['reserve0']=='1000000' or c['pool'].get('v3',{}).get('liquidity')=='3000000000'))

WITNESS_LOG = {'address':address(999),'topics':['0x'+'ab'*32],'data':'0x01'}
WITNESS_ROOT, _WITNESS_RECEIPTS = seal_receipts([{'logs':[WITNESS_LOG]}])
WITNESS_FIELDS = {k:v for k,v in _WITNESS_RECEIPTS[0].items() if k!='logs'}

class Chain(VersionChain):
 def __init__(self):super().__init__();self.witness_bad=None;self.primary_bad=None;self.witness_reads=0
 def block(self,n):
  block=super().block(n);block['receiptsRoot']=WITNESS_ROOT if n else EMPTY_ROOT;return block
 def provider(self,witness=False):
  chain=self;upstream=super().provider()
  class Handler(BaseHTTPRequestHandler):
   def log_message(self,*_):pass
   def do_POST(self):
    query=self.rfile.read(int(self.headers['Content-Length']));request=json.loads(query)
    call=urllib.request.Request(f'http://127.0.0.1:{upstream.server_port}',data=query,headers={'Content-Type':'application/json'})
    with urllib.request.urlopen(call,timeout=3) as response:payload=json.loads(response.read())
    if request['method']=='eth_getTransactionReceipt'and isinstance(payload.get('result'),dict):
     if witness:chain.witness_reads+=1
     receipt=payload['result'];receipt.update(WITNESS_FIELDS);receipt['logs']=[{'blockHash':receipt['blockHash'],'transactionHash':receipt['transactionHash'],'transactionIndex':receipt['transactionIndex'],'logIndex':'0x0','removed':False,'address':address(999),'topics':['0x'+'ab'*32],'data':'0x01'}]
     if (witness and chain.witness_bad=='trailing-log')or(not witness and chain.primary_bad=='trailing-log'):receipt['logs']=[]
     fault=chain.witness_bad if witness else chain.primary_bad
     if fault=='trailing-log-rebloom':receipt['logs']=[];receipt['logsBloom']='0x'+'00'*256
     if fault=='status-flip':receipt['status']='0x0'
     if fault=='unknown-type':receipt['type']='0x7e'
     if witness and chain.witness_bad=='log-content':receipt['logs'][0]['data']='0x02'
    if witness and chain.witness_bad=='block-parent'and request['method']=='eth_getBlockByNumber'and request['params'][0]=='0x'+format(chain.head,'x'):
     payload['result']['parentHash']='0x'+'ee'*32
    raw=json.dumps(payload).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
  server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start();server.upstream=upstream;return server

def run(prefix='receipt-witness', faults=None):
 if faults is None:
  faults=[('trailing-log',None,'trailing-log','RECEIPT_WITNESS_INVALID'),('primary-trailing-log','trailing-log',None,'RECEIPT_BLOOM_MISMATCH'),('log-content',None,'log-content','RECEIPT_WITNESS_INVALID'),('block-parent',None,'block-parent','RECEIPT_PROVIDER_DISAGREEMENT')]
 report={'profile':'multi-rpc-receipt-witness-real-pg-kafka','production_ready':False,'protocols':NAMES,'checks':[]};chain=Chain();servers=[chain.provider(),chain.provider(True)];processes={};logs={}
 try:
  with Infrastructure() as infra,tempfile.TemporaryDirectory(prefix='xone-'+prefix+'-') as folder:
   chain.timestamp=int(time.time())
   root=Path(folder);token=secrets.token_urlsafe(32);names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'};ports={a:port() for a in names};configs={};sources=[f'http://127.0.0.1:{ports[a]}' for a in ['go','rust']]
   for alias,name in names.items():
    cfg=config_for(name,root/alias,token,ports[alias]);cfg['journal'].update(transport='kafka',source_urls=sources,poll_interval_ms=100,request_timeout_ms=15000)
    cfg['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=alias,transaction_timeout_ms=4000)
    cfg['control'].update(backend='disabled' if name=='dex-router' else 'postgres',dsn_file='' if name=='dex-router' else str(infra.dsnfile),instance_id=alias,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000);cfg['startup']['max_baseline_age_ms']=180000
    if alias=='go':cfg['chains'][0].update(genesis_hash=GENESIS,confirmation_tag='latest',auto_reorg_window=4,rpc_urls=[f'http://127.0.0.1:{s.server_port}' for s in servers],pools=[profile(n)[0] for n in NAMES])
    elif alias=='rust':sample_for('solana:localnet',root/alias/'fixtures',chain.timestamp*1000)
    else:cfg['routing'].update(max_state_age_ms=180000,quote_ttl_ms=60000)
    configs[alias]=save_config(root/alias/'config',cfg)
   def base(a):return f'http://127.0.0.1:{ports[a]}'
   def start(a):
    name=names[a];logs[a]=(OUT/f'{prefix}-{a}.log').open('ab');processes[a]=subprocess.Popen(service_command(name,configs[a],root/a/'fixtures' if a=='rust' else None),cwd=ROOT/name,stdout=logs[a],stderr=subprocess.STDOUT)
   def stop(a):
    p=processes.pop(a,None)
    if p:
     p.terminate()
     try:p.wait(timeout=10)
     except subprocess.TimeoutExpired:p.kill();p.wait()
    if a in logs:logs.pop(a).close()
   def status(a):return http(base(a)+'/internal/chains/evm:31337/status',token)
   def check(a,name,zero=True):
    p,_=profile(name);body={'chainKey':'evm:31337','kind':'EXACT_IN','sellToken':p['token0' if zero else 'token1'],'buyToken':p['token1' if zero else 'token0'],'sellAmount':'10000','caller':p['token0'],'recipient':p['token1'],'slippageBps':50,'verification':'FAST'}
    code,value=http(base(a)+'/v1/quote',token,body);require(code==200, (name,a,code,value));require(value['grossBuyAmount']==oracle(name,zero), (name,value['grossBuyAmount'],oracle(name,zero)));require(value['consumedIn']=='10000' and value['refund']=='0' and value['verification']['level']=='INDICATIVE', 'acceptance condition failed');require(value['routes'][0]['path']['edges'][0]['protocol']==name, 'acceptance condition failed');return value
   def await_head(height):
    for a in ['router1','router2']:await_condition(lambda:status(a)[1].get('snapshot_ref',{}).get('block',{}).get('number_or_slot')==height,'multiDEX canonical block not visible')
   try:
    for a in ['go','rust']:start(a);wait(base(a)+'/health/ready')
    for a in ['router1','router2']:start(a);wait(base(a)+'/health/live');wait_scope(base(a),token,'evm:31337');wait_scope(base(a),token,'solana:localnet')
    for name in NAMES:
     for a in ['router1','router2']:check(a,name,True);check(a,name,False)
    report['checks'].append('six-protocol-version-scopes-at-one-block-consumed-by-two-complete-replicas')
    report['checks'].append('pancake-v1-v2-and-sushi-v2-fixed-fees-match-independent-oracles-in-both-directions')
    report['checks'].append('pancake-v3-uint32-feeProtocol-and-tick-math-match-pinned-official-sdk')
    baseline=http(base('go')+'/internal/chains/evm:31337/baseline',token)[1];pools={p['protocol']:p for p in baseline['pools']};require(len(pools)==6, 'acceptance condition failed')
    for name in ['pancake-v1','pancake-v2','sushiswap-v2']:require(pools[name]['v2']['factory']==profile(name)[0]['v2']['expected_factory'], 'acceptance condition failed')
    require(pools['pancake-v3']['v3']['fee_protocol']==209718400, 'acceptance condition failed')
    req=urllib.request.Request(base('go')+'/internal/chains/evm:31337/bootstrap',headers={'Authorization':'Bearer '+token})
    with urllib.request.urlopen(req,timeout=3) as response:raw=response.read()
    decoded=subprocess.run(['protoc','--proto_path='+str(ROOT/'common/protoc/proto'),'--decode=dex.v1.OwnerBootstrap',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=raw,capture_output=True,check=True).stdout.decode();require('v2 {' in decoded and 'fee_protocol: 209718400' in decoded, 'acceptance condition failed');report['checks'].append('protobuf-preserves-versioned-v2-identity-and-wide-pancake-v3-protocol-fee')
    chain.head=101;await_head(101);chain.head=102;await_head(102);require(chain.receipts>=2, 'acceptance condition failed');report['checks'].append('complete-receipts-and-state-dependencies-for-all-version-scopes-share-canonical-commit')
    require(chain.witness_reads>=2, 'second source never supplied complete receipts')
    report['checks'].append('both-endpoints-reproduce-complete-receipts-under-same-canonical-block')
    for fault,primary_fault,witness_fault,expected_error in faults:
     before={a:check(a,'pancake-v3')for a in ['router1','router2']}
     parser_before=http(base('go')+'/internal/chains/evm:31337/baseline',token)[1]['reference']
     logpath=OUT/(prefix+'-go.log');logoffset=logpath.stat().st_size
     chain.primary_bad=primary_fault;chain.witness_bad=witness_fault;chain.head+=1
     await_condition(lambda:expected_error in logpath.read_bytes()[logoffset:].decode(),'actual receipt rejection path was not exercised: '+expected_error,timeout=20)
     def journal_tail():return http(base('go')+'/internal/chains/evm:31337/journal?after='+str(parser_before['journal_seq']),token)[1]
     await_condition(lambda:any(record['kind']=='ONLINE_SCOPE_REVOKED'for record in journal_tail()),'receipt disagreement did not durably revoke old scope')
     records=journal_tail()
     require(not any(record['kind']=='BLOCK_COMMIT'and record['reference']['block']['number_or_slot']==chain.head for record in records), 'unqualified receipt block was published')
     if fault=='block-parent':
      for router in before:await_condition(lambda:status(router)[0]==503,'unconfirmed current-state source stayed ready');quote(base(router),token,'solana:localnet','B')
      chain.witness_bad=None
     # HEAD_FIRST may legitimately restore a current pool-state baseline when
     # both sources agree on the block, even while historical receipts differ.
     # It must invalidate old handles and never fabricate a complete fact block.
     for router in before:
      await_condition(lambda:status(router)[1].get('snapshot_ref',{}).get('online_generation',0)>before[router]['onlineGeneration'],'qualified current-state baseline did not restore generation',timeout=55)
      fresh=check(router,'pancake-v3');require(fresh['snapshotRef']['block']['number_or_slot']==chain.head, 'acceptance condition failed')
      code,error=http(base(router)+f'/v1/quotes/{before[router]["quoteId"]}/build',token,{},'POST');require(code==409 and error['error']=='SNAPSHOT_REORGED', 'acceptance condition failed')
      quote(base(router),token,'solana:localnet','B')
     records=journal_tail()
     require(any(record['kind']=='ONLINE_BASELINE_COMMIT'for record in records), 'acceptance condition failed')
     require(not any(record['kind']=='BLOCK_COMMIT'and record['reference']['block']['number_or_slot']==chain.head for record in records), 'acceptance condition failed')
     chain.primary_bad=None;chain.witness_bad=None;chain.head+=1;await_head(chain.head)
     for router in before:check(router,'pancake-v3')
     require(any(record['kind']=='BLOCK_COMMIT'and record['reference']['block']['number_or_slot']==chain.head for record in journal_tail()), 'repaired complete block did not resume canonical commits')
     report['checks'].append('receipt-'+fault+'-disagreement-invalidates-both-old-handles-no-bad-commit-and-current-baseline-restores')
    old=check('router1','pancake-v1');stop('router1');start('router1');wait(base('router1')+'/health/live');wait_scope(base('router1'),token,'evm:31337')
    for name in NAMES:check('router1',name)
    require(check('router1','pancake-v1')['onlineGeneration']==old['onlineGeneration'], 'acceptance condition failed');report['checks'].append('memory-only-router-rebuilds-current-scope-after-witness-recovery')
    report['witness_receipt_reads']=chain.witness_reads
    report['pinned_rpc_reads']=chain.pinned;report['receipt_reads']=chain.receipts;report['passed']=True
   finally:
    for a in list(processes):stop(a)
 finally:
  for s in servers:
   s.shutdown();s.server_close();s.upstream.shutdown();s.upstream.server_close()
 report.update(measured_at_unix_ms=int(time.time()*1000),tool_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest());return report
if __name__=='__main__':
 OUT.mkdir(parents=True,exist_ok=True)
 try:result=run()
 except Exception as e:
  (OUT/'receipt-witness-three-service-smoke.json').write_text(json.dumps({'passed':False,'production_ready':False,'error_type':type(e).__name__,'error':str(e)},indent=2)+'\n');raise
 (OUT/'receipt-witness-three-service-smoke.json').write_text(json.dumps(result,indent=2)+'\n');print(json.dumps(result,indent=2))
