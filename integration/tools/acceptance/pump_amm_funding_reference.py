#!/usr/bin/env python3
"""Verify exact PumpSwap funding instructions, original SBF settlement and atomic rollback."""
from pathlib import Path
import base64,hashlib,json,sys
from r01_public_catalog_solana_bank import native_imports
from solana_expanded_reference import ROOT,require
from solana_prepare_simulation_worker import simulate
PROGRAM='pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA';FEE='pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ';TOKEN='TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'
IDL=ROOT/'integration/tools/reference/pump-bonding/upstream/idl/pump_amm.json';IDL_SHA='b7d8c57a4d9c4dd0109a9ab893052352333252d4eedced10ac091d4d66cab89b'
def verify(path,index):
 doc=json.loads(Path(path).read_bytes());p=doc.get('preparation',doc);q=doc.get('quote')
 fixture=json.loads((ROOT/'integration/test/contracts/pump-amm-execution-bank.json').read_bytes());b=fixture['cases'][index];accounts=dict(fixture['shared_accounts']);accounts.update(b['accounts']);b=dict(b,accounts=accounts)
 require(hashlib.sha256(IDL.read_bytes()).hexdigest()==IDL_SHA,'pump-amm-fixed-idl')
 require(hashlib.sha256(json.dumps({k:v for k,v in p.items()if k!='preparationHash'},sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()).hexdigest()==p['preparationHash'],'pump-preparation-hash')
 require(p['ready']is True and p['signed']is False and p['submitted']is False and p['requiredSigners']==[b['caller']],'pump-only-unsigned-caller')
 S,P,K,I,M,T,A,C=native_imports();wire=base64.b64decode(p['unsignedTransaction'],validate=True);tx=T.from_bytes(wire)
 require(len(wire)<=1232 and wire[0]==1 and wire[1:65]==bytes(64)and wire[65:]==base64.b64decode(p['messageBase64'])and tx.message.header.num_required_signatures==1 and str(tx.message.account_keys[0])==b['caller'],'pump-wire-signer')
 kind='sell'if b['zeroForOne']else'buy'if b['exactOut']else'buy_exact_quote_in';definition=next(d for d in json.loads(IDL.read_bytes())['instructions']if d['name']==kind);values=b['ids']
 values=dict(values);options=b['preparationOptions'];caller=b['caller'];WSOL='So11111111111111111111111111111111111111112';SYSTEM='11111111111111111111111111111111';ATA='ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL'
 refund=options.get('refundWrappedSource',False);unwrap=options.get('unwrapDestinationSol',False);wrap_enabled=options.get('wrapSourceSol',False)
 seed='xone-'+('in-'if refund else'out-')+p['planHash'][:20];temporary=str(P.create_with_seed(P.from_string(caller),seed,P.from_string(TOKEN)))if refund or unwrap else None
 source=temporary if refund else b['sourceTokenAccount'];destination=temporary if unwrap else b['destinationTokenAccount']
 values['user_base_token_account']=source if b['zeroForOne']else destination;values['user_quote_token_account']=destination if b['zeroForOne']else source
 expected=[(values[a['name']],a.get('signer',False),a.get('writable',False))for a in definition['accounts']]
 secondary=str(P.find_program_address([b'pool-v2',bytes(P.from_string(values['base_mint']))],P.from_string(PROGRAM))[0]);expected.extend([(secondary,False,False),(values['buyback_recipient'],False,False),(values['buyback_ata'],False,True)])
 swap_index=next(i for i,d in enumerate(p['instructions'])if d['programId']==PROGRAM)
 d=p['instructions'][swap_index];ix=tx.message.instructions[swap_index];keys=list(map(str,tx.message.account_keys));require(str(tx.message.account_keys[ix.program_id_index])==d['programId']==PROGRAM and [keys[i]for i in ix.accounts]==[a['pubkey']for a in d['accounts']]and bytes(ix.data)==base64.b64decode(d['dataBase64']),'pump-wire-descriptors')
 require([(a['pubkey'],a['isSigner'],a['isWritable'])for a in d['accounts']]==expected,'pump-original-account-roles')
 target=int(b['target']);used=int(b['requiredInput']);output=int(b['actualOutput']);minimum=target if b['exactOut']else output*9950//10000;maximum=(used*10050+9999)//10000
 first=used if b['zeroForOne']or not b['exactOut']else target;second=minimum if b['zeroForOne']or not b['exactOut']else maximum
 require(bytes(ix.data)==bytes(definition['discriminator'])+first.to_bytes(8,'little')+second.to_bytes(8,'little')+(bytes(2)if not b['zeroForOne']else b''),'pump-original-instruction-amounts')
 require(p['sourceTokenAccount']==source and p['destinationTokenAccount']==destination and p['quotedInput']==str(used)and p['quotedOutput']==str(output)and p['minBuyAmount']==str(minimum)and p['requiredInput']==str(maximum if b['exactOut']and not b['zeroForOne']else used),'pump-context-amounts')
 # Reconstruct funding bytes from the bank/options, never the claimed DTO.
 vm=S().with_mainnet_features();rent=vm.minimum_balance_for_rent_exemption(165)
 def pending(key):return key not in accounts or accounts[key]['owner']==SYSTEM
 def initial(key,mint):
  a=accounts.get(key)
  if a is None:return 0
  if a['owner']==SYSTEM:return max(0,a['lamports']-rent)if mint==WSOL else 0
  return int.from_bytes(base64.b64decode(a['data'][0])[64:72],'little')
 input_mint=b['pool']['token0'if b['zeroForOne']else'token1'];output_mint=b['pool']['token1'if b['zeroForOne']else'token0']
 budget=maximum if b['exactOut']and not b['zeroForOne']else used
 source_create=wrap_enabled and pending(source);dest_create=options.get('createDestinationAta',False)and pending(destination)
 src_prefund=accounts.get(source,{}).get('lamports',0)if pending(source)else 0;dst_prefund=accounts.get(destination,{}).get('lamports',0)if pending(destination)else 0
 source_rent=max(0,rent-src_prefund)if source_create else 0;dest_rent=max(0,rent-dst_prefund)if dest_create or unwrap else 0
 total_rent=source_rent+dest_rent;persistent=total_rent-(rent if temporary else 0)
 wrap=max(0,budget-initial(source,input_mint))if wrap_enabled else 0
 native_delta=-5000-persistent-used if refund else output-5000-persistent if unwrap else -5000-total_rent-wrap
 require(p['feeLamports']=='5000'and p['totalFundingLamports']==str(5000+total_rent+wrap)and p['rentLamports']==str(total_rent)and p['expectedPayerLamportDelta']==str(native_delta),'pump-fee-funding')
 require(p['sourceAccountCreation']==source_create and p['accountCreation']==dest_create and p['wrapLamports']==str(wrap)and p['rentRefundLamports']==str(rent if temporary else 0)and p['refundWrappedSource']==refund and p['unwrapDestinationSol']==unwrap,'pump-lifecycle-binding')
 prefix=[];suffix=[]
 def descriptor(program,data,metas):return dict(programId=program,dataBase64=base64.b64encode(data).decode(),accounts=[dict(pubkey=key,isSigner=sign,isWritable=write)for key,sign,write in metas])
 if temporary:
  require(temporary not in accounts,'pump-temporary-absent')
  prefix.append(descriptor(SYSTEM,(3).to_bytes(4,'little')+bytes(P.from_string(caller))+len(seed.encode()).to_bytes(8,'little')+seed.encode()+rent.to_bytes(8,'little')+(165).to_bytes(8,'little')+bytes(P.from_string(TOKEN)),[(caller,True,True),(temporary,False,True)]))
  prefix.append(descriptor(TOKEN,bytes([18])+bytes(P.from_string(caller)),[(temporary,False,True),(WSOL,False,False)]))
  suffix.append(descriptor(TOKEN,bytes([9]),[(temporary,False,True),(caller,False,True),(caller,True,False)]))
 def create_ata(address,mint):
  canonical=str(P.find_program_address([bytes(P.from_string(caller)),bytes(P.from_string(TOKEN)),bytes(P.from_string(mint))],P.from_string(ATA))[0]);require(address==canonical,'pump-canonical-ata')
  return descriptor(ATA,bytes([1]),[(caller,True,True),(address,False,True),(caller,False,False),(mint,False,False),(SYSTEM,False,False),(TOKEN,False,False)])
 if source_create and not refund:prefix.append(create_ata(source,input_mint))
 if wrap:
  prefix.append(descriptor(SYSTEM,(2).to_bytes(4,'little')+wrap.to_bytes(8,'little'),[(caller,True,True),(source,False,True)]));prefix.append(descriptor(TOKEN,bytes([17]),[(source,False,True)]))
 if dest_create:prefix.append(create_ata(destination,output_mint))
 require(p['instructions']==prefix+[d]+suffix,'pump-exact-funding-instructions')
 require(len(tx.message.instructions)==len(p['instructions']),'pump-all-instructions-bound')
 rights={caller:(True,True)}
 for compiled,descriptor in zip(tx.message.instructions,p['instructions']):
  require(keys[compiled.program_id_index]==descriptor['programId']and bytes(compiled.data)==base64.b64decode(descriptor['dataBase64'])and [keys[i]for i in compiled.accounts]==[a['pubkey']for a in descriptor['accounts']],'pump-funding-wire-descriptors')
  rights.setdefault(descriptor['programId'],(False,False))
  for a in descriptor['accounts']:
   old=rights.get(a['pubkey'],(False,False));rights[a['pubkey']]=(old[0]or a['isSigner'],old[1]or a['isWritable'])
 require(set(rights)==set(keys)and len(keys)==len(set(keys)),'pump-key-set')
 h=tx.message.header
 for i,key in enumerate(keys):
  signed=i<h.num_required_signatures;writable=i<(h.num_required_signatures-h.num_readonly_signed_accounts if signed else len(keys)-h.num_readonly_unsigned_accounts)
  require(rights[key]==(signed,writable),'pump-effective-permissions')
 simulation=simulate(dict(bank=b,rpc=dict(method='simulateTransaction',params=[p['unsignedTransaction'],dict(sigVerify=False,replaceRecentBlockhash=False,accounts={'addresses':[source,destination,b['caller']]})])))
 require(simulation['value']['err']is None,'pump-readonly-original-simulation')
 signer=K.from_seed(hashlib.sha256(b'xone-pump-amm-user').digest());require(str(signer.pubkey())==b['caller']==b['recipient'],'pump-declared-user')
 def execute(control=False):
  svm=S().with_mainnet_features().with_blockhash_check(False)
  for program in [PROGRAM,FEE,TOKEN]:
   raw=base64.b64decode(accounts[program]['data'][0]);pd=str(P.from_bytes(raw[4:36]));svm.add_program_with_loader(P.from_string(program),base64.b64decode(accounts[pd]['data'][0])[45:],P.from_string('BPFLoaderUpgradeab1e11111111111111111111111'))
  for key,a in accounts.items():svm.set_account(P.from_string(key),A(a['lamports'],base64.b64decode(a['data'][0]),P.from_string(a['owner']),a['executable'],0))
  raw=base64.b64decode(accounts['SysvarC1ock11111111111111111111111111111111']['data'][0]);svm.set_clock(C(slot=int.from_bytes(raw[:8],'little'),epoch_start_timestamp=int.from_bytes(raw[8:16],'little',signed=True),epoch=int.from_bytes(raw[16:24],'little'),leader_schedule_epoch=int.from_bytes(raw[24:32],'little'),unix_timestamp=int.from_bytes(raw[32:40],'little',signed=True)))
  transaction=T.from_bytes(wire)
  if control:
   raw=bytearray(ix.data)
   if b['exactOut']and b['zeroForOne']:raw[8:16]=(used-1).to_bytes(8,'little')
   elif b['exactOut']:raw[16:24]=(used-1).to_bytes(8,'little')
   else:raw[16:24]=(output+1).to_bytes(8,'little')
   instructions=[I(P.from_string(d['programId']),bytes(raw)if i==swap_index else base64.b64decode(d['dataBase64']),[M(P.from_string(a['pubkey']),a['isSigner'],a['isWritable'])for a in d['accounts']])for i,d in enumerate(p['instructions'])]
   transaction=T.new_signed_with_payer(instructions,signer.pubkey(),[signer],tx.message.recent_blockhash)
  else:transaction.sign([signer],tx.message.recent_blockhash)
  def capture():
   result={}
   for key in sorted(set(accounts)|set(keys)):
    a=svm.get_account(P.from_string(key));result[key]=None if a is None else dict(owner=str(a.owner),lamports=a.lamports,executable=a.executable,dataSha256=hashlib.sha256(a.data).hexdigest(),dataHex=a.data.hex()if str(a.owner)==TOKEN or key==b['pool']['pool_id']else None)
   return result
  before=capture();native=json.loads(svm.send_transaction(transaction).to_json());after=capture()
  return dict(passed='err'not in native,before=before,after=after,native=native)
 n=execute();require(n['passed'],'pump-original-wire-execution:'+str(n['native']))
 for item in b['after']['pump']['accounts']:
  a=n['after'][item['address']];require(a['owner']==item['owner']and a['lamports']==item['lamports']and a['dataSha256']==hashlib.sha256(bytes.fromhex(item['data'])).hexdigest(),'pump-complete-original-post-state')
 balance=lambda a:int.from_bytes(bytes.fromhex(a['dataHex'])[64:72],'little')
 src=source;dst=destination;caller=b['caller']
 if temporary:require(n['before'][temporary]is None and n['after'][temporary]is None,'pump-temporary-closed')
 if not refund:require(initial(src,input_mint)+wrap-balance(n['after'][src])==used,'pump-real-source-settlement')
 if not unwrap:require(balance(n['after'][dst])-initial(dst,output_mint)==output,'pump-real-destination-settlement')
 require(n['after'][caller]['lamports']-n['before'][caller]['lamports']==native_delta,'pump-user-native-funding')
 totals={}
 for phase,sign in [('before',-1),('after',1)]:
  for a in n[phase].values():
   if a is None or a['owner']!=TOKEN or a['dataHex']is None:continue
   raw=bytes.fromhex(a['dataHex'])
   if len(raw)==165:
    totals[raw[:32]]=totals.get(raw[:32],0)+sign*int.from_bytes(raw[64:72],'little')
    if raw[109:113]==b'\x01\0\0\0':require(a['lamports']==int.from_bytes(raw[113:121],'little')+int.from_bytes(raw[64:72],'little'),'pump-native-backing')
 expected_wsol=used if refund else -output if unwrap else wrap
 # System-prefunded WSOL is converted into native tokens on initialization.
 if source_create and input_mint==WSOL:expected_wsol+=initial(src,input_mint)
 if dest_create and output_mint==WSOL:expected_wsol+=initial(dst,output_mint)
 require(all(v==(expected_wsol if mint==bytes(P.from_string(WSOL))else 0)for mint,v in totals.items()),'pump-token-conservation')
 control=execute(True);require(not control['passed'],'pump-original-slippage-control')
 for key,a in control['before'].items():
  if key!=caller:require(a==control['after'][key],'pump-complete-rollback')
 require(control['before'][caller]['lamports']-control['after'][caller]['lamports']==5000,'pump-failed-fee-only')
 return dict(passed=True,index=index,kind=b['kind'],boost=b['boost'],fundingScenario=b['fundingScenario'],wireBytes=len(wire),actualPersistentRentLamports=str(persistent),input=str(used),output=str(output),minimumOutput=str(minimum),surplus=str(output-minimum if b['exactOut']else 0),original=n,control=control,simulation=simulation,publicTransactionsSubmitted=False,productionReady=False)
if __name__=='__main__':
 if sys.argv[1]=='--matrix':
  folder=Path(sys.argv[2]);rows=[verify(folder/f'pump-amm-{i}-preparation.json',i)for i in range(9,31)];r=dict(passed=True,cases=len(rows),records=rows,publicTransactionsSubmitted=False);Path(sys.argv[3]).write_text(json.dumps(r,separators=(',',':'))+'\n');print(json.dumps(dict(passed=True,cases=len(rows))))
 else:
  r=verify(sys.argv[1],int(sys.argv[3])if len(sys.argv)>3 else 9);Path(sys.argv[2]).write_text(json.dumps(r,separators=(',',':'))+'\n');print(json.dumps({k:r[k]for k in ['passed','index','input','output','minimumOutput','surplus']}))
