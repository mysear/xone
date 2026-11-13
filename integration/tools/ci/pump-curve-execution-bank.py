#!/usr/bin/env python3
"""Original Pump curve native SOL banks, with user also paying network fee."""
from pathlib import Path
import base64,hashlib,importlib.util,json,sys
ROOT=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from r01_public_catalog_solana_bank import native_imports

def main():
 S,P,*_=native_imports();vm=S().with_mainnet_features()
 load=lambda p:json.loads((ROOT/p).read_bytes())
 shared=load('integration/test/contracts/pump-bonding-decoder.json')['shared_accounts']
 def module(name,path):
  spec=importlib.util.spec_from_file_location(name,ROOT/path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
 ref=module('curve_execution_projection','integration/tools/ci/pump-bonding-reference.py')
 targets=module('curve_execution_targets','integration/tools/ci/pump-sell-exact-out-reference.py')
 chosen=[('sell_exact_in',None),('sell_exact_out',None),('buy_exact_sol_in',None),('buy',None),('sell_exact_out',10**19)]
 cases=[]
 for kind,reserve in chosen:
  zero=kind.startswith('sell');exact=kind in ['sell_exact_out','buy'];target=1000 if exact else 1000000
  preview=targets.native.execute(kind='sell',amount=1,limit=1,quote_reserve=reserve)
  initial=ref.pool(preview['before'],preview['ids'])
  amount=targets.required(initial,target)if kind=='sell_exact_out'else target
  n=targets.native.execute(kind='sell'if zero else kind,amount=amount,limit=target if kind=='sell_exact_out'else 10**15 if kind=='buy'else 1,quote_reserve=reserve)
  if not n['passed']:raise ValueError('ORIGINAL_BANK_FAILED')
  model=ref.pool(n['before'],n['ids']);after=ref.pool(n['after'],n['ids'])
  accounts={}
  for key,a in n['before'].items():accounts[key]=dict(owner=a['owner'],lamports=a['lamports'],executable=a['executable'],data=[a['data_base64'],'base64'])
  for key in ['11111111111111111111111111111111','ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL','SysvarRent111111111111111111111111111111111']:
   a=vm.get_account(P.from_string(key))
   if a is None:raise ValueError('RUNTIME_ACCOUNT_REQUIRED:'+key)
   accounts[key]=dict(owner=str(a.owner),lamports=a.lamports,executable=a.executable,data=[base64.b64encode(a.data).decode(),'base64'])
  clock=model['pump']['slot'].to_bytes(8,'little')+(1794350000).to_bytes(8,'little')+(1000).to_bytes(8,'little')*2+model['pump']['timestamp'].to_bytes(8,'little')
  accounts['SysvarC1ock11111111111111111111111111111111']=dict(owner='Sysvar1111111111111111111111111111111111111',executable=False,lamports=1,data=[base64.b64encode(clock).decode(),'base64'])
  zero=kind.startswith('sell');exact=kind in ['sell_exact_out','buy'];used=-int(n['user_token_delta']if zero else n['user_sol_delta']);out=int(n['user_sol_delta']if zero else n['user_token_delta']);ids=n['ids'];target=int(n['limit'])if kind=='sell_exact_out'else out
  cases.append(dict(schema_version=1,sourceIndex=len(cases),kind=kind,exactOut=exact,zeroForOne=zero,pool=model,after=after,accounts={k:v for k,v in accounts.items()if k not in shared},caller=ids['user'],recipient=ids['user'],sourceTokenAccount=ids['user_ata']if zero else ids['user'],destinationTokenAccount=ids['user']if zero else ids['user_ata'],target=str(target),sellAmount=str(used),requiredInput=str(used),actualOutput=str(out),genesis='5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d',blockhash=str(P.from_bytes(bytes([77])*32)),ids=ids,authority='constructed original-SBF native SOL curve banks; user pays network fee in unsigned replay; finite local scope'))
 result=json.dumps(dict(shared_accounts=shared,cases=cases),separators=(',',':'))+'\n'
 for service in ['integration','dex-router']:(ROOT/service/'test/contracts/pump-curve-execution-bank.json').write_text(result)
 print(json.dumps(dict(passed=True,cases=len(cases),inputs=[c['requiredInput']for c in cases],outputs=[c['actualOutput']for c in cases],sha256=hashlib.sha256(result.encode()).hexdigest())))
if __name__=='__main__':main()
