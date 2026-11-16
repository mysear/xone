from pathlib import Path
import base64,copy,importlib.util,json,sys,unittest
ROOT=Path(__file__).resolve().parents[4];sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from r01_public_catalog_solana_bank import native_imports
class AdvancedWorker(unittest.TestCase):
 @classmethod
 def setUpClass(cls):
  spec=importlib.util.spec_from_file_location('worker',ROOT/'dex-router/crates/simulation/workers/solana_sbf.py');cls.worker=importlib.util.module_from_spec(spec);spec.loader.exec_module(cls.worker);cls.P=native_imports()[1];cls.fixture=json.loads((ROOT/'integration/test/contracts/rayclmm-advanced-execution-bank.json').read_bytes())
 def bank(self,i):
  b=self.fixture['cases'][i];return b['pool'],{**self.fixture['shared_accounts'],**b['accounts']}
 def check_mutation(self,i,role,offset,reason):
  p,a=self.bank(i);state=p['raydium_clmm'];key=p['pool_id']if role=='pool'else state['bitmap_extension']['address'];a[key]=copy.deepcopy(a[key]);data=bytearray(base64.b64decode(a[key]['data'][0]));data[offset]^=1;a[key]['data'][0]=base64.b64encode(data).decode()
  with self.assertRaisesRegex(ValueError,reason):self.worker.validate_rayclmm(p,a,self.P)
 def test_all162_original_advanced_models_are_valid(self):
  self.assertEqual(len(self.fixture['cases']),162)
  for i in range(162):
   with self.subTest(i=i):p,a=self.bank(i);self.worker.validate_rayclmm(p,a,self.P)
 def test_extension_discriminator_pool_and_both_bitmap_sides_are_bound(self):
  for i in [0,18,36,54,72,90,108,126,144]:
   for offset in [0,8,39,40,935,936,1831]:
    with self.subTest(i=i,offset=offset):self.check_mutation(i,'extension',offset,'SBF_RAYCLMM_EXTENSION_MISMATCH')
 def test_all_dynamic_fee_members_are_bound(self):
  for i in [108,126,144]:
   for offset in [1096,1098,1100,1102,1106,1110,1114,1118,1122,1129]:
    with self.subTest(i=i,offset=offset):self.check_mutation(i,'pool',offset,'SBF_RAYCLMM_POOL_MISMATCH')
 def test_dynamic_reserved_tail_is_not_reinterpreted_as_known_state(self):
  for offset in [1130,1175,1543]:self.check_mutation(108,'pool',offset,'SBF_RAYCLMM_VERSION')
 def test_custom_seed_bytes_are_bound(self):
  for i in [54,72,90,108,126,144]:self.check_mutation(i,'pool',391,'SBF_RAYCLMM_POOL_MISMATCH')
 def test_extension_absence_wrong_owner_and_executable_reject(self):
  for mode in ['missing','owner','executable','short','long']:
   p,a=self.bank(0);key=p['raydium_clmm']['bitmap_extension']['address'];a[key]=copy.deepcopy(a[key])
   if mode=='missing':a.pop(key)
   elif mode=='owner':a[key]['owner']='11111111111111111111111111111111'
   elif mode=='executable':a[key]['executable']=True
   else:
    data=base64.b64decode(a[key]['data'][0]);a[key]['data'][0]=base64.b64encode(data[:-1]if mode=='short'else data+b'\0').decode()
   with self.assertRaisesRegex(ValueError,'SBF_RAYCLMM_EXTENSION_MISMATCH'):self.worker.validate_rayclmm(p,a,self.P)
 def test_extension_model_width_is_explicit(self):
  for side in ['positive','negative']:
   p,a=self.bank(0);p=copy.deepcopy(p);p['raydium_clmm']['bitmap_extension'][side].pop()
   with self.assertRaisesRegex(ValueError,'SBF_RAYCLMM_EXTENSION_IDENTITY'):self.worker.validate_rayclmm(p,a,self.P)
 def test_seed_change_cannot_reuse_an_unrelated_pool_pda(self):
  p,a=self.bank(54);p=copy.deepcopy(p);p['raydium_clmm']['seed_index']+=1;key=p['pool_id'];a[key]=copy.deepcopy(a[key]);data=bytearray(base64.b64decode(a[key]['data'][0]));data[391:393]=p['raydium_clmm']['seed_index'].to_bytes(2,'little');a[key]['data'][0]=base64.b64encode(data).decode()
  with self.assertRaisesRegex(ValueError,'SBF_RAYCLMM_POOL_PDA'):self.worker.validate_rayclmm(p,a,self.P)
 def test_profile_downgrade_does_not_hide_extension_or_dynamic_state(self):
  for index in [0,54,108]:
   p,a=self.bank(index);p=copy.deepcopy(p);p['protocol']='raydium-clmm-static'
   with self.assertRaisesRegex(ValueError,'SBF_RAYCLMM_PROFILE'):self.worker.validate_rayclmm(p,a,self.P)
if __name__=='__main__':unittest.main()
