#!/usr/bin/env python3
"""Verify downloaded ATA-create + swap wire by executing original programs locally."""
import base64,hashlib,json,sys
from pathlib import Path
from solana_expanded_reference import ROOT,require
from r01_public_catalog_solana_bank import native_imports
from solana_prepare_simulation_worker import simulate
PROGRAM='LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo';TOKEN='TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA';ATA='ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL';SYSTEM='11111111111111111111111111111111'
def verify(path):
    doc=json.loads(Path(path).read_bytes());p=doc.get('preparation',doc);b=json.loads((ROOT/'integration/test/contracts/meteora-execution-bank.json').read_bytes())
    require(p['accountCreation']is True and p['signed']is False and p['submitted']is False,'ata-preparation-kind')
    require(hashlib.sha256(json.dumps({k:v for k,v in p.items()if k!='preparationHash'},sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()).hexdigest()==p['preparationHash'],'ata-manifest-hash')
    S,P,K,I,M,T,A,C=native_imports();raw=base64.b64decode(p['unsignedTransaction'],validate=True);tx=T.from_bytes(raw)
    require(raw[0]==1 and raw[1:65]==bytes(64)and raw[65:]==base64.b64decode(p['messageBase64']),'ata-wire-unsigned')
    require(len(tx.message.instructions)==len(p['instructions'])==2,'ata-instruction-count')
    owner=P.from_string(b['recipient']);mint=P.from_string(b['pool']['token1']);dest=str(P.find_program_address([bytes(owner),bytes(P.from_string(TOKEN)),bytes(mint)],P.from_string(ATA))[0]);require(p['destinationTokenAccount']==dest,'ata-derived-address')
    require(dest not in b['accounts'],'ata-initial-absent')
    require([i['programId']for i in p['instructions']]==[ATA,PROGRAM]and base64.b64decode(p['instructions'][0]['dataBase64'])==b'\x01','ata-idempotent-instruction')
    for actual,descriptor in zip(tx.message.instructions,p['instructions']):
        require(str(tx.message.account_keys[actual.program_id_index])==descriptor['programId']and bytes(actual.data)==base64.b64decode(descriptor['dataBase64'])and [str(tx.message.account_keys[i])for i in actual.accounts]==[a['pubkey']for a in descriptor['accounts']],'ata-serialized-descriptor')
    sim=simulate(dict(bank=b,rpc=dict(method='simulateTransaction',params=[p['unsignedTransaction'],dict(sigVerify=False,replaceRecentBlockhash=False,accounts={'addresses':[p['sourceTokenAccount'],dest]})])))
    require(sim['value']['err']is None,'ata-original-readonly-simulation')
    def execute(control=False):
        svm=S().with_mainnet_features().with_blockhash_check(False)
        for pid in [PROGRAM,TOKEN]:
            program=base64.b64decode(b['accounts'][pid]['data'][0]);pd=str(P.from_bytes(program[4:36]));svm.add_program_with_loader(P.from_string(pid),base64.b64decode(b['accounts'][pd]['data'][0])[45:],P.from_string('BPFLoaderUpgradeab1e11111111111111111111111'))
        for id,a in b['accounts'].items():svm.set_account(P.from_string(id),A(a['lamports'],base64.b64decode(a['data'][0]),P.from_string(a['owner']),a['executable'],0))
        clock=base64.b64decode(b['accounts']['SysvarC1ock11111111111111111111111111111111']['data'][0]);svm.set_clock(C(slot=int.from_bytes(clock[:8],'little'),epoch_start_timestamp=int.from_bytes(clock[8:16],'little',signed=True),epoch=int.from_bytes(clock[16:24],'little'),leader_schedule_epoch=int.from_bytes(clock[24:32],'little'),unix_timestamp=int.from_bytes(clock[32:40],'little',signed=True)))
        signer=K.from_seed(hashlib.sha256(b'xone-meteora-reference-payer').digest());wire=T.from_bytes(raw)
        if control:
            instructions=[]
            for index,d in enumerate(p['instructions']):
                data=bytearray(base64.b64decode(d['dataBase64']))
                if index==1:data[8:16]=(int(b['requiredInput'])-1).to_bytes(8,'little')
                instructions.append(I(P.from_string(d['programId']),bytes(data),[M(P.from_string(a['pubkey']),a['isSigner'],a['isWritable'])for a in d['accounts']]))
            wire=T.new_signed_with_payer(instructions,signer.pubkey(),[signer],tx.message.recent_blockhash)
        else:wire.sign([signer],wire.message.recent_blockhash)
        ids=set(b['accounts'])|{dest}
        def capture():
            out={}
            for id in sorted(ids):
                a=svm.get_account(P.from_string(id));out[id]=None if a is None else dict(owner=str(a.owner),lamports=a.lamports,executable=a.executable,data_base64=base64.b64encode(a.data).decode())
            return out
        before=capture();n=json.loads(svm.send_transaction(wire).to_json());return dict(passed='err'not in n,before=before,after=capture(),native=n)
    n=execute();require(n['passed'],'ata-original-create-swap:'+str(n['native']))
    source=p['sourceTokenAccount'];balance=lambda a:int.from_bytes(base64.b64decode(a['data_base64'])[64:72],'little')
    require(balance(n['before'][source])-balance(n['after'][source])==699 and balance(n['after'][dest])==1000,'ata-actual-transfers')
    require(n['before'][b['caller']]['lamports']-n['after'][b['caller']]['lamports']==int(p['totalFundingLamports']),'ata-actual-rent-and-fee')
    require(n['after'][dest]['owner']==TOKEN and n['after'][dest]['lamports']==int(p['rentLamports'])+1000,'ata-wsol-backing')
    totals={}
    for id,a in n['after'].items():
        for sign,bank in [(1,n['after']),(-1,n['before'])]:
            account=bank[id]
            if account is None:continue
            d=base64.b64decode(account['data_base64'])
            if len(d)==165 and account['owner']==TOKEN:totals[d[:32]]=totals.get(d[:32],0)+sign*int.from_bytes(d[64:72],'little')
    require(all(x==0 for x in totals.values()),'ata-token-conservation')
    expected=b['after']['meteora']
    for id,raw_expected in [(b['pool']['pool_id'],expected['pair_data'])]+[(a['address'],a['data'])for a in expected['bin_arrays']]:require(base64.b64decode(n['after'][id]['data_base64']).hex()==raw_expected,'ata-full-state')
    require(any(a and base64.b64decode(a['data_base64']).hex()==expected['oracle_data']for a in n['after'].values()),'ata-oracle-state')
    control=execute(True);require(not control['passed']and control['after'][dest]is None,'ata-create-rolled-back-with-failed-swap')
    for id,a in control['before'].items():
        if id!=b['caller']:require(a==control['after'][id],'ata-control-rollback')
    return dict(passed=True,input='699',output='1000',rentLamports=p['rentLamports'],totalFundingLamports=p['totalFundingLamports'],original=n,control=control,simulation=sim,publicTransactionsSubmitted=False,productionReady=False)
if __name__=='__main__':
    r=verify(sys.argv[1]if len(sys.argv)>1 else ROOT/'integration/artifacts/local/phase58-ata-preparation.json');(Path(sys.argv[2])if len(sys.argv)>2 else ROOT/'integration/artifacts/local/phase58-ata-original.json').write_text(json.dumps(r,separators=(',',':'))+'\n');print(json.dumps({k:r[k]for k in ['passed','input','output','rentLamports','totalFundingLamports','publicTransactionsSubmitted']}))
