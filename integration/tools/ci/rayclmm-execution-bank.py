"""Controlled standard classic-SPL CLMM execution banks; no production claim."""
from pathlib import Path
import base64,copy,hashlib,json,subprocess,sys
ROOT=Path(__file__).resolve().parents[3];sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from r01_public_catalog_solana_bank import native_imports,LOADER
S,P,K,I,M,T,A,C=native_imports();PID='CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK';TOKEN='TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA';TOKEN22='TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb';MEMO='MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr';CB='ComputeBudget111111111111111111111111111111';CLOCK='SysvarC1ock11111111111111111111111111111111';SYSTEM='11111111111111111111111111111111';WSOL='So11111111111111111111111111111111111111112';ATA='ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL';RENT='SysvarRent111111111111111111111111111111111';vm=S().with_mainnet_features();rent=vm.minimum_balance_for_rent_exemption(165)
source_pool=ROOT/'integration/artifacts/local/r01-solana-process-main-confirmed-20261107/clmm/result.json';source_accounts=ROOT/'integration/test/source/deployment-catalog/solana-rebuild/clmm-sourcebank/accounts.json';p=json.loads(source_pool.read_bytes())['parser_baseline']['body']['pools'][0];s=p['raydium_clmm'];r=json.loads(source_accounts.read_bytes());mapping=dict(zip(r['params'][0],r['response']['result']['value']));ids=[p['pool_id'],s['amm_config'],s['vault0'],s['vault1'],s['observation'],p['token0'],p['token1'],PID,s['programdata'],TOKEN,str(P.find_program_address([bytes(P.from_string(TOKEN))],P.from_string(LOADER))[0]),MEMO,CLOCK]+[a['address']for a in s['tick_arrays']];shared={key:mapping[key]for key in ids};runtime=[]
program22=vm.get_account(P.from_string(TOKEN22));pd=str(P.from_bytes(bytes(program22.data)[4:36]))
for key in [TOKEN22,pd,CB,SYSTEM,ATA,RENT]:
 a=vm.get_account(P.from_string(key));shared[key]=dict(owner=str(a.owner),lamports=a.lamports,executable=a.executable,data=[base64.b64encode(a.data).decode(),'base64']);runtime.append(key)
caller=str(K.from_seed(hashlib.sha256(b'R01-public-native-bank-payer').digest()).pubkey());recipient=str(P.from_bytes(hashlib.sha256(b'R01-distinct-net-recipient').digest()));native_path=Path('/tmp/xone-solana-advanced-ray/debug/solana-advanced-raydium-clmm-reference');native=subprocess.Popen([str(native_path)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True)
def raw(a):return base64.b64decode(a['data'][0])
def ask(request):native.stdin.write(json.dumps(request,separators=(',',':'))+'\n');native.stdin.flush();return json.loads(native.stdout.readline())
def ata(owner,mint):return str(P.find_program_address([bytes(P.from_string(owner)),bytes(P.from_string(TOKEN)),bytes(P.from_string(mint))],P.from_string(ATA))[0])
def user(mint,owner,amount):
 b=bytearray(165);b[:32]=bytes(P.from_string(mint));b[32:64]=bytes(P.from_string(owner));b[64:72]=amount.to_bytes(8,'little');b[108]=1;lamports=rent
 if mint==WSOL:b[109:113]=(1).to_bytes(4,'little');b[113:121]=rent.to_bytes(8,'little');lamports+=amount
 return dict(owner=TOKEN,lamports=lamports,executable=False,data=[base64.b64encode(b).decode(),'base64'])
cases=[]
try:
 for mode in ['rewards','static','1','2']:
  pool=copy.deepcopy(p);state=pool['raydium_clmm'];overrides={}
  if mode!='rewards':
   pool['protocol']='raydium-clmm-static'if mode=='static'else'raydium-clmm-token-fee';pool['adapter_version']='raydium-clmm/6d7a0f82-spl-static-1'if mode=='static'else'raydium-clmm/6d7a0f82-spl-token-fee-1';state.pop('reward_infos',None);state['collect_fee_on']=0 if mode=='static'else int(mode)
   b=bytearray(raw(shared[pool['pool_id']]));b[390]=state['collect_fee_on']
   for i in range(3):off=397+169*i;authority=b[off+121:off+153];b[off:off+169]=bytes(169);b[off+121:off+153]=authority
   overrides[pool['pool_id']]=copy.deepcopy(shared[pool['pool_id']]);overrides[pool['pool_id']]['data'][0]=base64.b64encode(b).decode()
   for a in state['tick_arrays']:
    b=bytearray(raw(shared[a['address']]))
    for i in range(60):b[44+i*168+68:44+i*168+116]=bytes(48)
    overrides[a['address']]=copy.deepcopy(shared[a['address']]);overrides[a['address']]['data'][0]=base64.b64encode(b).decode()
    for t in a['ticks']:t.pop('reward_growths_outside',None)
  first=len(cases)
  for zero in [True,False]:
   for exact in [True,False]:
    n=1000 if exact else 1000000;q=ask(dict(action='quote_exact_out'if exact else'quote',pool=pool,zero_for_one=zero,**({'output':str(n)}if exact else{'input':str(n)})))
    if q.get('reverted'):raise ValueError(q)
    src=str(P.from_bytes(hashlib.sha256(f'rayclmm-{mode}-{zero}-source'.encode()).digest()));dst=str(P.from_bytes(hashlib.sha256(f'rayclmm-{mode}-{zero}-destination'.encode()).digest()));accounts=copy.deepcopy(overrides);accounts[src]=user(pool['token0']if zero else pool['token1'],caller,10**14);accounts[dst]=user(pool['token1']if zero else pool['token0'],recipient,7);accounts[caller]=dict(owner=SYSTEM,lamports=10**10,executable=False,data=['','base64']);paid=str(q['amount_in'])if exact else str(n)
    cases.append(dict(pool=copy.deepcopy(pool),accounts=accounts,caller=caller,recipient=recipient,zeroForOne=zero,exactOut=exact,target=str(n)if exact else q['amount_out'],sellAmount=paid,requiredInput=paid,actualOutput=q['amount_out'],after=q['next_pool'],sourceTokenAccount=src,destinationTokenAccount=dst,preparationOptions=dict(sourceTokenAccount=src,destinationTokenAccount=dst),executionScenario=f'{mode}-'+('forward'if zero else'reverse')+'-'+('exact-out'if exact else'exact-in'),genesis='5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d',blockhash=str(P.from_bytes(bytes([77])*32))))
  for exact in [True,False]:
   for scenario,zero in [('canonical-source-and-destination',True),('temporary-source-and-destination',True),('temporary-output-unwrap',False)]:
    b=copy.deepcopy(cases[first+(0 if zero else 2)+(0 if exact else 1)]);b['executionScenario']=f'{mode}-{scenario}-'+('exact-out'if exact else'exact-in');b['accounts'].pop(b['destinationTokenAccount'])
    if zero:
     b['accounts'].pop(b['sourceTokenAccount']);b['sourceTokenAccount']=ata(caller,WSOL);b['destinationTokenAccount']=ata(recipient,pool['token1']);b['preparationOptions']=dict(wrapSourceSol=True,createDestinationAta=True)
     if scenario.startswith('temporary'):b['preparationOptions']['refundWrappedSource']=True
    else:b['recipient']=caller;b['destinationTokenAccount']=ata(caller,WSOL);b['preparationOptions']=dict(sourceTokenAccount=b['sourceTokenAccount'],unwrapDestinationSol=True)
    cases.append(b)
    if zero:
     b=copy.deepcopy(b);b['executionScenario']+='-prefunded-destination';b['accounts'][b['destinationTokenAccount']]=dict(owner=SYSTEM,lamports=1000000,executable=False,data=['','base64']);cases.append(b)
  if len(cases)-first!=14:raise ValueError('Variant denominator')
finally:native.stdin.close();native.wait(timeout=20)
inputs={str(path.relative_to(ROOT))if path.is_relative_to(ROOT)else str(path):hashlib.sha256(path.read_bytes()).hexdigest()for path in [source_pool,source_accounts,native_path,Path(__file__)]};doc=dict(schema_version=1,authority='Controlled retained original Ray CLMM pool and original program with explicit known runtime closure',inputs=inputs,shared_accounts=shared,cases=cases,runtime_accounts={key:dict(owner=shared[key]['owner'],dataSha256=hashlib.sha256(raw(shared[key])).hexdigest())for key in runtime},limitations='Static reward clearing, fee mode, user balances, System/ATA/Token2022 program closure are explicit local controls. No Router API or historical consensus/production qualification. Token2022 token extensions not admitted.');text=json.dumps(doc,separators=(',',':'))+'\n';(ROOT/'integration/test/contracts/rayclmm-execution-bank.json').write_text(text);(ROOT/'dex-router/test/contracts/rayclmm-execution-bank.json').write_text(text);print(json.dumps(dict(passed=True,cases=len(cases),bytes=len(text),sha256=hashlib.sha256(text.encode()).hexdigest())))
