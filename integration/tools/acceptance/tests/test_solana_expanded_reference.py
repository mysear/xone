from pathlib import Path
import copy,json,sys,unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import solana_expanded_reference as r
class ExpandedReferenceControls(unittest.TestCase):
 def test_token2022_withheld_amount_tamper_is_rejected(self):
  f=json.loads((r.ROOT/'integration/test/contracts/meteora-token2022.json').read_bytes());s=next(c['steps'][0]for c in f['cases']if c['steps'][0]['passed']);raw=bytearray.fromhex(s['after']['meteora']['token2022']['vault_data']);raw[170]^=1;s['after']['meteora']['token2022']['vault_data']=raw.hex()
  with self.assertRaisesRegex(ValueError,'full-projection:token2022'):r.verify_family('token2022',fixture=f)
 def test_order_filled_counter_tamper_is_rejected(self):
  f=json.loads((r.ROOT/'integration/test/contracts/meteora-orders.json').read_bytes());s=next(c['steps'][0]for c in f['cases']if c['steps'][0]['passed']);raw=bytearray.fromhex(s['after']['meteora']['bin_arrays'][0]['data']);raw[56+48]^=1;s['after']['meteora']['bin_arrays'][0]['data']=raw.hex()
  with self.assertRaisesRegex(ValueError,'full-projection:orders'):r.verify_family('orders',fixture=f)
 def test_pump_required_input_tamper_is_rejected(self):
  f=json.loads((r.ROOT/'integration/test/contracts/pump-sell-exact-out.json').read_bytes());c=next(c for c in f['cases']if c['passed']);c['required_input']=str(int(c['required_input'])+1)
  with self.assertRaisesRegex(ValueError,'pump-required-input'):r.verify_pump('sell-exact-out',fixture=f)
 def test_migration_handoff_cannot_substitute_an_unrelated_vault(self):
  e=json.loads((r.OUT/'core-pump-migration-original-20261111.json').read_bytes());key=e['migration']['migration_ids']['pool_base_token_account'];e['swaps'][0]['before'][key]['lamports']+=1
  with self.assertRaisesRegex(ValueError,'migration-created-account-handoff'):r.verify_migration(evidence=e)
if __name__=='__main__':unittest.main()
