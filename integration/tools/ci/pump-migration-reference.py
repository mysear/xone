#!/usr/bin/env python3
"""Actual fixed migrate CPI, then swaps against the created AMM accounts."""
from pathlib import Path
import base64,copy,hashlib,importlib.util,json,sys
ROOT=Path(__file__).resolve().parents[3];sys.path.insert(0,str(ROOT/'integration/tools/reference/pump-bonding/harness'))
def module(name,path):
 s=importlib.util.spec_from_file_location(name,ROOT/path);m=importlib.util.module_from_spec(s);s.loader.exec_module(m);return m
migration=module('original_migration','integration/tools/reference/pump-bonding/harness/migration.py');curve=module('migration_curve_projection','integration/tools/ci/pump-bonding-reference.py');amm=module('migration_amm_projection','integration/tools/ci/pump-amm-reference.py')
TARGET=ROOT/'integration/artifacts/local/core-pump-migration-original-20261111.json'
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def main():
 n=migration.execute(kind='migrate',amount=0,limit=0,complete=True,followups=(0,))
 if not n['passed']:raise ValueError('ORIGINAL_MIGRATE_FAILED')
 if len(n['sequence'])!=2 or not n['sequence'][1]['passed']or n['sequence'][1]['before']!=n['sequence'][1]['after']or n['sequence'][1]['before']!=n['after']:raise ValueError('ORIGINAL_MIGRATE_IDEMPOTENCE')
 native_swaps=[];cases=[]
 for kind,amount in [('sell',1000000),('buy',1000000),('buy_exact_quote_in',1000000)]:
  s=amm.native.execute(kind,amount,10**15 if kind=='buy'else 1,initial_migration=n)
  if not s['passed']:raise ValueError('MIGRATED_AMM_SWAP_FAILED:'+kind)
  ids=n['migration_ids']
  for name in ['pool','pool_base_token_account','pool_quote_token_account']:
   if n['after'][ids[name]]!=s['before'][ids[name]]:raise ValueError('MIGRATION_REAL_ACCOUNT_HANDOFF:'+name)
  native_swaps.append(s);cases.append(dict(kind=kind,amount=str(amount),pool=amm.pool(s['before'],s['ids']),after=amm.pool(s['after'],s['ids']),actual_base=s['user_base_delta'],actual_quote=s['user_quote_delta']))
 adversarial=copy.deepcopy(n);pool_key=n['migration_ids']['pool'];quote_key=n['migration_ids']['pool_quote_token_account'];real=int.from_bytes(base64.b64decode(n['after'][quote_key]['data_base64'])[64:72],'little');raw=bytearray(base64.b64decode(adversarial['after'][pool_key]['data_base64']));raw[245:261]=(3*real).to_bytes(16,'little',signed=True);adversarial['after'][pool_key]['data_base64']=base64.b64encode(raw).decode();control=amm.native.execute('sell',100000000000000,1,initial_migration=adversarial)
 if control['passed']or control['before']!=control['after']or not any('InsufficientRealQuoteReserves'in line for line in control['native'].get('meta',{}).get('logs',[])):raise ValueError('VIRTUAL_QUOTE_REAL_VAULT_BOUND_CONTROL')
 negative=dict(pool=amm.pool(control['before'],control['ids']),amount='100000000000000',passed=False)
 before=curve.pool(n['before'],n['ids']);after=curve.pool(n['after'],n['ids'])
 for p in [before,after]:p.update(protocol='pump-bonding-sol-migration',adapter_version='pump-bonding/7a19affaa2c6-sol-migration-1')
 # Explicitly use the post-acquisition local slot for the replayed after bank.
 for p in [before,after]:p['pump']['slot']=cases[0]['pool']['pump']['slot'];p['pump']['timestamp']=cases[0]['pool']['pump']['timestamp']
 TARGET.write_text(json.dumps(dict(schema_version=1,passed=True,generator_sha256=sha(__file__),migration_helper_sha256=sha(migration.__file__),amm_helper_sha256=sha(amm.native.__file__),migration=n,swaps=native_swaps,virtual_reserve_control=control,authority='controlled real migrate with Pump->PumpSwap->Token/Token2022/System/ATA CPIs and created-account handoff; headers explicitly local, no public executor qualification'),indent=2)+'\n')
 f=dict(schema_version=1,source_sha256=sha(TARGET),before=before,after=after,cases=cases,virtual_reserve_control=negative)
 for service in ['integration','dex-router']:(ROOT/service/'test/contracts/pump-migration.json').write_text(json.dumps(f,separators=(',',':'))+'\n')
 print(json.dumps(dict(passed=True,migration=1,created_account_swaps=len(cases))))
if __name__=='__main__':main()
