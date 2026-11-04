#!/usr/bin/env python3
"""Executed-reference stable models -> real canonical Kafka -> quote replicas."""
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
import json,hashlib,copy,secrets,subprocess,tempfile,time,threading,urllib.request
from receipt_reference import EMPTY_ROOT, SINGLE_EMPTY_ROOT, SINGLE_EMPTY_FIELDS
from infrastructure import ROOT,Infrastructure,port
from configuration import config_for,save_config,service_command,sample_for
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import http,quote,wait_scope
OUT=ROOT/'integration/artifacts/local';E=10**18
NAMES=['aerodrome-v2-volatile','aerodrome-v2-stable','curve-plain2-basic','balancer-v2-stable2','aerodrome-slipstream']
GOLDEN=json.loads((ROOT/'integration/test/contracts/stable-quotes.json').read_text());V3=json.loads((ROOT/'integration/test/contracts/v3-state.json').read_text());V3QUOTES=json.loads((ROOT/'integration/test/contracts/v3-quotes.json').read_text());GENESIS='0x'+'99'*32

def address(n):return f'0x{n:040x}'
def word(n):return f'{n%(1<<256):064x}'
def code(n):return bytes([0x60,n,0x60,0,0x55])
def digest(n):return hashlib.sha256(code(n)).hexdigest()
def fixture(name):
 if name=='aerodrome-slipstream':
  p=copy.deepcopy(V3);p['protocol']=name;p['adapter_version']=name+'/cl-1';p['v3']['factory_code_sha256']=digest(25);return p
 return copy.deepcopy(next(c['pool'] for c in GOLDEN['cases'] if c['pool']['protocol']==name and c['pool']['stable']['rate0']==str(E) and c['pool']['stable']['rate1']==str(E) and (name not in ['curve-plain2-basic','balancer-v2-stable2'] or c['pool']['stable']['amplification']==('10000' if name=='curve-plain2-basic' else '100000'))))
def profile(name):
 i=NAMES.index(name)+1;p=fixture(name);p['pool_id']=address(i);p['token0']=address(i*10+1);p['token1']=address(i*10+2)
 # Nonzero input sizes and meaningful balances also permit allocation replay.
 if name.startswith('aerodrome-v2'):p['reserve0']=str(1000000*E);p['reserve1']=str(1000000*E)
 cfg={'id':p['pool_id'],'protocol':name,'token0':p['token0'],'token1':p['token1'],'fee_numerator':p['fee_numerator'],'fee_denominator':p['fee_denominator'],'program_id':'','accounts':[]}
 if name=='aerodrome-slipstream':
  p['v3']['factory']=address(100+i);p['v3']['code_sha256']=digest(i);p['v3']['factory_code_sha256']=digest(20+i)
  cfg['v3']={'expected_factory':address(100+i),'expected_code_sha256':digest(i),'expected_factory_code_sha256':digest(20+i),'tick_word_radius':1,'max_initialized_ticks':32,'max_steps':512}
 else:
  s=p['stable'];s['code_sha256']=digest(i);cfg['stable']={'expected_code_sha256':digest(i)}
  if name.startswith('aerodrome-v2'):s['factory']=address(100+i);s['factory_code_sha256']=digest(20+i);cfg['stable'].update(expected_factory=s['factory'],expected_factory_code_sha256=s['factory_code_sha256'])
  if name=='balancer-v2-stable2':s['vault']=address(100+i);s['vault_code_sha256']=digest(20+i);s['pool_id']=p['pool_id']+'00'*12;cfg['stable'].update(expected_vault=s['vault'],expected_vault_code_sha256=s['vault_code_sha256'],pool_id=s['pool_id'])
 return p,cfg

def expected(name,input,zero):
 if name=='aerodrome-slipstream':return next(c['expected']['amount_out'] for c in V3QUOTES['cases'] if c['name']==f'case-0-{str(zero).lower()}-{input}')
 pool,_=profile(name)
 return next(c['expected']['amount_out'] for c in GOLDEN['cases'] if c['pool']['protocol']==name and c['input']==input and c['zero_for_one']==zero and c['pool']['reserve0']==pool['reserve0'] and c['pool']['reserve1']==pool['reserve1'] and c['pool']['stable']['rate0']==pool['stable']['rate0'] and c['pool']['stable']['rate1']==pool['stable']['rate1'] and c['pool']['stable']['amplification']==pool['stable']['amplification'])

class Chain:
 def __init__(self):self.head=100;self.timestamp=int(time.time());self.branch=0;self.bad=None;self.pinned=0;self.receipts=0
 def block(self,n):
  def h(x):return '0x'+hashlib.sha256(f'{self.branch if x>=101 else 0}/{x}'.encode()).hexdigest()
  return {'number':hex(n),'hash':GENESIS if n==0 else h(n),'parentHash':h(max(0,n-1)),'timestamp':hex(self.timestamp),'receiptsRoot':SINGLE_EMPTY_ROOT if n else EMPTY_ROOT,'transactions':[] if n==0 else ['0x'+hashlib.sha256(('tx/'+h(n)).encode()).hexdigest()]}
 def provider(self):
  chain=self;catalog={address(i+1):profile(n) for i,n in enumerate(NAMES)}
  class Handler(BaseHTTPRequestHandler):
   def log_message(self,*_):pass
   def do_POST(self):
    q=json.loads(self.rfile.read(int(self.headers['Content-Length'])));method=q['method'];params=q['params'];out=None;error=None
    if method=='eth_chainId':out='0x7a69'
    elif method=='eth_getBlockByNumber':out=chain.block(chain.head if params[0] in ['latest','safe','finalized'] else int(params[0],16))
    elif method=='eth_getTransactionReceipt':
     block=next((chain.block(n) for n in range(1,chain.head+1) if params[0] in chain.block(n)['transactions']),None)
     if block:out={'blockHash':block['hash'],'transactionHash':params[0],'transactionIndex':'0x0','logs':[],**SINGLE_EMPTY_FIELDS};chain.receipts+=1
    else:
     anchor=params[2] if method=='eth_getStorageAt' else params[1]
     identities={chain.block(n)['hash'] for n in range(chain.head+1)}
     if anchor.get('requireCanonical') is not True or anchor.get('blockHash') not in identities:error={'code':-32000,'message':'not canonical'}
     else:
      chain.pinned+=1;target=params[0] if method!='eth_call' else params[0]['to'];isdep=100<int(target,16)<110;pooladdr=address(int(target,16)-100) if isdep else target;p,cfg=catalog[pooladdr];idx=NAMES.index(p['protocol'])+1;name=p['protocol'];s=p.get('stable',{});v=p.get('v3',{});pad=lambda a:a[2:].rjust(64,'0')
      if method=='eth_getCode':out='0x'+code(20+idx if isdep else idx).hex()
      elif method=='eth_getStorageAt':out='0x'+word(0 if chain.bad=='curve-rate' else int(s['rate0' if int(params[1],16)==11 else 'rate1']))
      elif method=='eth_call':
       data=params[0]['data'];sel=data[:10];args=data[10:];selmap={'0xb187bd26':'paused','0x392f37e9':'metadata','0xcc56b2c5':'fee','0xc6610657':'coins','0x4903b0d1':'balances','0xddca3f43':'fee-simple','0x76a2f0f0':'amp','0xfee3f7f9':'admin','0x8d928af8':'vault','0x38fff2d0':'poolid','0xf6c00927':'poolreg','0x1c0de051':'pausestate','0x6daccffa':'balamp','0x1dd746ea':'scales','0xf94d4668':'pooltokens','0x55c67628':'swapfee'};which=selmap.get(sel)
       if sel=='0xc45a0155':out='0x'+pad(s.get('factory',v.get('factory')))
       elif sel=='0x0dfe1681':out='0x'+pad(p['token0'])
       elif sel=='0xd21220a7':out='0x'+pad(p['token1'])
       elif which=='paused':out='0x'+word(int(chain.bad=='aero-pause'))
       elif which=='metadata':out='0x'+''.join(word(x) for x in [int(s['rate0']),int(s['rate1']),int(p['reserve0']),int(p['reserve1']),int(name.endswith('stable'))])+pad(p['token0'])+pad(p['token1'])
       elif which in ['fee','fee-simple','swapfee']:out='0x'+word(p['fee_numerator'])
       elif which=='coins':out='0x'+pad(p['token0' if int(args,16)==0 else 'token1'])
       elif which=='balances':out='0x'+word(int(p['reserve0' if int(args,16)==0 else 'reserve1']))
       elif which=='amp':out='0x'+word(int(s['amplification']))
       elif which=='admin':out='0x'+word(s['admin_fee'])
       elif which=='vault':out='0x'+pad(s['vault'])
       elif which=='poolid':out=s['pool_id']
       elif which=='poolreg':out='0x'+pad(p['pool_id'])+word(2)
       elif which=='pausestate':out='0x'+word(int(chain.bad=='bal-pool-pause' and target==p['pool_id'] or chain.bad=='bal-vault-pause' and target==s['vault']))+word(0)+word(0)
       elif which=='balamp':out='0x'+word(int(s['amplification']))+word(0)+word(1000)
       elif which=='scales':out='0x'+word(32)+word(2)+word(int(s['rate0']))+word(int(s['rate1']))
       elif which=='pooltokens':out='0x'+word(96)+word(192)+word(chain.head)+word(2)+pad(p['token0'])+pad(p['token1'])+word(2)+word(int(p['reserve0']))+word(int(p['reserve1']))
       elif sel=='0xd0c93a7c':out='0x'+word(v['tick_spacing'])
       elif sel=='0x1a686502':out='0x'+word(int(v['liquidity']))
       elif sel=='0x3850c7bd':out='0x'+''.join(word(x) for x in [int(v['sqrt_price_x96']),v['tick'],0,1,1,1])
       elif sel=='0x5339c296':
        pos=int(args,16);pos-=1<<256 if pos&(1<<255) else 0;out='0x'+word(next((int(w['bitmap']) for w in v['tick_words'] if w['word_pos']==pos),0))
       elif sel=='0xf30dba93':
        tick=int(args,16);tick-=1<<256 if tick&(1<<255) else 0;t=next(t for t in v['ticks'] if t['tick']==tick);out='0x'+''.join(word(x) for x in [int(t['liquidity_gross']),int(t['liquidity_net']),0,0,0,0,0,0,0,1])
       else:
        # Slipstream factory module getter: derive signature once from pinned
        # ABI; this synthetic domain permits only zero external module.
        from eth_selectors import SWAP_FEE_MODULE,UNSTAKED_FEE_MODULE
        if sel in [SWAP_FEE_MODULE,UNSTAKED_FEE_MODULE]:out='0x'+word(int(chain.bad=='slip-swap-module' and sel==SWAP_FEE_MODULE or chain.bad=='slip-unstaked-module' and sel==UNSTAKED_FEE_MODULE))
        else:error={'code':-32601,'message':'unsupported selector '+sel}
      else:error={'code':-32601,'message':'unsupported RPC'}
    body={'jsonrpc':'2.0','id':q['id']};body.update({'error':error} if error else {'result':out});raw=json.dumps(body).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(raw)));self.end_headers();self.wfile.write(raw)
  server=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=server.serve_forever,daemon=True).start();return server

def run():
 report={'profile':'executed-reference-stable-and-slipstream-real-pg-kafka','production_ready':False,'protocols':NAMES,'checks':[]};chain=Chain();servers=[chain.provider(),chain.provider()];processes={};logs={}
 try:
  with Infrastructure() as infra,tempfile.TemporaryDirectory(prefix='xone-stable-') as folder:
   chain.timestamp=int(time.time());root=Path(folder);token=secrets.token_urlsafe(32);names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'};ports={a:port() for a in names};configs={};sources=[f'http://127.0.0.1:{ports[a]}' for a in ['go','rust']]
   for a,name in names.items():
    c=config_for(name,root/a,token,ports[a]);c['journal'].update(transport='kafka',source_urls=sources,poll_interval_ms=100,request_timeout_ms=15000);c['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=a,transaction_timeout_ms=4000);c['control'].update(backend='disabled' if name=='dex-router' else 'postgres',dsn_file='' if name=='dex-router' else str(infra.dsnfile),instance_id=a,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000);c['startup']['max_baseline_age_ms']=180000
    if a=='go':c['chains'][0].update(genesis_hash=GENESIS,confirmation_tag='latest',auto_reorg_window=4,rpc_urls=[f'http://127.0.0.1:{s.server_port}' for s in servers],pools=[profile(n)[1] for n in NAMES])
    elif a=='rust':sample_for('solana:localnet',root/a/'fixtures',chain.timestamp*1000)
    else:c['routing'].update(max_state_age_ms=180000,quote_ttl_ms=60000)
    configs[a]=save_config(root/a/'config',c)
   def base(a):return f'http://127.0.0.1:{ports[a]}'
   def start(a):
    name=names[a];logs[a]=(OUT/f'stable-{a}.log').open('ab');processes[a]=subprocess.Popen(service_command(name,configs[a],root/a/'fixtures' if a=='rust' else None),cwd=ROOT/name,stdout=logs[a],stderr=subprocess.STDOUT)
   def stop(a):
    p=processes.pop(a,None)
    if p:
     p.terminate()
     try:p.wait(timeout=10)
     except subprocess.TimeoutExpired:p.kill();p.wait()
    if a in logs:logs.pop(a).close()
   def status(a):return http(base(a)+'/internal/chains/evm:31337/status',token)
   def check(a,name,zero=True):
    p,_=profile(name);amount='10000' if name=='aerodrome-slipstream' else str(10**19 if name.startswith('aerodrome') else 10**10 if name=='balancer-v2-stable2' else 10**9)
    request={'chainKey':'evm:31337','kind':'EXACT_IN','sellToken':p['token0' if zero else 'token1'],'buyToken':p['token1' if zero else 'token0'],'sellAmount':amount,'caller':p['token0'],'recipient':p['token1'],'slippageBps':50,'verification':'FAST'}
    code,result=http(base(a)+'/v1/quote',token,request);assert code==200,(name,code,result);assert result['grossBuyAmount']==expected(name,amount,zero),(name,result['grossBuyAmount'],expected(name,amount,zero));assert result['consumedIn']==amount and result['refund']=='0' and result['verification']['level']=='INDICATIVE';return result
   try:
    for a in ['go','rust']:start(a);wait(base(a)+'/health/ready')
    for a in ['router1','router2']:start(a);wait(base(a)+'/health/live');wait_scope(base(a),token,'evm:31337')
    for name in NAMES:
     for a in ['router1','router2']:check(a,name);check(a,name,False)
    report['checks'].append('aero-volatile-stable-curve-plain2-balancer-stable2-slipstream-at-one-block-in-two-replicas')
    report['checks'].append('stable-amounts-match-executed-primary-evm-oracles-both-directions')
    snap=http(base('go')+'/internal/chains/evm:31337/baseline',token)[1];assert len(snap['pools'])==5;assert all(p.get('stable') for p in snap['pools'] if p['protocol']!='aerodrome-slipstream');report['checks'].append('protocol-specific-scales-amplification-fee-and-vault-identity-cross-public-wire')
    for h in [101,102]:
     chain.head=h
     for a in ['router1','router2']:await_condition(lambda:status(a)[1].get('snapshot_ref',{}).get('block',{}).get('number_or_slot')==h,'stable block not committed')
    old={a:check(a,NAMES[1]) for a in ['router1','router2']};chain.branch=1
    for a in old:await_condition(lambda:status(a)[1].get('snapshot_ref',{}).get('canonical_epoch',0)>old[a]['snapshotRef']['canonical_epoch'],'stable reorg failed');check(a,NAMES[1])
    report['checks'].append('stable-model-states-reorg-atomically-with-shared-canonical-epoch')
    for fault in ['aero-pause','curve-rate','bal-pool-pause','bal-vault-pause','slip-swap-module','slip-unstaked-module']:
     old={a:check(a,NAMES[1]) for a in ['router1','router2']};chain.bad=fault;chain.head+=1
     for a in old:await_condition(lambda:status(a)[0]==503,'incomplete stable dependency not revoked');quote(base(a),token,'solana:localnet','B')
     chain.bad=None
     for a in old:await_condition(lambda:status(a)[1].get('snapshot_ref',{}).get('online_generation',0)>old[a]['onlineGeneration'],'qualified stable scope not restored');check(a,NAMES[1])
     report['checks'].append('unsupported-'+fault+'-revokes-affected-scope-and-qualified-baseline-restores')
    report['passed']=True;report['pinned_rpc_reads']=chain.pinned;report['receipt_reads']=chain.receipts
   finally:
    for a in list(processes):stop(a)
 finally:
  for s in servers:s.shutdown();s.server_close()
 report.update(measured_at_unix_ms=int(time.time()*1000),tool_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest());return report
if __name__=='__main__':
 OUT.mkdir(parents=True,exist_ok=True)
 try:r=run()
 except Exception as e:(OUT/'stable-three-service-smoke.json').write_text(json.dumps({'passed':False,'production_ready':False,'error':str(e),'error_type':type(e).__name__},indent=2)+'\n');raise
 (OUT/'stable-three-service-smoke.json').write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r,indent=2))
