#!/usr/bin/env python3
"""Controlled original CP-Swap banks with explicit runtime program closures.

Original source Pool/Clock/code identities stay fixed. System/ATA/Rent bytes are
observed from the pinned local runtime, not asserted to be historical chain data.
"""
from pathlib import Path
import base64,copy,hashlib,json,subprocess,sys
ROOT=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from r01_public_catalog_solana_bank import native_imports
TOKEN='TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'
WSOL='So11111111111111111111111111111111111111112'
SYSTEM='11111111111111111111111111111111'
ATA='ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL'
RENT='SysvarRent111111111111111111111111111111111'
def main():
    source=ROOT/'integration/test/contracts/raydium-execution-bank.json';base=json.loads(source.read_bytes())
    S,P,K,I,M,T,A,C=native_imports();vm=S().with_mainnet_features();rent=vm.minimum_balance_for_rent_exemption(165)
    closure={}
    for pid in [SYSTEM,ATA,RENT]:
        a=vm.get_account(P.from_string(pid))
        if a is None:raise ValueError('Missing explicit runtime account '+pid)
        closure[pid]=dict(owner=str(a.owner),lamports=a.lamports,executable=a.executable,data=[base64.b64encode(a.data).decode(),'base64'])
    if hashlib.sha256(base64.b64decode(closure[ATA]['data'][0])).hexdigest()!='6804554e69fd3a58caa191dc4a58f4c67223d30ca28ab8987f39fc18d2f7374d':raise ValueError('Associated program identity')
    if hashlib.sha256(base64.b64decode(closure[SYSTEM]['data'][0])).hexdigest()!='d8e11c483f20607a1e5f015d768e4dd294213e33b948a69640f7bacaaf25e280':raise ValueError('System program identity')
    def ata(owner,mint):return str(P.find_program_address([bytes(P.from_string(owner)),bytes(P.from_string(TOKEN)),bytes(P.from_string(mint))],P.from_string(ATA))[0])
    helper=ROOT/'integration/.runtime/raydium-reference/target/debug/raydium-cpmm-reference';cases=[]
    for kind in ['EXACT_OUT','EXACT_IN']:
        for scenario,index in [('canonical-source-and-destination',0),('temporary-source-and-destination',0),('temporary-output-unwrap',1)]:
            b=copy.deepcopy(base['cases'][index]);b['accounts'].update(copy.deepcopy(closure));b['exactOut']=kind=='EXACT_OUT';b['fundingScenario']=scenario+'-'+kind.lower();b['actualOutput']=b['target'];b['preparationOptions']={}
            if kind=='EXACT_IN':
                pool=copy.deepcopy(b['pool']);pool['chain_key']='solana:localnet';q=json.loads(subprocess.check_output([str(helper),'quote'],input=json.dumps(dict(pool=pool,input=b['requiredInput'],zero_for_one=b['zeroForOne'])).encode()))
                if q.get('reverted'):raise ValueError(q)
                b['sellAmount']=b['requiredInput'];b['actualOutput']=q['amount_out'];b['after']=copy.deepcopy(b['pool']);b['after']['reserve0']=q['reserve0'];b['after']['reserve1']=q['reserve1'];b['after']['raydium_cpmm']=q['raydium_cpmm']
            if index==0:
                b['sourceTokenAccount']=ata(b['caller'],WSOL);b['destinationTokenAccount']=ata(b['recipient'],b['pool']['token1']);b['accounts'].pop(b['sourceTokenAccount'],None);b['accounts'].pop(b['destinationTokenAccount'],None)
                b['preparationOptions']=dict(wrapSourceSol=True,createDestinationAta=True)
                if scenario.startswith('temporary'):b['preparationOptions']['refundWrappedSource']=True
            else:
                b['recipient']=b['caller'];b['destinationTokenAccount']=ata(b['caller'],WSOL)
                b['preparationOptions']=dict(sourceTokenAccount=b['sourceTokenAccount'],unwrapDestinationSol=True)
            b['authority']='Controlled retained original CP-Swap source bank; explicit local-runtime System/ATA/Rent accounts, user funding/options, no historical consensus or production qualification'
            cases.append(b)
    # Empty System-owned canonical ATAs may be prefunded. Their existing value
    # reduces only this transaction's rent top-up and is not counted as payer rent.
    for source_index in [0,1,3,4]:
        b=copy.deepcopy(cases[source_index]);b['fundingScenario']+='-prefunded-destination';b['accounts'][b['destinationTokenAccount']]=dict(owner=SYSTEM,executable=False,lamports=1_000_000,data=['','base64']);cases.append(b)
    data=dict(schema_version=1,authority='CONTROLLED_ORIGINAL_SBF_FUNDING_BANK',source=dict(path=str(source.relative_to(ROOT)),sha256=hashlib.sha256(source.read_bytes()).hexdigest()),generator_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),program_accounts={k:dict(owner=a['owner'],sha256=hashlib.sha256(base64.b64decode(a['data'][0])).hexdigest())for k,a in closure.items()},rentLamports=str(rent),cases=cases)
    text=json.dumps(data,separators=(',',':'))+'\n'
    for service in ['integration','dex-router']:(ROOT/service/'test/contracts/raydium-funding-bank.json').write_text(text)
    print(json.dumps(dict(passed=True,cases=len(cases),sha256=hashlib.sha256(text.encode()).hexdigest(),source=data['source'],program_accounts=data['program_accounts'])))
if __name__=='__main__':main()
