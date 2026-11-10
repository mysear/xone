"""Normal-process replay must reject unrecorded calls and changed fixed-H identity."""
import importlib
import json
from pathlib import Path
import sys
import unittest
TOOLS=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(TOOLS))

class PublicProcessTests(unittest.TestCase):
    def module(self):
        self.assertTrue((TOOLS/'r01_public_catalog_evm_process.py').exists(),'normal EVM process harness missing')
        return importlib.import_module('r01_public_catalog_evm_process')

    def test_family_capability_does_not_promote_lfj_or_velodrome(self):
        m=self.module()
        self.assertIsNone(m.normal_profile('lfj-liquidity-book'))
        self.assertIsNone(m.normal_profile('velodrome-v2'))
        self.assertEqual(m.normal_profile('uniswap-v3'),'uniswap-v3')
        self.assertEqual(m.normal_profile('aerodrome-v2'),'aerodrome-v2-volatile')

    def test_fixed_h_call_refuses_wrong_anchor_and_public_writes(self):
        m=self.module()
        root=m.BASE/'avalanche-07';rpc=m.FixedRPC(root,{},[])
        hash_=rpc.header['hash'];address='0x'+'12'*20
        self.assertEqual(rpc.dispatch('eth_chainId',[]),'0xa86a')
        with self.assertRaises(ValueError):rpc.dispatch('eth_sendRawTransaction',['0x12'])
        with self.assertRaises(ValueError):rpc.dispatch('eth_call',[{'to':address,'data':'0x12345678'},{'blockHash':'0x'+'00'*32,'requireCanonical':True}])
        with self.assertRaises(ValueError):rpc.dispatch('eth_call',[{'to':address,'data':'0x12345678'},{'blockHash':hash_,'requireCanonical':True}])

    def test_recent_only_endpoint_genesis_failure_keeps_exact_public_fallback(self):
        m=self.module();rpc=m.FixedRPC(m.BASE/'bsc-06',{},[])
        self.assertEqual(rpc.dispatch('eth_getBlockByNumber',['0x0',False])['hash'],'0x0d21840abff46b96c84b2ac9e10e4f5cdaeb5693cb665db62a2f3b02d2d57b5b')

    def test_preserved_failed_public_attempt_does_not_poison_success_cache(self):
        m=self.module()
        capture=m.load_json(m.BASE/'avalanche-uniswap-v3-normal-process-v1/capture.json')
        self.assertEqual(len(m.captured_calls(capture)),capture['call_count'])

    def test_actual_original_receipts_and_block_projection_remain_bound(self):
        m=self.module();rpc=m.FixedRPC(m.BASE/'avalanche-07',{},[])
        block=rpc.dispatch('eth_getBlockByNumber',['finalized',False])
        self.assertEqual(block['hash'],rpc.header['hash'])
        self.assertEqual(block['transactions'],[x['hash'] for x in rpc.header['transactions']])
        with self.assertRaises(ValueError):rpc.dispatch('eth_getBlockByNumber',['0x123',False])

if __name__=='__main__':unittest.main()
