#!/usr/bin/env python3
"""Replay actual Router funding wire in original CP-Swap SBF, locally only."""
from pathlib import Path
import argparse,base64,copy,hashlib,json,struct,subprocess,sys
from r01_public_catalog_solana_bank import native_imports
ROOT=Path(__file__).resolve().parents[3]
TOKEN='TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA';SYSTEM='11111111111111111111111111111111';ATA='ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL';WSOL='So11111111111111111111111111111111111111112';CLOCK='SysvarC1ock11111111111111111111111111111111';RENT='SysvarRent111111111111111111111111111111111';PROGRAM='CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C';LOADER='BPFLoaderUpgradeab1e11111111111111111111111'
def require(value,reason):
    if not value:raise ValueError(reason)
def digest(value):return hashlib.sha256(value).hexdigest()
def canonical(value):return json.dumps(value,sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()
def snapshot_records(folder):
    result={}
    for path in sorted(Path(folder).glob('getMultipleAccounts-*.json')):
        record=json.loads(path.read_bytes());request=record['request'];response=record['response']['result'];ids=request['params'][0];values=response['value'];require(len(ids)==len(set(ids))==len(values),'CAPTURE_BATCH')
        capture=dict(schemaVersion=1,chainKey=record['chainKey'],genesisHash=record['genesisHash'],observedSlot=response['context']['slot'],accounts=dict(zip(ids,values)))
        result[digest(canonical(capture))]=(capture,path)
    return result

def verify_document(doc,bank):
    p=doc['preparation'];q=doc['quote'];e=doc['independent']
    require(p['ready']is True and p['signed']is False and p['submitted']is False,'UNSIGNED_BOUNDARY')
    require(digest(canonical({k:v for k,v in p.items()if k!='preparationHash'}))==p['preparationHash'],'PREPARATION_HASH')
    require(p['quoteId']==q['quoteId']and p['planHash']==q['planHash']and p['quoteViewId']==q['quoteViewId'],'QUOTE_BINDING')
    require(p['quotedInput']==bank['requiredInput']and p['quotedOutput']==bank['actualOutput'],'QUOTED_NATIVE_AMOUNTS')
    require(e['success']is True and e['sourceBankUnchanged']is True and e['accountSource']=='PREPARATION_RPC_CAPTURE'and e['historicalBankVerified']is False and e['independentProductionVerification']is False,'INDEPENDENT_SCOPE')
    require(e['quoteId']==p['quoteId']and e['preparationHash']==p['preparationHash']and e['planHash']==p['planHash'],'EVIDENCE_BINDING')
    require(e['actualInput']==p['quotedInput']and e['actualOutput']==p['quotedOutput'],'EVIDENCE_AMOUNTS')
    require(e['transactionSha256']==digest(base64.b64decode(p['unsignedTransaction'],validate=True)),'ACTUAL_WIRE_EVIDENCE')
    require(e['postAccountsHash']==digest(canonical(e['postAccounts'])),'POST_ACCOUNT_HASH')
    require(p['accountSnapshot']['source']=='PREPARATION_RPC_CAPTURE'and p['accountSnapshot']['historicalBankVerified']is False,'CAPTURE_SCOPE')
    require(p['feeLamports']==e['feeLamports']=='5000','NETWORK_FEE')
    require(p['expectedPayerLamportDelta']==e['payerLamportDelta'],'INDEPENDENT_PAYER_DELTA')
    require(e['evidenceHash']==digest(canonical({k:v for k,v in e.items()if k!='evidenceHash'})),'EVIDENCE_HASH')
    return p,q,e

def verify(api_path,index,captures):
    S,P,K,I,M,T,A,C=native_imports();b=json.loads((ROOT/'integration/test/contracts/raydium-funding-bank.json').read_bytes())['cases'][index];doc=json.loads(Path(api_path).read_bytes());p,q,e=verify_document(doc,b);pool=b['pool'];state=pool['raydium_cpmm'];zero=b['zeroForOne'];caller=b['caller'];recipient=b['recipient'];input_mint=pool['token0']if zero else pool['token1'];output_mint=pool['token1']if zero else pool['token0'];src=p['sourceTokenAccount'];dst=p['destinationTokenAccount'];opts=b['preparationOptions']
    capture,path=captures.get(p['accountSnapshot']['snapshotHash'],(None,None));require(capture is not None,'CAPTURE_HASH_NOT_OBSERVED');require(capture['observedSlot']==state['bank_slot']and capture['chainKey']==pool['chain_key']and capture['genesisHash']==b['genesis'],'CAPTURE_BANK_CONTEXT');require(p['accountSnapshot']['accountCount']==len(capture['accounts']),'CAPTURE_COUNT')
    accounts={k:v for k,v in capture['accounts'].items()if v is not None};require(src in capture['accounts']and dst in capture['accounts']and caller in accounts,'CAPTURE_DYNAMIC_DEPENDENCIES')
    wire=base64.b64decode(p['unsignedTransaction'],validate=True);tx=T.from_bytes(wire);keys=list(map(str,tx.message.account_keys));require(len(wire)<=1232 and wire[0]==1 and wire[1:65]==bytes(64)and wire[65:]==base64.b64decode(p['messageBase64'])and wire[65]==1,'WIRE_UNSIGNED_LAYOUT');require(tx.message.header.num_required_signatures==1 and keys[0]==caller and len(set(keys))==len(keys),'WIRE_SINGLE_SIGNER');require(set(keys)<=set(capture['accounts']),'CAPTURE_COMPILED_CLOSURE');require(digest(wire)==e['transactionSha256'],'ACTUAL_WIRE_EVIDENCE')
    pub=lambda key:bytes(P.from_string(key))
    ata=lambda owner,mint:str(P.find_program_address([pub(owner),pub(TOKEN),pub(mint)],P.from_string(ATA))[0])
    def descriptor(pid,data,metas):return dict(programId=pid,dataBase64=base64.b64encode(data).decode(),accounts=[dict(pubkey=k,isSigner=s,isWritable=w)for k,s,w in metas])
    def create(address,owner,mint):return descriptor(ATA,b'\x01',[(caller,True,True),(address,False,True),(owner,False,False),(mint,False,False),(SYSTEM,False,False),(TOKEN,False,False)])
    prefix=[];suffix=[];temporary=p.get('temporaryWsolAccount');refund=p['refundWrappedSource'];unwrap=p['unwrapDestinationSol'];minimum=S().with_mainnet_features().minimum_balance_for_rent_exemption(165)
    if temporary:
        seed=p['temporarySeed'];require(seed==f"xone-{'in'if refund else'out'}-{p['planHash'][:20]}",'TEMP_SEED');require(temporary==str(P.from_bytes(hashlib.sha256(pub(caller)+seed.encode()+pub(TOKEN)).digest())),'TEMP_ADDRESS');require(capture['accounts'].get(temporary)is None,'TEMP_INITIAL_ABSENCE');require(int(p['rentRefundLamports'])==minimum,'TEMP_RENT')
        raw=(3).to_bytes(4,'little')+pub(caller)+len(seed).to_bytes(8,'little')+seed.encode()+minimum.to_bytes(8,'little')+(165).to_bytes(8,'little')+pub(TOKEN)
        prefix+=[descriptor(SYSTEM,raw,[(caller,True,True),(temporary,False,True)]),descriptor(TOKEN,b'\x12'+pub(caller),[(temporary,False,True),(WSOL,False,False)])]
        suffix=[descriptor(TOKEN,b'\x09',[(temporary,False,True),(caller,False,True),(caller,True,False)])]
        require((refund and src==temporary and input_mint==WSOL)or(unwrap and dst==temporary and output_mint==WSOL and caller==recipient),'TEMP_CONTEXT')
    if p['sourceAccountCreation']and not refund:
        require(src==ata(caller,input_mint)and input_mint==WSOL,'SOURCE_ATA');prefix.append(create(src,caller,input_mint))
    wrap=int(p['wrapLamports'])
    if wrap:
        require(opts.get('wrapSourceSol')is True and input_mint==WSOL,'WRAP_OPT_IN');prefix+=[descriptor(SYSTEM,(2).to_bytes(4,'little')+wrap.to_bytes(8,'little'),[(caller,True,True),(src,False,True)]),descriptor(TOKEN,b'\x11',[(src,False,True)])]
    if p['accountCreation']:
        require(opts.get('createDestinationAta')is True and dst==ata(recipient,output_mint),'DESTINATION_ATA');prefix.append(create(dst,recipient,output_mint))
    observation=str(P.find_program_address([b'observation',pub(pool['pool_id'])],P.from_string(PROGRAM))[0]);metas=[(caller,True,True),(state['authority'],False,False),(state['amm_config'],False,False),(pool['pool_id'],False,True),(src,False,True),(dst,False,True),(state['vault0']if zero else state['vault1'],False,True),(state['vault1']if zero else state['vault0'],False,True),(TOKEN,False,False),(TOKEN,False,False),(input_mint,False,False),(output_mint,False,False),(observation,False,True)]
    first=int(q['maxSellAmount'])if b['exactOut']else int(q['sellAmount']);second=int(q['buyAmount'])if b['exactOut']else int(q['minBuyAmount']);disc=hashlib.sha256(b'global:swap_base_output'if b['exactOut']else b'global:swap_base_input').digest()[:8];swap=descriptor(PROGRAM,disc+first.to_bytes(8,'little')+second.to_bytes(8,'little'),metas);expected=prefix+[swap]+suffix;require(p['instructions']==expected,'INSTRUCTION_CONTEXT_AND_ORDER');require(len(tx.message.instructions)==len(expected),'INSTRUCTION_COUNT')
    rights={caller:(True,True)}
    for raw,compiled in zip(expected,tx.message.instructions):
        require(keys[compiled.program_id_index]==raw['programId']and bytes(compiled.data)==base64.b64decode(raw['dataBase64'])and[keys[i]for i in compiled.accounts]==[a['pubkey']for a in raw['accounts']],'WIRE_DESCRIPTOR')
        rights.setdefault(raw['programId'],(False,False))
        for a in raw['accounts']:
            old=rights.get(a['pubkey'],(False,False));rights[a['pubkey']]=(old[0]or a['isSigner'],old[1]or a['isWritable'])
    require(set(keys)==set(rights),'WIRE_KEY_SET');h=tx.message.header
    for i,key in enumerate(keys):
        signed=i<h.num_required_signatures;writable=i<(h.num_required_signatures-h.num_readonly_signed_accounts if signed else len(keys)-h.num_readonly_unsigned_accounts);require(rights[key]==(signed,writable),'WIRE_PRIVILEGES')
    signer=K.from_seed(hashlib.sha256(b'R01-public-native-bank-payer').digest());require(str(signer.pubkey())==caller,'CONTROLLED_SIGNER')
    def execute(control=False):
        vm=S().with_mainnet_features().with_blockhash_check(False)
        for pid,code_hash in [(PROGRAM,state['program_code_sha256']),(TOKEN,state['token_program_code_sha256'])]:
            raw=base64.b64decode(accounts[pid]['data'][0]);pd=str(P.from_bytes(raw[4:36]));body=base64.b64decode(accounts[pd]['data'][0])[45:];require(digest(body)==code_hash,'ORIGINAL_PROGRAM_HASH');vm.add_program_with_loader(P.from_string(pid),body,P.from_string(LOADER))
        from solders.rent import Rent
        rent_raw=base64.b64decode(accounts[RENT]['data'][0]);year,threshold,burn=struct.unpack('<QdB',rent_raw);vm.set_rent(Rent(year,threshold,burn))
        for key,a in accounts.items():vm.set_account(P.from_string(key),A(a['lamports'],base64.b64decode(a['data'][0]),P.from_string(a['owner']),a['executable'],0))
        clock=base64.b64decode(accounts[CLOCK]['data'][0]);vm.set_clock(C(slot=int.from_bytes(clock[:8],'little'),epoch_start_timestamp=int.from_bytes(clock[8:16],'little',signed=True),epoch=int.from_bytes(clock[16:24],'little'),leader_schedule_epoch=int.from_bytes(clock[24:32],'little'),unix_timestamp=int.from_bytes(clock[32:40],'little',signed=True)))
        def capture():
            out={}
            for key in sorted(set(accounts)|set(keys)):
                a=vm.get_account(P.from_string(key));out[key]=None if a is None else dict(lamports=a.lamports,owner=str(a.owner),executable=a.executable,data=bytes(a.data))
            return out
        before=capture();signed=T.from_bytes(wire)
        if control:
            descriptors=copy.deepcopy(expected);bad=bytearray(base64.b64decode(descriptors[len(prefix)]['dataBase64']));offset=8 if b['exactOut']else 16;amount=int(b['requiredInput'])-1 if b['exactOut']else int(b['actualOutput'])+1;bad[offset:offset+8]=amount.to_bytes(8,'little');descriptors[len(prefix)]['dataBase64']=base64.b64encode(bad).decode();instructions=[I(P.from_string(d['programId']),base64.b64decode(d['dataBase64']),[M(P.from_string(a['pubkey']),a['isSigner'],a['isWritable'])for a in d['accounts']])for d in descriptors];signed=T.new_signed_with_payer(instructions,signer.pubkey(),[signer],tx.message.recent_blockhash)
        else:signed.sign([signer],tx.message.recent_blockhash)
        native=json.loads(vm.send_transaction(signed).to_json());return before,capture(),native
    before,after,native=execute();require('err'not in native,'ORIGINAL_TRANSACTION_FAILED:'+str(native));require(after[caller]['lamports']-before[caller]['lamports']==int(p['expectedPayerLamportDelta'])==int(e['payerLamportDelta']),'PAYER_RENT_FEE_DELTA')
    post=b['after']['raydium_cpmm'];raw=bytearray(before[pool['pool_id']]['data'])
    for offset,key in [(341,'protocol_fees0'),(349,'protocol_fees1'),(357,'fund_fees0'),(365,'fund_fees1'),(381,'recent_epoch'),(397,'creator_fees0'),(405,'creator_fees1')]:raw[offset:offset+8]=post[key].to_bytes(8,'little')
    require(after[pool['pool_id']]['data']==raw,'FULL_POOL_POSTSTATE')
    for side in [0,1]:require(int.from_bytes(after[state[f'vault{side}']]['data'][64:72],'little')==post[f'raw_vault{side}'],'FULL_VAULT_POSTSTATE')
    oracle_pool=copy.deepcopy(pool);oracle_pool['chain_key']='solana:localnet';oracle=json.loads(subprocess.check_output([str(ROOT/'integration/.runtime/raydium-reference/target/debug/raydium-cpmm-reference'),'oracle-after'],input=json.dumps(dict(pool=oracle_pool,data=list(before[observation]['data']))).encode()));require(bytes(oracle['data'])==after[observation]['data'],'FULL_ORIGINAL_ORACLE')
    if not unwrap:require(int.from_bytes(after[dst]['data'][64:72],'little')-int(p['expectedDestinationInitialTokenBalance'])==int(b['actualOutput']),'USER_TOKEN_OUTPUT')
    if temporary:require(after[temporary]is None,'TEMP_NOT_CLOSED')
    for row in e['postAccounts']:
        a=after[row['address']];summary=dict(owner=SYSTEM,lamports='0',executable=False,dataSha256=digest(b''),dataBytes=0)if a is None else dict(owner=a['owner'],lamports=str(a['lamports']),executable=a['executable'],dataSha256=digest(a['data']),dataBytes=len(a['data']));require(summary=={k:v for k,v in row.items()if k!='address'},'INDEPENDENT_FULL_POST_ACCOUNT')
    cb,ca,control=execute(True);require('err'in control,'LIMIT_CONTROL_SUCCEEDED')
    for key,value in cb.items():
        if key!=caller:require(value==ca[key],'ALL_ACCOUNT_ATOMIC_ROLLBACK')
    require(cb[caller]['lamports']-ca[caller]['lamports']==5000,'FAILED_FEE_ONLY')
    def summaries(values):return {key:None if a is None else dict(owner=a['owner'],lamports=str(a['lamports']),executable=a['executable'],dataSha256=digest(a['data']),dataBytes=len(a['data']))for key,a in values.items()}
    return dict(passed=True,index=index,scenario=b['fundingScenario'],captureHash=p['accountSnapshot']['snapshotHash'],captureRecord=str(path.resolve().relative_to(ROOT)),preparationHash=p['preparationHash'],transactionSha256=digest(wire),wireBytes=len(wire),input=b['requiredInput'],output=b['actualOutput'],payerLamportDelta=p['expectedPayerLamportDelta'],persistentRentLamports=str(int(p['rentLamports'])-int(p['rentRefundLamports'])),native=native,control=control,before=summaries(before),after=summaries(after),controlBefore=summaries(cb),controlAfter=summaries(ca),independentPostAccountsHash=e['postAccountsHash'],fullPoolVaultAndOracle=True,publicTransactionsSubmitted=False,productionReady=False)

def main():
    parser=argparse.ArgumentParser();parser.add_argument('--api-dir',required=True);parser.add_argument('--rpc-dir',required=True);parser.add_argument('--output',required=True);args=parser.parse_args();captures=snapshot_records(args.rpc_dir);rows=[verify(Path(args.api_dir)/f'raydium-{i}-preparation.json',i,captures)for i in range(10)];report=dict(passed=True,scope='Original CP-Swap/Token SBF transaction replay with explicit captured runtime System/ATA/Rent; controlled bank, not historical consensus/production verification',cases=10,originalTransactions=20,captureCount=len(captures),rows=rows,publicTransactionsSubmitted=False,productionReady=False);Path(args.output).write_text(json.dumps(report,indent=2)+'\n');print(json.dumps(dict(passed=True,cases=10,originalTransactions=20)))
if __name__=='__main__':main()
