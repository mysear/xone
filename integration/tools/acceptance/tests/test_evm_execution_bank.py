import copy,io,sys,unittest
from unittest.mock import patch
from pathlib import Path
sys.path.insert(0,str(Path(__file__).resolve().parents[1]))
import evm_execution_smoke as m
class OriginalBank(unittest.TestCase):
    def test_source_header_and_receipts_are_actual_pre_execution_bank(self):
        bank=m.NativeBank();header=bank.rpc('eth_getBlockByNumber',['latest',False]);self.assertEqual(header['hash'],bank.case['state_anchor']['block_hash'])
        receipts=bank.rpc('eth_getBlockReceipts',[hex(bank.number)])
        self.assertEqual([r['transactionHash']for r in receipts],header['transactions'])
        self.assertTrue(all(r['blockHash']==header['hash']for r in receipts))
        self.assertEqual(bank.rpc('eth_getBlockByNumber',['0x0',False])['hash'],bank.case['state_anchor']['genesis_hash'])
    def test_configured_receipt_path_is_the_actual_transaction_receipt_rpc(self):
        import evm_execution_evidence as evidence
        self.assertIn('eth_getTransactionReceipt', evidence.RPC_READS)
        bank=m.NativeBank();header=bank.rpc('eth_getBlockByNumber',['latest',False])
        receipts=[bank.rpc('eth_getTransactionReceipt',[tx]) for tx in header['transactions']]
        self.assertTrue(receipts)
        self.assertEqual([r['transactionHash'] for r in receipts],header['transactions'])
        self.assertTrue(all(r['blockHash']==header['hash'] for r in receipts))

    def test_pinned_v2_getters_read_actual_recorded_storage(self):
        bank=m.NativeBank();p=bank.case['pool'];anchor={'blockHash':bank.header['hash'],'requireCanonical':True}
        def call(selector):return bank.rpc('eth_call',[{'to':p['pool_id'],'data':selector},anchor])
        self.assertEqual('0x'+call('0xc45a0155')[-40:],p['v2']['factory'])
        self.assertEqual('0x'+call('0x0dfe1681')[-40:],p['token0']);self.assertEqual('0x'+call('0xd21220a7')[-40:],p['token1'])
        data=call('0x0902f1ac')[2:];self.assertEqual(int(data[:64],16),int(p['reserve0']));self.assertEqual(int(data[64:128],16),int(p['reserve1']))
        self.assertEqual(m.hashlib.sha256(bytes.fromhex(bank.rpc('eth_getCode',[p['pool_id'],anchor])[2:])).hexdigest(),p['v2']['code_sha256'])
    def test_source_dependency_faults_and_unknown_contexts_fail_closed(self):
        bank=m.NativeBank();p=bank.case['pool'];anchor={'blockHash':bank.header['hash'],'requireCanonical':True}
        original=bank.rpc('eth_getCode',[p['pool_id'],anchor]);bank.set_fault('pool-code');self.assertNotEqual(original,bank.rpc('eth_getCode',[p['pool_id'],anchor]))
        bank.set_fault('factory-getter');self.assertEqual(int(bank.rpc('eth_call',[{'to':p['pool_id'],'data':'0xc45a0155'},anchor]),16),0)
        for bad in ['latest',{'blockHash':bank.header['hash']},{'blockHash':'0x'+'00'*32,'requireCanonical':True}]:
            with self.assertRaises(ValueError):bank.rpc('eth_getCode',[p['pool_id'],bad])
        with self.assertRaises(ValueError):bank.rpc('eth_getBlockByNumber',['0xffff',False])
    def test_faults_advance_to_real_native_banks_and_preserve_archive_headers(self):
        bank=m.NativeBank();first=copy.deepcopy(bank.header)
        for index in [1,2]:
            bank.select_case(index,'pool-code')
            self.assertGreater(bank.number,int(first['number'],16))
            self.assertEqual(bank.rpc('eth_getBlockByNumber',[first['number'],False]),first)
            self.assertEqual(bank.rpc('eth_getBlockByNumber',['latest',False])['hash'],bank.case['state_anchor']['block_hash'])
            bank.set_fault(None)

    def test_bundle_uses_only_strict_source_fields_and_actual_numeric_gas_context(self):
        bank=m.NativeBank();bundle=bank.bundle()
        self.assertEqual(set(bundle),{'schema_version','fixture','executor','transaction_context'})
        self.assertEqual(bundle['fixture']['execution_environment']['block_number'],bundle['fixture']['state_anchor']['block_number']+1)
        self.assertIs(type(bundle['transaction_context']['gas_limit']),int)
        self.assertEqual(bundle['fixture']['accounts'],bank.case['accounts'])
        bundle['fixture']['accounts'].pop();self.assertNotEqual(bundle['fixture']['accounts'],bank.case['accounts'])
    def test_simulation_client_waits_for_configured_server_budget(self):
        response=io.BytesIO(b'{"ok":true}');response.status=200
        with patch('smoke.urllib.request.urlopen',return_value=response) as opening:
            self.assertEqual(m.http('http://127.0.0.1/simulate','token',{},timeout=35),(200,{'ok':True}))
        self.assertEqual(opening.call_args.kwargs['timeout'],35)
        response=io.BytesIO(b'{}');response.status=200
        with patch('smoke.urllib.request.urlopen',return_value=response) as opening:
            m.http('http://127.0.0.1/health')
        self.assertEqual(opening.call_args.kwargs['timeout'],3)
if __name__=='__main__':unittest.main()
