#!/usr/bin/env python3
"""Original PumpSwap canonical/boost swap banks for real unsigned preparation."""
from pathlib import Path
import base64,hashlib,importlib.util,json,sys
ROOT=Path(__file__).resolve().parents[3]
def module(name,path):
 spec=importlib.util.spec_from_file_location(name,ROOT/path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
ref=module('pump_prep_projection','integration/tools/ci/pump-amm-reference.py')
targets=module('pump_prep_sell_targets','integration/tools/ci/pump-amm-sell-target-reference.py')
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from r01_public_catalog_solana_bank import native_imports

def main():
 S,P,*_=native_imports();vm=S().with_mainnet_features()
 shared=json.loads((ROOT/'integration/test/contracts/pump-amm-decoder.json').read_bytes())['shared_accounts']
 migration=json.loads((ROOT/'integration/artifacts/local/core-pump-migration-original-20261111.json').read_bytes())['migration']
 cases=[]
 for boost in [False,True]:
  initial=migration if boost else None
  seed=ref.native.execute(kind='sell',amount=1,limit=1,initial_migration=initial)
  pool=ref.pool(seed['before'],seed['ids'])
  for kind in ['sell_exact_in','sell_exact_out','buy_exact_quote_in','buy']:
   zero=kind.startswith('sell');exact=kind in ['sell_exact_out','buy'];amount=1000 if exact else 1000000
   actual=targets.required(pool,amount) if kind=='sell_exact_out'else amount
   instruction='sell'if zero else kind
   limit=amount if kind=='sell_exact_out'else 10**15 if kind=='buy'else 1
   native=ref.native.execute(kind=instruction,amount=actual,limit=limit,initial_migration=initial)
   if not native['passed']:raise ValueError(('ORIGINAL_BANK_FAILED',boost,kind,native['native']))
   model=ref.pool(native['before'],native['ids']);after=ref.pool(native['after'],native['ids'])
   accounts=dict(shared)
   for key,a in native['before'].items():accounts[key]=dict(owner=a['owner'],lamports=a['lamports'],executable=a['executable'],data=[a['data_base64'],'base64'])
   for key in ['11111111111111111111111111111111','ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL','SysvarRent111111111111111111111111111111111']:
    a=vm.get_account(P.from_string(key))
    if a is None:raise ValueError('RUNTIME_ACCOUNT_REQUIRED:'+key)
    accounts[key]=dict(owner=str(a.owner),lamports=a.lamports,executable=a.executable,data=[base64.b64encode(a.data).decode(),'base64'])
   clock=model['pump']['slot'].to_bytes(8,'little')+(1794370000).to_bytes(8,'little')+(1000).to_bytes(8,'little')*2+model['pump']['timestamp'].to_bytes(8,'little')
   accounts['SysvarC1ock11111111111111111111111111111111']=dict(owner='Sysvar1111111111111111111111111111111111111',executable=False,lamports=1,data=[base64.b64encode(clock).decode(),'base64'])
   used=-int(native['user_base_delta']if zero else native['user_quote_delta']);out=int(native['user_quote_delta']if zero else native['user_base_delta'])
   ids=native['ids'];cases.append(dict(schema_version=1,sourceIndex=len(cases),boost=boost,kind=kind,exactOut=exact,zeroForOne=zero,pool=model,after=after,accounts=accounts,caller=ids['user'],recipient=ids['user'],sourceTokenAccount=ids['user_base_token_account']if zero else ids['user_quote_token_account'],destinationTokenAccount=ids['user_quote_token_account']if zero else ids['user_base_token_account'],target=str(amount if exact else out),sellAmount=str(used),requiredInput=str(used),actualOutput=str(out),genesis='5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d',blockhash=str(P.from_bytes(bytes([77])*32)),ids=ids,authority='constructed canonical and actual-migration-created boost original-SBF banks; user also pays fee in unsigned replay; no mainnet submission or production qualification'))
 # Explicit minimum-credit surplus: the original sell instruction consumes
 # one base atom and credits 48185, while its requested minimum is only 1000.
 c=json.loads((ROOT/'integration/test/contracts/pump-amm-sell-target.json').read_bytes())['cases'][38]
 native=targets.execute(True,int(c['required_input']),int(c['target']),True)
 if not native['passed']:raise ValueError('SURPLUS_ORIGINAL_REQUIRED')
 row=dict(cases[5]);row.update(sourceIndex=8,pool=c['pool'],after=c['after'],requiredInput=c['required_input'],sellAmount=c['required_input'],actualOutput=c['actual_output'])
 bank=dict(shared)
 for key,a in native['before'].items():bank[key]=dict(owner=a['owner'],lamports=a['lamports'],executable=a['executable'],data=[a['data_base64'],'base64'])
 for key in ['11111111111111111111111111111111','ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL','SysvarRent111111111111111111111111111111111','SysvarC1ock11111111111111111111111111111111']:bank[key]=cases[5]['accounts'][key]
 row['accounts']=bank;cases.append(row)
 for row in cases:
  row['accounts']={key:value for key,value in row['accounts'].items()if key not in shared}
 result=json.dumps(dict(shared_accounts=shared,cases=cases),separators=(',',':'))+'\n'
 for service in ['integration','dex-router']:(ROOT/service/'test/contracts/pump-amm-execution-bank.json').write_text(result)
 print(json.dumps(dict(passed=True,cases=len(cases),inputs=[c['requiredInput']for c in cases],outputs=[c['actualOutput']for c in cases],sha256=hashlib.sha256(result.encode()).hexdigest())))
if __name__=='__main__':main()
