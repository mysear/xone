import copy,sys,unittest
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import v2_complete_bank as m
class CompleteBank(unittest.TestCase):
 def test_actual_source_getters_include_raw_cumulative_lock_and_factory(self):
  b=m.NativeCompleteBank();p=b.case['pool'];s=p['v2']['swap'];anchor={'blockHash':b.header['hash'],'requireCanonical':True}
  def call(to,data):return b.rpc('eth_call',[{'to':to,'data':data},anchor])
  for selector,key in [('0x5909c0d5','price0_cumulative_last'),('0x5a3d5493','price1_cumulative_last')]:self.assertEqual(int(call(p['pool_id'],selector),16),int(s[key]))
  for token,key in [(p['token0'],'raw_balance0'),(p['token1'],'raw_balance1')]:self.assertEqual(int(call(token,'0x70a08231'+'0'*24+p['pool_id'][2:]),16),int(s[key]))
  self.assertEqual(int(b.rpc('eth_getStorageAt',[p['pool_id'],'0xc',anchor]),16),1)
  self.assertEqual(m.hashlib.sha256(bytes.fromhex(b.rpc('eth_getCode',[p['v2']['factory'],anchor])[2:])).hexdigest(),s['factory_code_sha256'])
 def test_archived_headers_receipts_and_all_source_banks_are_original(self):
  b=m.NativeCompleteBank();first=copy.deepcopy(b.header);b.select_case(2)
  self.assertEqual(b.rpc('eth_getBlockByNumber',[first['number'],False]),first)
  receipts=[b.rpc('eth_getTransactionReceipt',[tx]) for tx in b.header['transactions']]
  self.assertTrue(receipts);self.assertTrue(all(r['blockHash']==b.header['hash'] for r in receipts))
  self.assertEqual(b.rpc('eth_getBlockByNumber',['0x0',False])['hash'],b.case['state_anchor']['genesis_hash'])
 def test_unknown_anchor_slot_and_dependency_faults_cannot_default_to_zero(self):
  b=m.NativeCompleteBank();p=b.case['pool'];a={'blockHash':b.header['hash'],'requireCanonical':True}
  for wrong in ['latest',{'blockHash':b.header['hash']},{'blockHash':'0x'+'00'*32,'requireCanonical':True}]:
   with self.assertRaises(ValueError):b.rpc('eth_getCode',[p['pool_id'],wrong])
  with self.assertRaises(ValueError):b.rpc('eth_getStorageAt',[p['pool_id'],'0xffff',a])
  b.set_fault('pool-code');self.assertEqual(b.rpc('eth_getCode',[p['pool_id'],a]),'0x00');b.set_fault('factory-getter');self.assertEqual(int(b.rpc('eth_call',[{'to':p['pool_id'],'data':'0xc45a0155'},a]),16),0)
  b.set_fault('lock');self.assertEqual(int(b.rpc('eth_getStorageAt',[p['pool_id'],'0xc',a]),16),0)
 def test_bundle_only_declares_known_h_and_parent_hashes_and_numeric_context(self):
  b=m.NativeCompleteBank();v=b.bundle();self.assertEqual(set(v),{'schema_version','fixture','executor','transaction_context'})
  self.assertEqual(v['fixture']['accounts'],b.case['accounts']);self.assertEqual(v['fixture']['state_anchor'],b.case['state_anchor']);self.assertEqual(v['fixture']['block_hashes'][0],{'number':b.number,'hash':b.header['hash']});self.assertEqual(v['fixture']['block_hashes'][1],{'number':b.number-1,'hash':b.header['parentHash']})
  self.assertIs(type(v['transaction_context']['gas_limit']),int);self.assertEqual(v['executor']['target'],b.case['transaction']['to'])
if __name__=='__main__':unittest.main()
