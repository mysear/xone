import base64,copy,hashlib,importlib.util,json,sys,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import family_reference as reference
class FamilyReference(unittest.TestCase):
 def test_original_files_and_raw_balances_match(self):
  self.assertTrue(reference.verify_pump()['passed']);self.assertTrue(reference.verify_only_y()['passed'])
if __name__=='__main__':unittest.main()
