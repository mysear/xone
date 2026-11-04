"""Corrupt-WAL startup rejection must be bound to the current process log."""
from pathlib import Path
import json
import sys
import unittest
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
from source_wal_smoke import corrupt_startup_rejected

def event(error):return (json.dumps({'event':'service_start_failed','fields':{'error_code':error}})+'\n').encode()
class SourceWalStartup(unittest.TestCase):
 def test_current_corruption_and_exit_one_are_required(self):
  old=event('SOURCE_WAL_CORRUPT');new=event('RPC_NETWORK')
  self.assertFalse(corrupt_startup_rejected(1,old+new,len(old)))
  self.assertFalse(corrupt_startup_rejected(1,old,len(old)))
  self.assertTrue(corrupt_startup_rejected(1,new+old,len(new)))
  for code in [0,2,-9,None]:self.assertFalse(corrupt_startup_rejected(code,old,0))
 def test_unstructured_or_different_event_cannot_fake_failure(self):
  for raw in [b'SOURCE_WAL_CORRUPT\n',b'{bad json\n',(json.dumps({'event':'other','fields':{'error_code':'SOURCE_WAL_CORRUPT'}})+'\n').encode()]:
   self.assertFalse(corrupt_startup_rejected(1,raw,0))
if __name__=='__main__':unittest.main()
