#!/usr/bin/env python3
"""Synthetic, offline three-process acceptance. Does not claim production verification."""
from configuration import config_for,save_config,sample_for,service_command
from pathlib import Path
import argparse,json,hashlib,os,secrets,shutil,socket,subprocess,tempfile,time,urllib.request,urllib.error
ROOT=Path(__file__).resolve().parents[3]
SERVICES=['chain-parser-go','chain-parser-rust','dex-router']
OUTPUT=ROOT/'integration/artifacts/local'
OUTPUT.mkdir(parents=True,exist_ok=True)
def port():
 with socket.socket() as s:s.bind(('127.0.0.1',0));return s.getsockname()[1]
def http(url,token=None,body=None,method=None,timeout=3):
 headers={}
 if token:headers['Authorization']='Bearer '+token
 if body is not None:headers['Content-Type']='application/json';body=json.dumps(body).encode()
 req=urllib.request.Request(url,data=body,headers=headers,method=method)
 try:
  with urllib.request.urlopen(req,timeout=timeout) as r:return r.status,json.load(r)
 except urllib.error.HTTPError as e:
  data=e.read()
  try:value=json.loads(data)
  except ValueError:value={}
  return e.code,value
def wait(url):
 end=time.monotonic()+20
 while time.monotonic()<end:
  try:
   if http(url)[0]==200:return
  except (OSError,ValueError):pass
  time.sleep(.05)
 raise RuntimeError('Process readiness timed out: '+url)
def wait_scope(base,token,chain):
 end=time.monotonic()+15
 while time.monotonic()<end:
  status,body=http(base+'/internal/chains/'+chain+'/status',token)
  if status==200 and body.get('ready'):return
  time.sleep(.05)
 raise RuntimeError('Chain quote scope was not ready: '+chain)
def quote(base,token,chain='evm:31337',buy='C'):
 body={'chainKey':chain,'kind':'EXACT_IN','sellToken':'A','buyToken':buy,'sellAmount':'10003','caller':'0x0000000000000000000000000000000000000001','recipient':'0x0000000000000000000000000000000000000002','slippageBps':50,'verification':'FAST'}
 status,result=http(base+'/v1/quote',token,body)
 assert status==200,(status,result)
 return body,result
def oracle(snapshot,result):
 pools={p['pool_id']:p.copy() for p in snapshot['pools']};total=0;inputs=0
 for route in result['routes']:
  amount=int(route['amount_in']);inputs+=amount
  for edge in route['path']['edges']:
   p=pools[edge['pool_id']];forward=edge['zero_for_one'];ri,ro=('reserve0','reserve1') if forward else ('reserve1','reserve0');rin,rout=int(p[ri]),int(p[ro]);adjusted=amount*(p['fee_denominator']-p['fee_numerator']);out=adjusted*rout//(rin*p['fee_denominator']+adjusted);p[ri]=str(rin+amount);p[ro]=str(rout-out);amount=out
  assert amount==int(route['amount_out']);total+=amount
 assert inputs==10003 and total==int(result['grossBuyAmount'])
def exhaustive_oracle(snapshot,amount=10003):
 pools=snapshot['pools'];edges={}
 for p in pools:
  edges.setdefault(p['token0'],[]).append({'pool_id':p['pool_id'],'zero_for_one':True,'to':p['token1']})
  edges.setdefault(p['token1'],[]).append({'pool_id':p['pool_id'],'zero_for_one':False,'to':p['token0']})
 paths=[]
 def walk(token,path,seen):
  if token=='C':paths.append(path);return
  if len(path)==3:return
  for edge in edges.get(token,[]):
   if edge['to'] not in seen:walk(edge['to'],path+[edge],seen|{edge['to']})
 walk('A',[],{'A'})
 def evaluate(allocation):
  state={p['pool_id']:p.copy() for p in pools};total=0
  for path,value in allocation:
   for edge in path:
    p=state[edge['pool_id']];ri,ro=('reserve0','reserve1') if edge['zero_for_one'] else ('reserve1','reserve0');rin,rout=int(p[ri]),int(p[ro]);net=value*(p['fee_denominator']-p['fee_numerator']);out=net*rout//(rin*p['fee_denominator']+net)
    if out==0:return -1
    p[ri]=str(rin+value);p[ro]=str(rout-out);value=out
   total+=value
  return total
 best=max(evaluate([(path,amount)]) for path in paths)
 for i,a in enumerate(paths):
  for b in paths[i+1:]:
   for first in range(1,amount):
    best=max(best,evaluate([(a,first),(b,amount-first)]),evaluate([(b,amount-first),(a,first)]))
 return best,len(paths)
def main():
 report={'profile':'offline-debug-parsers-and-real-router','production_ready':False,'checks':[],'commands':[]}
 for name in SERVICES:(OUTPUT/(name+'.log')).write_text('')
 with tempfile.TemporaryDirectory(prefix='xone-e2e-') as temp:
  tmp=Path(temp);token=secrets.token_urlsafe(32);ports={s:port() for s in SERVICES};processes={};logs={};configs={}
  for name in SERVICES:
   directory=tmp/name;config=config_for(name,directory,token,ports[name])
   if name!='dex-router':
    for chain in config['chains']:sample_for(chain['key'],directory/'fixtures',int(time.time()*1000))
   config['startup']['max_baseline_age_ms']=120000
   # This retained process oracle enumerates at most two routes.
   # Multi-route coverage uses the independent grid oracle and actual API test.
   if name=='dex-router':config['routing'].update(max_splits=2,max_state_age_ms=120000,quote_ttl_ms=60000)
   config['journal']['source_urls']=[f'http://127.0.0.1:{ports[s]}' for s in SERVICES[:2]]
   configs[name]=save_config(directory/'config',config)
  def launch(name):
   logs[name]=open(OUTPUT/(name+'.log'),'ab')
   processes[name]=subprocess.Popen(service_command(name,configs[name],tmp/name/'fixtures' if name!='dex-router' else None),cwd=ROOT/name,stdout=logs[name],stderr=subprocess.STDOUT)

  def stop(name):
   p=processes.pop(name,None)
   if p:
    p.terminate()
    try:p.wait(timeout=5)
    except subprocess.TimeoutExpired:p.kill();p.wait()
   if name in logs:logs.pop(name).close()
  try:
   for name in SERVICES:launch(name)
   for name in SERVICES:wait(f'http://127.0.0.1:{ports[name]}/health/ready')
   router=f'http://127.0.0.1:{ports["dex-router"]}';go=f'http://127.0.0.1:{ports["chain-parser-go"]}'
   wait_scope(router,token,'evm:31337');wait_scope(router,token,'solana:localnet');request,q=quote(router,token);assert q['verification']['level']=='INDICATIVE';assert q['quality']['historyIndependent'] is True;assert q['refund']=='0';snapshot=http(go+'/internal/chains/evm:31337/baseline',token)[1];oracle(snapshot,q);report['checks'].append('three-process-baseline-graph-and-shared-pool-oracle')
   optimum,path_count=exhaustive_oracle(snapshot);assert int(q['grossBuyAmount'])<=optimum;report['optimizer_oracle']={'scope':'all integer two-route allocations within three-hop synthetic graph','candidate_paths':path_count,'oracle_buy_atoms':str(optimum),'actual_buy_atoms':q['grossBuyAmount'],'gap_atoms':str(optimum-int(q['grossBuyAmount']))}
   assert http(router+'/v1/quote',None,request)[0]==401
   bad=request.copy();bad['sellAmount']=10003;assert http(router+'/v1/quote',token,bad)[0]==400
   bad=request.copy();bad['verification']='VERIFIED';assert http(router+'/v1/quote',token,bad)[0]==422
   report['checks'].append('auth-amount-string-and-no-false-verified')
   _,sol=quote(router,token,'solana:localnet','B');assert sol['chainKey']=='solana:localnet';report['checks'].append('both-parser-sources-and-chain-isolation')
   started=time.monotonic();old=q['onlineGeneration'];stop('chain-parser-go');launch('chain-parser-go');wait(go+'/health/ready')
   deadline=time.monotonic()+10
   while True:
    wait_scope(router,token,'evm:31337')
    _,new=quote(router,token)
    if new['onlineGeneration']>old:break
    assert time.monotonic()<deadline;time.sleep(.05)
   old_status,old_error=http(router+f'/v1/quotes/{q["quoteId"]}/build',token,{},'POST');assert old_status==409 and old_error['error']=='SNAPSHOT_REORGED',(old_status,old_error)
   report['checks'].append('parser-restart-new-generation-and-old-quote-invalidation');report['parser_restart_ms']=round((time.monotonic()-started)*1000)
   stop('chain-parser-rust');quote(router,token);launch('chain-parser-rust');wait(f'http://127.0.0.1:{ports["chain-parser-rust"]}/health/ready');deadline=time.monotonic()+10
   while True:
    wait_scope(router,token,'solana:localnet')
    _,sol_new=quote(router,token,'solana:localnet','B')
    if sol_new['onlineGeneration']>sol['onlineGeneration']:break
    assert time.monotonic()<deadline;time.sleep(.05)
   report['checks'].append('rocksdb-parser-restart-and-unaffected-other-chain')
   stop('dex-router');launch('dex-router');wait(router+'/health/ready');wait_scope(router,token,'evm:31337');_,rebuilt=quote(router,token);assert rebuilt['onlineGeneration']==new['onlineGeneration'];report['checks'].append('memory-only-router-head-first-recovery')
   status,checkpoints=http(go+'/internal/checkpoints',token,{},'POST');assert status==200 and checkpoints['manifests'];report['checks'].append('checksummed-owner-checkpoint-publication')
   jobdir=tmp/'analytics';jobdir.mkdir()
   events=[]
   for event_id,block,at,output in [('first','b1',1,'10'),('high','b2',2,'100'),('last','b3',3,'20')]:events.append({'kind':'SWAP','trade':{'id':event_id,'chain_key':'evm:31337','pool_id':'ab-1','block_hash':block,'timestamp_ms':at,'tx_index':0,'event_index':0,'base_amount':'10','quote_amount':output}})
   events+=[events[0],{'kind':'REVERT','chain_key':'evm:31337','block_hash':'b2'}];definition=jobdir/'trades.json';definition.write_text(json.dumps({'schema_version':1,'plugin_version':'candles/1','interval_ms':60000,'events':events}))
   output=jobdir/'output/analytics/evm:31337.json'
   job=subprocess.run([str(ROOT/'chain-parser-go/bin/fixture-parser-go'),'--replay',str(definition),'--output',str(output)],cwd=ROOT/'chain-parser-go',capture_output=True,text=True,timeout=10);assert job.returncode==0,job.stderr
   release=json.loads((jobdir/'output/analytics/evm:31337.json').read_text());assert release['online_state_unmodified'] and release['candles'][0]['count']==2 and release['candles'][0]['high_rational']=='2';quote(router,token);report['checks'].append('isolated-debug-analytics-replay-idempotency-and-reorg-candle-rebuild')

   report['passed']=True
  finally:
   for name in list(processes):stop(name)
 report['tool_sha256']=hashlib.sha256(Path(__file__).read_bytes()).hexdigest();report['measured_at_unix_ms']=int(time.time()*1000);(OUTPUT/'three-service-smoke.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
if __name__=='__main__':
 try:main()
 except Exception as failure:
  (OUTPUT/'three-service-smoke.json').write_text(json.dumps({'passed':False,'production_ready':False,'profile':'offline-debug-parsers-and-real-router','error_type':type(failure).__name__,'error':str(failure),'tool_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest()},indent=2)+'\n')
  raise
