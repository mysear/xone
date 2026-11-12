#!/usr/bin/env python3
"""Test-only read-only RPC simulation adapter backed by actual pinned SBF in LiteSVM."""
import base64,json,sys,os

def native_imports():
    # Standalone service test dependency; no sibling project's Python source.
    if os.environ.get('XONE_SOLANA_PYTHON_PATH'):sys.path.insert(0,os.environ['XONE_SOLANA_PYTHON_PATH'])
    from importlib.metadata import version
    if version('solders')!='0.29.0':raise ValueError('SIMULATION_RUNTIME_VERSION')
    from solders.litesvm import LiteSVM
    from solders.pubkey import Pubkey
    from solders.keypair import Keypair
    from solders.instruction import Instruction,AccountMeta
    from solders.transaction import Transaction
    from solders.account import Account
    from solders.clock import Clock
    return LiteSVM,Pubkey,Keypair,Instruction,AccountMeta,Transaction,Account,Clock

def simulate(request):
    bank=request['bank']; rpc=request['rpc']; mode=request.get('mode','ready')
    if rpc['method']!='simulateTransaction':raise ValueError('SIMULATION_READ_ONLY_METHOD')
    encoded,config=rpc['params']
    if config['sigVerify']is not False or config['replaceRecentBlockhash']is not False:raise ValueError('SIMULATION_FLAGS')
    S,P,K,I,M,T,A,C=native_imports();svm=S().with_mainnet_features().with_blockhash_check(False).with_sigverify(False)
    programs=['LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo','TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA']
    if 'TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb' in bank['accounts']:programs.append('TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb')
    for pid in programs:
        raw=base64.b64decode(bank['accounts'][pid]['data'][0]);pd=str(P.from_bytes(raw[4:36]));svm.add_program_with_loader(P.from_string(pid),base64.b64decode(bank['accounts'][pd]['data'][0])[45:],P.from_string('BPFLoaderUpgradeab1e11111111111111111111111'))
    for id,a in bank['accounts'].items():svm.set_account(P.from_string(id),A(a['lamports'],base64.b64decode(a['data'][0]),P.from_string(a['owner']),a['executable'],0))
    clock=base64.b64decode(bank['accounts']['SysvarC1ock11111111111111111111111111111111']['data'][0]);slot=int.from_bytes(clock[:8],'little');svm.set_clock(C(slot=slot,epoch_start_timestamp=int.from_bytes(clock[8:16],'little',signed=True),epoch=int.from_bytes(clock[16:24],'little'),leader_schedule_epoch=int.from_bytes(clock[24:32],'little'),unix_timestamp=int.from_bytes(clock[32:40],'little',signed=True)))
    tx=T.from_bytes(base64.b64decode(encoded,validate=True));keys=[str(x)for x in tx.message.account_keys]
    before={id:svm.get_account(P.from_string(id))for id in keys}
    result=svm.simulate_transaction(tx);native=json.loads(result.to_json());post={str(k):a for k,a in result.post_accounts()}if'err'not in native else{}
    # This method must not commit even the ATA create or payer rent.
    for id,a in before.items():
        now=svm.get_account(P.from_string(id))
        if (bytes(a)if a else None)!=(bytes(now)if now else None):raise ValueError('SIMULATION_MUTATED_BANK')
    values=[]
    for id in config['accounts']['addresses']:
        a=post.get(id) if id in post else svm.get_account(P.from_string(id))
        if a is None or (a.lamports==0 and len(a.data)==0):values.append(None);continue
        raw=bytearray(a.data)
        lamports=a.lamports
        if mode=='simulation_mismatch'and id==config['accounts']['addresses'][1]:
            raw[64:72]=(int.from_bytes(raw[64:72],'little')+1).to_bytes(8,'little')
            if raw[109:113]==b'\x01\0\0\0':lamports+=1
        if mode=='withheld_mismatch' and str(a.owner)=='TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb':
            cursor=166
            while cursor<len(raw):
                kind=int.from_bytes(raw[cursor:cursor+2],'little');size=int.from_bytes(raw[cursor+2:cursor+4],'little');cursor+=4
                if kind==2:raw[cursor:cursor+8]=(int.from_bytes(raw[cursor:cursor+8],'little')+1).to_bytes(8,'little');break
                cursor+=size
        values.append(dict(owner=str(a.owner),lamports=lamports,executable=a.executable,data=[base64.b64encode(raw).decode(),'base64']))
    if mode=='native_credit_mismatch' and len(config['accounts']['addresses'])==3 and values[2]is not None:values[2]['lamports']+=1
    if mode=='temp_not_closed':
        for i in range(min(2,len(values))):
            if values[i]is None:values[i]=dict(owner='11111111111111111111111111111111',lamports=1,executable=False,data=['','base64'])
    err=native.get('err')
    if mode=='simulation_error':err={'InstructionError':[0,'InvalidAccountData']}
    return dict(context={'slot':slot},value=dict(err=err,accounts=values,logs=native['meta']['logs'],unitsConsumed=native['meta']['compute_units_consumed']))
if __name__=='__main__':
    raw=sys.stdin.buffer.read(8*1024*1024+1)
    if len(raw)>8*1024*1024:raise ValueError('SIMULATION_INPUT_LIMIT')
    print(json.dumps(simulate(json.loads(raw)),separators=(',',':')))
