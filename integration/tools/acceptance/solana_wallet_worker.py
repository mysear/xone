#!/usr/bin/env python3
"""Controlled test wallet and loopback RPC execution only; never production signing."""
import base64,hashlib,json,sys
from solana_prepare_simulation_worker import native_imports
TOKEN='TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA';TOKEN22='TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb'
def main(request):
 S,P,K,I,M,T,A,C=native_imports();encoded=request['transaction'];tx=T.from_bytes(base64.b64decode(encoded,validate=True));caller=str(tx.message.account_keys[0])
 if request['operation']=='sign':
  keys=[K.from_seed(hashlib.sha256(seed).digest())for seed in [b'xone-meteora-reference-payer',b'R01-public-native-bank-payer',b'xone-pump-amm-user',b'xone-pump-trading-user']];key=next((k for k in keys if str(k.pubkey())==caller),None)
  if key is None:raise ValueError('UNKNOWN_CONTROLLED_TEST_PAYER')
  if bytes(tx.signatures[0])!=bytes(64):raise ValueError('EXPECTED_UNSIGNED_TEST_INPUT')
  tx.sign([key],tx.message.recent_blockhash);return dict(signedTransaction=base64.b64encode(bytes(tx)).decode(),signature=str(tx.signatures[0]))
 bank=request['bank'];svm=S().with_mainnet_features().with_blockhash_check(False).with_sigverify(True)
 target='CPMMoo8L3F4NbTegBCKVNunggL7H1ZpdTHKxQB5qKP1C'if bank['pool']['protocol']=='raydium-cpmm-creator'else'LBUZKhRxPF3XUpBCjp4YzTKgLccjZhTSDM9YuVaPwxo'
 if bank['pool']['protocol'].startswith('pump-amm-classic-wsol'):target='pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA'
 if bank['pool']['protocol']=='pump-bonding-sol-classic':target='6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P'
 programs=[target,TOKEN]
 if target in ['pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA','6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P']:programs.append('pfeeUxB6jkeY1Hxd7CsFCAjcbHA9rWtchMGdZ6VojVZ')
 if bank['pool'].get('meteora',{}).get('token2022'):programs.append(TOKEN22)
 for pid in programs:
  raw=base64.b64decode(bank['accounts'][pid]['data'][0]);pd=str(P.from_bytes(raw[4:36]));svm.add_program_with_loader(P.from_string(pid),base64.b64decode(bank['accounts'][pd]['data'][0])[45:],P.from_string('BPFLoaderUpgradeab1e11111111111111111111111'))
 for key,a in bank['accounts'].items():svm.set_account(P.from_string(key),A(a['lamports'],base64.b64decode(a['data'][0]),P.from_string(a['owner']),a['executable'],0))
 clock=base64.b64decode(bank['accounts']['SysvarC1ock11111111111111111111111111111111']['data'][0]);slot=int.from_bytes(clock[:8],'little');svm.set_clock(C(slot=slot,epoch_start_timestamp=int.from_bytes(clock[8:16],'little',signed=True),epoch=int.from_bytes(clock[16:24],'little'),leader_schedule_epoch=int.from_bytes(clock[24:32],'little'),unix_timestamp=int.from_bytes(clock[32:40],'little',signed=True)))
 if request.get('mode')=='send_fail':
  source=bank['destinationTokenAccount']if bank['sourceTokenAccount']==bank['caller']else bank['sourceTokenAccount'];a=svm.get_account(P.from_string(source));data=bytearray(a.data);data[108]=2;svm.set_account(P.from_string(source),A(a.lamports,bytes(data),a.owner,False,0))
 keys=list(map(str,tx.message.account_keys))
 def snapshot():
  balances=[];tokens=[]
  for i,key in enumerate(keys):
   a=svm.get_account(P.from_string(key));balances.append(0 if a is None else a.lamports)
   if a is not None and str(a.owner)in[TOKEN,TOKEN22]and len(a.data)>=165:
    data=a.data;tokens.append(dict(accountIndex=i,mint=str(P.from_bytes(data[:32])),owner=str(P.from_bytes(data[32:64])),uiTokenAmount=dict(amount=str(int.from_bytes(data[64:72],'little')),decimals=0,uiAmount=None,uiAmountString='not-used')))
  return balances,tokens
 before,pre_tokens=snapshot();native=json.loads(svm.send_transaction(tx).to_json());after,post_tokens=snapshot();meta=native.get('meta',native)
 return dict(signature=str(tx.signatures[0]),transaction=[encoded,'base64'],slot=slot,version='legacy',meta=dict(err=native.get('err'),fee=meta['fee'],preBalances=before,postBalances=after,preTokenBalances=pre_tokens,postTokenBalances=post_tokens,logMessages=meta['logs'],computeUnitsConsumed=meta['compute_units_consumed']))
if __name__=='__main__':print(json.dumps(main(json.loads(sys.stdin.buffer.read(16*1024*1024))),separators=(',',':')))
