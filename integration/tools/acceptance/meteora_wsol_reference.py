#!/usr/bin/env python3
"""Replay downloaded create-WSOL/transfer/SyncNative/swap wire with original programs."""
from pathlib import Path
import base64,hashlib,json,sys
from solana_expanded_reference import ROOT,require
from r01_public_catalog_solana_bank import native_imports
from solana_prepare_simulation_worker import simulate
PROGRAM='LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo';TOKEN='TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA';ATA='ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL';SYSTEM='11111111111111111111111111111111';WSOL='So11111111111111111111111111111111111111112'
def verify(path):
    doc=json.loads(Path(path).read_bytes());p=doc.get('preparation',doc);bank=json.loads((ROOT/'integration/test/contracts/meteora-wsol-bank.json').read_bytes())
    require(p['sourceAccountCreation']is True and p['sourceWsolRetained']is True and p['signed']is False and p['submitted']is False,'wrap-kind')
    require(hashlib.sha256(json.dumps({k:v for k,v in p.items()if k!='preparationHash'},sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()).hexdigest()==p['preparationHash'],'wrap-manifest-hash')
    S,P,K,I,M,T,A,C=native_imports();wire=base64.b64decode(p['unsignedTransaction'],validate=True);tx=T.from_bytes(wire)
    require(wire[0]==1 and wire[1:65]==bytes(64) and wire[65:]==base64.b64decode(p['messageBase64']),'wrap-wire')
    require(tx.message.header.num_required_signatures==1 and str(tx.message.account_keys[0])==bank['caller'],'wrap-signer')
    require(len(tx.message.instructions)==len(p['instructions'])==4,'wrap-instruction-count')
    require([d['programId']for d in p['instructions']]==[ATA,SYSTEM,TOKEN,PROGRAM],'wrap-instruction-order')
    require(base64.b64decode(p['instructions'][0]['dataBase64'])==b'\x01'and base64.b64decode(p['instructions'][2]['dataBase64'])==b'\x11','wrap-create-and-sync')
    transfer=base64.b64decode(p['instructions'][1]['dataBase64']);require(transfer[:4]==(2).to_bytes(4,'little')and int.from_bytes(transfer[4:],'little')==int(p['wrapLamports'])==1472,'wrap-transfer')
    require(p['sourceTokenAccount']==bank['sourceTokenAccount']and p['destinationTokenAccount']==bank['destinationTokenAccount'],'wrap-account-identity')
    require(p['rentLamports']=='2039280'and p['feeLamports']=='5000'and p['totalFundingLamports']=='2045752'and p['expectedResidualWsol']=='8','wrap-funding')
    for ix,d in zip(tx.message.instructions,p['instructions']):
        require(str(tx.message.account_keys[ix.program_id_index])==d['programId']and bytes(ix.data)==base64.b64decode(d['dataBase64'])and[str(tx.message.account_keys[i])for i in ix.accounts]==[a['pubkey']for a in d['accounts']],'wrap-wire-descriptors')
    signer=K.from_seed(hashlib.sha256(b'xone-meteora-reference-payer').digest());require(str(signer.pubkey())==bank['caller'],'wrap-test-signer')
    simulation=simulate(dict(bank=bank,rpc=dict(method='simulateTransaction',params=[p['unsignedTransaction'],dict(sigVerify=False,replaceRecentBlockhash=False,accounts={'addresses':[p['sourceTokenAccount'],p['destinationTokenAccount']]})])))
    require(simulation['value']['err']is None,'wrap-readonly-original-simulation')
    def execute(control=False):
        svm=S().with_mainnet_features().with_blockhash_check(False)
        for pid in [PROGRAM,TOKEN]:
            raw=base64.b64decode(bank['accounts'][pid]['data'][0]);pd=str(P.from_bytes(raw[4:36]));svm.add_program_with_loader(P.from_string(pid),base64.b64decode(bank['accounts'][pd]['data'][0])[45:],P.from_string('BPFLoaderUpgradeab1e11111111111111111111111'))
        for id,a in bank['accounts'].items():svm.set_account(P.from_string(id),A(a['lamports'],base64.b64decode(a['data'][0]),P.from_string(a['owner']),a['executable'],0))
        clock=base64.b64decode(bank['accounts']['SysvarC1ock11111111111111111111111111111111']['data'][0]);svm.set_clock(C(slot=int.from_bytes(clock[:8],'little'),epoch_start_timestamp=int.from_bytes(clock[8:16],'little',signed=True),epoch=int.from_bytes(clock[16:24],'little'),leader_schedule_epoch=int.from_bytes(clock[24:32],'little'),unix_timestamp=int.from_bytes(clock[32:40],'little',signed=True)))
        message=T.from_bytes(wire)
        if control:
            instructions=[]
            for index,d in enumerate(p['instructions']):
                raw=bytearray(base64.b64decode(d['dataBase64']))
                if index==3:raw[8:16]=(1463).to_bytes(8,'little')
                instructions.append(I(P.from_string(d['programId']),bytes(raw),[M(P.from_string(a['pubkey']),a['isSigner'],a['isWritable'])for a in d['accounts']]))
            message=T.new_signed_with_payer(instructions,signer.pubkey(),[signer],tx.message.recent_blockhash)
        else:message.sign([signer],message.message.recent_blockhash)
        def capture():
            result={}
            for id in sorted(set(bank['accounts'])|{p['sourceTokenAccount']}):
                a=svm.get_account(P.from_string(id));result[id]=None if a is None else dict(owner=str(a.owner),lamports=a.lamports,executable=a.executable,data_base64=base64.b64encode(a.data).decode())
            return result
        before=capture();native=json.loads(svm.send_transaction(message).to_json());return dict(passed='err'not in native,before=before,after=capture(),native=native)
    n=execute();require(n['passed'],'wrap-original-execution:'+str(n['native']))
    src=p['sourceTokenAccount'];dst=p['destinationTokenAccount'];amount=lambda a:int.from_bytes(base64.b64decode(a['data_base64'])[64:72],'little')
    require(n['before'][src]is None and amount(n['after'][src])==8,'wrap-residual')
    require(amount(n['after'][dst])-amount(n['before'][dst])==1000,'wrap-output')
    require(1472-amount(n['after'][src])==1464,'wrap-spend')
    require(n['before'][bank['caller']]['lamports']-n['after'][bank['caller']]['lamports']==2045752,'wrap-payer-funds')
    require(n['after'][src]['lamports']==2039280+8,'wrap-native-backing')
    totals={}
    for side,sign in [('before',-1),('after',1)]:
        for a in n[side].values():
            if a is None or a['owner']!=TOKEN:continue
            raw=base64.b64decode(a['data_base64'])
            if len(raw)==165:totals[raw[:32]]=totals.get(raw[:32],0)+sign*int.from_bytes(raw[64:72],'little')
    require(totals.pop(bytes(P.from_string(WSOL)))==1472 and all(v==0 for v in totals.values()),'wrap-token-conservation')
    expected=bank['after']['meteora']
    for id,raw in [(bank['pool']['pool_id'],expected['pair_data'])]+[(a['address'],a['data'])for a in expected['bin_arrays']]:require(base64.b64decode(n['after'][id]['data_base64']).hex()==raw,'wrap-pool-order-post-state')
    require(any(a and base64.b64decode(a['data_base64']).hex()==expected['oracle_data']for a in n['after'].values()),'wrap-oracle')
    control=execute(True);require(not control['passed']and control['after'][src]is None,'wrap-atomic-rollback')
    for id,a in control['before'].items():
        if id!=bank['caller']:require(a==control['after'][id],'wrap-control-full-rollback')
    return dict(passed=True,input='1464',output='1000',wrapLamports='1472',residualWsol='8',rentLamports='2039280',totalFundingLamports='2045752',original=n,control=control,simulation=simulation,publicTransactionsSubmitted=False)
if __name__=='__main__':
    r=verify(sys.argv[1]if len(sys.argv)>1 else ROOT/'integration/artifacts/local/phase59-wrap-preparation.json');(Path(sys.argv[2])if len(sys.argv)>2 else ROOT/'integration/artifacts/local/phase59-wrap-original.json').write_text(json.dumps(r,separators=(',',':'))+'\n');print(json.dumps({k:r[k]for k in ['passed','input','output','wrapLamports','residualWsol','totalFundingLamports','publicTransactionsSubmitted']}))
