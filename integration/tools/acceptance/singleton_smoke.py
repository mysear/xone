#!/usr/bin/env python3
"""V4/Infinity independent RPC layouts -> real PG/Kafka -> two complete routers."""
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
import json,hashlib,copy,secrets,subprocess,tempfile,time,threading
from receipt_reference import EMPTY_ROOT, SINGLE_EMPTY_ROOT, SINGLE_EMPTY_FIELDS
from infrastructure import ROOT,Infrastructure,port
from configuration import config_for,save_config,service_command,sample_for
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import http,quote,wait_scope
OUT=ROOT/'integration/artifacts/local'
DATA=json.loads((ROOT/'integration/test/evm/singleton-baseline.json').read_text());GOLDEN=json.loads((ROOT/'integration/test/contracts/singleton-quotes.json').read_text());NAMES=['uniswap-v4','pancake-infinity-cl'];GENESIS='0x'+'99'*32

def word(n):return f'{n%(1<<256):064x}'
def code(n):return bytes([0x60,n,0x60,0,0x55])
def profile(name):
 p=copy.deepcopy(next(p for p in DATA['pools'] if p['protocol']==name));s=p['singleton_cl'];c=s['core']
 config={'id':p['pool_id'],'protocol':name,'token0':p['token0'],'token1':p['token1'],'fee_numerator':p['fee_numerator'],'fee_denominator':1000000,'program_id':'','accounts':[],'singleton_cl':{'manager':s['manager'],'expected_manager_code_sha256':s['manager_code_sha256'],'pool_id':s['pool_id'],'lp_fee':s['lp_fee'],'tick_spacing':c['tick_spacing'],'parameters':s['parameters'],'tick_word_radius':1,'max_initialized_ticks':32,'max_steps':512}}
 return p,config

def expected(name,amount,zero):return next(c['expected']['amount_out'] for c in GOLDEN['cases'] if c['name']==f'{name}/directional-fee/{str(zero).lower()}/{amount}')

class Chain:
 def __init__(self):self.head=100;self.timestamp=int(time.time());self.branch=0;self.bad=None;self.pinned=0;self.receipts=0
 def block(self,n):
  def h(x):return '0x'+hashlib.sha256(f'{self.branch if x>=101 else 0}/{x}'.encode()).hexdigest()
  return {'number':hex(n),'hash':GENESIS if n==0 else h(n),'parentHash':h(max(0,n-1)),'timestamp':hex(self.timestamp),'receiptsRoot':SINGLE_EMPTY_ROOT if n else EMPTY_ROOT,'transactions':[] if n==0 else ['0x'+hashlib.sha256(('tx/'+h(n)).encode()).hexdigest()]}
 def provider(self):
  chain=self;catalog={p['singleton_cl']['manager']:p for p in DATA['pools']};selectors=DATA['selectors']
  class Handler(BaseHTTPRequestHandler):
   def log_message(self,*args):pass
   def do_POST(self):
    q=json.loads(self.rfile.read(int(self.headers['Content-Length'])));method=q['method'];params=q['params'];out=None;error=None
    if method=='eth_chainId':out='0x7a69'
    elif method=='eth_getBlockByNumber':out=chain.block(chain.head if params[0] in ['latest','safe','finalized'] else int(params[0],16))
    elif method=='eth_getTransactionReceipt':
     block=next((chain.block(n) for n in range(1,chain.head+1) if params[0] in chain.block(n)['transactions']),None)
     if block:out={'blockHash':block['hash'],'transactionHash':params[0],'transactionIndex':'0x0','logs':[],**SINGLE_EMPTY_FIELDS};chain.receipts+=1
    elif method in ['eth_call','eth_getCode']:
     anchor=params[1]
     if anchor.get('requireCanonical') is not True or anchor.get('blockHash') not in {chain.block(n)['hash'] for n in range(chain.head+1)}:error={'code':-32000,'message':'not canonical'}
     else:
      chain.pinned+=1;target=params[0] if method=='eth_getCode' else params[0]['to'];p=catalog[target];s=p['singleton_cl'];c=s['core'];isv4=p['protocol']=='uniswap-v4';pad=lambda a:a[2:].rjust(64,'0')
      if method=='eth_getCode':out='0x'+code(1 if isv4 else 2).hex() if chain.bad!='v4-code' or not isv4 else '0x00'
      else:
       data=params[0]['data'];sel=data[:10];args=data[10:]
       if isv4 and sel==selectors['extsload(bytes32)']:
        key='0x'+args;out=DATA['v4']['storage'].get(key,'0x'+word(0))
        if chain.bad=='v4-lp-fee' and key==DATA['v4']['base']:out='0x'+word(int(out,16)+(1<<208))
        elif chain.bad=='v4-missing-tick' and key in DATA['v4']['tick_keys']:out='0x'+word(0)
       elif not isv4:
        if sel==selectors['paused()']:out='0x'+word(int(chain.bad=='infinity-pause'))
        elif sel==selectors['poolIdToPoolKey(bytes32)']:
         hook=s['hooks'] if chain.bad!='infinity-hook' else p['token0'];parameters=int(s['parameters'],16)+(1 if chain.bad=='infinity-parameters' else 0);out='0x'+pad(p['token0'])+pad(p['token1'])+pad(hook)+pad(s['manager'])+word(s['lp_fee'])+word(parameters)
        elif sel==selectors['getSlot0(bytes32)']:
         fee0=4001 if chain.bad=='infinity-protocol-fee' else s['protocol_fee0'];out='0x'+word(int(c['sqrt_price_x96']))+word(c['tick'])+word(fee0|(s['protocol_fee1']<<12))+word(s['lp_fee'])
        elif sel==selectors['getLiquidity(bytes32)']:out='0x'+word(int(c['liquidity']))
        elif sel==selectors['getPoolBitmapInfo(bytes32,int16)']:
         pos=int(args[64:],16);pos-=1<<256 if pos&(1<<255) else 0;out='0x'+word(next((int(w['bitmap']) for w in c['tick_words'] if w['word_pos']==pos),0))
        elif sel==selectors['getPoolTickInfo(bytes32,int24)']:
         index=int(args[64:],16);index-=1<<256 if index&(1<<255) else 0;t=next(t for t in c['ticks'] if t['tick']==index);out='0x'+word(int(t['liquidity_gross']))+word(int(t['liquidity_net']))+word(0)+word(0)
        else:error={'code':-32601,'message':'unsupported Infinity selector '+sel}
       else:error={'code':-32601,'message':'unsupported V4 selector '+sel}
    else:error={'code':-32601,'message':'unsupported RPC '+method}
    body={'jsonrpc':'2.0','id':q['id']};body.update({'error':error} if error else {'result':out});raw=json.dumps(body).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
  server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start();return server

def run():
 report={'profile':'pinned-v4-infinity-cl-real-pg-kafka','production_ready':False,'protocols':NAMES,'checks':[]};chain=Chain();servers=[chain.provider(),chain.provider()];processes={};logs={}
 try:
  with Infrastructure() as infra,tempfile.TemporaryDirectory(prefix='xone-singleton-') as folder:
   chain.timestamp=int(time.time());root=Path(folder);token=secrets.token_urlsafe(32);names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'};ports={a:port() for a in names};configs={};sources=[f'http://127.0.0.1:{ports[a]}' for a in ['go','rust']]
   for a,name in names.items():
    c=config_for(name,root/a,token,ports[a]);c['journal'].update(transport='kafka',source_urls=sources,poll_interval_ms=100,request_timeout_ms=15000);c['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=a,transaction_timeout_ms=4000);c['control'].update(backend='disabled' if name=='dex-router' else 'postgres',dsn_file='' if name=='dex-router' else str(infra.dsnfile),instance_id=a,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000);c['startup']['max_baseline_age_ms']=180000
    if a=='go':c['chains'][0].update(genesis_hash=GENESIS,confirmation_tag='latest',auto_reorg_window=4,rpc_urls=[f'http://127.0.0.1:{s.server_port}' for s in servers],pools=[profile(n)[1] for n in NAMES])
    elif a=='rust':sample_for('solana:localnet',root/a/'fixtures',chain.timestamp*1000)
    else:c['routing'].update(max_state_age_ms=180000,quote_ttl_ms=60000)
    configs[a]=save_config(root/a/'config',c)
   def base(a):return f'http://127.0.0.1:{ports[a]}'
   def start(a):
    name=names[a];logs[a]=(OUT/f'singleton-{a}.log').open('ab');processes[a]=subprocess.Popen(service_command(name,configs[a],root/a/'fixtures' if a=='rust' else None),cwd=ROOT/name,stdout=logs[a],stderr=subprocess.STDOUT)
   def stop(a):
    p=processes.pop(a,None)
    if p:
     p.terminate()
     try:p.wait(timeout=10)
     except subprocess.TimeoutExpired:p.kill();p.wait()
    if a in logs:logs.pop(a).close()
   def status(a):return http(base(a)+'/internal/chains/evm:31337/status',token)
   def check(a,name,zero=True,amount='1000000'):
    p,_=profile(name)
    request={'chainKey':'evm:31337','kind':'EXACT_IN','sellToken':p['token0' if zero else 'token1'],'buyToken':p['token1' if zero else 'token0'],'sellAmount':amount,'caller':p['token0'],'recipient':p['token1'],'slippageBps':50,'verification':'FAST'}
    code,result=http(base(a)+'/v1/quote',token,request);assert code==200,(name,code,result);assert result['grossBuyAmount']==expected(name,amount,zero),(name,result['grossBuyAmount'],expected(name,amount,zero));assert result['consumedIn']==amount and result['refund']=='0' and result['verification']['level']=='INDICATIVE';return result
   try:
    for a in ['go','rust']:start(a);wait(base(a)+'/health/ready')
    for a in ['router1','router2']:start(a);wait(base(a)+'/health/live');wait_scope(base(a),token,'evm:31337')
    for name in NAMES:
     for a in ['router1','router2']:
      for amount in ['1000000','20000000']:check(a,name,True,amount);check(a,name,False,amount)
    report['checks'].append('v4-and-infinity-cl-same-block-in-two-complete-replicas')
    report['checks'].append('directional-protocol-fees-and-amounts-match-original-pool-swap-both-directions')
    snap=http(base('go')+'/internal/chains/evm:31337/baseline',token)[1];assert len(snap['pools'])==2
    assert all(p.get('singleton_cl') and not p.get('v3') and len(p['singleton_cl']['core']['ticks'])==4 for p in snap['pools']);assert {p['pool_id'] for p in snap['pools']}=={profile(n)[0]['pool_id'] for n in NAMES}
    report['checks'].append('typed-singleton-key-manager-and-complete-core-preserved-across-protobuf')
    for h in [101,102]:
     chain.head=h
     for a in ['router1','router2']:await_condition(lambda:status(a)[1].get('snapshot_ref',{}).get('block',{}).get('number_or_slot')==h,'stable block not committed')
    old={a:check(a,NAMES[1]) for a in ['router1','router2']};chain.branch=1
    for a in old:await_condition(lambda:status(a)[1].get('snapshot_ref',{}).get('canonical_epoch',0)>old[a]['snapshotRef']['canonical_epoch'],'stable reorg failed');check(a,NAMES[1])
    report['checks'].append('singleton-states-reorg-atomically-with-shared-canonical-epoch')
    for fault in ['v4-code','v4-lp-fee','v4-missing-tick','infinity-pause','infinity-hook','infinity-parameters','infinity-protocol-fee']:
     old={a:check(a,NAMES[1]) for a in ['router1','router2']};chain.bad=fault;chain.head+=1
     for a in old:await_condition(lambda:status(a)[0]==503,'incomplete stable dependency not revoked');quote(base(a),token,'solana:localnet','B')
     chain.bad=None
     for a in old:await_condition(lambda:status(a)[1].get('snapshot_ref',{}).get('online_generation',0)>old[a]['onlineGeneration'],'qualified stable scope not restored');check(a,NAMES[1])
     report['checks'].append('unsupported-'+fault+'-revokes-affected-scope-and-qualified-baseline-restores')
    stop('router1');start('router1');wait(base('router1')+'/health/live');wait_scope(base('router1'),token,'evm:31337')
    for n in NAMES:check('router1',n);check('router1',n,False)
    report['checks'].append('memory-only-router-restores-current-singleton-scope-from-owner-checkpoint-and-tail')
    report['passed']=True;report['pinned_rpc_reads']=chain.pinned;report['receipt_reads']=chain.receipts
   finally:
    for a in list(processes):stop(a)
 finally:
  for s in servers:s.shutdown();s.server_close()
 report.update(measured_at_unix_ms=int(time.time()*1000),tool_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest());return report
if __name__=='__main__':
 OUT.mkdir(parents=True,exist_ok=True)
 try:r=run()
 except Exception as e:(OUT/'singleton-three-service-smoke.json').write_text(json.dumps({'passed':False,'production_ready':False,'error':str(e),'error_type':type(e).__name__},indent=2)+'\n');raise
 (OUT/'singleton-three-service-smoke.json').write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r,indent=2))
