#!/usr/bin/env python3
"""Controlled adaptive SPL/WSOL execution banks with explicit real-code closure."""
from pathlib import Path
import argparse,base64,copy,hashlib,importlib.util,json,subprocess,sys
ROOT=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from r01_public_catalog_solana_bank import native_imports
TOKEN='TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA';PID='whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc';WSOL='So11111111111111111111111111111111111111112';SYSTEM='11111111111111111111111111111111';LOADER='BPFLoaderUpgradeab1e11111111111111111111111';CLOCK='SysvarC1ock11111111111111111111111111111111';ATA='ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL';BUDGET='ComputeBudget111111111111111111111111111111';RENT='SysvarRent111111111111111111111111111111111'
def raw(a):return base64.b64decode(a['data'][0],validate=True)
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def load_module(name,path):
 spec=importlib.util.spec_from_file_location(name,path);m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
class Native:
 def __init__(self,path):self.path=path;self.child=subprocess.Popen([str(path)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True)
 def ask(self,request):
  self.child.stdin.write(json.dumps(request,separators=(',',':'))+'\n');self.child.stdin.flush();line=self.child.stdout.readline()
  if not line:raise ValueError('Native reference stopped')
  result=json.loads(line)
  if result.get('fatal')or result.get('status')=='invalid_request':raise ValueError(result)
  return result
 def close(self):self.child.stdin.close();assert self.child.wait(timeout=20)==0

def normalize(config,bank,actual,source_state,P,wsol):
 config=copy.deepcopy(config);bank=copy.deepcopy(bank);proof=config['orca_whirlpool'];slot=source_state['bank_slot'];source_code=source_state['program_code_sha256'];token_code=source_state['token_program_code_sha256']
 for key in [PID,proof['expected_programdata'],TOKEN,str(P.find_program_address([bytes(P.from_string(TOKEN))],P.from_string(LOADER))[0])]:
  a=copy.deepcopy(actual[key]);a['space']=len(raw(a));bank['accounts'][key]=a
 proof.update(expected_program_code_sha256=source_code,expected_deployed_slot=source_state['deployed_slot'],expected_token_program_code_sha256=token_code)
 bank['slot']=slot;clock=bytearray(raw(bank['accounts'][CLOCK]));clock[:8]=slot.to_bytes(8,'little');bank['accounts'][CLOCK]['data'][0]=base64.b64encode(clock).decode()
 if wsol:
  old_pool=config['id'];old_mint=config['token0'];b=bytearray(raw(bank['accounts'][old_pool]));tier=int.from_bytes(b[43:45],'little');new_pool,bump=P.find_program_address([b'whirlpool',bytes(P.from_string(config['accounts'][3])),bytes(P.from_string(WSOL)),bytes(P.from_string(config['token1'])),tier.to_bytes(2,'little')],P.from_string(PID));new_pool=str(new_pool)
  if bytes(P.from_string(WSOL))>=bytes(P.from_string(config['token1'])):raise ValueError('Controlled mint order')
  config.update(id=new_pool,token0=WSOL);config['accounts'][0]=new_pool;config['accounts'][4]=WSOL;b[40]=bump;b[101:133]=bytes(P.from_string(WSOL));a=bank['accounts'].pop(old_pool);a['data'][0]=base64.b64encode(b).decode();bank['accounts'][new_pool]=a;bank['accounts'].pop(old_mint,None);a=copy.deepcopy(actual[WSOL]);a['space']=len(raw(a));bank['accounts'][WSOL]=a
  for key in config['accounts'][1:3]:
   a=bank['accounts'][key];b=bytearray(raw(a));b[32:64]=bytes(P.from_string(new_pool))
   if key==config['accounts'][1]:b[:32]=bytes(P.from_string(WSOL));b[109:113]=(1).to_bytes(4,'little');b[113:121]=(2039280).to_bytes(8,'little');a['lamports']=2039280+int.from_bytes(b[64:72],'little')
   a['data'][0]=base64.b64encode(b).decode()
  old_oracle=config['fixture']['oracle'];new_oracle=str(P.find_program_address([b'oracle',bytes(P.from_string(new_pool))],P.from_string(PID))[0]);config['fixture']['oracle']=new_oracle;a=bank['accounts'].pop(old_oracle);b=bytearray(raw(a));b[8:40]=bytes(P.from_string(new_pool));a['data'][0]=base64.b64encode(b).decode();bank['accounts'][new_oracle]=a
  for i,old_key in enumerate(config['fixture']['tick_arrays']):
   a=bank['accounts'].pop(old_key);b=bytearray(raw(a));start=int.from_bytes(b[8:12],'little',signed=True);new_key=str(P.find_program_address([b'tick_array',bytes(P.from_string(new_pool)),str(start).encode()],P.from_string(PID))[0]);config['fixture']['tick_arrays'][i]=new_key;offset=12 if b[:8]==hashlib.sha256(b'account:DynamicTickArray').digest()[:8]else 9956;b[offset:offset+32]=bytes(P.from_string(new_pool));a['data'][0]=base64.b64encode(b).decode();bank['accounts'][new_key]=a
 bank['source_slots']={key:slot for key in bank['accounts']};return config,bank

def main():
 parser=argparse.ArgumentParser();parser.add_argument('--output',default=str(ROOT/'integration/test/contracts/orca-adaptive-execution-bank.json'));args=parser.parse_args();source_path=ROOT/'integration/test/contracts/orca-execution-bank.json';source=json.loads(source_path.read_bytes());actual=source['shared_accounts'];source_state=source['cases'][0]['pool']['orca_whirlpool'];S,P,K,I,M,T,A,C=native_imports();vm=S().with_mainnet_features();rent=vm.minimum_balance_for_rent_exemption(165)
 caller=str(K.from_seed(hashlib.sha256(b'R01-public-native-bank-payer').digest()).pubkey());recipient=str(P.from_bytes(hashlib.sha256(b'R01-distinct-net-recipient').digest()));shared={};cases=[];inputs={str(source_path.relative_to(ROOT)):sha(source_path)}
 for key in [PID,source_state['programdata'],TOKEN,str(P.find_program_address([bytes(P.from_string(TOKEN))],P.from_string(LOADER))[0]),WSOL]:shared[key]=actual[key]
 for key in [SYSTEM,ATA,BUDGET,RENT]:
  a=vm.get_account(P.from_string(key));shared[key]=dict(owner=str(a.owner),lamports=a.lamports,executable=a.executable,data=[base64.b64encode(a.data).decode(),'base64'])
 def ata(owner,mint):return str(P.find_program_address([bytes(P.from_string(owner)),bytes(P.from_string(TOKEN)),bytes(P.from_string(mint))],P.from_string(ATA))[0])
 def token_account(mint,owner,amount):
  b=bytearray(165);b[:32]=bytes(P.from_string(mint));b[32:64]=bytes(P.from_string(owner));b[64:72]=amount.to_bytes(8,'little');b[108]=1;lamports=rent
  if mint==WSOL:b[109:113]=(1).to_bytes(4,'little');b[113:121]=rent.to_bytes(8,'little');lamports+=amount
  return dict(owner=TOKEN,lamports=lamports,executable=False,data=[base64.b64encode(b).decode(),'base64'])
 for family in ['orca-adaptive-service','orca-adaptive-rewards-service','orca-adaptive-dynamic-service']:
  source_file=ROOT/'integration/test/contracts'/(family+'.json');binary=Path('/tmp/xone-orca-adaptive-swap-native/debug')/(family+'-reference');projection_path=ROOT/'integration/tools/acceptance'/(family.replace('-service','').replace('-','_')+'_fixture.py');projection=load_module(family.replace('-','_'),projection_path);d=json.loads(source_file.read_bytes());native=Native(binary)
  for path in [source_file,binary,projection_path]:inputs[str(path.relative_to(ROOT))if path.is_relative_to(ROOT)else str(path)]=sha(path)
  start_index=len(cases);wsol_base={}
  try:
   for wsol in [False,True]:
    config,bank=normalize(d['scenarios'][0]['config'],d['scenarios'][0]['banks'][0],actual,source_state,P,wsol)
    raw_bank={k:bank[k]for k in ['slot','timestamp','source_slots','accounts']};state=native.ask(dict(action='state',config=config,bank=raw_bank));p=projection.pool(config,state);p['chain_key']='solana:mainnet-beta'
    for zero in [True,False]:
     for exact in [True,False]:
      params=dict(amount='1000',a_to_b=zero,sqrt_price_limit='0',timestamp=bank['timestamp'],max_steps=8192,amount_specified_is_input=not exact);q=native.ask(dict(action='trade',config=config,bank=raw_bank,params=params))
      if q['status']!='fulfilled'or q['amount_remaining']!='0':raise ValueError(q)
      after=projection.pool(config,q['next_state']);after['chain_key']=p['chain_key'];source_key=str(P.from_bytes(hashlib.sha256((family+str(wsol)+str(zero)+'source').encode()).digest()));destination=str(P.from_bytes(hashlib.sha256((family+str(wsol)+str(zero)+'destination').encode()).digest()));accounts={k:copy.deepcopy(a)for k,a in bank['accounts'].items()if k not in shared}
      accounts[caller]=dict(owner=SYSTEM,lamports=10**10,executable=False,data=['','base64']);accounts[source_key]=token_account(p['token0']if zero else p['token1'],caller,10**13);accounts[destination]=token_account(p['token1']if zero else p['token0'],recipient,7)
      b=dict(pool=copy.deepcopy(p),accounts=accounts,caller=caller,recipient=recipient,zeroForOne=zero,exactOut=exact,target=q['amount_out'],sellAmount=q['amount_in'],requiredInput=q['amount_in'],actualOutput=q['amount_out'],after=after,sourceTokenAccount=source_key,destinationTokenAccount=destination,preparationOptions=dict(sourceTokenAccount=source_key,destinationTokenAccount=destination),executionScenario=family+'-'+('wsol'if wsol else'tokens')+'-'+('forward'if zero else'reverse')+'-'+('exact-out'if exact else'exact-in'),genesis='5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d',blockhash=str(P.from_bytes(bytes([77])*32)),reference=dict(family=family,config=config,params=params,before_native_state=state))
      cases.append(b)
      if wsol:wsol_base[(zero,exact)]=len(cases)-1
   for exact in [True,False]:
    for scenario,zero in [('canonical-source-and-destination',True),('temporary-source-and-destination',True),('temporary-output-unwrap',False)]:
     b=copy.deepcopy(cases[wsol_base[(zero,exact)]]);b['executionScenario']=family+'-'+scenario+'-'+('exact-out'if exact else'exact-in');b['accounts'].pop(b['destinationTokenAccount'])
     if zero:
      b['accounts'].pop(b['sourceTokenAccount']);b['sourceTokenAccount']=ata(caller,WSOL);b['destinationTokenAccount']=ata(recipient,b['pool']['token1']);b['preparationOptions']=dict(wrapSourceSol=True,createDestinationAta=True)
      if scenario.startswith('temporary'):b['preparationOptions']['refundWrappedSource']=True
     else:b['recipient']=caller;b['destinationTokenAccount']=ata(caller,WSOL);b['preparationOptions']=dict(sourceTokenAccount=b['sourceTokenAccount'],unwrapDestinationSol=True)
     cases.append(b)
     if zero:
      b=copy.deepcopy(b);b['executionScenario']+='-prefunded-destination';b['accounts'][b['destinationTokenAccount']]=dict(owner=SYSTEM,lamports=1000000,executable=False,data=['','base64']);cases.append(b)
   if len(cases)-start_index!=18:raise ValueError('Explicit per-profile denominator')
  finally:native.close()
 inputs[str(Path(__file__).relative_to(ROOT))]=sha(__file__);text=json.dumps(dict(schema_version=1,authority='CONTROLLED_ADAPTIVE_CLASSIC_SPL_WITH_PUBLISHED_PROGRAM_BYTES',inputs=inputs,shared_accounts=shared,cases=cases,limitations='Native struct banks, PDA/WSOL transformations, wallet funding, slot alignment and actual bytecode closure are explicit local controls; no historical consensus or production claim.'),separators=(',',':'))+'\n';Path(args.output).write_text(text);
 if Path(args.output).resolve()==(ROOT/'integration/test/contracts/orca-adaptive-execution-bank.json').resolve():(ROOT/'dex-router/test/contracts/orca-adaptive-execution-bank.json').write_text(text)
 print(json.dumps(dict(passed=True,cases=len(cases),bytes=len(text),sha256=hashlib.sha256(text.encode()).hexdigest())))
if __name__=='__main__':main()
