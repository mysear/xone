#!/usr/bin/env python3
"""Decode actual downloaded unsigned message, locally sign test key, run original SBF."""
from pathlib import Path
import base64,hashlib,json,sys
from solana_expanded_reference import ROOT,require,tokens,delta_token
from r01_public_catalog_solana_bank import native_imports
TOKEN='TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA';PROGRAM='LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo';LOADER='BPFLoaderUpgradeab1e11111111111111111111111'
def verify(path):
    payload=json.loads(Path(path).read_bytes());p=payload.get('preparation',payload);bank=json.loads((ROOT/'integration/test/contracts/meteora-execution-bank.json').read_bytes())
    expected_hash=p['preparationHash'];unhashed={k:v for k,v in p.items()if k!='preparationHash'}
    require(hashlib.sha256(json.dumps(unhashed,sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()).hexdigest()==expected_hash,'unsigned-manifest-hash')
    raw=base64.b64decode(p['unsignedTransaction'],validate=True);require(raw[0]==1 and raw[1:65]==bytes(64)and raw[65:]==base64.b64decode(p['messageBase64']),'unsigned-wire')
    LiteSVM,Pubkey,Keypair,Instruction,AccountMeta,Transaction,Account,Clock=native_imports();tx=Transaction.from_bytes(raw)
    require(tx.message.header.num_required_signatures==1 and str(tx.message.account_keys[0])==bank['caller'],'unsigned-payer-and-signers')
    require(len(tx.message.instructions)==1,'unsigned-instruction-count');instruction=tx.message.instructions[0];descriptor=p['instructions'][0]
    require(str(tx.message.account_keys[instruction.program_id_index])==PROGRAM==descriptor['programId'],'unsigned-program')
    require([str(tx.message.account_keys[i])for i in instruction.accounts]==[a['pubkey']for a in descriptor['accounts']]and bytes(instruction.data)==base64.b64decode(descriptor['dataBase64']),'unsigned-serialized-instruction')
    signer=Keypair.from_seed(hashlib.sha256(b'xone-meteora-reference-payer').digest());require(str(signer.pubkey())==bank['caller'],'local-test-signer')
    def execute(control=False):
        svm=LiteSVM().with_mainnet_features().with_blockhash_check(False)
        for pid in [PROGRAM,TOKEN]:
            a=bank['accounts'][pid];program=base64.b64decode(a['data'][0]);pd=str(Pubkey.from_bytes(program[4:36]));code=base64.b64decode(bank['accounts'][pd]['data'][0])[45:];svm.add_program_with_loader(Pubkey.from_string(pid),code,Pubkey.from_string(LOADER))
        for id,a in bank['accounts'].items():svm.set_account(Pubkey.from_string(id),Account(a['lamports'],base64.b64decode(a['data'][0]),Pubkey.from_string(a['owner']),a['executable'],0))
        clock=base64.b64decode(bank['accounts']['SysvarC1ock11111111111111111111111111111111']['data'][0]);svm.set_clock(Clock(slot=int.from_bytes(clock[:8],'little'),epoch_start_timestamp=int.from_bytes(clock[8:16],'little',signed=True),epoch=int.from_bytes(clock[16:24],'little'),leader_schedule_epoch=int.from_bytes(clock[24:32],'little'),unix_timestamp=int.from_bytes(clock[32:40],'little',signed=True)))
        message=Transaction.from_bytes(raw)
        if control:
            data=bytearray(instruction.data);data[8:16]=(int(bank['requiredInput'])-1).to_bytes(8,'little');ix=Instruction(Pubkey.from_string(PROGRAM),bytes(data),[AccountMeta(Pubkey.from_string(a['pubkey']),a['isSigner'],a['isWritable'])for a in descriptor['accounts']]);message=Transaction.new_signed_with_payer([ix],signer.pubkey(),[signer],tx.message.recent_blockhash)
        else: message.sign([signer],message.message.recent_blockhash)
        def capture():
            result={}
            for id in bank['accounts']:
                a=svm.get_account(Pubkey.from_string(id));result[id]=dict(owner=str(a.owner),lamports=a.lamports,executable=a.executable,data_base64=base64.b64encode(a.data).decode())
            return result
        before=capture();result=json.loads(svm.send_transaction(message).to_json());after=capture();return dict(passed='err'not in result,before=before,after=after,native=result)
    n=execute();require(n['passed'],'unsigned-original-execution:'+str(n['native']));require(-delta_token(n,bank['pool']['token0'],bank['caller'])==int(bank['requiredInput'])and delta_token(n,bank['pool']['token1'],bank['recipient'])==int(bank['target']),'unsigned-original-transfers');tokens(n)
    expected=bank['after']['meteora'];
    for key,hexdata in [(bank['pool']['pool_id'],expected['pair_data'])]+[(a['address'],a['data'])for a in expected['bin_arrays']]:require(base64.b64decode(n['after'][key]['data_base64']).hex()==hexdata,'unsigned-original-post-state')
    oracle=next(key for key,a in n['after'].items()if base64.b64decode(a['data_base64']).hex()==expected['oracle_data']);require(bool(oracle),'unsigned-original-oracle')
    control=execute(True);require(not control['passed'],'unsigned-original-budget-control')
    # Network fee can change the fee payer even when instruction changes roll back.
    for id,a in control['before'].items():
        if id!=bank['caller']:require(a==control['after'][id],'unsigned-control-account-rollback')
    return dict(passed=True,authority='actual Router-produced unsigned wire, locally signed deterministic test key and fixed original SBF; no public broadcast',input=bank['requiredInput'],output=bank['target'],original=n,budget_control=control,publicTransactionsSubmitted=False)
if __name__=='__main__':
    path=sys.argv[1]if len(sys.argv)>1 else ROOT/'integration/artifacts/local/phase57-execution-preparation.json';r=verify(path);out=ROOT/'integration/artifacts/local/phase57-unsigned-original.json';out.write_text(json.dumps(r,separators=(',',':'))+'\n');print(json.dumps({k:r[k]for k in ['passed','authority','input','output','publicTransactionsSubmitted']}))
