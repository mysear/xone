#!/usr/bin/env python3
"""Finite normal RPC/PG/Kafka/dual-Router ExactIn plans, production model and SBF."""
from pathlib import Path
import argparse,base64,copy,importlib.util,json,secrets,subprocess,tempfile,time
from meteora_mm_smoke import Chain,ROOT,OUT,digest,helper_hashes
from infrastructure import Infrastructure,port
from configuration import config_for,save_config,service_command,sample_for
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import http,quote,wait_scope
from weighted_evidence import source_hash,write_result
FAMILIES=['bitmap','rewards','orders','orders-exact-out','token2022','pump-amm']
def require(ok,why):
 if not ok:raise ValueError(why)
def module(name,path):
 spec=importlib.util.spec_from_file_location(name,ROOT/path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m

def inputs(family):
 path=ROOT/'integration/test/contracts'/('pump-amm.json'if family=='pump-amm'else'meteora-'+family+'.json');f=json.loads(path.read_bytes());original_path=OUT/('core-pump-amm-original-20261111.json'if family=='pump-amm'else'core-meteora-'+family+'-original-20261111.json');original=json.loads(original_path.read_bytes());decoder=json.loads((ROOT/'integration/test/contracts/pump-amm-decoder.json').read_bytes())if family=='pump-amm'else f['decoder'];selected=[]
 if family=='pump-amm':selected=[i for i,c in enumerate(f['cases'])if c['kind']in ['sell','buy_exact_quote_in']and c['steps'][0]['passed']and c['steps'][0]['amount']in ['1000000','1000000000']]
 else:
  for zero in [False,True]:
   candidates=[i for i,c in enumerate(f['cases'])if c['zero_for_one']==zero and c['exact_out']==(family=='orders-exact-out')and c['steps'][0]['passed']]
   if family=='bitmap':candidates=[i for i in candidates if not original['records'][i]['negative']]
   if family=='token2022':
    for epoch in [1,2]:selected.append(next(i for i in candidates if original['records'][i]['epoch']==epoch and original['records'][i]['fees']==[250,1000,500,2000,2]and f['cases'][i]['steps'][0]['amount']=='1000000'))
   elif family=='orders-exact-out':selected.extend([next(i for i in candidates if original['records'][i]['mm']==1000 and original['records'][i]['open_amount']==3000 and original['records'][i]['processed']==2000 and f['cases'][i]['steps'][0]['amount']==amount)for amount in ['1000','6000']])
   elif family=='orders':selected.extend([next(i for i in candidates if original['records'][i]['mm']==1000 and original['records'][i]['open_amount']==3000 and original['records'][i]['processed']==2000 and f['cases'][i]['steps'][0]['amount']==amount)for amount in ['1000','3000']])
   elif family=='rewards':selected.extend([next(i for i in candidates if original['records'][i]['end']==end and original['records'][i]['last']==1734392400 and not original['records'][i]['empty']and f['cases'][i]['steps'][0]['amount']=='1000000')for end in [1734392500,1734392420]])
   else:selected.extend(next(i for i in candidates if f['cases'][i]['steps'][0]['amount']==amount)for amount in ['1000','5000'])
 cfg=decoder['config']if 'config'in decoder else decoder['cases'][selected[0]]['config'];d=dict(config=cfg,shared_accounts=decoder['shared_accounts'],cases=decoder['cases']);return f,original,d,selected,path,original_path

class ExtendedChain(Chain):
 def __init__(self,decoder,selected):
  super().__init__(dict(decoder=decoder));self.index=selected[0];m=decoder['cases'][self.index]['pool'].get('pump')or decoder['cases'][self.index]['pool']['meteora'];self.head=m['slot'];self.timestamp=m['timestamp']
 def block(self,n):
  b=super().block(n);b['blockTime']=self.timestamp;return b
 def accounts(self):
  d=self.f['decoder'];out=copy.deepcopy(d['shared_accounts']);out.update(copy.deepcopy(d['cases'][self.index]['accounts']));cfg=d['config'];clock='SysvarC1ock11111111111111111111111111111111'
  raw=bytearray(base64.b64decode(out[clock]['data'][0]));raw[:8]=(self.head+(self.bad=='clock-slot')).to_bytes(8,'little');out[clock]['data'][0]=base64.b64encode(raw).decode()
  if self.bad=='program-code':
   key=(cfg.get('pump')or cfg['meteora'])['expected_programdata'];raw=bytearray(base64.b64decode(out[key]['data'][0]));raw[45]^=1;out[key]['data'][0]=base64.b64encode(raw).decode()
  return out

def replay(family,entry,amount):
 if family=='orders-exact-out':return module('normal_orders_xo_ref','integration/tools/ci/meteora-orders-exact-out-reference.py').execute(entry['zero'],amount,entry['mm'],entry['processed'],entry['open_amount'])
 if family=='pump-amm':return module('normal_amm_ref','integration/tools/ci/pump-amm-reference.py').native.execute(kind=entry['native']['kind'],amount=amount,limit=1)
 if family=='bitmap':return module('normal_bitmap_ref','integration/tools/ci/meteora-bitmap-reference.py').execute(entry['negative'],entry['zero_for_one'],amount,False)
 if family=='rewards':return module('normal_reward_ref','integration/tools/ci/meteora-rewards-preflight.py').execute(entry['zero_for_one'],False,amount,entry['end'],entry['last'],empty=entry['empty'])
 if family=='orders':return module('normal_orders_ref','integration/tools/ci/meteora-orders-preflight.py').execute(entry['zero_for_one'],amount,False,entry['open_amount'],entry['processed'],entry['mm'])
 return module('normal_token_ref','integration/tools/ci/meteora-token2022-reference.py').native.execute(zero=entry['zero_for_one'],amount=amount,minimum=1,mm_only=True,protocol_share=2000,elapsed=300,dynamic_control=7500,token22_fee=entry['fees'],bank_epoch=entry['epoch'])

def run(family):
 f,original,decoder,selected,reference,original_path=inputs(family);chain=ExtendedChain(decoder,selected);servers=[chain.provider(),chain.provider()];processes={};logs={};records=[];source=source_hash();tag=secrets.token_hex(6);target=OUT/f'core-solana-expanded-{family}-normal-process.json';names={'go':'chain-parser-go','rust':'chain-parser-rust','router1':'dex-router','router2':'dex-router'};binaries=['chain-parser-go/bin/fixture-parser-go','chain-parser-rust/target/debug/chain-parser-rust','dex-router/target/debug/dex-router','dex-router/target/debug/examples/'+('meteora-exact-out-plan-projector'if family=='orders-exact-out'else'solana-plan-projector')];report=dict(schema_version=1,passed=False,family=family,source_tree_sha256=source,reference_sha256=digest(reference),original_sha256=digest(original_path),tool_sha256=digest(__file__),helper_sha256=helper_hashes(),binaries={p:digest(ROOT/p)for p in binaries},checks=[],selected_indices=selected,authority='controlled fixed original banks with relabeled local slot, finalized RPC, actual dedicated PG/Kafka and dual Router; '+('ExactOut'if family=='orders-exact-out'else'ExactIn'),normal_exact_out_exercised=family=='orders-exact-out',all_requested_features_complete=False,production_ready=False);write_result(target,report)
 try:
  with Infrastructure()as infra,tempfile.TemporaryDirectory(prefix='xone-expanded-'+family+'-')as folder:
   root=Path(folder);token=secrets.token_urlsafe(32);ports={a:port()for a in names};configs={};sources=[f'http://127.0.0.1:{ports[a]}'for a in ['go','rust']]
   for alias,name in names.items():
    c=config_for(name,root/alias,token,ports[alias]);c['history']['enabled']=False;c['journal'].update(transport='kafka',source_urls=sources,poll_interval_ms=150,request_timeout_ms=15000);c['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=alias,transaction_timeout_ms=4000);c['control'].update(backend='disabled'if name=='dex-router'else'postgres',dsn_file=''if name=='dex-router'else str(infra.dsnfile),instance_id=alias,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000);c['startup']['max_baseline_age_ms']=315360000000;c['journal']['max_payload_bytes']=33554432
    if alias=='rust':c['chains'][0].update(genesis_hash='localnet',confirmation_tag='finalized',rpc_urls=[f'http://127.0.0.1:{s.server_port}'for s in servers],pools=[copy.deepcopy(decoder['config'])])
    elif alias=='go':sample_for('evm:31337',root/alias/'fixtures',int(time.time()*1000))
    else:c['routing'].update(max_state_age_ms=315360000000,quote_ttl_ms=60000,max_splits=1,split_parts=4);c['api']['quote_timeout_ms']=30000
    configs[alias]=save_config(root/alias/'config',c)
   def base(a):return f'http://127.0.0.1:{ports[a]}'
   def start(a):
    logs[a]=(OUT/f'solana-expanded-{family}-{tag}-{a}.log').open('w');processes[a]=subprocess.Popen(service_command(names[a],configs[a],root/a/'fixtures'if a=='go'else None),cwd=ROOT/names[a],stdout=logs[a],stderr=subprocess.STDOUT)
   def stop(a):
    p=processes.pop(a);p.terminate()
    try:p.wait(timeout=10)
    except subprocess.TimeoutExpired:p.kill();p.wait()
    logs.pop(a).close()
   def status(a):return http(base(a)+'/internal/chains/solana:localnet/status',token)
   def current(a):return status(a)[1].get('snapshot_ref',{}).get('block',{}).get('number_or_slot')==chain.head
   def check(a,capture=True):
    index=chain.index;c=f['cases'][index];step=c['steps'][0];p=step['pool'];n=original['records'][index]['native'];zero=c['kind']=='sell'if family=='pump-amm'else c['zero_for_one'];user=p.get('pump',{}).get('user',n.get('payer'));recipient=user if family=='pump-amm'else n['recipient'];expected=str(step['actual_quote']if zero else step['actual_base'])if family=='pump-amm'else step['actual_output'];amount=step['amount'];exact=family=='orders-exact-out';request=dict(chainKey='solana:localnet',kind='EXACT_OUT'if exact else'EXACT_IN',sellToken=p['token0'if zero else'token1'],buyToken=p['token1'if zero else'token0'],caller=user,recipient=recipient,slippageBps=50,verification='FAST')
    if exact:request.update(buyAmount=amount,maxSellAmount='1000000000000000')
    else:request['sellAmount']=amount
    endpoint='/v1/quote-exact-out'if exact else'/v1/quote';code,q=http(base(a)+endpoint,token,request);require(code==200 and q['grossBuyAmount']==expected and q['consumedIn']==(step['actual_input']if exact else amount),'NORMAL_QUOTE:'+str((code,q)))
    if exact:
     require(q['requiredSellAmount']==step['actual_input']and q['buyAmount']==q['minBuyAmount']==amount and q['outputSurplus']==q['refund']=='0','NORMAL_EXACT_OUT_AMOUNTS');bad=dict(request,maxSellAmount=str(int(step['actual_input'])-1));require(http(base(a)+endpoint,token,bad)[0]!=200,'NORMAL_HARD_BUDGET')
    if capture:
     code,e=http(base(a)+'/internal/quotes/'+q['quoteId']+'/evidence',token);require(code==200,'NORMAL_PLAN');snapshot=http(base('rust')+'/internal/chains/solana:localnet/baseline',token)[1];require(snapshot['reference']==q['snapshotRef'],'NORMAL_BANK_REFERENCE');records.append(dict(case_index=index,snapshot=snapshot,request=request,quote=q,execution_plan=e['execution_plan']))
   try:
    for a in ['go','rust']:start(a);wait(base(a)+'/health/ready')
    for a in ['router1','router2']:start(a);wait(base(a)+'/health/live');wait_scope(base(a),token,'solana:localnet')
    for index in selected:
     chain.advance(index)
     for a in ['router1','router2']:await_condition(lambda a=a:current(a),'expanded bank not published');check(a)
    report['checks'].append('four-original-banks-two-router-processes')
    for fault in ['program-code','clock-slot']:
     chain.advance(bad=fault)
     for a in ['router1','router2']:await_condition(lambda a=a:status(a)[0]==503,'expanded fault not revoked:'+fault);quote(base(a),token,'evm:31337','B')
     chain.advance()
     for a in ['router1','router2']:await_condition(lambda a=a:current(a),'expanded fault not restored');check(a,False)
     report['checks'].append('fault-withdraw-and-recovery:'+fault)
    stop('router1');start('router1');wait(base('router1')+'/health/live');wait_scope(base('router1'),token,'solana:localnet');await_condition(lambda:current('router1'),'expanded restart tail not restored');check('router1');report['checks'].append('router-checkpoint-tail-restart')
   finally:
    for a in list(processes):stop(a)
 finally:
  for s in servers:s.shutdown();s.server_close()
 input_path=OUT/f'solana-expanded-{family}-plans-{tag}.json';model_path=OUT/f'solana-expanded-{family}-model-{tag}.json';native_path=OUT/f'solana-expanded-{family}-original-{tag}.json';input_path.write_text(json.dumps(dict(schema_version=1,records=records))+'\n');subprocess.run([str(ROOT/binaries[-1]),str(input_path),str(model_path)],check=True);model=json.loads(model_path.read_bytes());require(model['passed'],'NORMAL_MODEL_PROJECTION');natives=[]
 for record,projected in zip(records,model['records']):
  entry=original['records'][record['case_index']];n=replay(family,entry,int(record['request']['buyAmount'if family=='orders-exact-out'else'sellAmount']));original_step=entry['native']['sequence'][0]if family=='orders-exact-out'else entry['native'];require(n['passed']and n['before']==original_step['before']and n['after']==original_step['after'],'NORMAL_ORIGINAL_BANK_AND_SWAP');step=projected['steps'][0]
  if family=='pump-amm':require(str(-n['user_base_delta']if entry['native']['kind']=='sell'else-n['user_quote_delta'])==step['amount_in']and str(n['user_quote_delta']if entry['native']['kind']=='sell'else n['user_base_delta'])==step['amount_out'],'NORMAL_ACTUAL_AMM_FUNDS')
  else:require(n['payer_input_delta']==step['amount_in']and n['recipient_output_delta']==step['amount_out'],'NORMAL_ACTUAL_DLMM_FUNDS')
  expected=copy.deepcopy(f['cases'][record['case_index']]['steps'][0]['after']);actual=step['after'];expected['chain_key']=actual['chain_key'];name='pump'if family=='pump-amm'else'meteora';expected[name].update(slot=actual[name]['slot'],timestamp=actual[name]['timestamp'],derived=True);require(expected==actual,'NORMAL_COMPLETE_MODEL_AFTER');natives.append(dict(case_index=record['case_index'],plan_hash=projected['plan_hash'],native=n))
 native_path.write_text(json.dumps(dict(schema_version=1,records=natives),indent=2)+'\n');require(source_hash()==source,'NORMAL_SOURCE_CHANGED');report.update(passed=True,source_tree_sha256_after=source,api_records=len(records),original_sbf_swaps=len(natives),artifacts={name:dict(path=str(path.relative_to(ROOT)),sha256=digest(path))for name,path in [('captured_plans',input_path),('model_projection',model_path),('original_capture',native_path)]});write_result(target,report);print(json.dumps(dict(passed=True,family=family,api_records=len(records))));return report
if __name__=='__main__':
 parser=argparse.ArgumentParser();parser.add_argument('--family',choices=FAMILIES,required=True);args=parser.parse_args()
 try:run(args.family)
 except Exception as e:
  path=OUT/f'core-solana-expanded-{args.family}-normal-process.json';r=json.loads(path.read_bytes())if path.exists()else{};r.update(passed=False,error=str(e));write_result(path,r);raise
