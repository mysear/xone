"""Adversarial checks for Crypto process fixture identity and anchored RPC reads."""
import copy,importlib.util,sys,unittest
from pathlib import Path
TOOLS=Path(__file__).resolve().parents[1];sys.path.insert(0,str(TOOLS))
import curve_crypto_smoke as m
class CryptoReplay(unittest.TestCase):
 def test_complete_runtime_immutable_and_source_bank(self):
  chain=m.Chain();bank=chain.banks[chain.head];anchor={'blockHash':bank['hash'],'requireCanonical':True}
  self.assertEqual(len(chain.groups[0]),15)
  for item in chain.items:
   p=item['pool'];c=p['multi_asset']['curve_crypto'];runtime=bytes.fromhex(chain.rpc('eth_getCode',[p['pool_id'],anchor])[2:]);self.assertEqual(len(runtime),20974);self.assertEqual(m.sha(runtime),p['multi_asset']['code_sha256']);self.assertEqual(runtime[20814:20846],int(c['lp_token'],16).to_bytes(32,'big'));self.assertEqual(int(chain.rpc('eth_call',[{'to':c['lp_token'],'data':'0x07546172'},anchor]),16),int(p['pool_id'],16));self.assertEqual(int(chain.rpc('eth_getStorageAt',[p['pool_id'],'0x19',anchor]),16),int(c['invariant']))
 def test_unknown_and_noncanonical_bank_fails(self):
  chain=m.Chain();p=chain.items[0]['pool'];c=p['multi_asset']['curve_crypto']
  for anchor in ['latest',{'blockHash':c['block_hash']},{'blockHash':'0x'+'0'*64,'requireCanonical':True}]:
   with self.assertRaises(ValueError):chain.rpc('eth_getCode',[p['pool_id'],anchor])
 def test_faults_change_actual_parser_dependencies(self):
  chain=m.Chain()
  for fault in m.FAULTS:
   chain.advance(0,fault);bank=chain.banks[chain.head];p=chain.items[0]['pool'];c=p['multi_asset']['curve_crypto'];anchor={'blockHash':bank['hash'],'requireCanonical':True}
   if fault in ['template','lp-code']:
    target=p['pool_id']if fault=='template'else c['lp_token'];expected=p['multi_asset']['code_sha256']if fault=='template'else c['lp_token_code_sha256'];self.assertNotEqual(m.sha(bytes.fromhex(chain.rpc('eth_getCode',[target,anchor])[2:])),expected)
   elif fault=='minter':self.assertNotEqual(int(chain.rpc('eth_call',[{'to':c['lp_token'],'data':'0x07546172'},anchor]),16),int(p['pool_id'],16))
   elif fault=='ramp':self.assertGreater(int(chain.rpc('eth_getStorageAt',[p['pool_id'],'0x8',anchor]),16),1)
   else:self.assertEqual(int(chain.rpc('eth_call',[{'to':p['token0'],'data':'0x70a08231'+'0'*24+p['pool_id'][2:]},anchor]),16),0)
 def test_original_replay_inventory_keeps_success_revert_and_poststate(self):
  chain=m.Chain();accepted=reverted=0
  for group in chain.groups:
   for item in group:
    if item['expected'].get('reverted'):reverted+=1
    else:
     accepted+=1
     for key in ['state','balances','raw_balances','total_supply','receiver_balance']:self.assertIn(key,item['expected'])
  self.assertEqual((accepted,reverted),(20,10))
if __name__=='__main__':unittest.main()
