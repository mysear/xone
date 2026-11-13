#!/usr/bin/env python3
"""Pinned local SBF worker. Receives bytes via stdin; no RPC/signing/submission."""
import base64
import hashlib
import json
import math
import struct
import sys
from pathlib import Path

PROTOCOL='xone-local-sbf/1'
TOKEN='TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'
TOKEN22='TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb'
METEORA='LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo'
RAYDIUM='CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C'
SYSTEM='11111111111111111111111111111111'
LOADER='BPFLoaderUpgradeab1e11111111111111111111111'
IMMUTABLE='BPFLoader2111111111111111111111111111111111'
CLOCK='SysvarC1ock11111111111111111111111111111111'

def require(condition,code):
    if not condition:raise ValueError(code)

def raw(account):return base64.b64decode(account['data'][0],validate=True)

def main():
    runtime=Path(sys.argv[1]).resolve()
    sys.path.insert(0,str(runtime))
    import solders
    require(solders.__version__=='0.29.0','SBF_RUNTIME_VERSION')
    from solders.litesvm import LiteSVM
    from solders.pubkey import Pubkey
    from solders.account import Account
    from solders.clock import Clock
    from solders.transaction import Transaction
    from solders.compute_budget import ComputeBudget
    from solders.rent import Rent
    request=json.loads(sys.stdin.buffer.read(16*1024*1024+1))
    require(request['protocol']==PROTOCOL,'SBF_WORKER_PROTOCOL')
    preparation=request['preparation'];bank=request['bank'];limit=request['maxComputeUnits']
    wire=base64.b64decode(preparation['unsignedTransaction'],validate=True)
    require(len(wire)<=1232 and wire[:1]==b'\x01' and wire[1:65]==bytes(64),'SBF_UNSIGNED_REQUIRED')
    tx=Transaction.from_bytes(wire)
    require(tx.message.header.num_required_signatures==1 and str(tx.message.account_keys[0])==request['caller'],'SBF_CALLER_SIGNER')
    require(bytes(tx.message)==base64.b64decode(preparation['messageBase64'],validate=True),'SBF_MESSAGE_BINDING')
    budget=ComputeBudget(True);budget.compute_unit_limit=limit
    svm=LiteSVM().with_mainnet_features().with_compute_budget(budget).with_blockhash_check(False).with_sigverify(False)
    # Local and preparation-RPC banks both run under explicitly pinned VM
    # features; neither a slot label nor latest account RPC proves consensus.
    # The captured blockhash is used as message bytes only. Live signed RPC
    # preflight still checks its actual network validity before submission.
    programs={str(tx.message.account_keys[ix.program_id_index])for ix in tx.message.instructions}
    target=RAYDIUM if bank['pool']['protocol']=='raydium-cpmm-creator' else METEORA
    require(target in programs and programs<={SYSTEM,TOKEN,TOKEN22,target,'ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL'},'SBF_PROGRAM_ALLOWLIST')
    model=bank['pool'];state=model.get('raydium_cpmm') if target==RAYDIUM else model.get('meteora')
    require(state is not None,'SBF_POOL_MODEL')
    def account_bytes(key):return raw(bank['accounts'][key])
    clock_data=account_bytes(CLOCK)
    expected_slot=state['bank_slot'] if target==RAYDIUM else state['slot']
    expected_time=state['bank_unix_timestamp'] if target==RAYDIUM else state['timestamp']
    require(int.from_bytes(clock_data[:8],'little')==expected_slot and int.from_bytes(clock_data[32:40],'little')==expected_time,'SBF_BANK_CLOCK_MISMATCH')
    if target==METEORA:
        require(account_bytes(model['pool_id']).hex()==state['pair_data'],'SBF_BANK_POOL_MISMATCH')
        pair=account_bytes(model['pool_id'])
        oracle=str(Pubkey.from_bytes(pair[552:584]));require(account_bytes(oracle).hex()==state['oracle_data'],'SBF_BANK_ORACLE_MISMATCH')
        for item in state['bin_arrays']:require(account_bytes(item['address']).hex()==item['data'],'SBF_BANK_ARRAY_MISMATCH')
        if state.get('bitmap_extension'):require(account_bytes(state['bitmap_extension']['address']).hex()==state['bitmap_extension']['data'],'SBF_BANK_BITMAP_MISMATCH')
        if state.get('token2022'):
            t=state['token2022'];require(account_bytes(model['token0']).hex()==t['mint_data'] and int.from_bytes(clock_data[16:24],'little')==t['bank_epoch'],'SBF_BANK_TOKEN2022_MISMATCH')
            require(account_bytes(str(Pubkey.from_bytes(pair[152:184]))).hex()==t['vault_data'],'SBF_BANK_VAULT_MISMATCH')
    else:
        pair=account_bytes(model['pool_id']);require(len(pair)==637 and pair[329]&4==0,'SBF_BANK_POOL_MISMATCH')
        for offset,key in [(341,'protocol_fees0'),(349,'protocol_fees1'),(357,'fund_fees0'),(365,'fund_fees1'),(373,'open_time'),(381,'recent_epoch'),(397,'creator_fees0'),(405,'creator_fees1')]:require(int.from_bytes(pair[offset:offset+8],'little')==state[key],'SBF_BANK_LEDGER_MISMATCH')
        require(pair[389]==state['creator_fee_on']and bool(pair[390])==state['creator_fee_enabled'],'SBF_BANK_CREATOR_MISMATCH')
        cfg=account_bytes(state['amm_config'])
        for offset,value in [(12,model['fee_numerator']),(20,state['protocol_fee_rate']),(28,state['fund_fee_rate']),(108,state['creator_fee_rate']),(116,state['creator_fee_share_rate'])]:require(int.from_bytes(cfg[offset:offset+8],'little')==value,'SBF_BANK_CONFIG_MISMATCH')
        for side in [0,1]:require(int.from_bytes(account_bytes(state[f'vault{side}'])[64:72],'little')==state[f'raw_vault{side}'],'SBF_BANK_VAULT_MISMATCH')
        require(int.from_bytes(clock_data[16:24],'little')==state['bank_epoch'],'SBF_BANK_EPOCH_MISMATCH')
    dependencies={target,TOKEN}
    if bank['pool'].get('meteora',{}).get('token2022')is not None:dependencies.add(TOKEN22)
    for pid in sorted(dependencies):
        a=bank['accounts'][pid];data=raw(a)
        require(a['executable']is True and a['owner']==LOADER and len(data)==36 and data[:4]==b'\x02\0\0\0','SBF_PROGRAM_LAYOUT')
        pd=str(Pubkey.from_bytes(data[4:36]));programdata=bank['accounts'][pd]
        require(programdata['owner']==LOADER and programdata['executable']is False and raw(programdata)[:4]==b'\x03\0\0\0','SBF_PROGRAMDATA_LAYOUT')
        programdata_bytes=raw(programdata)
        deployment=int.from_bytes(programdata_bytes[4:12],'little')
        require(deployment<=expected_slot,'SBF_BANK_DEPLOYMENT_MISMATCH')
        if pid==target:require(deployment==state['deployed_slot'],'SBF_BANK_DEPLOYMENT_MISMATCH')
        if pid==TOKEN22:require(deployment==state['token2022']['deployed_slot'],'SBF_BANK_DEPLOYMENT_MISMATCH')
        payload=raw(programdata)[45:]
        expected=state['program_code_sha256'] if pid==target else state['token2022']['program_code_sha256'] if pid==TOKEN22 else state['token_program_code_sha256']
        require(hashlib.sha256(payload).hexdigest()==expected,'SBF_BANK_PROGRAM_MISMATCH')
        svm.add_program_with_loader(Pubkey.from_string(pid),payload,Pubkey.from_string(LOADER))
    if bank.get('rpcCapture'):
        accounts=bank['accounts'];absent=set(bank['absentAccounts']);keys=list(map(str,tx.message.account_keys))
        require(set(keys)<=set(accounts)|absent,'SBF_CAPTURE_ACCOUNT_MISSING')
        require(not(set(accounts)&absent),'SBF_CAPTURE_ACCOUNT_CONTRADICTION')
        for key in absent:require(svm.get_account(Pubkey.from_string(key))is None,'SBF_CAPTURE_ABSENT_RUNTIME_CONFLICT')
        rent_id='SysvarRent111111111111111111111111111111111'
        rent=accounts.get(rent_id)
        require(rent is not None and rent['owner']=='Sysvar1111111111111111111111111111111111111'and not rent['executable']and len(raw(rent))==17,'SBF_RENT_REQUIRED')
        lamports,threshold,burn=struct.unpack('<QdB',raw(rent));require(lamports>0 and math.isfinite(threshold)and threshold>0 and burn<=100,'SBF_RENT_INVALID')
        svm.set_rent(Rent(lamports,threshold,burn))
        # Bundled native/ATA/Memo programs cannot be replaced merely by setting
        # account bytes. Their captured executable identity must match the VM.
        for key,a in accounts.items():
            if a['executable']and key not in dependencies:
                builtin=svm.get_account(Pubkey.from_string(key))
                require(builtin is not None and builtin.executable and str(builtin.owner)==a['owner']and bytes(builtin.data)==raw(a),'SBF_AMBIENT_PROGRAM_MISMATCH')
    for key,a in bank['accounts'].items():
        svm.set_account(Pubkey.from_string(key),Account(a['lamports'],raw(a),Pubkey.from_string(a['owner']),a['executable'],0))
    clock=raw(bank['accounts'][CLOCK]);require(len(clock)==40,'SBF_CLOCK_LAYOUT')
    svm.set_clock(Clock(slot=int.from_bytes(clock[:8],'little'),epoch_start_timestamp=int.from_bytes(clock[8:16],'little',signed=True),epoch=int.from_bytes(clock[16:24],'little'),leader_schedule_epoch=int.from_bytes(clock[24:32],'little'),unix_timestamp=int.from_bytes(clock[32:40],'little',signed=True)))
    keys=list(map(str,tx.message.account_keys));before={key:svm.get_account(Pubkey.from_string(key))for key in keys}
    result=svm.simulate_transaction(tx);native=json.loads(result.to_json())
    for key,a in before.items():
        current=svm.get_account(Pubkey.from_string(key))
        require((bytes(a)if a else None)==(bytes(current)if current else None),'SBF_BANK_MUTATED')
    if native.get('err')is not None:
        return dict(protocol=PROTOCOL,success=False,error='SBF_TRANSACTION_FAILED',nativeError=native['err'],unitsConsumed=native['meta']['compute_units_consumed'],sourceBankUnchanged=True)
    post={str(key):value for key,value in result.post_accounts()}
    def account(key):
        a=post.get(key)if key in post else svm.get_account(Pubkey.from_string(key))
        return None if a is None or (a.lamports==0 and len(a.data)==0)else a
    caller=request['caller'];source=preparation['sourceTokenAccount'];destination=preparation['destinationTokenAccount']
    def amount(a,mint,owner,program):
        require(a is not None and not a.executable and str(a.owner)==program and len(a.data)>=165,'SBF_TOKEN_ACCOUNT')
        data=a.data;require(data[:32]==bytes(Pubkey.from_string(mint))and data[32:64]==bytes(Pubkey.from_string(owner))and data[108]==1,'SBF_TOKEN_IDENTITY')
        return int.from_bytes(data[64:72],'little')
    source_before=before.get(source);destination_before=before.get(destination)
    native_before=before[caller].lamports;native_after=account(caller)
    require(native_after is not None and str(native_after.owner)==SYSTEM and len(native_after.data)==0,'SBF_PAYER_ACCOUNT')
    fee=native['meta']['fee'];rent=int(preparation['rentLamports']);refund_rent=int(preparation['rentRefundLamports']);persistent=rent-refund_rent
    require(persistent>=0 and fee==int(preparation['feeLamports']),'SBF_FEE_RENT_BINDING')
    source_program=preparation['sourceTokenProgram'];dest_program=preparation['destinationTokenProgram']
    if preparation['refundWrappedSource']:
        require(account(source)is None,'SBF_TEMP_NOT_CLOSED')
        paid=native_before-native_after.lamports-fee-persistent
    else:
        initial=0 if source_before is None else amount(source_before,request['sellToken'],caller,source_program)
        paid=initial+int(preparation['wrapLamports'])-amount(account(source),request['sellToken'],caller,source_program)
    if preparation['unwrapDestinationSol']:
        require(account(destination)is None,'SBF_TEMP_NOT_CLOSED')
        received=native_after.lamports-native_before+fee+persistent
    else:
        initial=0 if destination_before is None else amount(destination_before,request['buyToken'],request['recipient'],dest_program)
        received=amount(account(destination),request['buyToken'],request['recipient'],dest_program)-initial
    require(paid==int(preparation['quotedInput'])and received==int(preparation['quotedOutput']),'SBF_AMOUNT_MISMATCH')
    require(native_after.lamports-native_before==int(preparation['expectedPayerLamportDelta']),'SBF_NATIVE_BALANCE_MISMATCH')
    def withheld(a):
        if a is None:return 0
        data=a.data;require(len(data)in [178,182]and data[165]==2,'SBF_TOKEN2022_LAYOUT');cursor=166;found=None;seen=set()
        while cursor<len(data):
            kind=int.from_bytes(data[cursor:cursor+2],'little');size=int.from_bytes(data[cursor+2:cursor+4],'little');cursor+=4
            require(kind not in seen and (kind,size)in [(2,8),(7,0)] and cursor+size<=len(data),'SBF_TOKEN2022_EXTENSION');seen.add(kind)
            if kind==2:found=int.from_bytes(data[cursor:cursor+8],'little')
            cursor+=size
        require(found is not None,'SBF_TOKEN2022_FEE_REQUIRED');return found
    if source_program==TOKEN22:require(withheld(account(source))==withheld(source_before),'SBF_SOURCE_WITHHELD_MISMATCH')
    if dest_program==TOKEN22:require(withheld(account(destination))-withheld(destination_before)==int(preparation['expectedDestinationTransferFee']),'SBF_DESTINATION_WITHHELD_MISMATCH')
    account_evidence=[]
    for key in sorted(post):
        a=post[key];account_evidence.append(dict(address=key,owner=str(a.owner),lamports=str(a.lamports),executable=a.executable,dataSha256=hashlib.sha256(a.data).hexdigest(),dataBytes=len(a.data)))
    return dict(protocol=PROTOCOL,success=True,actualInput=str(paid),actualOutput=str(received),feeLamports=str(fee),payerLamportDelta=str(native_after.lamports-native_before),unitsConsumed=native['meta']['compute_units_consumed'],maxComputeUnits=limit,sourceBankUnchanged=True,transactionSha256=hashlib.sha256(wire).hexdigest(),postAccounts=account_evidence,postAccountsHash=hashlib.sha256(json.dumps(account_evidence,sort_keys=True,separators=(',',':')).encode()).hexdigest(),independentProductionVerification=False,signed=False,submitted=False)

if __name__=='__main__':
    try:result=main()
    except Exception as error:result=dict(protocol=PROTOCOL,success=False,error=str(error)if isinstance(error,ValueError)else'SBF_WORKER_FAILURE')
    print(json.dumps(result,separators=(',',':')))
