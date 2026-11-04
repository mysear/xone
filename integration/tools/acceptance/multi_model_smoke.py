#!/usr/bin/env python3
"""Original-reference multi-pool snapshots -> normal Go RPC/PG/Kafka/two routers."""
from pathlib import Path
from http.server import BaseHTTPRequestHandler,ThreadingHTTPServer
import copy,json,hashlib,secrets,subprocess,tempfile,threading,time
from infrastructure import ROOT,Infrastructure,port
from configuration import config_for,save_config,service_command,sample_for
from receipt_reference import EMPTY_ROOT,SINGLE_EMPTY_ROOT,SINGLE_EMPTY_FIELDS
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import http,wait_scope
OUT=ROOT/'integration/artifacts/local';GENESIS='0x'+'99'*32

def require(v,message):
 if not v:raise ValueError(message)
def address(n):return f'0x{n:040x}'
def word(n):return f'{int(n)%(1<<256):064x}'
def code(n):return bytes([0x60,n,0x60,0,0x55])
def digest(n):return hashlib.sha256(code(n)).hexdigest()
def load(name):return json.loads((ROOT/'integration/test/contracts'/name).read_text())
def catalog():
 data=[]
 requests=[('balancer-v2-weighted-multi','balancer-multi-quotes.json','cases',3,None),('balancer-v2-weighted-multi','balancer-multi-quotes.json','cases',4,None),('balancer-v2-managed-snapshot','balancer-multi-managed.json','cases',3,None),('balancer-v2-composable-regular','balancer-composable-admitted.json','regular',3,None),('curve-plain3-basic','curve-multi-quotes.json','cases',3,'Plain3Basic'),('curve-plain4-basic','curve-multi-quotes.json','cases',4,'Plain4Basic')]
 for index,(protocol,file,key,n,model) in enumerate(requests):
  rows=load(file)[key]
  def valid(c):
   if len(c['balances'])!=n or c['expected'].get('reverted') or int(c['expected'].get('amount_out','0'))<=0 or int(c['input'])<=2:return False
   if model and c.get('model')!=model:return False
   if protocol.startswith('balancer') and not 10**12<=int(c['fee'])<=10**17:return False
   return True
  row=copy.deepcopy(next(c for c in rows if valid(c)));poolid=address(100+index);vault=address(200+index);scales=row.get('scales',row.get('rates'));bpt=row.get('bpt_index');tokens=[address(1000+index*20+i) for i in range(n)]
  if bpt is not None:
   # Keep the BPT occurrence at its original registered index and sorted order.
   poolid=address(1000+index*20+bpt);tokens[bpt]=poolid
  assets=[{'token':token,'balance':str(row['balances'][i]),'scaling':str(scales[i]),'code_sha256':digest(50+index*5+i) if token!=poolid else digest(index+1)}for i,token in enumerate(tokens)]
  state={'code_sha256':digest(index+1),'vault':vault if protocol.startswith('balancer')else'','vault_code_sha256':digest(20+index) if protocol.startswith('balancer')else'','pool_id':poolid+('0000'if bpt is not None else'0001')+'0'*19+'1' if protocol.startswith('balancer')else'','assets':assets,'weights':row.get('weights',[]),'amplification':row.get('amplification',row.get('amp','0')),'admin_fee':int(row.get('admin_fee','0'))}
  adapter={'balancer-v2-weighted-multi':'balancer-v2-weighted-multi/static-1','balancer-v2-managed-snapshot':'balancer-v2-managed-snapshot/no-bpt-fees-1','balancer-v2-composable-regular':'balancer-v2-composable-regular/no-rates-1'}.get(protocol,protocol+'/factory-1')
  p={'chain_key':'evm:31337','pool_id':poolid,'protocol':protocol,'token0':tokens[0],'token1':tokens[1],'reserve0':assets[0]['balance'],'reserve1':assets[1]['balance'],'fee_numerator':int(row['fee']),'fee_denominator':10**18 if protocol.startswith('balancer')else 10**10,'adapter_version':adapter,'token_behavior':'standard','multi_asset':state}
  cfg={'id':poolid,'protocol':protocol,'token0':tokens[0],'token1':tokens[1],'fee_numerator':p['fee_numerator'],'fee_denominator':p['fee_denominator'],'program_id':'','accounts':[],'multi_asset':{'expected_code_sha256':state['code_sha256'],'expected_vault':state['vault'],'expected_vault_code_sha256':state['vault_code_sha256'],'pool_id':state['pool_id'],'tokens':[{'token':a['token'],'expected_code_sha256':a['code_sha256']}for a in assets],'token_policy':'standard-with-own-bpt'if bpt is not None else'standard-no-bpt'}}
  data.append({'pool':p,'config':cfg,'case':row,'index':index,'bpt_index':bpt})
 return data
class Chain:
 def __init__(self):self.head=100;self.timestamp=int(time.time());self.bad=None;self.items=catalog();self.pinned=0;self.receipts=0
 def block(self,n):
  def h(i):return'0x'+hashlib.sha256(f'multi/{i}'.encode()).hexdigest()
  return{'number':hex(n),'hash':GENESIS if n==0 else h(n),'parentHash':h(max(0,n-1)),'timestamp':hex(self.timestamp),'receiptsRoot':EMPTY_ROOT if n==0 else SINGLE_EMPTY_ROOT,'transactions':[]if n==0 else['0x'+hashlib.sha256(('tx/'+h(n)).encode()).hexdigest()]}
 def advance(self,bad=None):self.head+=1;self.bad=bad;self.timestamp=int(time.time())
 def provider(self):
  chain=self
  class Handler(BaseHTTPRequestHandler):
   def log_message(self,*_):pass
   def do_POST(self):
    q=json.loads(self.rfile.read(int(self.headers['Content-Length'])));m=q['method'];args=q['params'];out=None;error=None
    try:
     if m=='eth_chainId':out='0x7a69'
     elif m=='eth_getBlockByNumber':out=chain.block(chain.head if args[0]in['latest','safe','finalized']else int(args[0],16))
     elif m=='eth_getTransactionReceipt':
      b=next((chain.block(n)for n in range(1,chain.head+1)if args[0]in chain.block(n)['transactions']),None)
      if b:out={'blockHash':b['hash'],'transactionHash':args[0],'transactionIndex':'0x0','logs':[],**SINGLE_EMPTY_FIELDS};chain.receipts+=1
     else:
      anchor=args[2]if m=='eth_getStorageAt'else args[1];require(anchor.get('requireCanonical')is True and anchor.get('blockHash')in{chain.block(n)['hash']for n in range(chain.head+1)},'not pinned');chain.pinned+=1
      target=args[0]if m!='eth_call'else args[0]['to'];item=next(x for x in chain.items if target in[x['pool']['pool_id'],x['pool']['multi_asset']['vault']]+[a['token']for a in x['pool']['multi_asset']['assets']]);p=item['pool'];s=p['multi_asset'];assets=s['assets'];n=len(assets);idx=item['index'];bad=chain.bad if idx==0 else None;pad=lambda a:a[2:].rjust(64,'0')
      if m=='eth_getCode':
       cid=idx+1 if target==p['pool_id']else 20+idx if target==s['vault']else 50+idx*5+next(i for i,a in enumerate(assets)if a['token']==target);out='0x'+code(cid).hex()
      elif m=='eth_getStorageAt':
       i=int(args[1],16)-(2*n+7);require(0<=i<n,'bad rate slot');out='0x'+word(assets[i]['scaling'])
      else:
       data=args[0]['data'];sel=data[:10]
       if sel=='0x8d928af8':out='0x'+pad(s['vault'])
       elif sel=='0x38fff2d0':out=s['pool_id']
       elif sel=='0xf6c00927':out='0x'+pad(p['pool_id'])+word(0 if item['bpt_index']is not None else 1)
       elif sel=='0x1c0de051':out='0x'+word(0)*3
       elif sel in['0xf89f27ed','0x1dd746ea']:
        values=s['weights']if sel=='0xf89f27ed'else[a['scaling']for a in assets];values=list(values)
        if bad=='weights'and sel=='0xf89f27ed':values[-1]=str(int(values[-1])-1)
        out='0x'+word(32)+word(n)+''.join(word(v)for v in values)
       elif sel=='0x313ce567':a=next(a for a in assets if a['token']==target);out='0x'+word(36-(len(a['scaling'])-1))
       elif sel=='0xf94d4668':out='0x'+word(96)+word(128+32*n)+word(chain.head)+word(n)+''.join(pad(a['token'])for a in assets)+word(n)+''.join(word(a['balance'])for a in assets)
       elif sel=='0xb05f8e48':a=next(a for a in assets if data.endswith(a['token'][2:]));out='0x'+word(a['balance'])+word(int(bad=='managed'))+word(chain.head)+word(int(bad=='manager'))
       elif sel in['0x55c67628','0xddca3f43']:out='0x'+word(p['fee_numerator'])
       elif sel=='0x47bc4d92':out='0x'+word(1)
       elif sel in['0x77eef903','0x90605f3a']:out='0x'+word(0)
       elif sel=='0x82687a56':out='0x'+word(item['bpt_index'])
       elif sel=='0x238a2d59':out='0x'+word(32)+word(n)+word(0)*n
       elif sel=='0x6daccffa':out='0x'+word(s['amplification'])+word(0)+word(1000)
       elif sel=='0xc6610657':out='0x'+pad(assets[int(data[10:],16)]['token'])
       elif sel=='0x4903b0d1':out='0x'+word(assets[int(data[10:],16)]['balance'])
       elif sel=='0x76a2f0f0':out='0x'+word(s['amplification'])
       elif sel=='0xfee3f7f9':out='0x'+word(s['admin_fee'])
       else:raise ValueError('unexpected '+sel)
    except Exception as e:error={'code':-32000,'message':str(e)}
    body={'jsonrpc':'2.0','id':q['id']};body.update({'error':error}if error else{'result':out});raw=json.dumps(body).encode();self.send_response(200);self.send_header('Content-Type','application/json');self.send_header('Content-Length',str(len(raw)));self.end_headers()
    try:self.wfile.write(raw)
    except(BrokenPipeError,ConnectionResetError):pass
  s=ThreadingHTTPServer(('127.0.0.1',0),Handler);threading.Thread(target=s.serve_forever,daemon=True).start();return s
PROCESS_CHECKS=['six-multi-asset-pools-normal-fixed-block-rpc-and-complete-protobuf-scope','all-physical-pool-assets-and-unselected-balances-match-original-reference','multi-asset-token-selected-quotes-match-original-models-in-both-replicas','unsupported-vector-dependencies-revoke-and-restore-entire-scope','memory-router-restores-full-multi-token-state','verified-and-build-remain-disabled']
def run():
 chain=Chain();servers=[chain.provider(),chain.provider()];processes={};logs={};report={'passed':False,'production_ready':False,'checks':[],'scope':'multiasset-plain3-plain4-weighted-managed-subset-composable-regular'}
 try:
  with Infrastructure()as infra,tempfile.TemporaryDirectory(prefix='xone-multi-model-')as folder:
   root=Path(folder);token=secrets.token_urlsafe(32);names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'};ports={a:port()for a in names};configs={};sources=[f'http://127.0.0.1:{ports[a]}'for a in['go','rust']];chain.timestamp=int(time.time())
   for a,name in names.items():
    c=config_for(name,root/a,token,ports[a]);c['journal'].update(transport='kafka',source_urls=sources,poll_interval_ms=100,request_timeout_ms=15000);c['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=a,transaction_timeout_ms=4000);c['control'].update(backend='disabled'if name=='dex-router'else'postgres',dsn_file=''if name=='dex-router'else str(infra.dsnfile),instance_id=a,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000);c['startup']['max_baseline_age_ms']=180000
    if a=='go':c['chains'][0].update(genesis_hash=GENESIS,confirmation_tag='latest',auto_reorg_window=4,rpc_urls=[f'http://127.0.0.1:{s.server_port}'for s in servers],pools=[i['config']for i in chain.items])
    elif a=='rust':sample_for('solana:localnet',root/a/'fixtures',chain.timestamp*1000)
    else:c['routing'].update(max_state_age_ms=180000,quote_ttl_ms=60000)
    configs[a]=save_config(root/a/'config',c)
   def base(a):return f'http://127.0.0.1:{ports[a]}'
   def start(a):logs[a]=(OUT/f'multi-model-{a}.log').open('ab');processes[a]=subprocess.Popen(service_command(names[a],configs[a],root/a/'fixtures'if a=='rust'else None),cwd=ROOT/names[a],stdout=logs[a],stderr=subprocess.STDOUT)
   def stop(a):
    p=processes.pop(a,None)
    if p:
     p.terminate()
     try:p.wait(timeout=10)
     except subprocess.TimeoutExpired:p.kill();p.wait()
    if a in logs:logs.pop(a).close()
   def status(a):return http(base(a)+'/internal/chains/evm:31337/status',token)
   def owner():
    code,s=http(base('go')+'/internal/chains/evm:31337/baseline',token);require(code==200,(code,s));return s
   def synced():
    ref=owner()['reference']
    for a in['router1','router2']:await_condition(lambda:status(a)[1].get('snapshot_ref')==ref,'replica full bank mismatch')
   def quote(a,item):
    p=item['pool'];c=item['case'];assets=p['multi_asset']['assets'];request={'chainKey':'evm:31337','kind':'EXACT_IN','sellToken':assets[c['index_in']]['token'],'buyToken':assets[c['index_out']]['token'],'sellAmount':c['input'],'caller':assets[c['index_in']]['token'],'recipient':assets[c['index_out']]['token'],'slippageBps':50,'verification':'FAST'};code,v=http(base(a)+'/v1/quote',token,request);require(code==200,(p['protocol'],code,v));require(v['grossBuyAmount']==c['expected']['amount_out'] and v['consumedIn']==c['input'] and v['refund']=='0','oracle amount mismatch '+p['protocol']);require(v['snapshotRef']==owner()['reference'] and v['onlineGeneration']==v['snapshotRef']['online_generation'],'quote bank mismatch');require(v['verification']['level']=='INDICATIVE','overstated verification');require(len(v['quality']['candidateScope'])==6 and set(v['quality']['candidateScope'])=={i['pool']['pool_id']for i in chain.items},'partial poolscope');return v,request
   try:
    for a in['go','rust']:start(a);wait(base(a)+'/health/ready')
    for a in['router1','router2']:start(a);wait(base(a)+'/health/live');wait_scope(base(a),token,'evm:31337')
    synced();report['checks'].append(PROCESS_CHECKS[0]);snap=owner();require(len(snap['pools'])==6,'owner poolcount');actual={p['pool_id']:p for p in snap['pools']}
    for item in chain.items:require(actual[item['pool']['pool_id']]==item['pool'],'complete physical snapshot mismatch '+item['pool']['protocol']+' expected='+json.dumps(item['pool'],sort_keys=True)+' actual='+json.dumps(actual[item['pool']['pool_id']],sort_keys=True))
    report['checks'].append(PROCESS_CHECKS[1])
    for a in['router1','router2']:
     for item in chain.items:quote(a,item)
    report['checks'].append(PROCESS_CHECKS[2])
    for bad in['weights','manager','managed']:
     old=owner()['reference']['online_generation'];chain.advance(bad)
     for a in['router1','router2']:await_condition(lambda:status(a)[0]==503,'invalidvector remainedready')
     chain.bad=None;await_condition(lambda:status('go')[1].get('snapshot_ref',{}).get('online_generation',0)>old,'generation didnotrecover');synced()
     for a in['router1','router2']:quote(a,chain.items[0])
    report['checks'].append(PROCESS_CHECKS[3]);stop('router2');start('router2');wait(base('router2')+'/health/live');wait_scope(base('router2'),token,'evm:31337');synced();quote('router2',chain.items[0]);report['checks'].append(PROCESS_CHECKS[4])
    for a in['router1','router2']:
     quoted,request=quote(a,chain.items[0]);request['verification']='VERIFIED';c,v=http(base(a)+'/v1/quote',token,request);require(c==422 and'INDEPENDENT_SIMULATION_UNAVAILABLE'in json.dumps(v),'verified enabled');c,v=http(base(a)+'/v1/quotes/'+quoted['quoteId']+'/build',token,{});require(c==422,'build enabled')
    report['checks'].append(PROCESS_CHECKS[5]);require(report['checks']==PROCESS_CHECKS,'missingchecks');report.update(passed=True,pool_count=6,pinned_reads=chain.pinned,receipt_reads=chain.receipts)
   finally:
    for a in list(processes):stop(a)
 finally:
  for s in servers:s.shutdown();s.server_close()
 return report
if __name__=='__main__':
 OUT.mkdir(parents=True,exist_ok=True);target=OUT/'multi-model-three-service-smoke.json';target.write_text(json.dumps({'passed':False,'state':'RUNNING'})+'\n')
 try:r=run();r['measured_at_unix_ms']=int(time.time()*1000);target.write_text(json.dumps(r,indent=2)+'\n');print(json.dumps(r,indent=2))
 except BaseException as e:target.write_text(json.dumps({'passed':False,'error':str(e),'measured_at_unix_ms':int(time.time()*1000)})+'\n');raise
