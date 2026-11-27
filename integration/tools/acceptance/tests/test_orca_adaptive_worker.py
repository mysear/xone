from pathlib import Path
import base64,copy,hashlib,importlib.util,json,sys,unittest
ROOT=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
from r01_public_catalog_solana_bank import native_imports
class AdaptiveOracleWorker(unittest.TestCase):
 @classmethod
 def setUpClass(cls):
  spec=importlib.util.spec_from_file_location('adaptive_worker',ROOT/'dex-router/crates/simulation/workers/solana_sbf.py');cls.worker=importlib.util.module_from_spec(spec);spec.loader.exec_module(cls.worker);cls.P=native_imports()[1];cls.f=json.loads((ROOT/'integration/test/contracts/orca-adaptive-execution-bank.json').read_bytes())
 def value(self,index):
  b=self.f['cases'][index];return b['pool'],{**self.f['shared_accounts'],**b['accounts']}
 def test_all54_original_packed_states_are_valid(self):
  self.assertEqual(len(self.f['cases']),54)
  for i in range(54):
   with self.subTest(i=i):p,a=self.value(i);self.worker.validate_orca(p,a,self.P)
 def test_all_oracle_fields_and_reserved_ranges_are_bound(self):
  for i in range(54):
   for offset in [0,8,40,48,50,52,54,58,62,64,66,82,90,98,102,106,110,126,253]:
    with self.subTest(i=i,offset=offset):
     p,a=self.value(i);key=p['orca_whirlpool']['oracle'];a[key]=copy.deepcopy(a[key]);b=bytearray(base64.b64decode(a[key]['data'][0]));b[offset]^=1;a[key]['data'][0]=base64.b64encode(b).decode()
     with self.assertRaisesRegex(ValueError,'SBF_ORCA_ORACLE_MISMATCH'):self.worker.validate_orca(p,a,self.P)
 def test_oracle_presence_owner_and_executable_are_bound(self):
  for i in [0,18,36]:
   for mode in ['missing','owner','executable']:
    with self.subTest(i=i,mode=mode):
     p,a=self.value(i);key=p['orca_whirlpool']['oracle'];a[key]=copy.deepcopy(a[key])
     if mode=='missing':a.pop(key)
     elif mode=='owner':a[key]['owner']='11111111111111111111111111111111'
     else:a[key]['executable']=True
     with self.assertRaisesRegex(ValueError,'SBF_ORCA_ORACLE_MISMATCH'):self.worker.validate_orca(p,a,self.P)
 def test_profile_cannot_downgrade_initialized_oracle_to_static(self):
  for i in [0,18,36]:
   p,a=self.value(i);p=copy.deepcopy(p);p['protocol']='orca-whirlpools-static'
   with self.assertRaisesRegex(ValueError,'SBF_ORCA_PROFILE'):self.worker.validate_orca(p,a,self.P)
 def test_oracle_whirlpool_binding_is_required(self):
  p,a=self.value(0);p=copy.deepcopy(p);p['orca_whirlpool']['adaptive']['whirlpool']='11111111111111111111111111111111'
  with self.assertRaisesRegex(ValueError,'SBF_ORCA_ORACLE_BINDING'):self.worker.validate_orca(p,a,self.P)
 def test_reserved_arrays_cannot_be_truncated_or_extended(self):
  for name in ['constants_reserved','variables_reserved','reserved']:
   for change in ['short','long']:
    p,a=self.value(0);p=copy.deepcopy(p);b=p['orca_whirlpool']['adaptive'][name]
    if change=='short':b.pop()
    else:b.append(0)
    with self.assertRaisesRegex(ValueError,'SBF_ORCA_ORACLE_RESERVED_WIDTH'):self.worker.validate_orca(p,a,self.P)
class SparseArrayWorker(unittest.TestCase):
    def test_bank_requires_the_empty_system_account_at_the_derived_pda(self):
        spec = importlib.util.spec_from_file_location(
            'orca_sparse_worker', ROOT / 'dex-router/crates/simulation/workers/solana_sbf.py',
        )
        worker = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(worker)
        pubkey = native_imports()[1]
        from orca_adaptive_dynamic_fixture import corpus, pool

        scenario = corpus()['scenarios'][0]
        bank = scenario['banks'][0]
        model = pool(scenario['config'], bank['native_state'])
        state = model['orca_whirlpool']
        start = state['tick_arrays'][-1]['start_tick_index'] + 88 * state['tick_spacing']
        address, _ = pubkey.find_program_address(
            [b'tick_array', bytes(pubkey.from_string(model['pool_id'])), str(start).encode()],
            pubkey.from_string(worker.ORCA),
        )
        sparse = dict(address=str(address), start_tick_index=start, layout='sparse',
                      initialized_bitmap='0', ticks=[])
        state['tick_arrays'].append(sparse)
        observed = dict(owner=worker.SYSTEM, executable=False, lamports=1_000_000,
                        data=['', 'base64'])
        accounts = dict(bank['accounts'], **{str(address): observed})
        worker.validate_orca(model, accounts, pubkey)
        for invalid in [None, dict(observed, owner=worker.ORCA),
                        dict(observed, executable=True), dict(observed, data=['AQ==', 'base64'])]:
            with self.subTest(invalid=invalid), self.assertRaisesRegex(ValueError, 'SBF_ORCA_SPARSE_ARRAY_MISMATCH'):
                worker.validate_orca(model, dict(accounts, **{str(address): invalid}), pubkey)
        sparse['address'] = state['tick_arrays'][0]['address']
        with self.assertRaisesRegex(ValueError, 'SBF_ORCA_SPARSE_ARRAY_MISMATCH'):
            worker.validate_orca(model, accounts, pubkey)


if __name__=='__main__':unittest.main()
