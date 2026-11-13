#!/usr/bin/env python3
"""Verify actual Router PumpSwap wire and complete original SBF post-bank/rollback."""
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
 expected=[(values[a['name']],a.get('signer',False),a.get('writable',False))for a in definition['accounts']]
 secondary=str(P.find_program_address([b'pool-v2',bytes(P.from_string(values['base_mint']))],P.from_string(PROGRAM))[0]);expected.extend([(secondary,False,False),(values['buyback_recipient'],False,False),(values['buyback_ata'],False,True)])
 require(len(tx.message.instructions)==len(p['instructions'])==1,'pump-single-swap')
 d=p['instructions'][0];ix=tx.message.instructions[0];keys=list(map(str,tx.message.account_keys));require(str(tx.message.account_keys[ix.program_id_index])==d['programId']==PROGRAM and [keys[i]for i in ix.accounts]==[a['pubkey']for a in d['accounts']]and bytes(ix.data)==base64.b64decode(d['dataBase64']),'pump-wire-descriptors')
 require([(a['pubkey'],a['isSigner'],a['isWritable'])for a in d['accounts']]==expected,'pump-original-account-roles')
 target=int(b['target']);used=int(b['requiredInput']);output=int(b['actualOutput']);minimum=target if b['exactOut']else output*9950//10000;maximum=(used*10050+9999)//10000
 first=used if b['zeroForOne']or not b['exactOut']else target;second=minimum if b['zeroForOne']or not b['exactOut']else maximum
 require(bytes(ix.data)==bytes(definition['discriminator'])+first.to_bytes(8,'little')+second.to_bytes(8,'little')+(bytes(2)if not b['zeroForOne']else b''),'pump-original-instruction-amounts')
 require(p['sourceTokenAccount']==b['sourceTokenAccount']and p['destinationTokenAccount']==b['destinationTokenAccount']and p['quotedInput']==str(used)and p['quotedOutput']==str(output)and p['minBuyAmount']==str(minimum)and p['requiredInput']==str(maximum if b['exactOut']and not b['zeroForOne']else used),'pump-context-amounts')
 require(p['feeLamports']==p['totalFundingLamports']=='5000'and p['rentLamports']=='0'and p['expectedPayerLamportDelta']=='-5000','pump-fee-funding')
 rights={b['caller']:(True,True),PROGRAM:(False,False)}
 for key,sign,write in expected:
  old=rights.get(key,(False,False));rights[key]=(old[0]or sign,old[1]or write)
 require(set(rights)==set(keys)and len(keys)==len(set(keys)),'pump-key-set')
 h=tx.message.header
 for i,key in enumerate(keys):
  signed=i<h.num_required_signatures;writable=i<(h.num_required_signatures-h.num_readonly_signed_accounts if signed else len(keys)-h.num_readonly_unsigned_accounts)
  require(rights[key]==(signed,writable),'pump-effective-permissions')
 simulation=simulate(dict(bank=b,rpc=dict(method='simulateTransaction',params=[p['unsignedTransaction'],dict(sigVerify=False,replaceRecentBlockhash=False,accounts={'addresses':[b['sourceTokenAccount'],b['destinationTokenAccount'],b['caller']]})])))
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
   transaction=T.new_signed_with_payer([I(P.from_string(PROGRAM),bytes(raw),[M(P.from_string(key),sign,write)for key,sign,write in expected])],signer.pubkey(),[signer],tx.message.recent_blockhash)
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
 src=b['sourceTokenAccount'];dst=b['destinationTokenAccount'];caller=b['caller']
 require(balance(n['before'][src])-balance(n['after'][src])==used and balance(n['after'][dst])-balance(n['before'][dst])==output,'pump-real-user-token-settlement')
 require(n['before'][caller]['lamports']-n['after'][caller]['lamports']==5000,'pump-user-network-fee')
 totals={}
 for phase,sign in [('before',-1),('after',1)]:
  for a in n[phase].values():
   if a is None or a['owner']!=TOKEN or a['dataHex']is None:continue
   raw=bytes.fromhex(a['dataHex'])
   if len(raw)==165:
    totals[raw[:32]]=totals.get(raw[:32],0)+sign*int.from_bytes(raw[64:72],'little')
    if raw[109:113]==b'\x01\0\0\0':require(a['lamports']==int.from_bytes(raw[113:121],'little')+int.from_bytes(raw[64:72],'little'),'pump-native-backing')
 require(all(v==0 for v in totals.values()),'pump-token-conservation')
 control=execute(True);require(not control['passed'],'pump-original-slippage-control')
 for key,a in control['before'].items():
  if key!=caller:require(a==control['after'][key],'pump-complete-rollback')
 require(control['before'][caller]['lamports']-control['after'][caller]['lamports']==5000,'pump-failed-fee-only')
 return dict(passed=True,index=index,kind=b['kind'],boost=b['boost'],input=str(used),output=str(output),minimumOutput=str(minimum),surplus=str(output-minimum if b['exactOut']else 0),original=n,control=control,simulation=simulation,publicTransactionsSubmitted=False,productionReady=False)
if __name__=='__main__':
 if sys.argv[1]=='--matrix':
  folder=Path(sys.argv[2]);rows=[verify(folder/f'pump-amm-{i}-preparation.json',i)for i in range(9)];r=dict(passed=True,cases=len(rows),records=rows,publicTransactionsSubmitted=False);Path(sys.argv[3]).write_text(json.dumps(r,separators=(',',':'))+'\n');print(json.dumps(dict(passed=True,cases=len(rows))))
 else:
  r=verify(sys.argv[1],int(sys.argv[3])if len(sys.argv)>3 else 8);Path(sys.argv[2]).write_text(json.dumps(r,separators=(',',':'))+'\n');print(json.dumps({k:r[k]for k in ['passed','index','input','output','minimumOutput','surplus']}))
