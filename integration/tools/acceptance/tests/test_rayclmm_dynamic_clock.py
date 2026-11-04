"""A slow smoke must produce fresh next banks without changing the 180s limit."""
import base64
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import rayclmm_dynamic_smoke as smoke

class DynamicSmokeClock(unittest.TestCase):
 def test_new_solana_bank_refreshes_actual_clock_after_181_seconds(self):
  initial=1_793_245_486
  with patch.object(smoke.time,'time',return_value=initial):chain=smoke.Chain()
  before=chain.pool()
  with patch.object(smoke.time,'time',return_value=initial+181):chain.advance()
  clock=base64.b64decode(chain.accounts()[smoke.CLOCK]['data'][0])
  self.assertEqual(int.from_bytes(clock[:8],'little'),chain.head)
  self.assertEqual(int.from_bytes(clock[32:40],'little'),initial+181)
  self.assertEqual(chain.block(chain.head)['blockTime'],initial+181)
  self.assertEqual(chain.pool(),before)
  smoke.validate_time_policy(chain)
 def test_evm_control_gets_a_fresh_next_block_with_same_pools(self):
  initial=1_793_245_486_000
  with tempfile.TemporaryDirectory() as directory:
   root=Path(directory);smoke.sample_for('evm:31337',root,initial)
   file=root/'evm-31337.json';before=json.loads(file.read_text())
   smoke.advance_evm_fixture(root,initial+181_000)
   after=json.loads(file.read_text());old=before['reference']['block'];new=after['reference']['block']
   self.assertEqual(new['number_or_slot'],old['number_or_slot']+1)
   self.assertEqual(new['parent_hash'],old['hash'])
   self.assertNotEqual(new['hash'],old['hash'])
   self.assertEqual(new['timestamp_ms'],initial+181_000)
   self.assertEqual(after['pools'],before['pools'])
   self.assertEqual({k:v for k,v in after['reference'].items() if k!='block'},{k:v for k,v in before['reference'].items() if k!='block'})
if __name__=='__main__':unittest.main()
