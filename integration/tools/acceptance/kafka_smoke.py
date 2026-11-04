#!/usr/bin/env python3
"""Real local PG/Kafka, two parser owners, two complete router replicas and empty-disk takeover."""
from configuration import config_for,save_config,sample_for,service_command
from infrastructure import Infrastructure,ROOT,port
from smoke import http,wait_scope,quote,oracle
from pathlib import Path
import hashlib,json,secrets,shutil,subprocess,tempfile,time,signal,urllib.request
OUT=ROOT/'integration/artifacts/local';OUT.mkdir(parents=True,exist_ok=True)
def wait(url,token=None,timeout=40):
 end=time.monotonic()+timeout
 while time.monotonic()<end:
  try:
   if http(url,token)[0]==200:return
  except (OSError,ValueError):pass
  time.sleep(.1)
 raise RuntimeError('Readiness timeout: '+url)
def run():
 report={'profile':'real-local-pg13-kafka3.9-multi-replica','production_ready':False,'checks':[]}
 with Infrastructure() as infra,tempfile.TemporaryDirectory(prefix='xone-kafka-smoke-') as folder:
  root=Path(folder);token=secrets.token_urlsafe(32);ports={n:port() for n in ['go1','go2','rust1','rust2','router1','router2']};configs={};processes={};logs={};names={'go1':'chain-parser-go','go2':'chain-parser-go','rust1':'chain-parser-rust','rust2':'chain-parser-rust','router1':'dex-router','router2':'dex-router'};source_urls=[f'http://127.0.0.1:{ports[n]}' for n in ['go1','go2','rust1','rust2']]
  for alias,name in names.items():
   directory=root/alias;config=config_for(name,directory,token,ports[alias])
   if name!='dex-router':
    for chain in config['chains']:sample_for(chain['key'],directory/'fixtures',int(time.time()*1000))
   config['journal'].update(transport='kafka',source_urls=source_urls,poll_interval_ms=100,request_timeout_ms=2000)
   config['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=alias,transaction_timeout_ms=2000)
   config['control'].update(backend='disabled' if name=='dex-router' else 'postgres',dsn_file='' if name=='dex-router' else str(infra.dsnfile),instance_id=alias,lease_ttl_ms=8000,renewal_ms=500,min_commit_remaining_ms=3000)
   config['startup']['max_baseline_age_ms']=120000
   if name=='dex-router':config['routing'].update(max_state_age_ms=120000,quote_ttl_ms=60000)
   configs[alias]=save_config(directory/'config',config)
  def start(alias):
   name=names[alias];logs[alias]=open(OUT/f'kafka-{alias}.log','ab')
   processes[alias]=subprocess.Popen(service_command(name,configs[alias],root/alias/'fixtures' if name!='dex-router' else None),cwd=ROOT/name,stdout=logs[alias],stderr=subprocess.STDOUT)

  def stop(alias,kill=False):
   p=processes.pop(alias,None)
   if p:
    p.send_signal(signal.SIGCONT)
    p.kill() if kill else p.terminate()
    try:p.wait(timeout=10)
    except subprocess.TimeoutExpired:p.kill();p.wait()
   if alias in logs:logs.pop(alias).close()
  def base(alias):return f'http://127.0.0.1:{ports[alias]}'
  def current(alias,chain):return http(base(alias)+'/internal/chains/'+chain+'/status',token)
  def await_generation(alias,chain,old,buy='C'):
   deadline=time.monotonic()+30
   while time.monotonic()<deadline:
    status,state=current(alias,chain)
    if status==200 and state['snapshot_ref']['online_generation']>old:return quote(base(alias),token,chain,buy)[1]
    time.sleep(.1)
   raise RuntimeError('Generation takeover timed out: '+alias+'/'+chain)
  try:
   for alias in ['go1','rust1']:start(alias);wait(base(alias)+'/health/ready')
   start('go2');wait(base('go2')+'/health/live');assert http(base('go2')+'/health/ready')[0]==503;assert http(base('go2')+'/health/startup')[0]==200;start('rust2');wait(base('rust2')+'/health/live');assert http(base('rust2')+'/health/startup')[0]==200;assert http(base('rust2')+'/health/ready')[0]==503;report['checks'].append('both-standby-types-complete-startup-without-advertising-state-ready')
   for alias in ['router1','router2']:start(alias);wait(base(alias)+'/health/live');wait_scope(base(alias),token,'evm:31337');wait_scope(base(alias),token,'solana:localnet')
   _,first=quote(base('router1'),token);_,replica=quote(base('router2'),token);assert first['grossBuyAmount']==replica['grossBuyAmount'];snapshot=http(base('go1')+'/internal/chains/evm:31337/baseline',token)[1];oracle(snapshot,first);request=urllib.request.Request(base('go1')+'/internal/chains/evm:31337/bootstrap',headers={'Authorization':'Bearer '+token});
   with urllib.request.urlopen(request,timeout=3) as response:
    assert response.headers['content-type']=='application/x-protobuf';raw=response.read()
   decoded=subprocess.run(['protoc','--proto_path='+str(ROOT/'common/protoc/proto'),'--decode=dex.v1.OwnerBootstrap',str(ROOT/'common/protoc/proto/dex/v1/state.proto')],input=raw,capture_output=True,check=True).stdout.decode();assert 'uint256_be:' in decoded and 'canonical.evm-31337.v1' in decoded;report['checks'].append('live-protobuf-bootstrap-and-binary-uint256-amounts');report['checks'].append('two-independent-complete-router-replicas-from-transactional-owner-cursors')
   _,sol=quote(base('router1'),token,'solana:localnet','B');assert sol['onlineGeneration']>=1;report['checks'].append('go-and-rust-producers-publish-shared-transactional-wire-contract')
   before=time.monotonic();processes['go1'].send_signal(signal.SIGSTOP);takeover=await_generation('router1','evm:31337',first['onlineGeneration']);other=await_generation('router2','evm:31337',first['onlineGeneration']);assert takeover['snapshotRef']['canonical_epoch']==first['snapshotRef']['canonical_epoch'];assert takeover['snapshotRef']['writer_epoch']>first['snapshotRef']['writer_epoch'];assert takeover['grossBuyAmount']==other['grossBuyAmount'];report['paused_owner_empty_disk_takeover_ms']=round((time.monotonic()-before)*1000);report['checks'].append('paused-owner-empty-private-kv-standby-takeover-without-reorg')
   status,error=http(base('router1')+f'/v1/quotes/{first["quoteId"]}/build',token,{},'POST');assert status==409 and error['error']=='SNAPSHOT_REORGED';processes['go1'].send_signal(signal.SIGCONT);wait(base('go1')+'/health/live');assert http(base('go1')+'/health/ready')[0]==503;report['checks'].append('paused-old-owner-cannot-resurrect-expired-lease-or-become-ready');stop('go1',True);start('go1');wait(base('go1')+'/health/live');assert http(base('go1')+'/health/ready')[0]==503;report['checks'].append('old-owner-restart-remains-standby-and-old-quotes-invalidated')
   stop('router1');start('router1');wait(base('router1')+'/health/live');wait_scope(base('router1'),token,'evm:31337');_,rebuilt=quote(base('router1'),token);assert rebuilt['onlineGeneration']==takeover['onlineGeneration'];report['checks'].append('memory-only-router-recovers-current-checkpoint-and-bounded-kafka-tail')
   stop('rust1',True);wait(base('rust2')+'/health/ready');new_sol=await_generation('router1','solana:localnet',sol['onlineGeneration'],'B');assert new_sol['snapshotRef']['canonical_epoch']==sol['snapshotRef']['canonical_epoch'];quote(base('router2'),token);report['checks'].append('rust-empty-disk-postgres-kafka-floor-recovery-and-other-chain-isolation')
   report['passed']=True
  finally:
   for alias in list(processes):stop(alias)
 report['measured_at_unix_ms']=int(time.time()*1000);report['tool_sha256']=hashlib.sha256(Path(__file__).read_bytes()).hexdigest();(OUT/'kafka-three-service-smoke.json').write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(report,indent=2))
if __name__=='__main__':
 try:run()
 except Exception as e:
  (OUT/'kafka-three-service-smoke.json').write_text(json.dumps({'passed':False,'production_ready':False,'error':str(e),'error_type':type(e).__name__},indent=2)+'\n');raise
