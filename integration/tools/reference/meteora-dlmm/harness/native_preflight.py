"""Execute retained upstream DLMM SBF and account fixtures in controlled LiteSVM."""
from pathlib import Path
import base64,hashlib,json,struct,sys
from idl_layout import ROOT,BASE,IDL,account,decode,encode
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from r01_public_catalog_solana_bank import native_imports,TOKEN,WSOL,MEMO
from r01_public_catalog_solana_rebuild import LOADER

def execute(zero=True,amount=1000000,minimum=1,exact_out=False,mm_only=False,protocol_share=None,elapsed=None,dynamic_control=None,followups=()):
    LiteSVM,Pubkey,Keypair,Instruction,AccountMeta,Transaction,Account,Clock=native_imports()
    pid=Pubkey.from_string(IDL['address']);folder=BASE/'commons/tests/fixtures/EtAdVRLFH22rjWh3mcUasKFF27WtHhsaCvK27tPFFWig';pair,coverage=account('LbPair',folder/'lb_pair.bin')
    svm=LiteSVM().with_mainnet_features();program=BASE/'commons/tests/artifacts/lb_clmm_prod.so';svm.add_program_with_loader(pid,program.read_bytes(),Pubkey.from_string(LOADER))
    token=Pubkey.from_string(TOKEN);loaded=[]
    overrides=[];prepared={}
    if mm_only:
        import copy
        pair=copy.deepcopy(pair);pair['parameters']['function_type']=1
        if protocol_share is not None:pair['parameters']['protocol_share']=protocol_share
        if dynamic_control is not None:pair['parameters']['variable_fee_control']=dynamic_control
        if elapsed is not None:pair['v_parameters']['last_update_timestamp']=1734392448-elapsed
        raw=(folder/'lb_pair.bin').read_bytes();prepared['lb_pair.bin']=raw[:8]+encode({'defined':{'name':'LbPair'}},pair)
        totals=[0,0]
        for filename in ['bin_array_1.bin','bin_array_2.bin']:
            bins,_=account('BinArray',folder/filename)
            for value in bins['bins']:
                for key in ['fulfilled_order_amount_x','fulfilled_order_amount_y','limit_order_fee_ask_side','limit_order_fee_bid_side','open_order_amount','total_processing_order_amount','processed_order_remaining_amount','order_age','limit_order_ask_side']:value[key]=0
                totals[0]+=value['amount_x'];totals[1]+=value['amount_y']
            prepared[filename]=(folder/filename).read_bytes()[:8]+encode({'defined':{'name':'BinArray'}},bins)
        for side,name in enumerate(['reserve_x.bin','reserve_y.bin']):
            raw=bytearray((folder/name).read_bytes());struct.pack_into('<Q',raw,64,totals[side]+pair['protocol_fee']['amount_x'if side==0 else'amount_y']);prepared[name]=bytes(raw)
        overrides=[dict(file=name,original_sha256=hashlib.sha256((folder/name).read_bytes()).hexdigest(),controlled_sha256=hashlib.sha256(raw).hexdigest())for name,raw in prepared.items()]

    for filename,address,owner in [('lb_pair.bin',folder.name,pid),('oracle.bin',pair['oracle'],pid),('reserve_x.bin',pair['reserve_x'],token),('reserve_y.bin',pair['reserve_y'],token),('token_x_mint.bin',pair['token_x_mint'],token)]:
        raw=prepared.get(filename,(folder/filename).read_bytes());lamports=max(10_000_000_000,svm.minimum_balance_for_rent_exemption(len(raw)))
        if len(raw)==165 and raw[109:113]==b'\x01\0\0\0':lamports=struct.unpack_from('<Q',raw,113)[0]+struct.unpack_from('<Q',raw,64)[0]
        svm.set_account(Pubkey.from_string(address),Account(lamports,raw,owner,False,0));loaded.append(address)
    mint=bytearray(82);mint[44]=9;mint[45]=1;svm.set_account(Pubkey.from_string(WSOL),Account(svm.minimum_balance_for_rent_exemption(82),bytes(mint),token,False,0));loaded.append(WSOL)
    arrays=[]
    for filename in ['bin_array_1.bin','bin_array_2.bin']:
        value,_=account('BinArray',folder/filename);key,_=Pubkey.find_program_address([b'bin_array',bytes(Pubkey.from_string(folder.name)),int(value['index']).to_bytes(8,'little',signed=True)],pid);svm.set_account(key,Account(10_000_000_000,prepared.get(filename,(folder/filename).read_bytes()),pid,False,0));arrays.append((int(value['index']),key));loaded.append(str(key))
    clock=Clock(slot=300000000,epoch_start_timestamp=1734391000,epoch=1,leader_schedule_epoch=1,unix_timestamp=1734392448);svm.set_clock(clock)
    payer=Keypair.from_seed(hashlib.sha256(b'xone-meteora-reference-payer').digest());recipient=Pubkey.from_bytes(hashlib.sha256(b'xone-meteora-reference-recipient').digest());svm.airdrop(payer.pubkey(),10_000_000_000)
    user_in=Pubkey.from_bytes(hashlib.sha256(b'xone-meteora-reference-input').digest());user_out=Pubkey.from_bytes(hashlib.sha256(b'xone-meteora-reference-output').digest());mint_in=pair['token_x_mint']if zero else pair['token_y_mint'];mint_out=pair['token_y_mint']if zero else pair['token_x_mint']
    for address,mint_addr,owner,quantity in [(user_in,mint_in,payer.pubkey(),10**15),(user_out,mint_out,recipient,7)]:
        raw=bytearray(165);raw[:32]=bytes(Pubkey.from_string(mint_addr));raw[32:64]=bytes(owner);struct.pack_into('<Q',raw,64,quantity);raw[108]=1;rent=svm.minimum_balance_for_rent_exemption(165);lamports=rent
        if mint_addr==WSOL:struct.pack_into('<IQ',raw,109,1,rent);lamports+=quantity
        svm.set_account(address,Account(lamports,bytes(raw),token,False,0));loaded.append(str(address))
    event,_=Pubkey.find_program_address([b'__event_authority'],pid);name='swap_exact_out2'if exact_out else'swap2';ix=next(i for i in IDL['instructions']if i['name']==name)
    values=dict(lb_pair=folder.name,bin_array_bitmap_extension=str(pid),reserve_x=pair['reserve_x'],reserve_y=pair['reserve_y'],user_token_in=str(user_in),user_token_out=str(user_out),token_x_mint=pair['token_x_mint'],token_y_mint=pair['token_y_mint'],oracle=pair['oracle'],host_fee_in=str(pid),user=str(payer.pubkey()),token_x_program=TOKEN,token_y_program=TOKEN,memo_program=MEMO,event_authority=str(event),program=str(pid))
    metas=[AccountMeta(Pubkey.from_string(values[a['name']]),a.get('signer',False),False if a.get('optional')else a.get('writable',False))for a in ix['accounts']]
    metas.extend(AccountMeta(key,False,True)for _,key in sorted(arrays,reverse=zero))
    def capture():
        result={}
        for address in loaded:
            a=svm.get_account(Pubkey.from_string(address));result[address]=dict(lamports=a.lamports,owner=str(a.owner),executable=a.executable,rent_epoch=a.rent_epoch,data_base64=base64.b64encode(a.data).decode())
        return result
    def swap_step(step_amount,step_limit):
        before=capture()
        data=bytes(ix['discriminator'])+struct.pack('<QQ',step_limit,step_amount)if exact_out else bytes(ix['discriminator'])+struct.pack('<QQ',step_amount,step_limit)
        data+=struct.pack('<I',0)
        tx=Transaction.new_signed_with_payer([Instruction(pid,data,metas)],payer.pubkey(),[payer],svm.latest_blockhash());native=json.loads(svm.send_transaction(tx).to_json());after=capture()
        used=int.from_bytes(base64.b64decode(before[str(user_in)]['data_base64'])[64:72],'little')-int.from_bytes(base64.b64decode(after[str(user_in)]['data_base64'])[64:72],'little')
        received=int.from_bytes(base64.b64decode(after[str(user_out)]['data_base64'])[64:72],'little')-int.from_bytes(base64.b64decode(before[str(user_out)]['data_base64'])[64:72],'little')
        return dict(specified_amount=str(step_amount),limit=str(step_limit),native=native,before=before,after=after,payer_input_delta=str(used),recipient_output_delta=str(received),passed='err'not in native)
    initial=swap_step(amount,minimum)
    native,before,after=initial['native'],initial['before'],initial['after'];used,received=int(initial['payer_input_delta']),int(initial['recipient_output_delta'])
    sequence=[]
    if initial['passed']:
        sequence.append(initial)
        for step_amount in followups:sequence.append(swap_step(step_amount,minimum))
    return dict(sequence=sequence,controlled_mm_only=mm_only,account_overrides=overrides,schema_version=1,passed='err'not in native,zero_for_one=zero,kind='EXACT_OUT'if exact_out else'EXACT_IN',specified_amount=str(amount),limit=str(minimum),payer=str(payer.pubkey()),recipient=str(recipient),payer_input_delta=str(used),recipient_output_delta=str(received),clock=dict(slot=clock.slot,unix_timestamp=clock.unix_timestamp),program_sha256=hashlib.sha256(program.read_bytes()).hexdigest(),native=native,before=before,after=after,authority='upstream-retained SBF/account fixture in controlled LiteSVM; patched WSOL mint/backing/user funding and Clock explicit; no mainnet programdata or historical bank qualification')

if __name__=='__main__':
    result=execute(zero='--reverse'not in sys.argv,amount=1000000,minimum=1,mm_only='--mm-only'in sys.argv);target=ROOT/'integration/artifacts/local'/('core-meteora-dlmm-native-mm-preflight-20261110.json'if '--mm-only'in sys.argv else'core-meteora-dlmm-native-preflight-20261110.json');target.write_text(json.dumps(result,indent=2)+'\n');print(json.dumps({k:result[k]for k in ['passed','zero_for_one','specified_amount','payer_input_delta','recipient_output_delta','program_sha256']}));print(result['native']if not result['passed']else'original program and token CPI executed')
