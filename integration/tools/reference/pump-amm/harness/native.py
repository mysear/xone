"""Original PumpSwap + fee SBF with controlled canonical classic/WSOL bank."""
from pathlib import Path
import base64,hashlib,json,struct,sys
ROOT=Path(__file__).resolve().parents[5];BASE=Path(__file__).resolve().parents[1];BONDING=BASE.parent/'pump-bonding';sys.path.insert(0,str(BONDING/'harness'))
from layout import IDLS,account,account_bytes,b58encode,b58decode
IDLS['pump_amm']=json.loads((BONDING/'upstream/idl/pump_amm.json').read_bytes());IDL=IDLS['pump_amm'];PROGRAM=IDL['address'];FEE='pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ';TOKEN='TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA';WSOL='So11111111111111111111111111111111111111112';ASSOCIATED='ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL';SYSTEM='11111111111111111111111111111111'
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from r01_public_catalog_solana_bank import native_imports
from r01_public_catalog_solana_rebuild import LOADER

def execute(kind='sell',amount=1000000,limit=1,followups=(),initial_migration=None):
 LiteSVM,Pubkey,Keypair,Instruction,AccountMeta,Transaction,Account,Clock=native_imports()
 def pda(seeds,program=PROGRAM):return str(Pubkey.find_program_address(seeds,Pubkey.from_string(program))[0])
 def key(label):return b58encode(hashlib.sha256(label.encode()).digest())
 def ata(mint,owner):return pda([b58decode(owner),b58decode(TOKEN),b58decode(mint)],ASSOCIATED)
 binary=(BASE/'upstream/pump_amm.so').read_bytes();fee=(BONDING/'upstream/artifacts/fee.so').read_bytes()
 if hashlib.sha256(binary).hexdigest()!='6a7ad34596d83b4c435b32f458a56e718e9f82eb4ad38e4316616d15d24d8ff7'or hashlib.sha256(fee).hexdigest()!='4f2955d0f6b40cc9ff7daa18ccad7d19119ba1249759f4e7e3bb691beee6b35e':raise ValueError('PUMP_AMM_PIN_CHANGED')
 svm=LiteSVM().with_mainnet_features();svm.add_program_with_loader(Pubkey.from_string(PROGRAM),binary,Pubkey.from_string(LOADER));svm.add_program_with_loader(Pubkey.from_string(FEE),fee,Pubkey.from_string(LOADER));loaded=[]
 def put(id,raw,owner,lamports=None):
  svm.set_account(Pubkey.from_string(id),Account(svm.minimum_balance_for_rent_exemption(len(raw))if lamports is None else lamports,raw,Pubkey.from_string(owner),False,0));loaded.append(id)
 pins=json.loads((BASE/'public-bank-pins.json').read_bytes());captured={r['address']:(BASE/r['path']).read_bytes()for r in pins['files']if'address'in r};global_id=pins['global_id'];fees=pins['fee_id'];g,_=account('GlobalConfig',captured[global_id],'pump_amm');put(global_id,captured[global_id],PROGRAM);put(fees,captured[fees],FEE)
 user=Keypair.from_seed(hashlib.sha256(b'xone-pump-amm-user').digest());payer=Keypair.from_seed(hashlib.sha256(b'xone-pump-amm-network-payer').digest());svm.airdrop(user.pubkey(),10**12);svm.airdrop(payer.pubkey(),10**10);loaded.append(str(user.pubkey()));base=key('xone-pump-amm-base-mint')if initial_migration is None else initial_migration['migration_ids']['mint'];coin_creator=key('xone-pump-amm-coin-creator')if initial_migration is None else account('BondingCurve',base64.b64decode(initial_migration['before'][initial_migration['ids']['curve']]['data_base64']))[0]['creator'];creator=pda([b'pool-authority',b58decode(base)],'6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P');pool,bump=Pubkey.find_program_address([b'pool',b'\0\0',b58decode(creator),b58decode(base),b58decode(WSOL)],Pubkey.from_string(PROGRAM));pool=str(pool);lp=pda([b'pool_lp_mint',b58decode(pool)]);vault0=ata(base,pool);vault1=ata(WSOL,pool)
 state=dict(pool_bump=bump,index=0,creator=creator,base_mint=base,quote_mint=WSOL,lp_mint=lp,pool_base_token_account=vault0,pool_quote_token_account=vault1,lp_supply=10**12,coin_creator=coin_creator,is_mayhem_mode=False,is_cashback_coin=False,virtual_quote_reserves=0,creator_fee_bps=0,can_edit_creator_fee=False,is_holder_reward=False,protocol_fees=0,creator_fees=0);put(pool,account_bytes('Pool',state,idl='pump_amm'),PROGRAM)
 for mint,quantity,decimals in [(base,10**15,6),(WSOL,0,9)]:
  raw=bytearray(82);struct.pack_into('<Q',raw,36,quantity);raw[44]=decimals;raw[45]=1;put(mint,bytes(raw),TOKEN)
 creator_authority=pda([b'creator_vault',b58decode(coin_creator)]);creator_vault=ata(WSOL,creator_authority);protocol_recipient=g['protocol_fee_recipients'][0];protocol_ata=ata(WSOL,protocol_recipient);buyback=g['buyback_fee_recipients'][0];buyback_ata=ata(WSOL,buyback);user0=ata(base,str(user.pubkey()));user1=ata(WSOL,str(user.pubkey()))
 for id,mint,owner,quantity in [(vault0,base,pool,620100000000000),(vault1,WSOL,pool,89725080388),(user0,base,str(user.pubkey()),10**14),(user1,WSOL,str(user.pubkey()),10**15),(creator_vault,WSOL,creator_authority,0),(protocol_ata,WSOL,protocol_recipient,0),(buyback_ata,WSOL,buyback,0)]:
  raw=bytearray(165);raw[:32]=b58decode(mint);raw[32:64]=b58decode(owner);struct.pack_into('<Q',raw,64,quantity);raw[108]=1;rent=svm.minimum_balance_for_rent_exemption(165)
  if mint==WSOL:struct.pack_into('<IQ',raw,109,1,rent)
  put(id,bytes(raw),TOKEN,rent+(quantity if mint==WSOL else 0))
 gvol=pda([b'global_volume_accumulator']);uvol=pda([b'user_volume_accumulator',bytes(user.pubkey())]);put(gvol,account_bytes('GlobalVolumeAccumulator',dict(start_time=0,end_time=0,seconds_in_a_day=86400,mint=SYSTEM,total_token_supply=[0]*30,sol_volumes=[0]*30),640,idl='pump_amm'),PROGRAM);put(uvol,account_bytes('UserVolumeAccumulator',dict(user=str(user.pubkey()),needs_claim=False,total_unclaimed_tokens=0,total_claimed_tokens=0,current_sol_volume=0,last_update_timestamp=0,has_total_claimed_tokens=False,cashback_earned=0,total_cashback_claimed=0),137,idl='pump_amm'),PROGRAM)
 if initial_migration is not None:
  if not initial_migration['passed']:raise ValueError('MIGRATION_REQUIRED')
  if pool!=initial_migration['migration_ids']['pool']:raise ValueError('MIGRATION_TARGET_IDENTITY')
  for id in [pool,base,WSOL,vault0,vault1]:
   a=initial_migration['after'][id];put(id,base64.b64decode(a['data_base64']),a['owner'],a['lamports'])
 values=dict(pool=pool,user=str(user.pubkey()),global_config=global_id,base_mint=base,quote_mint=WSOL,user_base_token_account=user0,user_quote_token_account=user1,pool_base_token_account=vault0,pool_quote_token_account=vault1,protocol_fee_recipient=protocol_recipient,protocol_fee_recipient_token_account=protocol_ata,base_token_program=TOKEN,quote_token_program=TOKEN,system_program=SYSTEM,associated_token_program=ASSOCIATED,event_authority=pda([b'__event_authority']),program=PROGRAM,coin_creator_vault_ata=creator_vault,coin_creator_vault_authority=creator_authority,global_volume_accumulator=gvol,user_volume_accumulator=uvol,fee_config=fees,fee_program=FEE)
 ix=next(i for i in IDL['instructions']if i['name']==kind);metas=[AccountMeta(Pubkey.from_string(values[a['name']]),a.get('signer',False),a.get('writable',False))for a in ix['accounts']]+[AccountMeta(Pubkey.from_string(pda([b'pool-v2',b58decode(base)])),False,False),AccountMeta(Pubkey.from_string(buyback),False,False),AccountMeta(Pubkey.from_string(buyback_ata),False,True)]
 svm.set_clock(Clock(slot=pins['slot'],epoch_start_timestamp=1794370000,epoch=1000,leader_schedule_epoch=1000,unix_timestamp=1794375000))
 def capture():
  out={}
  for id in sorted(set(loaded)):
   a=svm.get_account(Pubkey.from_string(id));out[id]=dict(owner=str(a.owner),lamports=a.lamports,executable=a.executable,data_base64=base64.b64encode(a.data).decode())
  return out
 def step(value,bound):
  raw=bytes(ix['discriminator'])+struct.pack('<QQ',value,bound)+(b'\0\0'if kind in ['buy','buy_exact_quote_in']else b'');before=capture();tx=Transaction.new_signed_with_payer([Instruction(Pubkey.from_string(PROGRAM),raw,metas)],payer.pubkey(),[payer,user],svm.latest_blockhash());result=json.loads(svm.send_transaction(tx).to_json());after=capture()
  def delta(id):return int.from_bytes(base64.b64decode(after[id]['data_base64'])[64:72],'little')-int.from_bytes(base64.b64decode(before[id]['data_base64'])[64:72],'little')
  return dict(passed='err'not in result,kind=kind,amount=str(value),limit=str(bound),before=before,after=after,native=result,user_base_delta=delta(user0),user_quote_delta=delta(user1))
 first=step(amount,limit);sequence=[first]
 if first['passed']:
  for value in followups:sequence.append(step(value,limit));svm.expire_blockhash()
 return dict(first,sequence=sequence,ids=dict(values,lp_mint=lp,buyback_recipient=buyback,buyback_ata=buyback_ata),authority='controlled canonical-PDA classic/WSOL PumpSwap bank with acquired original binaries; no migration replay or trusted history')
if __name__=='__main__':
 n=execute();(ROOT/'integration/artifacts/local/core-pump-amm-preflight-20261111.json').write_text(json.dumps(n,indent=2)+'\n');print(json.dumps({k:n[k]for k in ['passed','user_base_delta','user_quote_delta']}));print(n['native']if not n['passed']else'original PumpSwap and Token CPI completed')
