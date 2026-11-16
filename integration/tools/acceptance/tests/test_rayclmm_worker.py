from pathlib import Path
import base64,copy,importlib.util,json,sys,unittest
ROOT=Path(__file__).resolve().parents[4];sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from r01_public_catalog_solana_bank import native_imports
class RayClassicWorker(unittest.TestCase):
 @classmethod
 def setUpClass(cls):
  spec=importlib.util.spec_from_file_location('worker',ROOT/'dex-router/crates/simulation/workers/solana_sbf.py');cls.worker=importlib.util.module_from_spec(spec);spec.loader.exec_module(cls.worker);cls.P=native_imports()[1];cls.fixture=json.loads((ROOT/'integration/test/contracts/rayclmm-execution-bank.json').read_bytes())
 def bank(self,index):
  b=self.fixture['cases'][index];return b['pool'],{**self.fixture['shared_accounts'],**b['accounts']}
 def mutation(self,index,address,offset,reason):
  p,a=self.bank(index);key=address(p);a[key]=copy.deepcopy(a[key]);b=bytearray(base64.b64decode(a[key]['data'][0]));b[offset]^=1;a[key]['data'][0]=base64.b64encode(b).decode()
  with self.assertRaisesRegex(ValueError,reason):self.worker.validate_rayclmm(p,a,self.P)
 def test_all56_original_packed_models_pass(self):
  self.assertEqual(len(self.fixture['cases']),56)
  for i in range(56):
   with self.subTest(i=i):p,a=self.bank(i);self.worker.validate_rayclmm(p,a,self.P)
 def test_pool_economic_fields_are_bound(self):
  for i in [0,14,28,42]:
   for offset in [8,9,73,105,137,169,201,233,235,237,253,269,277,293,309,317,390,391,904,1064,1072,1080,1088]:
    with self.subTest(i=i,offset=offset):self.mutation(i,lambda p:p['pool_id'],offset,'SBF_RAYCLMM_POOL_MISMATCH')
 def test_unmodelled_pool_flags_dynamic_and_reserved_data_reject(self):
  for offset in [273,393,1096,1543]:self.mutation(0,lambda p:p['pool_id'],offset,'SBF_RAYCLMM_VERSION')
 def test_observation_fields_and_reserved_are_bound(self):
  for offset in [0,8,9,17,19,51,55,63,4451,4482]:self.mutation(0,lambda p:p['raydium_clmm']['observation'],offset,'SBF_RAYCLMM_OBSERVATION_MISMATCH')
 def test_tick_fields_and_orders_are_bound(self):
  for i in [0,14,28,42]:
   for offset in [0,8,40,48,64,80,96,112,10124,10125]:
    with self.subTest(i=i,offset=offset):self.mutation(i,lambda p:p['raydium_clmm']['tick_arrays'][0]['address'],offset,'SBF_RAYCLMM_ARRAY_MISMATCH')
   self.mutation(i,lambda p:p['raydium_clmm']['tick_arrays'][0]['address'],168,'SBF_RAYCLMM_ORDERS')
 def test_opaque_unused_padding_and_order_cohort_are_retained(self):
  p,a=self.bank(0)
  for key,offset in [(p['pool_id'],325),(p['pool_id'],1032),(p['raydium_clmm']['tick_arrays'][0]['address'],160)]:
   a[key]=copy.deepcopy(a[key]);d=bytearray(base64.b64decode(a[key]['data'][0]));d[offset]^=1;a[key]['data'][0]=base64.b64encode(d).decode()
  self.worker.validate_rayclmm(p,a,self.P)
 def test_vault_donation_is_retained_but_underbacking_is_rejected(self):
  p,a=self.bank(0);key=p['raydium_clmm']['vault0'];a[key]=copy.deepcopy(a[key]);a[key]['lamports']+=7;self.worker.validate_rayclmm(p,a,self.P);d=base64.b64decode(a[key]['data'][0]);a[key]['lamports']=int.from_bytes(d[113:121],'little')+int.from_bytes(d[64:72],'little')-1
  with self.assertRaisesRegex(ValueError,'SBF_RAYCLMM_VAULT_BACKING'):self.worker.validate_rayclmm(p,a,self.P)
 def test_unknown_profile_and_advanced_state_do_not_borrow_classic_execution(self):
  for name in ['raydium-clmm-bitmap','raydium-clmm-custom-seed','raydium-clmm-dynamic','unknown']:
   p,a=self.bank(0);p=copy.deepcopy(p);p['protocol']=name
   with self.assertRaisesRegex(ValueError,'SBF_RAYCLMM_PROFILE'):self.worker.validate_rayclmm(p,a,self.P)
if __name__=='__main__':unittest.main()
