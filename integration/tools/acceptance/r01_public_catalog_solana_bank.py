#!/usr/bin/env python3
"""Capture public Solana pool accounts and replay actual SBF in a native local bank.

The sourcebank is one RPC getMultipleAccounts observation. Its slot and separate
getBlock response are NOT a historical original-bank proof. Replay deliberately
funds a local payer and two local SPL accounts; every override is reported.
No RPC transaction submission exists in this tool.
"""
import argparse
import base64
import copy
import datetime
from functools import lru_cache
import hashlib
import json
from pathlib import Path
import struct
import sys
import urllib.request

from r01_public_catalog_solana_rebuild import ROOT,BASE,LOADER,VerificationError,require,digest,b58encode,b58decode,bind_programdata,token_delta,programdata_pda,inspect_elf

TOKEN='TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'
TOKEN22='TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb'
MEMO='MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr'
CLOCK='SysvarC1ock11111111111111111111111111111111'
WSOL='So11111111111111111111111111111111111111112'
GENESIS='5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d'
READ_METHODS={'getMultipleAccounts','getBlock','getBlockTime','getGenesisHash','minimumLedgerSlot','getFirstAvailableBlock'}
DEPS=ROOT/'integration/artifacts/local/r01-solana-rebuild/python'


def verify_runtime_lock(lock):
    require(lock['package']=='solders' and lock['version']=='0.29.0','native-runtime-version')
    for ref in lock['files']:
        path=(ROOT/ref['path']).resolve();require(path.is_relative_to(DEPS.resolve()),'native-runtime-cache-scope')
        require(digest(path.read_bytes())==ref['sha256'],'native-runtime-file-pin')
    return lock


@lru_cache(maxsize=1)
def native_imports():
    verify_runtime_lock(json.loads((BASE/'solana-rebuild/runtime-lock.json').read_text()))
    if DEPS.is_dir():sys.path.insert(0,str(DEPS))
    from solders.litesvm import LiteSVM
    from solders.pubkey import Pubkey
    from solders.keypair import Keypair
    from solders.instruction import Instruction,AccountMeta
    from solders.transaction import Transaction
    from solders.account import Account
    from solders.clock import Clock
    return LiteSVM,Pubkey,Keypair,Instruction,AccountMeta,Transaction,Account,Clock


def rpc_read(endpoint,method,params):
    require(method in READ_METHODS,'rpc-write-or-unsupported-method')
    request={'jsonrpc':'2.0','id':1,'method':method,'params':params}
    started=datetime.datetime.now(datetime.timezone.utc).isoformat()
    with urllib.request.urlopen(urllib.request.Request(endpoint,data=json.dumps(request).encode(),headers={'Content-Type':'application/json','User-Agent':'XONE-public-native-bank/1.0'}),timeout=45) as response:
        raw=response.read(32*1024*1024+1);require(len(raw)<=32*1024*1024,'rpc-response-budget')
        return {'endpoint':endpoint,'method':method,'params':params,'observed_at':started,'http_status':response.status,'raw_response_sha256':digest(raw),'raw_response_base64':base64.b64encode(raw).decode(),'response':json.loads(raw)}


def verify_rpc_record(record):
    raw=base64.b64decode(record['raw_response_base64'],validate=True)
    require(digest(raw)==record['raw_response_sha256'],'rpc-raw-response-hash')
    require(json.loads(raw)==record['response'],'rpc-parsed-response-substitution')
    return record


def account_bytes(account):
    require(account['data'][1]=='base64','account-encoding')
    return base64.b64decode(account['data'][0],validate=True)


def decode_token_account(account):
    require(account['owner']==TOKEN and account['executable'] is False,'original-spl-token-account-owner')
    raw=account_bytes(account);require(len(raw)==165 and raw[108]==1,'initialized-classic-spl-account')
    return {'mint':b58encode(raw[:32]),'owner':b58encode(raw[32:64]),'amount':struct.unpack_from('<Q',raw,64)[0]}


def decode_clock(data):
    require(len(data)==40,'clock-layout')
    return dict(zip(['slot','epoch_start_timestamp','epoch','leader_schedule_epoch','unix_timestamp'],struct.unpack('<QqQQq',data)))


def parse_pool(protocol,address,account):
    raw=account_bytes(account)
    def key(offset):return b58encode(raw[offset:offset+32])
    result={'protocol':protocol,'pool':address}
    require(raw[:8]==hashlib.sha256(('account:'+('Whirlpool' if protocol=='orca-whirlpools' else 'PoolState')).encode()).digest()[:8],'pool-discriminator')
    if protocol=='raydium-cpmm':
        require(len(raw)>=333,'cpmm-pool-layout')
        result.update(config=key(8),vault_a=key(72),vault_b=key(104),mint_a=key(168),mint_b=key(200),token_program_a=key(232),token_program_b=key(264),observation=key(296))
    elif protocol=='raydium-clmm':
        require(len(raw)>=273,'clmm-pool-layout')
        result.update(config=key(9),vault_a=key(137),vault_b=key(169),mint_a=key(73),mint_b=key(105),observation=key(201),tick_spacing=struct.unpack_from('<H',raw,235)[0],tick_current=struct.unpack_from('<i',raw,269)[0],sqrt_price=int.from_bytes(raw[253:269],'little'))
    elif protocol=='orca-whirlpools':
        require(len(raw)>=245,'orca-pool-layout')
        result.update(config=key(8),vault_a=key(133),vault_b=key(213),mint_a=key(101),mint_b=key(181),tick_spacing=struct.unpack_from('<H',raw,41)[0],tick_current=struct.unpack_from('<i',raw,81)[0],sqrt_price=int.from_bytes(raw[65:81],'little'))
    else:raise VerificationError('unsupported-pool-protocol')
    return result


def pda(seeds,program):
    _,Pubkey,*_=native_imports()
    return str(Pubkey.find_program_address(seeds,Pubkey.from_string(program))[0])


def pool_extra_keys(pool,program):
    protocol=pool['protocol'];keys={}
    if protocol=='raydium-cpmm':keys['authority']=pda([b'vault_and_lp_mint_auth_seed'],program)
    else:
        count=60 if protocol=='raydium-clmm' else 88;spacing=pool['tick_spacing'];require(spacing>0,'tick-spacing')
        start=pool['tick_current']//(count*spacing)*(count*spacing)
        ticks=[]
        # Both directions are captured so discovery drift cannot silently select
        # a tick-array address inferred from a different RPC bank.
        for n in range(-3,4):
            value=start+n*count*spacing
            if -887272<=value<=887272:
                seed=struct.pack('>i',value) if protocol=='raydium-clmm' else str(value).encode()
                ticks.append({'start':value,'address':pda([b'tick_array',b58decode(pool['pool']),seed],program)})
        keys['tick_arrays']=ticks
        if protocol=='raydium-clmm':keys['bitmap']=pda([b'pool_tick_array_bitmap_extension',b58decode(pool['pool'])],program)
        else:keys['observation']=pda([b'oracle',b58decode(pool['pool'])],program)
    return keys


def reference(path):
    path=path.resolve();raw=path.read_bytes();return {'path':str(path.relative_to(ROOT)),'sha256':digest(raw),'bytes':len(raw)}


def load_reference(ref):
    path=(ROOT/ref['path']).resolve();require(path.is_relative_to(ROOT),'evidence-outside-workspace')
    raw=path.read_bytes();require(digest(raw)==ref['sha256'],'evidence-file-hash')
    return json.loads(raw)


def capture(protocol,pool_address,output,endpoint):
    require(len(b58decode(pool_address))==32,'pool-public-key');require(not output.exists(),'capture-destination-exists');output.mkdir(parents=True)
    row=next(r for r in json.loads((BASE/'catalog.json').read_text())['deployments'] if r['protocol']==protocol)
    genesis=rpc_read(endpoint,'getGenesisHash',[]);(output/'genesis.json').write_text(json.dumps(genesis,indent=2)+'\n');require(genesis['response'].get('result')==GENESIS,'mainnet-genesis')
    discovery=rpc_read(endpoint,'getMultipleAccounts',[[pool_address,TOKEN,MEMO],{'encoding':'base64','commitment':'finalized'}])
    (output/'discovery.json').write_text(json.dumps(discovery,indent=2)+'\n');require('result' in discovery['response'],'rpc-discovery-error:'+str(discovery['response'].get('error')));pool_account=discovery['response']['result']['value'][0];require(pool_account is not None and pool_account['owner']==row['address'],'pool-program-owner')
    parsed=parse_pool(protocol,pool_address,pool_account);extras=pool_extra_keys(parsed,row['address'])
    addresses=[row['address'],row['upgrade']['programdata'],pool_address,parsed['vault_a'],parsed['vault_b'],parsed['mint_a'],parsed['mint_b'],parsed['config'],TOKEN,TOKEN22,MEMO,CLOCK]
    addresses.extend([parsed['observation']] if 'observation' in parsed else [])
    for key,value in extras.items():
        if key=='tick_arrays':addresses.extend(x['address'] for x in value)
        else:addresses.append(value)
    for account in discovery['response']['result']['value'][1:]:
        if account and account['owner']==LOADER:
            raw=account_bytes(account)
            if len(raw)==36 and raw[:4]==struct.pack('<I',2):addresses.append(b58encode(raw[4:]))
    addresses=list(dict.fromkeys(addresses))
    snapshot=rpc_read(endpoint,'getMultipleAccounts',[addresses,{'encoding':'base64','commitment':'finalized'}]);(output/'accounts.json').write_text(json.dumps(snapshot,indent=2)+'\n');require('result' in snapshot['response'],'rpc-snapshot-error:'+str(snapshot['response'].get('error')));result=snapshot['response']['result'];current=result['value'][addresses.index(pool_address)]
    final_pool=parse_pool(protocol,pool_address,current)
    # The final snapshot is authoritative. Require the originally discovered
    # dependencies still agree; recapture when the pool moves across the range.
    require(all(parsed.get(k)==final_pool.get(k) for k in ['config','vault_a','vault_b','mint_a','mint_b']),'pool-dependency-drift')
    if 'tick_current' in final_pool:
        count=60 if protocol=='raydium-clmm' else 88
        final_start=final_pool['tick_current']//(count*final_pool['tick_spacing'])*(count*final_pool['tick_spacing'])
        require(final_start in [x['start'] for x in extras['tick_arrays']],'tick-array-discovery-drift')
    binding_row=copy.deepcopy(row);binding_row['anchor']['height']=result['context']['slot']
    pd_observation={'method':'getMultipleAccounts','params':[[row['address'],row['upgrade']['programdata']]],'response':{'result':{'context':result['context'],'value':result['value'][:2]}}}
    binding=bind_programdata(binding_row,pd_observation)
    for name,value in [('genesis',genesis),('discovery',discovery),('accounts',snapshot)]:
        (output/(name+'.json')).write_text(json.dumps(value,indent=2)+'\n')
    block=rpc_read(endpoint,'getBlock',[result['context']['slot'],{'encoding':'json','transactionDetails':'none','rewards':False,'maxSupportedTransactionVersion':0}]);(output/'block.json').write_text(json.dumps(block,indent=2)+'\n')
    manifest={'schema_version':1,'bank_kind':'rpc-account-snapshot-for-local-new-bank','historical_original_bank_verified':False,'catalog_row':row,'pool':final_pool,'extras':extras,'binding':binding,'accounts':reference(output/'accounts.json'),'genesis':reference(output/'genesis.json'),'discovery':reference(output/'discovery.json'),'block':reference(output/'block.json'),'state_authority':'one finalized getMultipleAccounts response; no snapshot bank hash or ledger ancestry proof'}
    (output/'sourcebank.json').write_text(json.dumps(manifest,indent=2)+'\n');return manifest


def validate_capture(manifest):
    require(manifest.get('schema_version')==1 and manifest.get('bank_kind')=='rpc-account-snapshot-for-local-new-bank','native-sourcebank-required')
    require(verify_rpc_record(load_reference(manifest['genesis']))['response'].get('result')==GENESIS,'capture-genesis')
    snapshot=verify_rpc_record(load_reference(manifest['accounts']));result=snapshot['response']['result'];addresses=snapshot['params'][0];accounts=result['value']
    require(snapshot['method']=='getMultipleAccounts' and len(addresses)==len(accounts) and len(set(addresses))==len(addresses),'snapshot-address-inventory')
    # Authenticate the unmodified original catalog row, including ProgramData and
    # source payload identity. A fixture cannot substitute its own deployment.
    row=manifest['catalog_row'];trusted=next(r for r in json.loads((BASE/'catalog.json').read_text())['deployments'] if r['id']==row['id'])
    require(all(row[k]==trusted[k] for k in ['id','chain','protocol','address','version','chain_key','genesis_hash','anchor','runtime','upgrade']),'catalog-row-substitution')
    require(addresses[:2]==[row['address'],row['upgrade']['programdata']],'snapshot-program-addresses')
    binding_row=copy.deepcopy(row);binding_row['anchor']['height']=result['context']['slot']
    binding=bind_programdata(binding_row,{'method':'getMultipleAccounts','params':[addresses[:2]],'response':{'result':{'context':result['context'],'value':accounts[:2]}}})
    require(binding==manifest['binding'],'snapshot-binding-substitution')
    mapping=dict(zip(addresses,accounts));pool=manifest['pool'];pool_account=mapping[pool['pool']]
    require(pool_account['owner']==row['address'],'snapshot-pool-owner')
    parsed=parse_pool(row['protocol'],pool['pool'],pool_account);require(parsed==pool,'snapshot-pool-decode')
    derived=pool_extra_keys(parsed,row['address']);extras=manifest['extras']
    if row['protocol']=='raydium-cpmm':require(extras==derived,'snapshot-authority-pda')
    else:
        for tick in extras['tick_arrays']:
            seed=struct.pack('>i',tick['start']) if row['protocol']=='raydium-clmm' else str(tick['start']).encode()
            require(tick['address']==pda([b'tick_array',b58decode(pool['pool']),seed],row['address']),'snapshot-tick-pda')
        for field in ('bitmap','observation'):
            if field in derived:require(extras[field]==derived[field],'snapshot-extra-pda')
    for label in ('a','b'):
        vault=decode_token_account(mapping[pool['vault_'+label]]);require(vault['mint']==pool['mint_'+label],'pool-vault-mint')
        require(mapping[pool['mint_'+label]]['owner']==TOKEN,'classic-token-mint-required')
    clock=decode_clock(account_bytes(mapping[CLOCK]));require(clock['slot']==result['context']['slot'],'snapshot-clock-bank-slot')
    return mapping,binding,clock


def program_payload(address,mapping,bank_slot):
    require(type(bank_slot) is int and bank_slot>=0,'native-program-bank-slot')
    require(address in mapping and mapping[address] is not None,'native-program-account-missing')
    program=mapping[address];require(program['executable'] is True,'native-program-executable')
    raw=account_bytes(program)
    if program['owner']==LOADER:
        require(len(raw)==36 and raw[:4]==(2).to_bytes(4,'little'),'native-program-upgrade-header')
        pd=b58encode(raw[4:]);require(pd==programdata_pda(address),'native-programdata-PDA-pointer')
        require(pd in mapping and mapping[pd] is not None,'native-programdata-missing')
        account=mapping[pd];require(account['owner']==LOADER and account['executable'] is False,'native-programdata-owner-executable')
        data=account_bytes(account);require(len(data)>45 and data[:4]==(3).to_bytes(4,'little') and data[12] in (0,1),'native-programdata-state')
        require(int.from_bytes(data[4:12],'little')<=bank_slot,'native-programdata-upgrade-after-bank')
        payload=data[45:]
    else:
        require(program['owner']=='BPFLoader2111111111111111111111111111111111','native-immutable-loader-owner')
        payload=raw
    inspect_elf(payload)
    return payload


def run_native(manifest,amount=1000000,*,minimum_amount_out=1):
    require(type(amount) is int and 0<amount<=1000000000,'controlled-swap-amount')
    require(type(minimum_amount_out) is int and 0<=minimum_amount_out<2**64,'controlled-minimum-output-u64')
    mapping,binding,clock=validate_capture(manifest)
    LiteSVM,Pubkey,Keypair,Instruction,AccountMeta,Transaction,Account,Clock=native_imports()
    svm=LiteSVM().with_mainnet_features();row=manifest['catalog_row'];pid=Pubkey.from_string(row['address']);pool=manifest['pool'];extras=manifest['extras']
    payload=account_bytes(mapping[row['upgrade']['programdata']])[45:]
    svm.add_program_with_loader(pid,payload,Pubkey.from_string(LOADER))
    loaded=[{'program':str(pid),'payload_sha256':digest(payload),'payload_bytes':len(payload),'loader':LOADER,'upgrade_slot':binding['upgrade_slot']}]
    # Replace LiteSVM's bundled token/memo code with the captured actual programs.
    for address in (TOKEN,MEMO):
        a=mapping[address];code=program_payload(address,mapping,binding['observed_context_slot'])
        svm.add_program_with_loader(Pubkey.from_string(address),code,Pubkey.from_string(a['owner']))
        loaded.append({'program':address,'payload_sha256':digest(code),'payload_bytes':len(code),'loader':a['owner']})
    for address,a in mapping.items():
        if a is not None:svm.set_account(Pubkey.from_string(address),Account(a['lamports'],account_bytes(a),Pubkey.from_string(a['owner']),a['executable'],a['rentEpoch']))
    svm.set_clock(Clock(**clock))
    payer=Keypair.from_seed(hashlib.sha256(b'R01-public-native-bank-payer').digest());recipient=Pubkey.from_bytes(hashlib.sha256(b'R01-distinct-net-recipient').digest())
    svm.airdrop(payer.pubkey(),10000000000)
    user_a=Pubkey.from_bytes(hashlib.sha256((row['id']+'-input').encode()).digest());user_b=Pubkey.from_bytes(hashlib.sha256((row['id']+'-recipient').encode()).digest())
    overrides=[]
    for address,mint,owner,quantity in [(user_a,pool['mint_a'],payer.pubkey(),1000000000),(user_b,pool['mint_b'],recipient,7)]:
        raw=bytearray(165);raw[:32]=b58decode(mint);raw[32:64]=bytes(owner);struct.pack_into('<Q',raw,64,quantity);raw[108]=1;rent=svm.minimum_balance_for_rent_exemption(165);lamports=rent
        if mint==WSOL:struct.pack_into('<IQ',raw,109,1,rent);lamports+=quantity
        a=Account(lamports,bytes(raw),Pubkey.from_string(TOKEN),False,0);svm.set_account(address,a)
        overrides.append({'address':str(address),'purpose':'synthetic-local-input-funding' if address==user_a else 'distinct-recipient-token-account','account':json.loads(a.to_json())})
    def meta(key,writable=False,signer=False):return AccountMeta(Pubkey.from_string(str(key)),signer,writable)
    protocol=row['protocol']
    if protocol=='raydium-cpmm':
        metas=[meta(payer.pubkey(),True,True),meta(extras['authority']),meta(pool['config']),meta(pool['pool'],True),meta(user_a,True),meta(user_b,True),meta(pool['vault_a'],True),meta(pool['vault_b'],True),meta(TOKEN),meta(TOKEN),meta(pool['mint_a']),meta(pool['mint_b']),meta(pool['observation'],True)]
        instruction_name='swap_base_input'
    elif protocol=='raydium-clmm':
        metas=[meta(payer.pubkey(),True,True),meta(pool['config']),meta(pool['pool'],True),meta(user_a,True),meta(user_b,True),meta(pool['vault_a'],True),meta(pool['vault_b'],True),meta(pool['observation'],True),meta(TOKEN),meta(TOKEN22),meta(MEMO),meta(pool['mint_a']),meta(pool['mint_b']),meta(extras['bitmap'],True)]
        count=60*pool['tick_spacing'];start=pool['tick_current']//count*count;ticks={x['start']:x['address'] for x in extras['tick_arrays']}
        metas.extend(meta(ticks[start-n*count],True) for n in range(3) if start-n*count in ticks and mapping.get(ticks[start-n*count]) is not None)
        instruction_name='swap_v2'
    else:
        count=88*pool['tick_spacing'];start=pool['tick_current']//count*count;ticks={x['start']:x['address'] for x in extras['tick_arrays']}
        metas=[meta(TOKEN),meta(payer.pubkey(),False,True),meta(pool['pool'],True),meta(user_a,True),meta(pool['vault_a'],True),meta(user_b,True),meta(pool['vault_b'],True)]
        metas.extend(meta(ticks[start-n*count],True) for n in range(3));metas.append(meta(extras['observation'],True));instruction_name='swap'
    discriminator=hashlib.sha256(('global:'+instruction_name).encode()).digest()[:8]
    def get_state(address):
        a=svm.get_account(Pubkey.from_string(str(address)));require(a is not None,'native-account-missing');return {'lamports':a.lamports,'owner':str(a.owner),'executable':a.executable,'rentEpoch':a.rent_epoch,'data':[base64.b64encode(a.data).decode(),'base64']}
    before={str(key):get_state(key) for key in (user_a,user_b,pool['vault_a'],pool['vault_b'],pool['pool'])};cases=[]
    for label,minimum in [('negative-slippage',2**64-1),('success',minimum_amount_out)]:
        data=discriminator+struct.pack('<QQ',amount,minimum)
        if protocol=='raydium-clmm':data+=(4295048017).to_bytes(16,'little')+b'\x01'
        elif protocol=='orca-whirlpools':data+=(4295048016).to_bytes(16,'little')+b'\x01\x01'
        ix=Instruction(pid,data,metas);tx=Transaction.new_signed_with_payer([ix],payer.pubkey(),[payer],svm.latest_blockhash());result=svm.send_transaction(tx);raw=json.loads(result.to_json());after={str(key):get_state(key) for key in (user_a,user_b,pool['vault_a'],pool['vault_b'],pool['pool'])}
        success='err' not in raw
        delta=token_delta(decode_token_account(before[str(user_b)]),decode_token_account(after[str(user_b)]),pool['mint_b'],str(recipient))
        case={'case':label,'instruction':instruction_name,'instruction_data_base64':base64.b64encode(data).decode(),'accounts':[{'address':str(m.pubkey),'signer':m.is_signer,'writable':m.is_writable} for m in metas],'transaction_base64':base64.b64encode(bytes(tx)).decode(),'native_result':raw,'success':success,'net_recipient_raw_delta':delta,'before':before,'after':after,'all_tracked_accounts_unchanged':before==after}
        cases.append(case)
        if label=='negative-slippage':
            expected={'raydium-cpmm':6005,'raydium-clmm':6018,'orca-whirlpools':6036}[protocol]
            require(not success and before==after and raw.get('err')=={'InstructionError':[0,{'Custom':expected}]},'native-slippage-negative-control-failed')
    successful=cases[1]['success'] and cases[1]['net_recipient_raw_delta']>0
    if successful:
        final=cases[1]['after'];net=cases[1]['net_recipient_raw_delta']
        for address,mint,owner,expected_delta in [(str(user_a),pool['mint_a'],str(payer.pubkey()),-amount),(pool['vault_a'],pool['mint_a'],decode_token_account(before[pool['vault_a']])['owner'],amount),(pool['vault_b'],pool['mint_b'],decode_token_account(before[pool['vault_b']])['owner'],-net)]:
            require(token_delta(decode_token_account(before[address]),decode_token_account(final[address]),mint,owner)==expected_delta,'native-token-conservation')
    # Require the real token program was invoked by the actual pool program.
    logs=cases[1]['native_result'].get('logs',[])
    cpi=any(f'Program {TOKEN} invoke [2]' in x for x in logs)
    library=next((DEPS/'solders').glob('*.so'))
    return {'schema_version':1,'bank_kind':'local-new-native-LiteSVM-bank','historical_original_bank_verified':False,'consensus_bank_hash_verified':False,'binding':binding,'sourcebank_manifest_sha256':digest(json.dumps(manifest,sort_keys=True,separators=(',',':')).encode()),'clock':clock,'native_library':reference(library),'runtime_feature_policy':'solders 0.29.0 LiteSVM mainnet_feature_set; not historical feature reconstruction','loaded_programs':loaded,'payer':str(payer.pubkey()),'recipient':str(recipient),'recipient_is_distinct_from_payer':recipient!=payer.pubkey(),'local_overrides':overrides,'local_payer_funding_lamports':10000000000,'cases':cases,'original_token_cpi_observed':cpi,'controlled_net_recipient_verified':bool(successful and cpi),'qualification_scope':'actual deployed SBF and original classic SPL token in a controlled local bank using one public RPC account snapshot'}


def execution_quantities(manifest,execution):
    """Read distinct token quantities from actual raw before/after accounts.

    Pool gross here means output vault debit after pool trade/creator fees,
    before a possible SPL transfer fee or any chain/gas cost conversion.
    These fixtures use original classic SPL tokens; no chain-fee model is used.
    """
    case=execution['cases'][1];require(case['success'] is True and 'err' not in case['native_result'],'successful-native-case-required-for-output-quantities')
    pool=manifest['pool'];before=case['before'];after=case['after']
    a=decode_token_account(before[pool['vault_b']]);b=decode_token_account(after[pool['vault_b']])
    gross=-token_delta(a,b,pool['mint_b'],a['owner'])
    recipients=[]
    for address,account in before.items():
        if len(account_bytes(account))!=165 or account['owner']!=TOKEN:continue
        parsed=decode_token_account(account)
        if parsed['mint']==pool['mint_b'] and parsed['owner']==execution['recipient']:recipients.append(address)
    require(len(recipients)==1,'native-recipient-account-identity')
    address=recipients[0];received=token_delta(decode_token_account(before[address]),decode_token_account(after[address]),pool['mint_b'],execution['recipient'])
    require(gross>=0 and received>=0,'native-output-direction')
    return {'pool_gross':gross,'recipient_token_delta':received,'after_chain_fees':None,'mint':pool['mint_b'],'unit':'token-base-units','recipient_token_account':address,'quantity_basis':{'pool_gross':'pool_output_vault_debit_after_pool_fees_before_transfer_or_chain_fees','recipient_token_delta':'actual_recipient_token_account_after_minus_before','after_chain_fees':'unavailable_no_chain_fee_conversion_model'}}


def replay_selected_quantities(manifest,selection):
    """Finite test ABI projection of a normal Router Plan's selected quantities.

    This does not supply or claim a production Router Solana transaction adapter.
    The existing test encoder creates the same program-specific swap instruction
    as the reference replay, with the selected amount and exact minimum output.
    """
    _,binding,_=validate_capture(manifest);pool=manifest['pool'];row=manifest['catalog_row']
    block=verify_rpc_record(load_reference(manifest['block']))['response']['result']
    expected={'pool':pool['pool'],'program':row['address'],'programdata':row['upgrade']['programdata'],'upgrade_slot':row['upgrade']['slot'],'full_payload_sha256':row['runtime']['sha256'],'sourcebank_slot':binding['observed_context_slot'],'blockhash':block['blockhash'],'mint_in':pool['mint_a'],'mint_out':pool['mint_b'],'zero_for_one':True}
    require(all(selection.get(k)==v for k,v in expected.items()),'selected-plan-fixed-deployment-or-bank-mismatch')
    _,Pubkey,Keypair,*_=native_imports()
    payer=str(Keypair.from_seed(hashlib.sha256(b'R01-public-native-bank-payer').digest()).pubkey());recipient=str(Pubkey.from_bytes(hashlib.sha256(b'R01-distinct-net-recipient').digest()))
    require(selection.get('caller')==payer and selection.get('recipient')==recipient,'selected-plan-actor-mismatch')
    require(selection.get('production_instruction_provided') is False,'test-ABI-projection-authority-required')
    replay=run_native(manifest,amount=selection['amount_in'],minimum_amount_out=selection['minimum_amount_out'])
    case=replay['cases'][1];encoded=base64.b64decode(case['instruction_data_base64'],validate=True)
    require(int.from_bytes(encoded[8:16],'little')==selection['amount_in'] and int.from_bytes(encoded[16:24],'little')==selection['minimum_amount_out'],'selected-quantity-ABI-encoding')
    success=replay['controlled_net_recipient_verified']
    return {'schema_version':1,'selection':selection,'instruction_authority':'existing finite test harness ABI encoder; no production Router transaction adapter','selection_origin_authority':'caller-supplied; process harness separately binds actual normal Router cached Plan evidence','production_router_instruction_execution_verified':False,'production_execution_adapter_claimed':False,'selected_quantities_native_execution_succeeded':success,'quantities':execution_quantities(manifest,replay) if success else None,'execution':replay,'quantity_basis':['pool_gross','recipient_token_delta','after_chain_fees_unavailable'],'historical_original_bank_verified':False}


def verify_saved_execution(manifest,evidence):
    replay=run_native(manifest)
    require(replay==evidence,'saved-native-execution-does-not-match-fresh-replay')
    require(replay['controlled_net_recipient_verified'] and not replay['historical_original_bank_verified'],'native-qualification-scope')
    return {'verified':True,'authority':'fresh-offline-native-execution','controlled_net_recipient_verified':True,'historical_original_bank_verified':False,'evidence_sha256':digest(json.dumps(evidence,sort_keys=True,separators=(',',':')).encode())}


def main():
    parser=argparse.ArgumentParser(description=__doc__);subs=parser.add_subparsers(dest='command',required=True)
    cap=subs.add_parser('capture');cap.add_argument('--protocol',required=True,choices=['raydium-cpmm','raydium-clmm','orca-whirlpools']);cap.add_argument('--pool',required=True);cap.add_argument('--output',type=Path,required=True);cap.add_argument('--endpoint',default='https://api.mainnet-beta.solana.com')
    run=subs.add_parser('run');run.add_argument('--sourcebank',type=Path,required=True);run.add_argument('--output',type=Path,required=True);run.add_argument('--amount',type=int,default=1000000)
    verify=subs.add_parser('verify');verify.add_argument('--sourcebank',type=Path,required=True);verify.add_argument('--evidence',type=Path,required=True)
    args=parser.parse_args()
    if args.command=='verify':print(json.dumps(verify_saved_execution(json.loads(args.sourcebank.read_text()),json.loads(args.evidence.read_text())),indent=2));return
    if args.command=='capture':result=capture(args.protocol,args.pool,args.output,args.endpoint);print(json.dumps({'sourcebank':str(args.output/'sourcebank.json'),'binding':result['binding']},indent=2))
    else:
        result=run_native(json.loads(args.sourcebank.read_text()),args.amount);args.output.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({k:result[k] for k in ['bank_kind','controlled_net_recipient_verified','historical_original_bank_verified','original_token_cpi_observed']},indent=2))
        if not result['controlled_net_recipient_verified']:raise SystemExit(1)

if __name__=='__main__':main()
