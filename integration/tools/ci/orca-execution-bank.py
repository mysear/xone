#!/usr/bin/env python3
"""Controlled classic Whirlpool instruction banks around retained original bytes.

The published Pool/Clock/program identity stays fixed. Static reward removal,
fixed-to-dynamic storage conversion, user funds and runtime accounts are explicit
local controls; none of these declares historical consensus or production trust.
"""
from pathlib import Path
import base64,copy,hashlib,json,subprocess,sys
ROOT=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from r01_public_catalog_solana_bank import native_imports
TOKEN='TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA';WSOL='So11111111111111111111111111111111111111112';SYSTEM='11111111111111111111111111111111';ATA='ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL';BUDGET='ComputeBudget111111111111111111111111111111';RENT='SysvarRent111111111111111111111111111111111'
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def raw(a):return base64.b64decode(a['data'][0],validate=True)
def main():
    original=ROOT/'integration/artifacts/local/r01-solana-process-main-confirmed-20261107/orca/result.json'
    source=ROOT/'integration/test/source/deployment-catalog/solana-rebuild/orca-sourcebank/accounts.json'
    helper=ROOT/'integration/tools/reference/orca-whirlpools/target/release/solana-advanced-orca-reference'
    pool=json.loads(original.read_bytes())['parser_baseline']['body']['pools'][0]
    record=json.loads(source.read_bytes());mapping=dict(zip(record['params'][0],record['response']['result']['value']));s=pool['orca_whirlpool']
    if pool['protocol']!='orca-whirlpools-rewards'or hashlib.sha256(raw(mapping[s['programdata']])[45:]).hexdigest()!=s['program_code_sha256']:raise ValueError('Original source program binding')
    S,P,K,I,M,T,A,C=native_imports();vm=S().with_mainnet_features();rent=vm.minimum_balance_for_rent_exemption(165)
    ids=[pool['pool_id'],s['whirlpools_config'],s['vault0'],s['vault1'],pool['token0'],pool['token1'],s['program_id'],s['programdata'],TOKEN,str(P.find_program_address([bytes(P.from_string(TOKEN))],P.from_string('BPFLoaderUpgradeab1e11111111111111111111111'))[0]),'SysvarC1ock11111111111111111111111111111111']+[a['address']for a in s['tick_arrays']]
    common={k:mapping[k]for k in ids}
    closure={}
    for key in [SYSTEM,ATA,RENT,BUDGET]:
        a=vm.get_account(P.from_string(key));closure[key]=dict(owner=str(a.owner),lamports=a.lamports,executable=a.executable,data=[base64.b64encode(a.data).decode(),'base64'])
    common.update(closure)
    if hashlib.sha256(raw(closure[ATA])).hexdigest()!='6804554e69fd3a58caa191dc4a58f4c67223d30ca28ab8987f39fc18d2f7374d'or raw(closure[BUDGET])!=b'compute_budget_program':raise ValueError('Pinned runtime program identity')
    caller=str(K.from_seed(hashlib.sha256(b'R01-public-native-bank-payer').digest()).pubkey());recipient=str(P.from_bytes(hashlib.sha256(b'R01-distinct-net-recipient').digest()))
    def ata(owner,mint):return str(P.find_program_address([bytes(P.from_string(owner)),bytes(P.from_string(TOKEN)),bytes(P.from_string(mint))],P.from_string(ATA))[0])
    def account(key,mint,owner,amount):
        b=bytearray(165);b[:32]=bytes(P.from_string(mint));b[32:64]=bytes(P.from_string(owner));b[64:72]=amount.to_bytes(8,'little');b[108]=1;lamports=rent
        if mint==WSOL:b[109:113]=(1).to_bytes(4,'little');b[113:121]=rent.to_bytes(8,'little');lamports+=amount
        return dict(owner=TOKEN,executable=False,lamports=lamports,data=[base64.b64encode(b).decode(),'base64'])
    native=subprocess.Popen([str(helper)],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True)
    def quote(p,zero,exact,n):
        native.stdin.write(json.dumps(dict(action='quote_exact_out'if exact else'quote',pool=p,zero_for_one=zero,**({'output':str(n)}if exact else{'input':str(n)})))+'\n');native.stdin.flush();q=json.loads(native.stdout.readline())
        if q.get('reverted'):raise ValueError(q)
        return q
    cases=[]
    try:
        for profile,layout in [('rewards','fixed'),('static','fixed'),('rewards','dynamic')]:
            p=copy.deepcopy(pool);overrides={}
            if profile=='static':
                p['protocol']='orca-whirlpools-static';p['adapter_version']='orca-whirlpools/cc683ec9-spl-static-1';p['orca_whirlpool'].pop('reward_infos');b=bytearray(raw(common[p['pool_id']]));authority=b[333:365];b[269:653]=bytes(384);b[333:365]=authority;overrides[p['pool_id']]=copy.deepcopy(common[p['pool_id']]);overrides[p['pool_id']]['data'][0]=base64.b64encode(b).decode()
                for a in p['orca_whirlpool']['tick_arrays']:
                    b=bytearray(raw(common[a['address']]))
                    for i in range(88):b[12+i*113+65:12+(i+1)*113]=bytes(48)
                    overrides[a['address']]=copy.deepcopy(common[a['address']]);overrides[a['address']]['data'][0]=base64.b64encode(b).decode()
                    for t in a['ticks']:t.pop('reward_growths_outside')
            if layout=='dynamic':
                for a in p['orca_whirlpool']['tick_arrays']:
                    a['layout']='dynamic';before=raw(common[a['address']]);b=bytearray(hashlib.sha256(b'account:DynamicTickArray').digest()[:8]+int(a['start_tick_index']).to_bytes(4,'little',signed=True)+bytes(P.from_string(p['pool_id']))+int(a['initialized_bitmap']).to_bytes(16,'little'))
                    for i in range(88):
                        t=before[12+i*113:12+(i+1)*113];b+=t if t[0]else b'\0'
                    overrides[a['address']]=copy.deepcopy(common[a['address']]);overrides[a['address']]['data'][0]=base64.b64encode(b).decode()
            for zero in [True,False]:
                for exact in [True,False]:
                    n=1000 if exact else 1000000;q=quote(p,zero,exact,n);source_key=str(P.from_bytes(hashlib.sha256(f'xone-orca-source-{zero}'.encode()).digest()));destination=str(P.from_bytes(hashlib.sha256(f'xone-orca-destination-{zero}'.encode()).digest()));accounts=copy.deepcopy(overrides)
                    for key,mint,owner,quantity in [(source_key,p['token0']if zero else p['token1'],caller,10**15),(destination,p['token1']if zero else p['token0'],recipient,7)]:accounts[key]=account(key,mint,owner,quantity)
                    accounts[caller]=dict(owner=SYSTEM,lamports=10**10,executable=False,data=['','base64'])
                    cases.append(dict(pool=copy.deepcopy(p),accounts=accounts,caller=caller,recipient=recipient,zeroForOne=zero,exactOut=exact,target=str(n if exact else q['amount_out']),sellAmount=str(q['amount_in']if exact else n),requiredInput=str(q['amount_in']if exact else n),actualOutput=q['amount_out'],after=q['next_pool'],sourceTokenAccount=source_key,destinationTokenAccount=destination,preparationOptions=dict(sourceTokenAccount=source_key,destinationTokenAccount=destination),executionScenario=f'{profile}-{layout}-'+('a-to-b'if zero else'b-to-a')+'-'+('exact-out'if exact else'exact-in'),genesis='5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d',blockhash=str(P.from_bytes(bytes([77])*32))))
        # Larger actual native target crosses initialized ticks and changes their
        # fee/reward outside ledgers. Keep full seven-array source observation.
        for index,target in [(0,100000000000),(2,1000000000000)]:
            b=copy.deepcopy(cases[index]);q=quote(b['pool'],b['zeroForOne'],True,target);b.update(target=str(target),sellAmount=q['amount_in'],requiredInput=q['amount_in'],actualOutput=q['amount_out'],after=q['next_pool'],executionScenario=b['executionScenario']+'-tick-cross');cases.append(b)
        for exact in [True,False]:
            for scenario,index in [('canonical-source-and-destination',0 if exact else 1),('temporary-source-and-destination',0 if exact else 1),('temporary-output-unwrap',2 if exact else 3)]:
                b=copy.deepcopy(cases[index]);b['executionScenario']=scenario+'-'+('exact-out'if exact else'exact-in');old_in=b['sourceTokenAccount'];old_out=b['destinationTokenAccount'];b['accounts'].pop(old_out,None)
                if b['zeroForOne']:
                    b['accounts'].pop(old_in,None);b['sourceTokenAccount']=ata(caller,WSOL);b['destinationTokenAccount']=ata(recipient,b['pool']['token1']);b['preparationOptions']=dict(wrapSourceSol=True,createDestinationAta=True)
                    if scenario.startswith('temporary'):b['preparationOptions']['refundWrappedSource']=True
                else:
                    b['recipient']=caller;b['destinationTokenAccount']=ata(caller,WSOL);b['preparationOptions']=dict(sourceTokenAccount=old_in,unwrapDestinationSol=True)
                cases.append(b)
        for index in [14,15,17,18]:
            b=copy.deepcopy(cases[index]);b['executionScenario']+='-prefunded-destination';b['accounts'][b['destinationTokenAccount']]=dict(owner=SYSTEM,lamports=1000000,executable=False,data=['','base64']);cases.append(b)
        # Quote windows can end after one or two directional arrays. The
        # legacy ABI repeats the last observed account, while the original
        # builder deduplicates it; no outside-window empty array is invented.
        for base_index in [0,8]:
            for zero in [True,False]:
                for available in [1,2]:
                    b=copy.deepcopy(cases[base_index+(0 if zero else 2)]);s=b['pool']['orca_whirlpool'];span=s['tick_spacing']*88;center=s['tick_current_index']//span*span
                    low=center if zero and available==1 else center-2*span if not zero and available==1 else center-span
                    s['tick_arrays']=[a for a in s['tick_arrays']if low<=a['start_tick_index']<=low+2*span]
                    if len(s['tick_arrays'])!=3:raise ValueError('Controlled edge window')
                    q=quote(b['pool'],zero,True,1000);b.update(requiredInput=q['amount_in'],sellAmount=q['amount_in'],actualOutput=q['amount_out'],after=q['next_pool'],executionScenario=b['executionScenario']+f'-window-{available}')
                    cases.append(b)
    finally:native.stdin.close();native.wait(timeout=20)
    inputs={str(p.relative_to(ROOT)):sha(p)for p in [Path(__file__),original,source,helper]}
    doc=dict(schema_version=1,authority='CONTROLLED_RETAINED_ORCA_CLASSIC_SBF_BANK',inputs=inputs,runtime_accounts={key:dict(owner=a['owner'],sha256=hashlib.sha256(raw(a)).hexdigest())for key,a in closure.items()},shared_accounts=common,cases=cases,limitations='Static reward removal, dynamic array conversion, wallets and native runtime accounts are explicit controls; no historical consensus, independent deployment qualification or production claim.')
    text=json.dumps(doc,separators=(',',':'))+'\n'
    for service in ['integration','dex-router']:(ROOT/service/'test/contracts/orca-execution-bank.json').write_text(text)
    print(json.dumps(dict(passed=True,cases=len(cases),sha256=hashlib.sha256(text.encode()).hexdigest(),shared_accounts=len(common),bytes=len(text),profiles=sorted({b['pool']['protocol']for b in cases}))))
if __name__=='__main__':main()
