#!/usr/bin/env python3
"""Replay actual unsigned Token2022 ATA/swap wires with pinned original SBF."""
from pathlib import Path
import base64
import hashlib
import json
import sys
from r01_public_catalog_solana_bank import native_imports
from solana_expanded_reference import ROOT, require
from solana_prepare_simulation_worker import simulate

PROGRAM = 'LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo'
TOKEN = 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'
TOKEN22 = 'TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb'
ATA = 'ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL'
SYSTEM = '11111111111111111111111111111111'
WSOL = 'So11111111111111111111111111111111111111112'


def extensions(raw):
    if len(raw) not in [178, 182] or raw[165] != 2:
        raise ValueError('TOKEN2022_ACCOUNT_SIZE_OR_TYPE')
    values = {}
    cursor = 166
    while cursor < len(raw):
        kind = int.from_bytes(raw[cursor:cursor+2], 'little')
        size = int.from_bytes(raw[cursor+2:cursor+4], 'little')
        cursor += 4
        if kind in values or (kind, size) not in [(2, 8), (7, 0)] or cursor+size > len(raw):
            raise ValueError('TOKEN2022_ACCOUNT_EXTENSION')
        values[kind] = raw[cursor:cursor+size]
        cursor += size
    require(2 in values, 'token2022-fee-extension-required')
    return values


def verify(path, index=1):
    value = json.loads(Path(path).read_bytes())
    p = value.get('preparation', value)
    bank = json.loads((ROOT / 'integration/test/contracts/meteora-token2022-execution-bank.json').read_bytes())['cases'][index]
    require(p['signed'] is False and p['submitted'] is False and p['ready'] is True, 'token2022-unsigned')
    require(hashlib.sha256(json.dumps({k:v for k,v in p.items() if k!='preparationHash'}, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()).hexdigest() == p['preparationHash'], 'token2022-manifest-hash')
    refund = p['refundWrappedSource']
    require(type(refund) is bool and not p['unwrapDestinationSol'] and (not refund or not bank['zeroForOne']), 'token2022-kind')
    S,P,K,I,M,T,A,C = native_imports()
    caller, recipient = bank['caller'], bank['recipient']
    output_program = TOKEN if bank['zeroForOne'] else TOKEN22
    mint = bank['pool']['token1'] if bank['zeroForOne'] else bank['pool']['token0']
    destination = str(P.find_program_address([bytes(P.from_string(recipient)), bytes(P.from_string(output_program)), bytes(P.from_string(mint))], P.from_string(ATA))[0])
    require(destination == bank['canonicalDestination'] and destination not in bank['accounts'] and p['destinationTokenAccount'] == destination, 'token2022-canonical-destination')
    temporary = str(P.create_with_seed(P.from_string(caller), 'xone-in-'+p['planHash'][:20], P.from_string(TOKEN))) if refund else None
    source = temporary if refund else bank['sourceTokenAccount']
    rent = S().with_mainnet_features().minimum_balance_for_rent_exemption(bank['destinationSize'])
    total_rent = rent + (2039280 if refund else 0)
    expected_input, output = int(bank['requiredInput']), int(bank['target'])
    maximum = (expected_input*10050+9999)//10000
    fee = [0,26,100,53,100][index]
    require(p['destinationAccountSize'] == bank['destinationSize'] and p['destinationTokenProgram'] == output_program and
            p['rentLamports'] == str(total_rent) and p['destinationRentLamports'] == str(rent) and p['feeLamports'] == '5000' and
            p['expectedDestinationTransferFee'] == str(fee) and p['requiredInput'] == str(maximum) and p['sourceTokenAccount'] == source,
            'token2022-fees-size-rent')
    require(p['totalFundingLamports'] == str(total_rent+5000+(maximum if refund else 0)) and
            p['expectedPayerLamportDelta'] == str(-(rent+5000+(expected_input if refund else 0))), 'token2022-total-funding')
    wire = base64.b64decode(p['unsignedTransaction'], validate=True)
    tx = T.from_bytes(wire)
    require(wire[0] == 1 and wire[1:65] == bytes(64) and wire[65:] == base64.b64decode(p['messageBase64']) and
            tx.message.header.num_required_signatures == 1 and str(tx.message.account_keys[0]) == caller, 'token2022-wire-signer')
    require(len(tx.message.instructions) == len(p['instructions']) == (7 if refund else 2), 'token2022-count')
    decoded = []
    rights = {caller:(True,True)}
    for ix,d in zip(tx.message.instructions, p['instructions']):
        require(str(tx.message.account_keys[ix.program_id_index]) == d['programId'] and bytes(ix.data) == base64.b64decode(d['dataBase64']) and
                [str(tx.message.account_keys[i]) for i in ix.accounts] == [a['pubkey'] for a in d['accounts']], 'token2022-wire-descriptors')
        metas = [M(P.from_string(a['pubkey']),a['isSigner'],a['isWritable']) for a in d['accounts']]
        instruction = I(P.from_string(d['programId']),bytes(ix.data),metas)
        decoded.append(instruction)
        rights.setdefault(d['programId'],(False,False))
        for m in metas:
            old = rights.get(str(m.pubkey),(False,False))
            rights[str(m.pubkey)] = (old[0] or m.is_signer,old[1] or m.is_writable)
    keys = list(map(str, tx.message.account_keys));header=tx.message.header
    require(len(keys)==len(set(keys)) and set(keys)==set(rights),'token2022-wire-keyset')
    for n,key in enumerate(keys):
        signed=n<header.num_required_signatures
        writable=n<(header.num_required_signatures-header.num_readonly_signed_accounts if signed else len(keys)-header.num_readonly_unsigned_accounts)
        require(rights[key]==(signed,writable),'token2022-wire-permissions')
    create_index = 4 if refund else 0
    ata = decoded[create_index]
    require(ata == I(P.from_string(ATA),b'\x01',[M(P.from_string(caller),True,True),M(P.from_string(destination),False,True),
            M(P.from_string(recipient),False,False),M(P.from_string(mint),False,False),M(P.from_string(SYSTEM),False,False),
            M(P.from_string(output_program),False,False)]), 'token2022-original-ata-instruction')
    swap = decoded[create_index+1]
    require(str(swap.program_id)==PROGRAM and bytes(swap.data)==bytes([43,215,247,132,137,60,243,81])+maximum.to_bytes(8,'little')+output.to_bytes(8,'little')+bytes(4), 'token2022-original-swap-amounts')
    require(str(swap.accounts[4].pubkey)==source and str(swap.accounts[5].pubkey)==destination and str(swap.accounts[11].pubkey)==TOKEN22 and str(swap.accounts[12].pubkey)==TOKEN, 'token2022-swap-token-programs')
    if refund:
        from solders.system_program import create_account_with_seed
        require(decoded[0] == create_account_with_seed(dict(from_pubkey=P.from_string(caller),to_pubkey=P.from_string(temporary),base=P.from_string(caller),seed='xone-in-'+p['planHash'][:20],lamports=2039280,space=165,owner=P.from_string(TOKEN))), 'token2022-temporary-create')
        require(decoded[1] == I(P.from_string(TOKEN),b'\x12'+bytes(P.from_string(caller)),[M(P.from_string(source),False,True),M(P.from_string(WSOL),False,False)]) and
                decoded[2] == I(P.from_string(SYSTEM),(2).to_bytes(4,'little')+maximum.to_bytes(8,'little'),[M(P.from_string(caller),True,True),M(P.from_string(source),False,True)]) and
                decoded[3] == I(P.from_string(TOKEN),b'\x11',[M(P.from_string(source),False,True)]) and
                decoded[-1] == I(P.from_string(TOKEN),b'\x09',[M(P.from_string(source),False,True),M(P.from_string(caller),False,True),M(P.from_string(caller),True,False)]), 'token2022-temporary-funding-close')
    sim=simulate(dict(bank=bank,rpc=dict(method='simulateTransaction',params=[p['unsignedTransaction'],dict(sigVerify=False,replaceRecentBlockhash=False,accounts={'addresses':[source,destination,caller]})])))
    require(sim['value']['err'] is None,'token2022-readonly-simulation')
    signer=K.from_seed(hashlib.sha256(b'xone-meteora-reference-payer').digest())
    require(str(signer.pubkey())==caller,'token2022-test-signer')

    def execute(control=False):
        svm=S().with_mainnet_features().with_blockhash_check(False)
        for pid in [PROGRAM,TOKEN,TOKEN22]:
            raw=base64.b64decode(bank['accounts'][pid]['data'][0]);pd=str(P.from_bytes(raw[4:36]))
            svm.add_program_with_loader(P.from_string(pid),base64.b64decode(bank['accounts'][pd]['data'][0])[45:],P.from_string('BPFLoaderUpgradeab1e11111111111111111111111'))
        for key,a in bank['accounts'].items():
            svm.set_account(P.from_string(key),A(a['lamports'],base64.b64decode(a['data'][0]),P.from_string(a['owner']),a['executable'],0))
        clock=base64.b64decode(bank['accounts']['SysvarC1ock11111111111111111111111111111111']['data'][0])
        svm.set_clock(C(slot=int.from_bytes(clock[:8],'little'),epoch_start_timestamp=int.from_bytes(clock[8:16],'little',signed=True),epoch=int.from_bytes(clock[16:24],'little'),leader_schedule_epoch=int.from_bytes(clock[24:32],'little'),unix_timestamp=int.from_bytes(clock[32:40],'little',signed=True)))
        transaction=T.from_bytes(wire)
        if control:
            data=bytearray(swap.data);data[8:16]=(expected_input-1).to_bytes(8,'little')
            instructions=list(decoded);instructions[create_index+1]=I(swap.program_id,bytes(data),swap.accounts)
            transaction=T.new_signed_with_payer(instructions,signer.pubkey(),[signer],tx.message.recent_blockhash)
        else:transaction.sign([signer],tx.message.recent_blockhash)
        def capture():
            result={}
            for key in sorted(set(bank['accounts'])|{source,destination}):
                a=svm.get_account(P.from_string(key));result[key]=None if a is None or (a.lamports==0 and not a.data) else dict(owner=str(a.owner),lamports=a.lamports,executable=a.executable,data_base64=base64.b64encode(a.data).decode())
            return result
        before=capture();native=json.loads(svm.send_transaction(transaction).to_json())
        return dict(passed='err'not in native,before=before,after=capture(),native=native)
    n=execute();require(n['passed'],'token2022-original-execution:'+str(n['native']))
    raw=base64.b64decode(n['after'][destination]['data_base64'])
    require(len(raw)==bank['destinationSize'] and raw[:32]==bytes(P.from_string(mint)) and raw[32:64]==bytes(P.from_string(recipient)) and
            int.from_bytes(raw[64:72],'little')==output and n['after'][destination]['owner']==output_program, 'token2022-actual-net-credit-and-owner')
    if output_program==TOKEN22:
        ext=extensions(raw);require(set(ext)=={2,7} and int.from_bytes(ext[2],'little')==fee and n['after'][destination]['lamports']==rent,'token2022-actual-immutable-withheld-rent')
    else:require(n['after'][destination]['lamports']==rent+output,'token2022-classic-wsol-backing')
    require(n['before'][caller]['lamports']-n['after'][caller]['lamports']==rent+5000+(expected_input if refund else 0),'token2022-actual-native-debit')
    if refund:
        require(n['before'][source] is None and n['after'][source] is None and n['before'][bank['sourceTokenAccount']]==n['after'][bank['sourceTokenAccount']],'token2022-temp-closed-existing-source-preserved')
    else:
        amount=lambda a:int.from_bytes(base64.b64decode(a['data_base64'])[64:72],'little')
        require(amount(n['before'][source])-amount(n['after'][source])==expected_input,'token2022-actual-input')
    require(n['before'][bank['destinationTokenAccount']]==n['after'][bank['destinationTokenAccount']],'token2022-original-recipient-account-preserved')
    expected=bank['after']['meteora']
    for key,data in [(bank['pool']['pool_id'],expected['pair_data'])]+[(a['address'],a['data'])for a in expected['bin_arrays']]:
        require(base64.b64decode(n['after'][key]['data_base64']).hex()==data,'token2022-complete-pool-post-state')
    require(any(a and base64.b64decode(a['data_base64']).hex()==expected['oracle_data']for a in n['after'].values()),'token2022-oracle')
    require(any(a and a['owner']==TOKEN22 and base64.b64decode(a['data_base64']).hex()==expected['token2022']['vault_data']for a in n['after'].values()),'token2022-full-vault-withheld-post-state')
    totals={}
    for side,sign in [('before',-1),('after',1)]:
        for a in n[side].values():
            if a is None or a['owner']not in [TOKEN,TOKEN22]:continue
            raw=base64.b64decode(a['data_base64'])
            if len(raw)<165 or (len(raw)!=165 and raw[165]!=2):continue
            total=int.from_bytes(raw[64:72],'little')+(int.from_bytes(extensions(raw)[2],'little')if a['owner']==TOKEN22 else 0)
            totals[raw[:32]]=totals.get(raw[:32],0)+sign*total
    require(totals.pop(bytes(P.from_string(WSOL)))==(expected_input if refund else 0) and all(v==0 for v in totals.values()),'token2022-balance-plus-withheld-conservation')
    control=execute(True);require(not control['passed']and control['after'][destination] is None,'token2022-creation-atomic-rollback')
    for key,a in control['before'].items():
        if key!=caller:require(a==control['after'][key],'token2022-full-failure-rollback')
    require(control['before'][caller]['lamports']-control['after'][caller]['lamports']==5000,'token2022-failed-fee-only')
    return dict(passed=True,index=index,input=bank['requiredInput'],output=bank['target'],destinationSize=bank['destinationSize'],destinationRent=rent,transferFee=fee,refund=refund,
                original=n,control=control,simulation=sim,publicTransactionsSubmitted=False,productionReady=False)


if __name__=='__main__':
    if sys.argv[1]=='--matrix':
        folder=Path(sys.argv[2]);records=[]
        for index in range(5):
            records.append(verify(folder/f'token2022-{index}-preparation.json',index))
        records.append(verify(folder/'token2022-refund-preparation.json',1))
        result=dict(passed=True,cases=len(records),records=records,publicTransactionsSubmitted=False)
        Path(sys.argv[3]).write_text(json.dumps(result,separators=(',',':'))+'\n')
        print(json.dumps(dict(passed=True,cases=len(records))))
    else:
        result=verify(sys.argv[1],int(sys.argv[3])if len(sys.argv)>3 else 1)
        Path(sys.argv[2]).write_text(json.dumps(result,separators=(',',':'))+'\n')
        print(json.dumps({k:result[k]for k in ['passed','index','input','output','destinationSize','destinationRent','transferFee','refund']}))
