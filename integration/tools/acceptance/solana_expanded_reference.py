#!/usr/bin/env python3
"""Verify original bytes, user transfers, conserved tokens and full projections."""
from pathlib import Path
import base64,copy,hashlib,importlib.util,json,sys
from weighted_evidence import ROOT,OUT,sha
sys.path.insert(0,str(ROOT/'integration/tools/reference/pump-bonding/harness'))
from r01_public_catalog_solana_rebuild import b58decode
DLMM={'bitmap':56,'rewards':136,'orders':96,'token2022':120}
def require(ok,why):
 if not ok:raise ValueError('SOLANA_EXPANDED_REFERENCE:'+why)
def module(name,path):
 s=importlib.util.spec_from_file_location(name,ROOT/path);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m

def tokens(n):
 before,after=n['before'],n['after'];require(before.keys()==after.keys(),'token-account-set');totals={}
 for id,a in before.items():
  b=base64.b64decode(a['data_base64'],validate=True);c=base64.b64decode(after[id]['data_base64'],validate=True)
  if len(b)not in [165,178]:continue
  require(len(c)==len(b)and b[:64]==c[:64],'token-identity')
  x=int.from_bytes(b[64:72],'little');y=int.from_bytes(c[64:72],'little')
  if len(b)==178:
   require(b[165:170]==c[165:170]==b'\x02\x02\x00\x08\x00','withheld-layout');x+=int.from_bytes(b[170:178],'little');y+=int.from_bytes(c[170:178],'little')
  totals[b[:32]]=totals.get(b[:32],0)+y-x
  if b[109:113]==b'\x01\0\0\0':require(after[id]['lamports']-a['lamports']==int.from_bytes(c[64:72],'little')-int.from_bytes(b[64:72],'little'),'native-backing')
 require(all(v==0 for v in totals.values()),'token-plus-withheld-conservation')
def delta_token(n,mint,owner):
 matches=[]
 for key,a in n['before'].items():
  b=base64.b64decode(a['data_base64'],validate=True)
  if len(b)in[165,178]and b[:32]==b58decode(mint)and b[32:64]==b58decode(owner):matches.append(key)
 require(len(matches)==1,'user-token-account')
 key=matches[0];return int.from_bytes(base64.b64decode(n['after'][key]['data_base64'])[64:72],'little')-int.from_bytes(base64.b64decode(n['before'][key]['data_base64'])[64:72],'little')

def verify_family(family,reexecute=False,fixture=None):
 path=OUT/f'core-meteora-{family}-original-20261111.json';e=json.loads(path.read_bytes());f=json.loads((ROOT/f'integration/test/contracts/meteora-{family}.json').read_bytes())if fixture is None else fixture;gen=module('expanded_'+family,'integration/tools/ci/meteora-'+family+'-reference.py')
 require(f['source_sha256']==sha(path)and e['generator_sha256']==sha(gen.__file__),'source-binding:'+family);require(len(e['records'])==len(f['cases'])==DLMM[family],'case-coverage:'+family);full=rejected=0
 smoke=module('expanded_replay','integration/tools/acceptance/solana_expanded_smoke.py')if reexecute else None
 for index,(entry,c)in enumerate(zip(e['records'],f['cases'])):
  require(index==entry['index']==c['index'],'case-identity');n=entry['native'];require(entry['zero_for_one']==c['zero_for_one']and entry['exact_out']==c['exact_out'],'direction-and-kind');steps=n['sequence']if n['passed']else[n];require(len(steps)==len(c['steps']),'sequence-length')
  for original,s in zip(steps,c['steps']):
   require(original['passed']==s['passed']==('err'not in original['native']),'actual-result');args=[n['clock'],n['program_sha256']]+([n['pair_id']]if family=='bitmap'else[])
   require(gen.pool(original['before'],*args)==s['pool']and gen.pool(original['after'],*args)==s['after'],'full-projection:'+family);require(s['actual_input']==original['payer_input_delta']and s['actual_output']==original['recipient_output_delta']and s['amount']==original['specified_amount'],'reported-funds')
   if not original['passed']:require(original['before']==original['after'],'failure-rollback');rejected+=1;continue
   zero=c['zero_for_one'];p=s['pool'];paid=-delta_token(original,p['token0'if zero else'token1'],n['payer']);received=delta_token(original,p['token1'if zero else'token0'],n['recipient']);require(str(paid)==s['actual_input']and str(received)==s['actual_output'],'actual-raw-funds');require((s['actual_output']if c['exact_out']else s['actual_input'])==s['amount'],'specified-amount');tokens(original);full+=1
  if reexecute:
   if family=='rewards':fresh=gen.native.execute(entry['zero_for_one'],entry['exact_out'],int(n['specified_amount']),entry['end'],entry['last'],empty=entry['empty'],followups=(10000,20000)if int(n['specified_amount'])==1000000 and not entry.get('second')else(),second=entry.get('second',False))
   elif family=='orders':fresh=gen.native.execute(entry['zero_for_one'],int(n['specified_amount']),False,entry['open_amount'],entry['processed'],entry['mm'],followups=(100,1000)if int(n['specified_amount'])==1000 else())
   elif family=='bitmap':fresh=gen.execute(entry['negative'],entry['zero_for_one'],int(n['specified_amount']),entry['exact_out'])
   else:fresh=gen.native.execute(zero=entry['zero_for_one'],amount=int(n['specified_amount']),minimum=10**15 if entry['exact_out']else 1,exact_out=entry['exact_out'],mm_only=True,protocol_share=2000,elapsed=300,dynamic_control=7500,token22_fee=entry['fees'],bank_epoch=entry['epoch'],followups=(10000,20000)if int(n['specified_amount'])==1000000 else())
   require(fresh['before']==n['before']and fresh['after']==n['after']and fresh['passed']==n['passed'],'fresh-original:'+family);require(len(fresh['sequence'])==len(n['sequence']),'fresh-sequence-count')
   for a,b in zip(fresh['sequence'],n['sequence']):require(a['before']==b['before']and a['after']==b['after'],'fresh-followup:'+family)
 return dict(passed=True,cases=len(f['cases']),full_steps=full,rejected_steps=rejected,reexecuted=reexecute)

def verify_pump(kind,reexecute=False,fixture=None):
 stem='pump-'+kind;path=OUT/f'core-{stem}-original-20261111.json';e=json.loads(path.read_bytes());f=json.loads((ROOT/f'integration/test/contracts/{stem}.json').read_bytes())if fixture is None else fixture;gen=module('expanded_pump_'+kind,'integration/tools/ci/'+stem+'-reference.py');require(f['source_sha256']==sha(path)and e['generator_sha256']==sha(gen.__file__),'pump-source:'+kind);full=0
 if kind=='amm':
  require(len(e['records'])==len(f['cases'])==27,'amm-coverage')
  for c,row in zip(e['records'],f['cases']):
   n=c['native'];steps=n['sequence']if n['passed']else[n];require(len(steps)==len(row['steps']),'amm-sequence')
   for raw,s in zip(steps,row['steps']):
    require(raw['passed']==s['passed']==('err'not in raw['native']),'amm-result');require(gen.pool(raw['before'],n['ids'])==s['pool']and gen.pool(raw['after'],n['ids'])==s['after'],'amm-full-projection');require(raw['user_base_delta']==s['actual_base']and raw['user_quote_delta']==s['actual_quote'],'amm-reported-funds')
    if not raw['passed']:require(raw['before']==raw['after'],'amm-rollback');continue
    require(delta_token(raw,n['ids']['base_mint'],n['ids']['user'])==raw['user_base_delta']and delta_token(raw,n['ids']['quote_mint'],n['ids']['user'])==raw['user_quote_delta'],'amm-actual-funds');tokens(raw);full+=1
   if reexecute:
    fresh=gen.native.execute(n['kind'],int(n['amount']),int(n['limit']),followups=(10000,20000)if n['amount']=='1000000'else());require(fresh['before']==n['before']and fresh['after']==n['after']and fresh['passed']==n['passed'],'amm-fresh-original')
 elif kind in ['exact-out','sell-exact-out']:
  require(len(e['records'])==len(f['cases'])==(18 if kind=='exact-out'else 20),'pump-target-coverage')
  projection=gen.projection
  for c,row in zip(e['records'],f['cases']):
   n=c['native'];ok=n is not None and n['passed'];require(ok==row['passed'],'pump-target-result')
   if not ok:
    if n is not None:require(n['before']==n['after'],'pump-target-rollback')
    continue
   require(projection.pool(n['before'],n['ids'])==row['pool']and projection.pool(n['after'],n['ids'])==row['after'],'pump-target-projection');require(row['required_input']==str(-n['user_sol_delta']if kind=='exact-out'else-n['user_token_delta']),'pump-required-input');target=int(c['target']);require(n['user_token_delta']==target if kind=='exact-out'else n['user_sol_delta']>=target,'pump-target-credit')
   control=c['input_minus_one'];require(control is not None and not control['passed']and control['before']==control['after']==n['before'],'pump-original-input-minus-one');tokens(n);full+=1
   if reexecute:
    fresh=gen.native.execute(kind='buy'if kind=='exact-out'else'sell',amount=int(n['amount']),limit=int(n['limit']),complete=c.get('complete',False),**({'quote_reserve':c['quote_reserve']}if kind=='sell-exact-out'else{}));require(fresh['before']==n['before']and fresh['after']==n['after']and fresh['passed'],'pump-fresh-target')
 else:raise ValueError('UNKNOWN_PUMP_KIND')
 return dict(passed=True,full_steps=full,reexecuted=reexecute)

def verify_migration(reexecute=False,evidence=None):
 path=OUT/'core-pump-migration-original-20261111.json';e=json.loads(path.read_bytes())if evidence is None else evidence;f=json.loads((ROOT/'integration/test/contracts/pump-migration.json').read_bytes());gen=module('expanded_migration','integration/tools/ci/pump-migration-reference.py');require(f['source_sha256']==sha(path)and e['generator_sha256']==sha(gen.__file__),'migration-source');n=e['migration'];require(n['passed']and len(e['swaps'])==len(f['cases'])==3,'actual-migration-coverage');require(len(n['sequence'])==2 and n['sequence'][1]['passed']and n['sequence'][1]['before']==n['sequence'][1]['after']==n['after'],'migration-idempotence');ids=n['migration_ids']
 for key in ['before','after']:
  actual=gen.curve.pool(n[key],n['ids']);actual.update(protocol='pump-bonding-sol-migration',adapter_version='pump-bonding/7a19affaa2c6-sol-migration-1');actual['pump'].update(slot=f[key]['pump']['slot'],timestamp=f[key]['pump']['timestamp']);require(actual==f[key],'migration-curve-projection')
 for s,row in zip(e['swaps'],f['cases']):
  require(s['passed'],'migrated-swap-failed')
  for key in ['pool','pool_base_token_account','pool_quote_token_account']:require(n['after'][ids[key]]==s['before'][ids[key]],'migration-created-account-handoff')
  require(gen.amm.pool(s['before'],s['ids'])==row['pool']and gen.amm.pool(s['after'],s['ids'])==row['after'],'migrated-full-projection');tokens(s)
 control=e['virtual_reserve_control'];require(not control['passed']and control['before']==control['after']and any('InsufficientRealQuoteReserves'in line for line in control['native'].get('meta',{}).get('logs',[])),'migration-virtual-bound-control');require(gen.amm.pool(control['before'],control['ids'])==f['virtual_reserve_control']['pool'],'migration-virtual-bound-projection')
 if reexecute:
  fresh=gen.migration.execute(kind='migrate',amount=0,limit=0,complete=True,followups=(0,));require(fresh['passed']and fresh['before']==n['before']and fresh['after']==n['after'],'fresh-migration')
 return dict(passed=True,migrations=1,actual_created_pool_swaps=3,reexecuted=reexecute)

def main():
 reexecute='--reexecute'in sys.argv
 result=dict(passed=True,dlmm={name:verify_family(name,reexecute)for name in DLMM},pump={name:verify_pump(name,reexecute)for name in ['amm','exact-out','sell-exact-out']},migration=verify_migration(reexecute),all_requested_features_complete=False,production_ready=False)
 print(json.dumps(result))
if __name__=='__main__':main()
