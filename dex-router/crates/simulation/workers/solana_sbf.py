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
PUMP='6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P'
PUMP_AMM='pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA'
FEE='pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ'
ORCA='whirLbMiicVdio4qvUfM5KAg6Ct8VwpYzGff3uctyCc'
COMPUTE_BUDGET='ComputeBudget111111111111111111111111111111'
RAYDIUM='CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C'
SYSTEM='11111111111111111111111111111111'
LOADER='BPFLoaderUpgradeab1e11111111111111111111111'
IMMUTABLE='BPFLoader2111111111111111111111111111111111'
CLOCK='SysvarC1ock11111111111111111111111111111111'
ORCA_TOKEN22='orca-whirlpools-adaptive-token2022'
ORCA_PROFILES=('orca-whirlpools-static','orca-whirlpools-rewards','orca-whirlpools-adaptive','orca-whirlpools-adaptive-rewards','orca-whirlpools-adaptive-dynamic',ORCA_TOKEN22)

def require(condition,code):
    if not condition:raise ValueError(code)

def raw(account):return base64.b64decode(account['data'][0],validate=True)

def verify_post_accounts(witnesses, account, code):
    actual = []
    for expected in witnesses:
        after = account(expected['address'])
        require(after is not None and not after.executable, code)
        witness = dict(address=expected['address'], owner=str(after.owner),
                       lamports=str(after.lamports), dataSha256=hashlib.sha256(after.data).hexdigest())
        require(witness == expected, code)
        actual.append(witness)
    return actual

def verify_reward_claim(request, preparation, before, account, native):
    position = preparation['position']
    vault = preparation['rewardVault']
    destination = preparation['rewardAccount']
    caller = request['caller']
    witnesses = preparation['expectedPostAccounts']
    update = preparation.get('rewardUpdate')
    addresses = [position, vault, destination]
    if update is not None:
        addresses.append(preparation['poolId'])
    require([w['address'] for w in witnesses] == addresses,
            'SBF_ORCA_REWARD_POST_STATE')
    actual = verify_post_accounts(witnesses, account, 'SBF_ORCA_REWARD_POST_STATE')

    def read_amount(data, offset):
        return int.from_bytes(data[offset:offset + 8], 'little')

    owed_offset = 160 + 24 * preparation['rewardIndex']
    remaining = read_amount(account(position).data, owed_offset)
    gross = read_amount(before[vault].data, 64) - read_amount(account(vault).data, 64)
    net = read_amount(account(destination).data, 64) - read_amount(before[destination].data, 64)
    accrued = (remaining + gross - read_amount(before[position].data, owed_offset)) % (1 << 64)
    require(accrued == (int(update['accruedReward']) if update is not None else 0)
            and gross == int(preparation['grossReward']) and net == int(preparation['netReward'])
            and gross - net == int(preparation['transferFee'])
            and remaining == int(preparation['remainingRewardOwed']),
            'SBF_ORCA_REWARD_BALANCE_MISMATCH')
    fee = native['meta']['fee']
    require(fee == int(preparation['feeLamports']) and before[caller].lamports - account(caller).lamports == fee,
            'SBF_FEE_RENT_BINDING')
    return dict(protocol=PROTOCOL, success=True, sourceBankUnchanged=True,
                grossReward=str(gross), netReward=str(net), remainingRewardOwed=str(remaining),
                transferFee=str(gross - net), feeLamports=str(fee),
                unitsConsumed=native['meta']['compute_units_consumed'], postAccounts=actual,
                independentProductionVerification=False, signed=False, submitted=False)

def verify_rayclmm_claim(request, preparation, before, account, native):
    transfers = preparation['fees'] + preparation['rewards']
    balances = {}
    for transfer in transfers:
        gross, net = int(transfer['gross']), int(transfer['net'])
        require(gross >= net >= 0 and gross - net == int(transfer['transferFee']),
                'SBF_RAYCLMM_POSITION_BALANCE')
        for address, delta in [(transfer['vault'], -gross), (transfer['account'], net)]:
            balances[address] = balances.get(address, 0) + delta
    for address, delta in balances.items():
        initial, final = before[address], account(address)
        require(initial is not None and final is not None and
                int.from_bytes(final.data[64:72], 'little') -
                int.from_bytes(initial.data[64:72], 'little') == delta,
                'SBF_RAYCLMM_POSITION_BALANCE')
    witnesses = preparation['expectedPostAccounts']
    require([w['address'] for w in witnesses] ==
            sorted(set(balances) | {preparation['position'], preparation['poolId']}),
            'SBF_RAYCLMM_POSITION_POST_STATE')
    actual = verify_post_accounts(witnesses, account, 'SBF_RAYCLMM_POSITION_POST_STATE')
    caller = request['caller']
    fee = native['meta']['fee']
    require(fee == int(preparation['feeLamports']) and
            before[caller].lamports - account(caller).lamports == fee, 'SBF_FEE_RENT_BINDING')
    return dict(protocol=PROTOCOL, success=True, sourceBankUnchanged=True,
                fees=preparation['fees'], rewards=preparation['rewards'], feeLamports=str(fee),
                postAccounts=actual, unitsConsumed=native['meta']['compute_units_consumed'],
                independentProductionVerification=False, signed=False, submitted=False)

def encode_adaptive_oracle(state,Pubkey):
    require(len(state['constants_reserved'])==16 and len(state['variables_reserved'])==16 and len(state['reserved'])==128,'SBF_ORCA_ORACLE_RESERVED_WIDTH')
    data=bytearray(254);data[:8]=hashlib.sha256(b'account:Oracle').digest()[:8];data[8:40]=bytes(Pubkey.from_string(state['whirlpool']))
    for offset,key in [(40,'trade_enable_timestamp'),(82,'last_reference_update_timestamp'),(90,'last_major_swap_timestamp')]:data[offset:offset+8]=int(state[key]).to_bytes(8,'little')
    for offset,key in [(48,'filter_period'),(50,'decay_period'),(52,'reduction_factor'),(62,'tick_group_size'),(64,'major_swap_threshold_ticks')]:data[offset:offset+2]=int(state[key]).to_bytes(2,'little')
    for offset,key in [(54,'adaptive_fee_control_factor'),(58,'max_volatility_accumulator'),(98,'volatility_reference'),(106,'volatility_accumulator')]:data[offset:offset+4]=int(state[key]).to_bytes(4,'little')
    data[102:106]=int(state['tick_group_index_reference']).to_bytes(4,'little',signed=True);data[66:82]=bytes(state['constants_reserved']);data[110:126]=bytes(state['variables_reserved']);data[126:254]=bytes(state['reserved']);return bytes(data)

def validate_orca(model,accounts,Pubkey,claim=False):
    state=model['orca_whirlpool'];pool=accounts[model['pool_id']];data=raw(pool)
    require(pool['owner']==ORCA and not pool['executable'] and len(data)==653,'SBF_ORCA_POOL_LAYOUT')
    require(model['protocol']in ORCA_PROFILES and ((state.get('adaptive')is None)==(model['protocol']in['orca-whirlpools-static','orca-whirlpools-rewards'])),'SBF_ORCA_PROFILE')
    expected=bytearray(653);expected[:8]=hashlib.sha256(b'account:Whirlpool').digest()[:8]
    def put(offset,value,size=16,signed=False):expected[offset:offset+size]=int(value).to_bytes(size,'little',signed=signed)
    def key(offset,value):expected[offset:offset+32]=bytes(Pubkey.from_string(value))
    key(8,state['whirlpools_config']);put(40,state['whirlpool_bump'],1)
    for offset,k in [(41,'tick_spacing'),(43,'fee_tier_index'),(45,'fee_rate'),(47,'protocol_fee_rate')]:put(offset,state[k],2)
    for offset,k in [(49,'liquidity'),(65,'sqrt_price_x64'),(165,'fee_growth_global0'),(245,'fee_growth_global1')]:put(offset,state[k])
    put(81,state['tick_current_index'],4,True)
    for offset,k in [(85,'protocol_fee_owed0'),(93,'protocol_fee_owed1'),(261,'reward_last_updated_timestamp')]:put(offset,state[k],8)
    for offset,value in [(101,model['token0']),(133,state['vault0']),(181,model['token1']),(213,state['vault1'])]:key(offset,value)
    rewards=state.get('reward_infos',[])
    if not rewards:expected[333:365]=data[333:365]
    for i,r in enumerate(rewards):
        off=269+i*128;key(off,r['mint']);key(off+32,r['vault']);expected[off+64:off+96]=bytes(r['extension']);put(off+96,r['emissions_per_second_x64']);put(off+112,r['growth_global_x64'])
    require(data==expected,'SBF_ORCA_POOL_MISMATCH')
    if claim:
        return
    oracle=accounts.get(state['oracle']);adaptive=state.get('adaptive')
    if adaptive is None:require(oracle is None or(oracle['owner']==SYSTEM and not oracle['executable']and raw(oracle)==b''),'SBF_ORCA_ORACLE_UNSUPPORTED')
    else:
        require(adaptive['whirlpool']==model['pool_id'],'SBF_ORCA_ORACLE_BINDING')
        require(oracle is not None and oracle['owner']==ORCA and not oracle['executable']and len(raw(oracle))==254 and raw(oracle)==encode_adaptive_oracle(adaptive,Pubkey),'SBF_ORCA_ORACLE_MISMATCH')
    token_state=state.get('token_2022')
    require((token_state is not None)==(model['protocol']==ORCA_TOKEN22),'SBF_ORCA_PROFILE')
    for side in [0,1]:
        a=accounts[state[f'vault{side}']]
        asset=(token_state or {}).get(f'token{side}')
        if asset is not None:
            mint=accounts[model[f'token{side}']]
            require(mint['owner']==TOKEN22 and not mint['executable'] and raw(mint).hex()==asset['mint_data'],'SBF_ORCA_MINT_MISMATCH')
            require(a['owner']==TOKEN22 and not a['executable'] and raw(a).hex()==asset['vault_data'],'SBF_ORCA_VAULT_MISMATCH')
        else:
            data=raw(a)
            require(a['owner']==TOKEN and not a['executable'] and len(data)==165 and int.from_bytes(data[64:72],'little')==state[f'raw_vault{side}'],'SBF_ORCA_VAULT_MISMATCH')
    for a in state['tick_arrays']:
        validate_orca_array(model, a, accounts, Pubkey)

def validate_orca_array(model, a, accounts, Pubkey):
    state = model['orca_whirlpool']
    pool = bytes(Pubkey.from_string(model['pool_id']))
    start = a['start_tick_index']
    if a['layout'] == 'sparse':
        address, _ = Pubkey.find_program_address([b'tick_array', pool, str(start).encode()], Pubkey.from_string(ORCA))
        observed = accounts.get(str(address))
        require(
            model['protocol'] in ('orca-whirlpools-adaptive-dynamic', ORCA_TOKEN22)
            and a['address'] == str(address) and int(a['initialized_bitmap']) == 0 and not a['ticks']
            and observed is not None and observed['owner'] == SYSTEM
            and not observed['executable'] and raw(observed) == b'',
            'SBF_ORCA_SPARSE_ARRAY_MISMATCH',
        )
        return
    dynamic = a['layout'] == 'dynamic'
    discriminator = b'account:DynamicTickArray' if dynamic else b'account:TickArray'
    data = bytearray(hashlib.sha256(discriminator).digest()[:8] + start.to_bytes(4, 'little', signed=True))
    if dynamic:
        data += pool + int(a['initialized_bitmap']).to_bytes(16, 'little')
    ticks = {tick['tick']: tick for tick in a['ticks']}
    for i in range(88):
        tick = ticks.get(start + i * state['tick_spacing'])
        entry = bytearray(1 if dynamic and tick is None else 113)
        if tick is not None:
            entry[0] = 1
            entry[1:17] = int(tick['liquidity_net']).to_bytes(16, 'little', signed=True)
            for offset, field in [(17, 'liquidity_gross'), (33, 'fee_growth_outside0'), (49, 'fee_growth_outside1')]:
                entry[offset:offset + 16] = int(tick[field]).to_bytes(16, 'little')
            for index, value in enumerate(tick.get('reward_growths_outside', [])):
                offset = 65 + index * 16
                entry[offset:offset + 16] = int(value).to_bytes(16, 'little')
        data += entry
    if not dynamic:
        data += pool
    observed = accounts[a['address']]
    require(observed['owner'] == ORCA and not observed['executable'] and raw(observed) == data,
            'SBF_ORCA_ARRAY_MISMATCH')

# Original classic-token CLMM layout validation.
RAYCLMM='CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK'
MEMO='MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr'
RAY_TOKEN22_LEGACY_CODE='495e9d7680dd555cb126a6a8e5464af5be9b01f02f2cd70634352722d22e3cad'
RAY_TOKEN22_DEPLOYED_CODE='15a2e615a42e20f31714bbaaee43002ae6f1cda2e8d7118243117ab3c1342e2d'
RAY_TOKEN22_DEPLOYED_SLOT=460497271
MEMO_CODE='f520eaf096361abbb9639ea4dc3e5388a87b9330e121f476607b87c46ef67954'
RAYCLMM_ORDERS='raydium-clmm-limit-orders'
RAYCLMM_ADVANCED='raydium-clmm-advanced'
RAYCLMM_DYNAMIC_PROFILES=('raydium-clmm-dynamic',RAYCLMM_ADVANCED)
RAYCLMM_BITMAP_PROFILES=('raydium-clmm-bitmap','raydium-clmm-custom-seed',*RAYCLMM_DYNAMIC_PROFILES)
RAYCLMM_PROFILES=('raydium-clmm-static','raydium-clmm-rewards','raydium-clmm-token-fee',RAYCLMM_ORDERS,*RAYCLMM_BITMAP_PROFILES)
def validate_rayclmm(model,accounts,Pubkey):
    profile = model['protocol']
    require(profile in RAYCLMM_PROFILES, 'SBF_RAYCLMM_PROFILE')
    s = model['raydium_clmm']
    dynamic_profile = profile in RAYCLMM_DYNAMIC_PROFILES
    order_profile = profile in (RAYCLMM_ORDERS, RAYCLMM_ADVANCED)
    require((s.get('dynamic_fee') is not None) == dynamic_profile and
            (s.get('bitmap_extension') is not None) == (profile in RAYCLMM_BITMAP_PROFILES) and
            (dynamic_profile or ((s.get('seed_index',0)>0)==(profile=='raydium-clmm-custom-seed'))),
            'SBF_RAYCLMM_PROFILE')
    a = accounts[model['pool_id']]
    data = raw(a)
    require(a['owner']==RAYCLMM and not a['executable'] and len(data)==1544, 'SBF_RAYCLMM_POOL_LAYOUT')
    b = bytearray(data)
    require(data[273:277]==bytes(4)and data[389]&~63==0 and data[389]&16==0 and data[393:397]==bytes(4)and not any(data[1130 if s.get('dynamic_fee')is not None else 1096:]),'SBF_RAYCLMM_VERSION')
    def put(offset,value,size=16,signed=False):b[offset:offset+size]=int(value).to_bytes(size,'little',signed=signed)
    def key(offset,value):b[offset:offset+32]=bytes(Pubkey.from_string(value))
    b[:8]=hashlib.sha256(b'account:PoolState').digest()[:8];put(8,s['pool_bump'],1)
    for offset,value in [(9,s['amm_config']),(73,model['token0']),(105,model['token1']),(137,s['vault0']),(169,s['vault1']),(201,s['observation'])]:key(offset,value)
    put(233,s['mint0_decimals'],1);put(234,s['mint1_decimals'],1);put(235,s['tick_spacing'],2)
    for offset,name in [(237,'liquidity'),(253,'sqrt_price_x64'),(277,'fee_growth_global0'),(293,'fee_growth_global1')]:put(offset,s[name])
    put(269,s['tick_current'],4,True)
    for offset,name in [(309,'protocol_fees0'),(317,'protocol_fees1'),(1064,'fund_fees0'),(1072,'fund_fees1'),(1080,'open_time'),(1088,'recent_epoch')]:put(offset,s[name],8)
    put(390,s.get('collect_fee_on',0),1);put(391,s.get('seed_index',0),2)
    rewards=s.get('reward_infos',[]);require((profile in ('raydium-clmm-rewards',RAYCLMM_ADVANCED))==(len(rewards)==3),'SBF_RAYCLMM_REWARDS')
    for i in range(3):
        off=397+i*169
        if rewards:
            r=rewards[i];put(off,r['reward_state'],1)
            for delta,name in [(1,'open_time'),(9,'end_time'),(17,'last_update_time'),(41,'reward_total_emitted'),(49,'reward_claimed')]:put(off+delta,r[name],8)
            put(off+25,r['emissions_per_second_x64']);put(off+153,r['reward_growth_global_x64'])
            for delta,name in [(57,'token_mint'),(89,'token_vault'),(121,'authority')]:key(off+delta,r[name])
        else:b[off:off+121]=bytes(121);b[off+153:off+169]=bytes(16)
    dynamic=s.get('dynamic_fee')
    if dynamic is not None:
        for offset,name in [(1096,'filter_period'),(1098,'decay_period'),(1100,'reduction_factor')]:put(offset,dynamic[name],2)
        for offset,name in [(1102,'dynamic_fee_control'),(1106,'max_volatility_accumulator'),(1114,'volatility_reference'),(1118,'volatility_accumulator')]:put(offset,dynamic[name],4)
        put(1110,dynamic['tick_spacing_index_reference'],4,True);put(1122,dynamic['last_update_timestamp'],8)
    require(len(s['bitmap'])==16,'SBF_RAYCLMM_BITMAP')
    for i,value in enumerate(s['bitmap']):put(904+8*i,value,8)
    require(data==b,'SBF_RAYCLMM_POOL_MISMATCH');cfg=accounts[s['amm_config']];c=raw(cfg);require(cfg['owner']==RAYCLMM and not cfg['executable']and len(c)==117 and c[:8]==hashlib.sha256(b'account:AmmConfig').digest()[:8]and c[57:61]==bytes(4)and not any(c[93:]),'SBF_RAYCLMM_CONFIG_LAYOUT')
    for offset,name in [(43,'protocol_fee_rate'),(47,'trade_fee_rate'),(53,'fund_fee_rate')]:require(int.from_bytes(c[offset:offset+4],'little')==s[name],'SBF_RAYCLMM_CONFIG_MISMATCH')
    require(int.from_bytes(c[51:53],'little')==s['tick_spacing'],'SBF_RAYCLMM_CONFIG_MISMATCH')
    seeds=[b'pool',bytes(Pubkey.from_string(s['amm_config'])),bytes(Pubkey.from_string(model['token0'])),bytes(Pubkey.from_string(model['token1']))]
    if s.get('seed_index',0)>0:seeds.append(s['seed_index'].to_bytes(2,'little'))
    address,bump=Pubkey.find_program_address(seeds,Pubkey.from_string(RAYCLMM));require(str(address)==model['pool_id']and bump==s['pool_bump'],'SBF_RAYCLMM_POOL_PDA')
    extension=s.get('bitmap_extension')
    if extension is not None:
        address=str(Pubkey.find_program_address([b'pool_tick_array_bitmap_extension',bytes(Pubkey.from_string(model['pool_id']))],Pubkey.from_string(RAYCLMM))[0]);require(address==extension['address']and len(extension['positive'])==112 and len(extension['negative'])==112,'SBF_RAYCLMM_EXTENSION_IDENTITY')
        a=accounts.get(address);expected=hashlib.sha256(b'account:TickArrayBitmapExtension').digest()[:8]+bytes(Pubkey.from_string(model['pool_id']))+b''.join(int(word).to_bytes(8,'little')for word in extension['positive']+extension['negative']);require(a is not None and a['owner']==RAYCLMM and not a['executable']and raw(a)==expected,'SBF_RAYCLMM_EXTENSION_MISMATCH')
    a=accounts[s['observation']];d=raw(a);require(a['owner']==RAYCLMM and not a['executable']and len(d)==4483,'SBF_RAYCLMM_OBSERVATION_LAYOUT');o=bytearray(4483);o[:8]=hashlib.sha256(b'account:ObservationState').digest()[:8];o[8]=s['observation_initialized'];o[9:17]=s['observation_recent_epoch'].to_bytes(8,'little');o[17:19]=s['observation_index'].to_bytes(2,'little');o[19:51]=bytes(Pubkey.from_string(model['pool_id']));require(len(s['observations'])==100,'SBF_RAYCLMM_OBSERVATION_COUNT')
    for i,entry in enumerate(s['observations']):off=51+44*i;o[off:off+4]=entry['block_timestamp'].to_bytes(4,'little');o[off+4:off+12]=entry['tick_cumulative'].to_bytes(8,'little',signed=True)
    require(d==o,'SBF_RAYCLMM_OBSERVATION_MISMATCH')
    for a in s['tick_arrays']:
        account=accounts[a['address']];d=raw(account);require(account['owner']==RAYCLMM and not account['executable']and len(d)==10240 and not any(d[10133:]),'SBF_RAYCLMM_ARRAY_LAYOUT');expected=bytearray(d);expected[:8]=hashlib.sha256(b'account:TickArrayState').digest()[:8];expected[8:40]=bytes(Pubkey.from_string(model['pool_id']));expected[40:44]=a['start_tick_index'].to_bytes(4,'little',signed=True);ticks={t['tick']:t for t in a['ticks']}
        initialized=0
        for i in range(60):
            off=44+i*168;index=a['start_tick_index']+i*s['tick_spacing'];t=ticks.get(index)
            order_start=116 if order_profile else 124
            expected[off+order_start:off+168]=bytes(168-order_start)
            if t is None:require(int.from_bytes(d[off:off+4],'little',signed=True)in[0,index],'SBF_RAYCLMM_ARRAY_LAYOUT');expected[off+4:off+116]=bytes(112)
            else:
                expected[off:off+4]=t['tick'].to_bytes(4,'little',signed=True);expected[off+4:off+20]=int(t['liquidity_net']).to_bytes(16,'little',signed=True)
                for delta,name in [(20,'liquidity_gross'),(36,'fee_growth_outside0'),(52,'fee_growth_outside1')]:expected[off+delta:off+delta+16]=int(t[name]).to_bytes(16,'little')
                expected[off+68:off+116]=bytes(48)
                for j,value in enumerate(t.get('reward_growths_outside',[])):expected[off+68+16*j:off+84+16*j]=int(value).to_bytes(16,'little')
                orders=t.get('limit_orders')
                if orders is not None:
                    require(order_profile,'SBF_RAYCLMM_ORDERS')
                    for delta,name in [(116,'order_phase'),(124,'orders_amount'),(132,'part_filled_orders_remaining')]:
                        expected[off+delta:off+delta+8]=int(orders[name]).to_bytes(8,'little')
                    expected[off+140:off+156]=int(orders['unfilled_ratio_x64']).to_bytes(16,'little')
                initialized+=bool(int(t['liquidity_gross']) or orders and (orders['orders_amount'] or orders['part_filled_orders_remaining']))
        expected[10124]=initialized;expected[10125:10133]=a['recent_epoch'].to_bytes(8,'little');require(d==expected,'SBF_RAYCLMM_ARRAY_MISMATCH')
    for side in [0,1]:
        mint=model[f'token{side}'];vault=s[f'vault{side}'];a=accounts[vault];d=raw(a);require(a['owner']==TOKEN and not a['executable']and len(d)==165 and d[:32]==bytes(Pubkey.from_string(mint))and d[32:64]==bytes(Pubkey.from_string(model['pool_id']))and d[108]==1 and int.from_bytes(d[64:72],'little')==s[f'raw_vault{side}'],'SBF_RAYCLMM_VAULT')
        if mint=='So11111111111111111111111111111111111111112':require(d[109:113]==b'\1\0\0\0'and a['lamports']>=int.from_bytes(d[113:121],'little')+s[f'raw_vault{side}'],'SBF_RAYCLMM_VAULT_BACKING')

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
    claim=preparation.get('operation')=='ORCA_COLLECT_REWARD'
    reward_update = preparation.get('rewardUpdate') if claim else None
    rayclmm_claim=preparation.get('operation')=='RAYCLMM_COLLECT_POSITION'
    reward_program=preparation.get('rewardTokenProgram') if claim else None
    wire=base64.b64decode(preparation['unsignedTransaction'],validate=True)
    require(len(wire)<=1232 and wire[:1]==b'\x01' and wire[1:65]==bytes(64),'SBF_UNSIGNED_REQUIRED')
    signed=request.get('signedTransaction')
    if signed is not None:
        signed_wire=base64.b64decode(signed,validate=True)
        require(len(signed_wire)==len(wire) and signed_wire[:1]==b'\x01'
                and signed_wire[65:]==wire[65:],'SBF_SIGNED_MESSAGE_MISMATCH')
        wire=signed_wire
    tx=Transaction.from_bytes(wire)
    tx.sanitize()
    header = tx.message.header
    keys = list(map(str, tx.message.account_keys))
    require(header.num_required_signatures == 1 and header.num_readonly_signed_accounts == 0
            and keys[0] == request['caller'], 'SBF_CALLER_SIGNER')
    require(len(keys) == len(set(keys)), 'SBF_ACCOUNT_LOADED_TWICE')
    writable_end = len(keys) - header.num_readonly_unsigned_accounts
    writable_keys = set(keys[:writable_end])
    readonly_keys = set(keys[writable_end:])
    account_locks = dict(writable=keys[:writable_end], readonly=keys[writable_end:])
    require(bytes(tx.message)==base64.b64decode(preparation['messageBase64'],validate=True),'SBF_MESSAGE_BINDING')
    if signed is not None:
        require(str(tx.message.recent_blockhash)==preparation['recentBlockhash'],'SBF_BLOCKHASH_MISMATCH')
    budget=ComputeBudget(True);budget.compute_unit_limit=limit
    svm=LiteSVM().with_mainnet_features().with_compute_budget(budget).with_blockhash_check(False).with_sigverify(signed is not None)
    # Local and preparation-RPC banks both run under explicitly pinned VM
    # features; neither a slot label nor latest account RPC proves consensus.
    # The captured blockhash is used as message bytes only. Live signed RPC
    # preflight still checks its actual network validity before submission.
    programs={str(tx.message.account_keys[ix.program_id_index])for ix in tx.message.instructions}
    target=RAYDIUM if bank['pool']['protocol']=='raydium-cpmm-creator' else METEORA
    if bank['pool']['protocol'].startswith('pump-amm-classic-wsol'):target=PUMP_AMM
    if bank['pool']['protocol']=='pump-bonding-sol-classic':target=PUMP
    if bank['pool']['protocol']in ORCA_PROFILES:target=ORCA
    if bank['pool']['protocol']in RAYCLMM_PROFILES:target=RAYCLMM
    require(not claim or (target==ORCA and bank.get('rpcCapture') and reward_program in (TOKEN,TOKEN22)),'SBF_ORCA_REWARD_CONTEXT')
    if claim:
        require(programs == {ORCA, COMPUTE_BUDGET} and
                len(tx.message.instructions) == (3 if reward_update is not None else 2),
                'SBF_ORCA_REWARD_INSTRUCTION')
    if rayclmm_claim:
        require(target==RAYCLMM and bank.get('rpcCapture')
                and programs=={RAYCLMM,COMPUTE_BUDGET} and len(tx.message.instructions)==2,
                'SBF_RAYCLMM_POSITION_INSTRUCTION')
    require(target in programs and programs<=({SYSTEM,TOKEN,TOKEN22,target,'ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL'}|({COMPUTE_BUDGET}if target in[ORCA,RAYCLMM] else set())|({MEMO}if target==RAYCLMM else set())),'SBF_PROGRAM_ALLOWLIST')
    if target in[ORCA,RAYCLMM]:
        limits=[ix for ix in tx.message.instructions if str(tx.message.account_keys[ix.program_id_index])==COMPUTE_BUDGET]
        require(len(limits)==1 and len(limits[0].accounts)==0 and bytes(limits[0].data)==b'\x02'+(1400000).to_bytes(4,'little')and preparation.get('computeUnitLimit')==1400000 and limit>=1400000,'SBF_ORCA_COMPUTE_BUDGET')
    model=bank['pool'];state=model.get('raydium_clmm')if target==RAYCLMM else model.get('orca_whirlpool')if target==ORCA else model.get('raydium_cpmm') if target==RAYDIUM else model.get('pump')if target in [PUMP_AMM,PUMP] else model.get('meteora')
    require(state is not None,'SBF_POOL_MODEL')
    def account_bytes(key):return raw(bank['accounts'][key])
    clock_data=account_bytes(CLOCK)
    expected_slot=state['bank_slot'] if target in[RAYDIUM,ORCA,RAYCLMM] else state['slot']
    expected_time=state['bank_unix_timestamp'] if target in[RAYDIUM,ORCA,RAYCLMM] else state['timestamp']
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
    elif target==RAYCLMM:
        validate_rayclmm(model,bank['accounts'],Pubkey)
        require(int.from_bytes(clock_data[16:24],'little')==state['bank_epoch'],'SBF_RAYCLMM_EPOCH')
        if rayclmm_claim:
            pool = model['pool_id']
            fees = preparation['fees']
            rewards = [r for r in state['reward_infos'] if r['token_mint'] != SYSTEM]
            reward_transfers = preparation['rewards']
            require(preparation['poolId']==pool and len(fees)==2 and len(rewards)==len(reward_transfers),
                    'SBF_RAYCLMM_POSITION_CONTEXT')
            metas = [request['caller'], preparation['positionTokenAccount'],
                     preparation['position'], pool, pool, state['vault0'], state['vault1'],
                     preparation['lowerTickArray'], preparation['upperTickArray'],
                     fees[0]['account'], fees[1]['account'], TOKEN, TOKEN22, MEMO,
                     model['token0'], model['token1']]
            span = 60 * state['tick_spacing']
            arrays = {a['address']: a['start_tick_index'] for a in state['tick_arrays']}
            position_data = raw(bank['accounts'][preparation['position']])
            extension_required = False
            for offset, address in zip((73, 77), metas[7:9]):
                tick = int.from_bytes(position_data[offset:offset + 4], 'little', signed=True)
                index = tick // span
                require(arrays.get(address) == index * span, 'SBF_RAYCLMM_POSITION_TICKS')
                extension_required |= not -512 <= index <= 511
            if extension_required:
                extension = state.get('bitmap_extension')
                require(extension is not None, 'SBF_RAYCLMM_POSITION_BITMAP')
                metas.append(extension['address'])
            for reward, transfer in zip(rewards, reward_transfers):
                require(reward['token_vault']==transfer['vault'], 'SBF_RAYCLMM_POSITION_CONTEXT')
                metas.extend([reward['token_vault'], transfer['account'], reward['token_mint']])
            ix = next(ix for ix in tx.message.instructions
                      if str(tx.message.account_keys[ix.program_id_index])==RAYCLMM)
            data = hashlib.sha256(b'global:decrease_liquidity_v2').digest()[:8] + bytes(32)
            require(bytes(ix.data)==data and [str(tx.message.account_keys[i]) for i in ix.accounts]==metas,
                    'SBF_RAYCLMM_POSITION_INSTRUCTION')
    elif target==ORCA:
        validate_orca(model,bank['accounts'],Pubkey,claim)
        if claim:
            index=preparation['rewardIndex']
            require(isinstance(index,int) and 0<=index<len(state['reward_infos']),'SBF_ORCA_REWARD_INDEX')
            reward=state['reward_infos'][index]
            require(reward['mint']==preparation['rewardMint'] and reward['vault']==preparation['rewardVault'],'SBF_ORCA_REWARD_BINDING')
            ix=[i for i in tx.message.instructions if str(tx.message.account_keys[i.program_id_index])==ORCA]
            v2=reward_program==TOKEN22
            name=b'global:collect_reward_v2' if v2 else b'global:collect_reward'
            data=hashlib.sha256(name).digest()[:8]+bytes([index])+(bytes(1) if v2 else b'')
            metas=[model['pool_id'],request['caller'],preparation['position'],
                   preparation['positionTokenAccount'],preparation['rewardAccount']]
            if v2:metas.append(reward['mint'])
            metas.extend([reward['vault'],reward_program])
            if v2:metas.append(MEMO)
            require(len(ix) == (2 if reward_update is not None else 1) and bytes(ix[-1].data)==data and
                    [str(tx.message.account_keys[i]) for i in ix[-1].accounts]==metas,
                    'SBF_ORCA_REWARD_INSTRUCTION')
            if reward_update is not None:
                pool = model['pool_id']
                position = preparation['position']
                bounds = [reward_update['lowerTickArray'], reward_update['upperTickArray']]
                update_data = hashlib.sha256(b'global:update_fees_and_rewards').digest()[:8]
                require(bytes(ix[0].data) == update_data and
                        [str(tx.message.account_keys[i]) for i in ix[0].accounts] == [pool, position, *bounds],
                        'SBF_ORCA_REWARD_UPDATE_INSTRUCTION')
                position_data = raw(bank['accounts'][position])
                span = 88 * state['tick_spacing']
                arrays = {a['address']: a for a in state['tick_arrays'] if a['layout'] != 'sparse'}
                for address, offset in zip(bounds, (88, 92)):
                    tick = int.from_bytes(position_data[offset:offset + 4], 'little', signed=True)
                    start = tick // span * span
                    pda, _ = Pubkey.find_program_address(
                        [b'tick_array', bytes(Pubkey.from_string(pool)), str(start).encode()], Pubkey.from_string(ORCA))
                    require(address == str(pda) and address in arrays and arrays[address]['start_tick_index'] == start,
                            'SBF_ORCA_REWARD_ARRAY_BINDING')
                    validate_orca_array(model, arrays[address], bank['accounts'], Pubkey)
        else:
            oracle = state['oracle']
            expected_locks = writable_keys if state.get('adaptive') is not None else readonly_keys
            require(oracle in expected_locks, 'SBF_ORCA_ORACLE_PRIVILEGE')
        require(int.from_bytes(clock_data[16:24],'little')==state['bank_epoch'],'SBF_ORCA_EPOCH_MISMATCH')
    elif target in [PUMP_AMM,PUMP]:
        require(state['user']==request['caller']==request['recipient'],'SBF_PUMP_USER_MISMATCH')
        for item in state['accounts']:
            require(account_bytes(item['address']).hex()==item['data'] and bank['accounts'][item['address']]['owner']==item['owner'] and bank['accounts'][item['address']]['lamports']==item['lamports'],'SBF_BANK_POOL_MISMATCH')
    else:
        pair=account_bytes(model['pool_id']);require(len(pair)==637 and pair[329]&4==0,'SBF_BANK_POOL_MISMATCH')
        for offset,key in [(341,'protocol_fees0'),(349,'protocol_fees1'),(357,'fund_fees0'),(365,'fund_fees1'),(373,'open_time'),(381,'recent_epoch'),(397,'creator_fees0'),(405,'creator_fees1')]:require(int.from_bytes(pair[offset:offset+8],'little')==state[key],'SBF_BANK_LEDGER_MISMATCH')
        require(pair[389]==state['creator_fee_on']and bool(pair[390])==state['creator_fee_enabled'],'SBF_BANK_CREATOR_MISMATCH')
        cfg=account_bytes(state['amm_config'])
        for offset,value in [(12,model['fee_numerator']),(20,state['protocol_fee_rate']),(28,state['fund_fee_rate']),(108,state['creator_fee_rate']),(116,state['creator_fee_share_rate'])]:require(int.from_bytes(cfg[offset:offset+8],'little')==value,'SBF_BANK_CONFIG_MISMATCH')
        for side in [0,1]:require(int.from_bytes(account_bytes(state[f'vault{side}'])[64:72],'little')==state[f'raw_vault{side}'],'SBF_BANK_VAULT_MISMATCH')
        require(int.from_bytes(clock_data[16:24],'little')==state['bank_epoch'],'SBF_BANK_EPOCH_MISMATCH')
    dependencies={target,TOKEN}
    orca_token22=model['protocol']==ORCA_TOKEN22 and not claim
    if target==RAYCLMM or orca_token22:dependencies.update([TOKEN22,MEMO])
    if target in [PUMP_AMM,PUMP]:dependencies.add(FEE)
    token22_state=state.get('token_2022') if orca_token22 else state.get('token2022')
    if token22_state is not None:dependencies.add(TOKEN22)
    if claim:
        dependencies={ORCA,reward_program}
        if reward_program==TOKEN22:dependencies.add(MEMO)
    for pid in sorted(dependencies):
        a=bank['accounts'][pid];data=raw(a)
        if pid==MEMO:
            require(a['executable']is True and a['owner']==IMMUTABLE and hashlib.sha256(data).hexdigest()==MEMO_CODE,'SBF_RAYCLMM_MEMO_CODE' if target==RAYCLMM else 'SBF_ORCA_MEMO_CODE')
            svm.add_program_with_loader(Pubkey.from_string(pid),data,Pubkey.from_string(IMMUTABLE))
            continue
        require(a['executable']is True and a['owner']==LOADER and len(data)==36 and data[:4]==b'\x02\0\0\0','SBF_PROGRAM_LAYOUT')
        pd=str(Pubkey.from_bytes(data[4:36]));programdata=bank['accounts'][pd]
        require(programdata['owner']==LOADER and programdata['executable']is False and raw(programdata)[:4]==b'\x03\0\0\0','SBF_PROGRAMDATA_LAYOUT')
        programdata_bytes=raw(programdata)
        deployment=int.from_bytes(programdata_bytes[4:12],'little')
        require(deployment<=expected_slot,'SBF_BANK_DEPLOYMENT_MISMATCH')
        if pid==target:require(deployment==state['deployed_slot'],'SBF_BANK_DEPLOYMENT_MISMATCH')
        if pid==FEE:require(deployment==state['fee_deployed_slot'],'SBF_BANK_DEPLOYMENT_MISMATCH')
        if pid==TOKEN22 and target!=RAYCLMM:
            if claim:
                require(deployment==preparation['rewardTokenDeployedSlot'],'SBF_BANK_DEPLOYMENT_MISMATCH')
            else:
                require(pd==token22_state['programdata'] and deployment==token22_state['deployed_slot'],'SBF_BANK_DEPLOYMENT_MISMATCH')
        payload=raw(programdata)[45:]
        if claim and pid==reward_program:
            expected=preparation['rewardTokenCodeSha256']
        elif target==RAYCLMM and pid==TOKEN22:
            expected=RAY_TOKEN22_DEPLOYED_CODE if deployment==RAY_TOKEN22_DEPLOYED_SLOT else RAY_TOKEN22_LEGACY_CODE
        else:
            expected=state['program_code_sha256'] if pid==target else state['fee_program_code_sha256']if pid==FEE else token22_state['program_code_sha256'] if pid==TOKEN22 else state['token_program_code_sha256']
        require(hashlib.sha256(payload).hexdigest()==expected,'SBF_BANK_PROGRAM_MISMATCH')
        svm.add_program_with_loader(Pubkey.from_string(pid),payload,Pubkey.from_string(LOADER))
    if bank.get('rpcCapture'):
        accounts=bank['accounts'];absent=set(bank['absentAccounts'])
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
    def captured_account(a):
        return Account(a['lamports'], raw(a), Pubkey.from_string(a['owner']),
                       a['executable'], a.get('rentEpoch', 0))
    for key,a in bank['accounts'].items():
        svm.set_account(Pubkey.from_string(key), captured_account(a))
    clock=raw(bank['accounts'][CLOCK]);require(len(clock)==40,'SBF_CLOCK_LAYOUT')
    svm.set_clock(Clock(slot=int.from_bytes(clock[:8],'little'),epoch_start_timestamp=int.from_bytes(clock[8:16],'little',signed=True),epoch=int.from_bytes(clock[16:24],'little'),leader_schedule_epoch=int.from_bytes(clock[24:32],'little'),unix_timestamp=int.from_bytes(clock[32:40],'little',signed=True)))
    svm.set_account(Pubkey.from_string(CLOCK), captured_account(bank['accounts'][CLOCK]))
    before={key:svm.get_account(Pubkey.from_string(key))for key in keys}
    result=svm.simulate_transaction(tx);native=json.loads(result.to_json())
    for key,a in before.items():
        current=svm.get_account(Pubkey.from_string(key))
        require((bytes(a)if a else None)==(bytes(current)if current else None),'SBF_BANK_MUTATED')
    if native.get('err')is not None:
        return dict(protocol=PROTOCOL,success=False,error='SBF_TRANSACTION_FAILED',nativeError=native['err'],unitsConsumed=native['meta']['compute_units_consumed'],sourceBankUnchanged=True)
    post={str(key):value for key,value in result.post_accounts()}
    require(post.keys() <= before.keys(), 'SBF_POST_ACCOUNT_NOT_IN_MESSAGE')
    for key in readonly_keys & post.keys():
        require(before[key] is not None and bytes(post[key]) == bytes(before[key]),
                'SBF_READONLY_ACCOUNT_CHANGED')
    def account(key):
        a=post.get(key)if key in post else svm.get_account(Pubkey.from_string(key))
        return None if a is None or (a.lamports==0 and len(a.data)==0)else a
    if claim:
        evidence = verify_reward_claim(request,preparation,before,account,native)
        evidence['messageAccountLocks'] = account_locks
        return evidence
    if rayclmm_claim:
        evidence = verify_rayclmm_claim(request,preparation,before,account,native)
        evidence['messageAccountLocks'] = account_locks
        return evidence
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
    def funding(before,after,mint,owner,program,creating):
        pending=before is None or (creating and str(before.owner)==SYSTEM and not before.executable and len(before.data)==0)
        if not pending:return amount(before,mint,owner,program),0
        require(creating,'SBF_ACCOUNT_CREATION_REQUIRED')
        amount(after,mint,owner,program)
        reserve=after.lamports
        if mint=='So11111111111111111111111111111111111111112':
            require(program==TOKEN and after.data[109:113]==b'\x01\x00\x00\x00','SBF_NATIVE_RESERVE')
            reserve=int.from_bytes(after.data[113:121],'little')
            require(reserve==svm.minimum_balance_for_rent_exemption(165),'SBF_NATIVE_RENT')
        prefund=before.lamports if before else 0
        return max(0,prefund-reserve)if mint=='So11111111111111111111111111111111111111112'else 0,max(0,reserve-prefund)
    source_native=request['sellToken']=='native:solana:sol';destination_native=request['buyToken']=='native:solana:sol'
    require((preparation.get('sourceNativeSol')is True)==source_native and (preparation.get('destinationNativeSol')is True)==destination_native,'SBF_NATIVE_ASSET_MISMATCH')
    if source_native:require(source==caller and source_program==SYSTEM,'SBF_NATIVE_SOURCE')
    if destination_native:require(destination==caller==request['recipient']and dest_program==SYSTEM,'SBF_NATIVE_DESTINATION')
    if source_native or preparation['refundWrappedSource']:source_initial,source_rent=0,0
    else:source_initial,source_rent=funding(source_before,account(source),request['sellToken'],caller,source_program,preparation['sourceAccountCreation'])
    if destination_native or preparation['unwrapDestinationSol']:dest_initial,dest_rent=0,0
    else:dest_initial,dest_rent=funding(destination_before,account(destination),request['buyToken'],request['recipient'],dest_program,preparation['accountCreation'])
    require(source_rent+dest_rent==persistent,'SBF_RENT_CHANGED')
    if source_native:
        paid=native_before-native_after.lamports-fee-persistent
    elif preparation['refundWrappedSource']:
        require(source_before is None and account(source)is None,'SBF_TEMP_NOT_CLOSED')
        paid=native_before-native_after.lamports-fee-persistent
    else:
        initial=source_initial
        paid=initial+int(preparation['wrapLamports'])-amount(account(source),request['sellToken'],caller,source_program)
    if destination_native:
        received=native_after.lamports-native_before+fee+persistent
    elif preparation['unwrapDestinationSol']:
        require(destination_before is None and account(destination)is None,'SBF_TEMP_NOT_CLOSED')
        received=native_after.lamports-native_before+fee+persistent
    else:
        initial=dest_initial
        received=amount(account(destination),request['buyToken'],request['recipient'],dest_program)-initial
    require(paid==int(preparation['quotedInput'])and received==int(preparation['quotedOutput']),'SBF_AMOUNT_MISMATCH')
    require(native_after.lamports-native_before==int(preparation['expectedPayerLamportDelta']),'SBF_NATIVE_BALANCE_MISMATCH')
    def withheld(a):
        if a is None or (str(a.owner)==SYSTEM and not a.executable and not a.data):return 0
        data=a.data;require(len(data)in [178,182]and data[165]==2,'SBF_TOKEN2022_LAYOUT');cursor=166;found=None;seen=set()
        while cursor<len(data):
            kind=int.from_bytes(data[cursor:cursor+2],'little');size=int.from_bytes(data[cursor+2:cursor+4],'little');cursor+=4
            require(kind not in seen and (kind,size)in [(2,8),(7,0)] and cursor+size<=len(data),'SBF_TOKEN2022_EXTENSION');seen.add(kind)
            if kind==2:found=int.from_bytes(data[cursor:cursor+8],'little')
            cursor+=size
        require(found is not None,'SBF_TOKEN2022_FEE_REQUIRED');return found
    if source_program==TOKEN22:require(withheld(account(source))==withheld(source_before),'SBF_SOURCE_WITHHELD_MISMATCH')
    if dest_program==TOKEN22:require(withheld(account(destination))-withheld(destination_before)==int(preparation['expectedDestinationTransferFee']),'SBF_DESTINATION_WITHHELD_MISMATCH')
    if orca_token22:
        vaults=preparation['expectedOrcaVaults']
        require(len(vaults)==2,'SBF_ORCA_VAULT_POST_STATE')
        for side, expected in enumerate(vaults):
            key=state[f'vault{side}']
            expected_bytes=expected['token2022Data']
            asset=token22_state.get(f'token{side}')
            require((expected_bytes is not None)==(asset is not None),'SBF_ORCA_VAULT_POST_STATE')
            if expected_bytes is None:
                data=bytearray(before[key].data)
                data[64:72]=int(expected['amount']).to_bytes(8,'little')
                expected_bytes=data.hex()
            vault=account(key)
            program=TOKEN22 if expected['token2022Data'] is not None else TOKEN
            balance=amount(vault,model[f'token{side}'],model['pool_id'],program)
            require(balance==int(expected['amount']) and vault.data.hex()==expected_bytes,'SBF_ORCA_VAULT_POST_STATE')
    account_evidence=[]
    for key in sorted(post):
        a=post[key];account_evidence.append(dict(address=key,owner=str(a.owner),lamports=str(a.lamports),executable=a.executable,dataSha256=hashlib.sha256(a.data).hexdigest(),dataBytes=len(a.data)))
    evidence = dict(protocol=PROTOCOL,success=True,actualInput=str(paid),actualOutput=str(received),feeLamports=str(fee),payerLamportDelta=str(native_after.lamports-native_before),unitsConsumed=native['meta']['compute_units_consumed'],maxComputeUnits=limit,sourceBankUnchanged=True,transactionSha256=hashlib.sha256(wire).hexdigest(),postAccounts=account_evidence,postAccountsHash=hashlib.sha256(json.dumps(account_evidence,sort_keys=True,separators=(',',':')).encode()).hexdigest(),independentProductionVerification=False,signed=signed is not None,submitted=False)
    if signed is not None:
        evidence['walletSignature'] = str(tx.signatures[0])
    evidence['messageAccountLocks'] = account_locks
    return evidence

if __name__=='__main__':
    try:result=main()
    except Exception as error:result=dict(protocol=PROTOCOL,success=False,error=str(error)if isinstance(error,ValueError)else'SBF_WORKER_FAILURE')
    print(json.dumps(result,separators=(',',':')))
