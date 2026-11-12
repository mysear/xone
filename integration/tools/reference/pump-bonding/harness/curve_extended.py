"""Execute acquired fixed Pump/Fee SBF with explicitly constructed classic/SOL banks."""
from pathlib import Path
import base64,hashlib,json,struct,sys
from layout import ROOT,BASE,IDLS,account,account_bytes,b58decode,b58encode
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from r01_public_catalog_solana_bank import native_imports,TOKEN,pda
from r01_public_catalog_solana_rebuild import LOADER
PUMP=IDLS['pump']['address'];FEES=IDLS['pump_fees']['address'];SYSTEM='11111111111111111111111111111111';ASSOCIATED='ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL'
def execute(kind='buy',amount=1000000,limit=10**15,complete=False,followups=(),quote_reserve=None):
    LiteSVM,Pubkey,Keypair,Instruction,AccountMeta,Transaction,Account,Clock=native_imports()
    for name,expected in [('pump.so','7a19affaa2c6187e04a5fe0ea298cee81fa5eb8c24cfcc30d85f615e89f7b02d'),('fee.so','4f2955d0f6b40cc9ff7daa18ccad7d19119ba1249759f4e7e3bb691beee6b35e')]:
        if hashlib.sha256((BASE/'upstream/artifacts'/name).read_bytes()).hexdigest()!=expected:raise ValueError('PUMP_FIXED_PROGRAM_CHANGED')
    svm=LiteSVM().with_mainnet_features();pid=Pubkey.from_string(PUMP);fid=Pubkey.from_string(FEES)
    svm.add_program_with_loader(pid,(BASE/'upstream/artifacts/pump.so').read_bytes(),Pubkey.from_string(LOADER));svm.add_program_with_loader(fid,(BASE/'upstream/artifacts/fee.so').read_bytes(),Pubkey.from_string(LOADER))
    pins=json.loads((BASE/'public-bank-pins.json').read_bytes());captured={ref['address']:(BASE/ref['path']).read_bytes()for ref in pins['files']}
    global_id=pda([b'global'],PUMP);global_state,_=account('Global',captured[global_id]);fee_id=pda([b'fee_config',b58decode(PUMP)],FEES)
    loaded=[]
    def put(key,data,owner,lamports=None):
        key=str(key);lamports=max(10000000,svm.minimum_balance_for_rent_exemption(len(data)))if lamports is None else lamports;svm.set_account(Pubkey.from_string(key),Account(lamports,data,Pubkey.from_string(owner),False,0));loaded.append(key)
    put(global_id,captured[global_id],PUMP);put(fee_id,captured[fee_id],FEES)
    user=Keypair.from_seed(hashlib.sha256(b'xone-pump-trading-user').digest());payer=Keypair.from_seed(hashlib.sha256(b'xone-pump-transaction-fee-payer').digest());svm.airdrop(user.pubkey(),10**12);svm.airdrop(payer.pubkey(),10**10);loaded.append(str(user.pubkey()))
    creator=b58encode(hashlib.sha256(b'xone-pump-creator').digest());mint=b58encode(hashlib.sha256(b'xone-pump-classic-mint').digest());curve=pda([b'bonding-curve',b58decode(mint)],PUMP)
    vault=pda([b58decode(curve),b58decode(TOKEN),b58decode(mint)],ASSOCIATED);user_ata=pda([bytes(user.pubkey()),b58decode(TOKEN),b58decode(mint)],ASSOCIATED)
    creator_vault=pda([b'creator-vault',b58decode(creator)],PUMP);event=pda([b'__event_authority'],PUMP);gvol=pda([b'global_volume_accumulator'],PUMP);uvol=pda([b'user_volume_accumulator',bytes(user.pubkey())],PUMP)
    mint_raw=bytearray(82);struct.pack_into('<Q',mint_raw,36,10**15);mint_raw[44]=6;mint_raw[45]=1;put(mint,bytes(mint_raw),TOKEN)
    curve_state=dict(virtual_token_reserves=900000000000000,virtual_quote_reserves=40000000000,real_token_reserves=620100000000000,real_quote_reserves=10000000000,token_total_supply=10**15,complete=complete,creator=creator,is_mayhem_mode=False,is_cashback_coin=False,quote_mint=SYSTEM,creator_fee_bps=0,can_edit_creator_fee=False,is_holder_reward=False,creator_fee=0,protocol_fees=0,depth=0,initial_virtual_quote_reserves=30000000000,post_complete_base_out=0,post_complete_quote_in=0)
    if quote_reserve is not None:curve_state['real_quote_reserves']=quote_reserve;curve_state['virtual_quote_reserves']=quote_reserve+global_state['initial_virtual_sol_reserves']
    if complete:curve_state['real_token_reserves']=0;curve_state['virtual_token_reserves']=global_state['initial_virtual_token_reserves']-global_state['initial_real_token_reserves']
    curve_raw=account_bytes('BondingCurve',curve_state,166);put(curve,curve_raw,PUMP,svm.minimum_balance_for_rent_exemption(166)+curve_state['real_quote_reserves'])
    for address,owner,quantity in [(vault,curve,curve_state['real_token_reserves']),(user_ata,str(user.pubkey()),10**14)]:
        raw=bytearray(165);raw[:32]=b58decode(mint);raw[32:64]=b58decode(owner);struct.pack_into('<Q',raw,64,quantity);raw[108]=1;put(address,bytes(raw),TOKEN)
    put(creator_vault,b'',SYSTEM);put(global_state['fee_recipient'],b'',SYSTEM)
    for address in global_state['buyback_fee_recipients']:put(address,b'',SYSTEM)
    g=dict(start_time=0,end_time=0,seconds_in_a_day=86400,mint=SYSTEM,total_token_supply=[0]*30,sol_volumes=[0]*30);put(gvol,account_bytes('GlobalVolumeAccumulator',g,640),PUMP)
    u=dict(user=str(user.pubkey()),needs_claim=False,total_unclaimed_tokens=0,total_claimed_tokens=0,current_sol_volume=0,last_update_timestamp=0,has_total_claimed_tokens=False,cashback_earned=0,total_cashback_claimed=0,stable_cashback_earned=0,total_stable_cashback_claimed=0);put(uvol,account_bytes('UserVolumeAccumulator',u,137),PUMP)
    svm.set_clock(Clock(slot=467957307,epoch_start_timestamp=1794350000,epoch=1000,leader_schedule_epoch=1000,unix_timestamp=1794355000))
    values=dict(global_=global_id,fee_recipient=global_state['fee_recipient'],mint=mint,bonding_curve=curve,associated_bonding_curve=vault,associated_user=user_ata,user=str(user.pubkey()),system_program=SYSTEM,token_program=TOKEN,creator_vault=creator_vault,event_authority=event,program=PUMP,global_volume_accumulator=gvol,user_volume_accumulator=uvol,fee_config=fee_id,fee_program=FEES)
    ix=next(i for i in IDLS['pump']['instructions']if i['name']==kind);metas=[AccountMeta(Pubkey.from_string(values['global_'if a['name']=='global'else a['name']]),a.get('signer',False),a.get('writable',False))for a in ix['accounts']]+[AccountMeta(Pubkey.from_string(pda([b'bonding-curve-v2',b58decode(mint)],PUMP)),False,False),AccountMeta(Pubkey.from_string(global_state['buyback_fee_recipients'][0]),False,True)]
    def capture():
        out={}
        for address in sorted(set(loaded)):
            a=svm.get_account(Pubkey.from_string(address));
            if a is None:
                out[address]=None;continue
            out[address]=dict(owner=str(a.owner),lamports=a.lamports,executable=a.executable,data_base64=base64.b64encode(a.data).decode())
        return out
    def step(value,bound):
        raw=bytes(ix['discriminator'])+struct.pack('<QQ',value,bound)+(b'\0\0'if kind in ['buy','buy_exact_sol_in']else b'')
        before=capture();tx=Transaction.new_signed_with_payer([Instruction(pid,raw,metas)],payer.pubkey(),[payer,user],svm.latest_blockhash());result=json.loads(svm.send_transaction(tx).to_json());after=capture()
        user_key=str(user.pubkey());return dict(passed='err'not in result,kind=kind,amount=str(value),limit=str(bound),before=before,after=after,native=result,user_sol_delta=after[user_key]['lamports']-before[user_key]['lamports'],user_token_delta=int.from_bytes(base64.b64decode(after[user_ata]['data_base64'])[64:72],'little')-int.from_bytes(base64.b64decode(before[user_ata]['data_base64'])[64:72],'little'))
    first=step(amount,limit);sequence=[first]if first['passed']else[]
    if first['passed']:
        for value in followups:sequence.append(step(value,limit))
    return dict(first,sequence=sequence,ids=dict(curve=curve,mint=mint,vault=vault,user=str(user.pubkey()),user_ata=user_ata,creator=creator,creator_vault=creator_vault,global_id=global_id,fee_id=fee_id,global_volume=gvol,user_volume=uvol,buyback=global_state['buyback_fee_recipients'][0],fee_recipient=global_state['fee_recipient']),authority='controlled constructed classic/SOL bank, acquired fixed Pump/Fee binaries and live global/config snapshots; no canonical historical bank or public executor qualification')
