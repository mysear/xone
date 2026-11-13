#!/usr/bin/env python3
"""Execute Router-produced CP-Swap wire in the retained original SBF, locally only."""
from pathlib import Path
import base64,copy,hashlib,json,subprocess,sys
from r01_public_catalog_solana_bank import native_imports
from solana_expanded_reference import ROOT,require
from solana_prepare_simulation_worker import simulate
PROGRAM='CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C';TOKEN='TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'
def verify(path,index=0):
 doc=json.loads(Path(path).read_bytes());p=doc.get('preparation',doc)
 b=json.loads((ROOT/'integration/test/contracts/raydium-execution-bank.json').read_bytes())['cases'][index];pool=b['pool'];state=pool['raydium_cpmm'];zero=b['zeroForOne']
 require(p['ready']is True and p['signed']is False and p['submitted']is False,'raydium-unsigned')
 require(hashlib.sha256(json.dumps({k:v for k,v in p.items()if k!='preparationHash'},sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()).hexdigest()==p['preparationHash'],'raydium-preparation-hash')
 S,P,K,I,M,T,A,C=native_imports();wire=base64.b64decode(p['unsignedTransaction'],validate=True);tx=T.from_bytes(wire)
 require(wire[0]==1 and wire[1:65]==bytes(64)and wire[65:]==base64.b64decode(p['messageBase64'])and tx.message.header.num_required_signatures==1 and str(tx.message.account_keys[0])==b['caller'],'raydium-wire-signer')
 require(len(tx.message.instructions)==len(p['instructions'])==1,'raydium-instruction-count')
 descriptor=p['instructions'][0];ix=tx.message.instructions[0];keys=tx.message.account_keys
 require(str(keys[ix.program_id_index])==descriptor['programId']==PROGRAM and bytes(ix.data)==base64.b64decode(descriptor['dataBase64'])and [str(keys[i])for i in ix.accounts]==[a['pubkey']for a in descriptor['accounts']],'raydium-wire-descriptor')
 caller=b['caller'];src=b['sourceTokenAccount'];dst=b['destinationTokenAccount'];required=int(b['requiredInput']);target=int(b['target']);maximum=(required*10050+9999)//10000
 observation=str(P.find_program_address([b'observation',bytes(P.from_string(pool['pool_id']))],P.from_string(PROGRAM))[0])
 expected=[(caller,True,True),(state['authority'],False,False),(state['amm_config'],False,False),(pool['pool_id'],False,True),(src,False,True),(dst,False,True),(state['vault0']if zero else state['vault1'],False,True),(state['vault1']if zero else state['vault0'],False,True),(TOKEN,False,False),(TOKEN,False,False),(pool['token0']if zero else pool['token1'],False,False),(pool['token1']if zero else pool['token0'],False,False),(observation,False,True)]
 require([(a['pubkey'],a['isSigner'],a['isWritable'])for a in descriptor['accounts']]==expected,'raydium-context-roles')
 discriminator=hashlib.sha256(b'global:swap_base_output').digest()[:8]
 require(bytes(ix.data)==discriminator+maximum.to_bytes(8,'little')+target.to_bytes(8,'little'),'raydium-output-driven-instruction')
 require(p['requiredInput']==str(maximum)and p['quotedInput']==str(required)and p['quotedOutput']==str(target)and p['feeLamports']==p['totalFundingLamports']=='5000'and p['rentLamports']=='0'and p['sourceTokenAccount']==src and p['destinationTokenAccount']==dst,'raydium-context-amounts')
 rights={caller:(True,True),PROGRAM:(False,False)}
 for key,sign,writable in expected:
  old=rights.get(key,(False,False));rights[key]=(old[0]or sign,old[1]or writable)
 keyset=list(map(str,keys));h=tx.message.header
 require(len(set(keyset))==len(keyset)and set(keyset)==set(rights),'raydium-key-set')
 for i,key in enumerate(keyset):
  signed=i<h.num_required_signatures;writable=i<(h.num_required_signatures-h.num_readonly_signed_accounts if signed else len(keyset)-h.num_readonly_unsigned_accounts)
  require(rights[key]==(signed,writable),'raydium-effective-permissions')
 sim=simulate(dict(bank=b,rpc=dict(method='simulateTransaction',params=[p['unsignedTransaction'],dict(sigVerify=False,replaceRecentBlockhash=False,accounts={'addresses':[src,dst]})])))
 require(sim['value']['err']is None,'raydium-readonly-original-simulation')
 signer=K.from_seed(hashlib.sha256(b'R01-public-native-bank-payer').digest());require(str(signer.pubkey())==caller,'raydium-test-signer')
 def execute(control=False):
  svm=S().with_mainnet_features().with_blockhash_check(False)
  for pid in [PROGRAM,TOKEN]:
   raw=base64.b64decode(b['accounts'][pid]['data'][0]);pd=str(P.from_bytes(raw[4:36]));svm.add_program_with_loader(P.from_string(pid),base64.b64decode(b['accounts'][pd]['data'][0])[45:],P.from_string('BPFLoaderUpgradeab1e11111111111111111111111'))
  for key,a in b['accounts'].items():svm.set_account(P.from_string(key),A(a['lamports'],base64.b64decode(a['data'][0]),P.from_string(a['owner']),a['executable'],0))
  raw=base64.b64decode(b['accounts']['SysvarC1ock11111111111111111111111111111111']['data'][0]);svm.set_clock(C(slot=int.from_bytes(raw[:8],'little'),epoch_start_timestamp=int.from_bytes(raw[8:16],'little',signed=True),epoch=int.from_bytes(raw[16:24],'little'),leader_schedule_epoch=int.from_bytes(raw[24:32],'little'),unix_timestamp=int.from_bytes(raw[32:40],'little',signed=True)))
  signed=T.from_bytes(wire)
  if control:
   raw=bytearray(ix.data);raw[8:16]=(required-1).to_bytes(8,'little');signed=T.new_signed_with_payer([I(P.from_string(PROGRAM),bytes(raw),[M(P.from_string(k),s,w)for k,s,w in expected])],signer.pubkey(),[signer],tx.message.recent_blockhash)
  else:signed.sign([signer],tx.message.recent_blockhash)
  def capture():
   result={}
   for key in sorted(set(b['accounts'])|set(keyset)):
    a=svm.get_account(P.from_string(key));result[key]=None if a is None else dict(lamports=a.lamports,owner=str(a.owner),executable=a.executable,data_base64=base64.b64encode(a.data).decode())
   return result
  before=capture();native=json.loads(svm.send_transaction(signed).to_json());return dict(passed='err'not in native,before=before,after=capture(),native=native)
 n=execute();require(n['passed'],'raydium-original-execution:'+str(n['native']))
 amount=lambda a:int.from_bytes(base64.b64decode(a['data_base64'])[64:72],'little')
 require(amount(n['before'][src])-amount(n['after'][src])==required and amount(n['after'][dst])-amount(n['before'][dst])==target,'raydium-real-user-debit-credit')
 require(n['before'][caller]['lamports']-n['after'][caller]['lamports']==5000,'raydium-real-payer-fee')
 post=b['after']['raydium_cpmm'];raw=bytearray(base64.b64decode(n['before'][pool['pool_id']]['data_base64']))
 for offset,key in [(341,'protocol_fees0'),(349,'protocol_fees1'),(357,'fund_fees0'),(365,'fund_fees1'),(381,'recent_epoch'),(397,'creator_fees0'),(405,'creator_fees1')]:raw[offset:offset+8]=post[key].to_bytes(8,'little')
 require(base64.b64decode(n['after'][pool['pool_id']]['data_base64'])==raw,'raydium-full-pool-state')
 for side in [0,1]:require(amount(n['after'][state[f'vault{side}']])==post[f'raw_vault{side}'],'raydium-raw-vault-ledgers')
 # Original update accumulates within the current observation even below the
 # fifteen-second ring-advance boundary. Invoke it instead of assuming a no-op.
 oracle_pool=copy.deepcopy(pool);oracle_pool['chain_key']='solana:localnet'
 binary=ROOT/'integration/.runtime/raydium-reference/target/debug/raydium-cpmm-reference'
 expected_oracle=json.loads(subprocess.check_output([binary,'oracle-after'],input=json.dumps(dict(pool=oracle_pool,data=list(base64.b64decode(n['before'][observation]['data_base64'])))).encode(),stderr=subprocess.PIPE))
 require('data'in expected_oracle and bytes(expected_oracle['data'])==base64.b64decode(n['after'][observation]['data_base64']),'raydium-full-original-oracle-post-state')
 totals={}
 for phase,sign in [('before',-1),('after',1)]:
  for a in n[phase].values():
   if a is None or a['owner']!=TOKEN:continue
   raw=base64.b64decode(a['data_base64'])
   if len(raw)==165:totals[raw[:32]]=totals.get(raw[:32],0)+sign*int.from_bytes(raw[64:72],'little')
 require(all(v==0 for v in totals.values()),'raydium-token-conservation')
 control=execute(True);require(not control['passed'],'raydium-input-minus-one')
 for key,a in control['before'].items():
  if key!=caller:require(a==control['after'][key],'raydium-all-account-rollback')
 require(control['before'][caller]['lamports']-control['after'][caller]['lamports']==5000,'raydium-failed-fee-only')
 return dict(passed=True,index=index,input=str(required),output=str(target),original=n,control=control,simulation=sim,publicTransactionsSubmitted=False,productionReady=False)
if __name__=='__main__':
 r=verify(sys.argv[1],int(sys.argv[3])if len(sys.argv)>3 else 0);Path(sys.argv[2]).write_text(json.dumps(r,separators=(',',':'))+'\n');print(json.dumps({k:r[k]for k in ['passed','index','input','output','publicTransactionsSubmitted']}))
