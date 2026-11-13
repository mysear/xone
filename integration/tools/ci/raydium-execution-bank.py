#!/usr/bin/env python3
"""Controlled user accounts around the retained original CP-Swap sourcebank."""
from pathlib import Path
import base64,copy,hashlib,json,subprocess,sys
ROOT=Path(__file__).resolve().parents[3]
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from r01_public_catalog_solana_bank import native_imports
TOKEN='TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'
WSOL='So11111111111111111111111111111111111111112'
def main():
 base=ROOT/'integration/test/source/deployment-catalog/solana-rebuild'
 normal=json.loads((base/'quantity-basis-v2/cpmm-normal-v2.json').read_bytes())
 pool=normal['parser_baseline']['body']['pools'][0]
 recorded=json.loads((base/'cpmm-sourcebank-correct/accounts.json').read_bytes())
 mapping=dict(zip(recorded['params'][0],recorded['response']['result']['value']))
 S,P,K,I,M,T,A,C=native_imports();vm=S().with_mainnet_features()
 caller=str(K.from_seed(hashlib.sha256(b'R01-public-native-bank-payer').digest()).pubkey())
 recipient=str(P.from_bytes(hashlib.sha256(b'R01-distinct-net-recipient').digest()))
 accounts={key:value for key,value in mapping.items()if value is not None}
 cases=[]
 for zero in [True,False]:
  bank=copy.deepcopy(accounts);source=str(P.from_bytes(hashlib.sha256(f'xone-cpmm-prep-input-{zero}'.encode()).digest()));destination=str(P.from_bytes(hashlib.sha256(f'xone-cpmm-prep-output-{zero}'.encode()).digest()))
  for key,mint,owner,amount in [(source,pool['token0']if zero else pool['token1'],caller,10**12),(destination,pool['token1']if zero else pool['token0'],recipient,7)]:
   raw=bytearray(165);raw[:32]=bytes(P.from_string(mint));raw[32:64]=bytes(P.from_string(owner));raw[64:72]=amount.to_bytes(8,'little');raw[108]=1
   rent=vm.minimum_balance_for_rent_exemption(165);lamports=rent
   if mint==WSOL:raw[109:113]=(1).to_bytes(4,'little');raw[113:121]=rent.to_bytes(8,'little');lamports+=amount
   bank[key]=dict(owner=TOKEN,executable=False,lamports=lamports,data=[base64.b64encode(raw).decode(),'base64'])
  bank[caller]=dict(owner='11111111111111111111111111111111',executable=False,lamports=10_000_000_000,data=['','base64'])
  binary=ROOT/'integration/.runtime/raydium-reference/target/debug/raydium-cpmm-reference'
  oracle_pool=copy.deepcopy(pool);oracle_pool['chain_key']='solana:localnet'
  native=json.loads(subprocess.check_output([binary,'quote-exact-out'],input=json.dumps(dict(pool=oracle_pool,output='1000',zero_for_one=zero)).encode(),stderr=subprocess.PIPE))
  if native.get('reverted'):raise ValueError(native)
  native['pool']['chain_key']=pool['chain_key']
  cases.append(dict(schema_version=1,authority='retained sourcebank with explicit controlled user balances; original native helper uses localnet chain label only for arithmetic projection, no state/amount changes; no historical consensus or production qualification',pool=pool,accounts=bank,caller=caller,recipient=recipient,zeroForOne=zero,sourceTokenAccount=source,destinationTokenAccount=destination,target='1000',requiredInput=native['amount_in'],after=native['pool'],genesis='5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d',blockhash=str(P.from_bytes(bytes([77])*32))))
 for service in ['integration','dex-router']:(ROOT/service/'test/contracts/raydium-execution-bank.json').write_text(json.dumps(dict(cases=cases),separators=(',',':'))+'\n')
 print(json.dumps(dict(passed=True,cases=len(cases),inputs=[c['requiredInput']for c in cases])))
if __name__=='__main__':main()
