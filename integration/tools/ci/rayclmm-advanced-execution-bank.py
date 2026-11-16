"""Explicit controlled advanced CLMM banks from original structs and pinned code."""
from pathlib import Path
import base64,copy,hashlib,json,subprocess,sys
ROOT=Path(__file__).resolve().parents[3];sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from r01_public_catalog_solana_bank import native_imports,LOADER
S,P,K,I,M,T,A,C=native_imports();PID='CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK';TOKEN='TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA';TOKEN22='TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb';MEMO='MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr';CB='ComputeBudget111111111111111111111111111111';CLOCK='SysvarC1ock11111111111111111111111111111111';SYSTEM='11111111111111111111111111111111';WSOL='So11111111111111111111111111111111111111112';ATA='ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL';RENT='SysvarRent111111111111111111111111111111111'
basefile=ROOT/'integration/test/contracts/rayclmm-execution-bank.json';base=json.loads(basefile.read_bytes());source=base['shared_accounts'];original_state=base['cases'][0]['pool']['raydium_clmm'];shared={key:copy.deepcopy(source[key])for key in [PID,original_state['programdata'],TOKEN,str(P.find_program_address([bytes(P.from_string(TOKEN))],P.from_string(LOADER))[0]),TOKEN22,str(P.find_program_address([bytes(P.from_string(TOKEN22))],P.from_string(LOADER))[0]),MEMO,CB,SYSTEM,ATA,RENT,WSOL]};caller=str(K.from_seed(hashlib.sha256(b'R01-public-native-bank-payer').digest()).pubkey());recipient=str(P.from_bytes(hashlib.sha256(b'R01-distinct-net-recipient').digest()));nativefile=Path('/tmp/xone-solana-advanced-ray/debug/solana-advanced-raydium-clmm-reference');native=subprocess.Popen([str(nativefile)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True);inputs={str(basefile.relative_to(ROOT)):hashlib.sha256(basefile.read_bytes()).hexdigest(),str(nativefile):hashlib.sha256(nativefile.read_bytes()).hexdigest()};cases=[]
def ask(request):native.stdin.write(json.dumps(request,separators=(',',':'))+'\n');native.stdin.flush();line=native.stdout.readline();assert line;return json.loads(line)
def pub(key):return bytes(P.from_string(key))
def pda(seeds):return P.find_program_address(seeds,P.from_string(PID))
def ata(owner,mint):return str(P.find_program_address([pub(owner),pub(TOKEN),pub(mint)],P.from_string(ATA))[0])
def user(mint,owner,amount):
 b=bytearray(165);b[:32]=pub(mint);b[32:64]=pub(owner);b[64:72]=amount.to_bytes(8,'little');b[108]=1;rent=2039280;lamports=rent
 if mint==WSOL:b[109:113]=(1).to_bytes(4,'little');b[113:121]=rent.to_bytes(8,'little');lamports+=amount
 return dict(owner=TOKEN,lamports=lamports,executable=False,data=[base64.b64encode(b).decode(),'base64'])
try:
 for filename in ['rayclmm-bitmap-quotes.json','rayclmm-seed-quotes.json','rayclmm-dynamic-quotes.json']:
  path=ROOT/'integration/test/contracts'/filename;inputs[str(path.relative_to(ROOT))]=hashlib.sha256(path.read_bytes()).hexdigest();original=json.loads(path.read_bytes())['base_pool']
  for mode in [0,1,2]:
   wsol_indices={}
   for wsol in [False,True]:
    pool=copy.deepcopy(original);pool['chain_key']='solana:mainnet-beta';state=pool['raydium_clmm'];state.update(collect_fee_on=mode,bank_slot=original_state['bank_slot'],deployed_slot=original_state['deployed_slot'],program_code_sha256=original_state['program_code_sha256'],token_program_code_sha256=original_state['token_program_code_sha256'])
    if wsol:
     pool['token0']=WSOL;state['mint0_decimals']=9;seed=state.get('seed_index',0);seeds=[b'pool',pub(state['amm_config']),pub(pool['token0']),pub(pool['token1'])]+([seed.to_bytes(2,'little')]if seed else[]);key,bump=pda(seeds);pool['pool_id']=str(key);state['pool_bump']=bump
     for side in [0,1]:state[f'vault{side}']=str(pda([b'pool_vault',bytes(key),pub(pool[f'token{side}'])])[0])
     state['observation']=str(pda([b'observation',bytes(key)])[0]);state['bitmap_extension']['address']=str(pda([b'pool_tick_array_bitmap_extension',bytes(key)])[0])
     for a in state['tick_arrays']:a['address']=str(pda([b'tick_array',bytes(key),a['start_tick_index'].to_bytes(4,'big',signed=True)])[0])
    bank=ask(dict(action='fixture_target',pool=pool))['rpc_accounts'];accounts={key:copy.deepcopy(a)for key,a in bank.items()if key not in shared}
    if wsol:
     accounts[state['vault0']]=user(WSOL,pool['pool_id'],state['raw_vault0']);accounts[pool['token0']]=shared[WSOL]
    for zero in [True,False]:
     for exact in [True,False]:
      n=1000 if exact else 1000000;q=ask(dict(action='quote_exact_out'if exact else'quote',pool=pool,zero_for_one=zero,**({'output':str(n)}if exact else{'input':str(n)})))
      if q.get('reverted'):raise ValueError((pool['protocol'],mode,wsol,zero,exact,q))
      source_key=str(P.from_bytes(hashlib.sha256(f"advanced-{pool['protocol']}-{mode}-{wsol}-{zero}-input".encode()).digest()));destination=str(P.from_bytes(hashlib.sha256(f"advanced-{pool['protocol']}-{mode}-{wsol}-{zero}-output".encode()).digest()));case_accounts=copy.deepcopy(accounts);case_accounts[caller]=dict(owner=SYSTEM,lamports=10**10,executable=False,data=['','base64']);case_accounts[source_key]=user(pool['token0']if zero else pool['token1'],caller,10**14);case_accounts[destination]=user(pool['token1']if zero else pool['token0'],recipient,7);paid=q['amount_in']if exact else str(n)
      b=dict(pool=copy.deepcopy(pool),accounts=case_accounts,caller=caller,recipient=recipient,zeroForOne=zero,exactOut=exact,target=q['amount_out'],sellAmount=paid,requiredInput=paid,actualOutput=q['amount_out'],after=q['next_pool'],sourceTokenAccount=source_key,destinationTokenAccount=destination,preparationOptions=dict(sourceTokenAccount=source_key,destinationTokenAccount=destination),executionScenario=f"{pool['protocol']}-fee{mode}-"+('wsol'if wsol else'tokens')+'-'+('forward'if zero else'reverse')+'-'+('exact-out'if exact else'exact-in'),genesis='5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d',blockhash=str(P.from_bytes(bytes([77])*32)));cases.append(b)
      if wsol:wsol_indices[(zero,exact)]=len(cases)-1
   for exact in [True,False]:
    for scenario,zero in [('canonical-source-and-destination',True),('temporary-source-and-destination',True),('temporary-output-unwrap',False)]:
     b=copy.deepcopy(cases[wsol_indices[(zero,exact)]]);b['executionScenario']+='-'+scenario;b['accounts'].pop(b['destinationTokenAccount'])
     if zero:
      b['accounts'].pop(b['sourceTokenAccount']);b['sourceTokenAccount']=ata(caller,WSOL);b['destinationTokenAccount']=ata(recipient,b['pool']['token1']);b['preparationOptions']=dict(wrapSourceSol=True,createDestinationAta=True)
      if scenario.startswith('temporary'):b['preparationOptions']['refundWrappedSource']=True
     else:b['recipient']=caller;b['destinationTokenAccount']=ata(caller,WSOL);b['preparationOptions']=dict(sourceTokenAccount=b['sourceTokenAccount'],unwrapDestinationSol=True)
     cases.append(b)
     if zero:
      b=copy.deepcopy(b);b['executionScenario']+='-prefunded-destination';b['accounts'][b['destinationTokenAccount']]=dict(owner=SYSTEM,lamports=1000000,executable=False,data=['','base64']);cases.append(b)
finally:native.stdin.close();native.wait(timeout=20)
assert len(cases)==162;inputs[str(Path(__file__).relative_to(ROOT))]=hashlib.sha256(Path(__file__).read_bytes()).hexdigest();doc=dict(schema_version=1,authority='Controlled advanced original structs with explicit published program closure and declared mint/PDA changes',inputs=inputs,shared_accounts=shared,cases=cases,limitations='All three models x all three fee modes, ordinary and controlled WSOL pairs, explicit ATA/wrap/refund/unwrap/prefund; no Router API, historical consensus, production claim or Token2022 assets');text=json.dumps(doc,separators=(',',':'))+'\n';(ROOT/'integration/test/contracts/rayclmm-advanced-execution-bank.json').write_text(text);(ROOT/'dex-router/test/contracts/rayclmm-advanced-execution-bank.json').write_text(text);print(json.dumps(dict(passed=True,cases=len(cases),bytes=len(text),sha256=hashlib.sha256(text.encode()).hexdigest())))
