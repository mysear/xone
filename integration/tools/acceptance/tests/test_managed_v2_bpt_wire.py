"""Independent C++ wire syntax; synthetic state never claims VM authority."""
import importlib.util
from pathlib import Path
import unittest
ROOT=Path(__file__).resolve().parents[4]
def codec():
    spec=importlib.util.spec_from_file_location('managed_bpt_wire',ROOT/'integration/tools/ci/managed-v2-bpt-wire-golden.py')
    module=importlib.util.module_from_spec(spec);spec.loader.exec_module(module);return module
class BptWireTests(unittest.TestCase):
    def test_cxx_roundtrip_retains_explicit_false_zero_and_separate_field41(self):
        c=codec();s=c.build_snapshot();raw=c.encode_protobuf(s);text=c.legacy._protoc('decode',raw)
        self.assertIn(b'bpt {',text);self.assertIn(b'allowed: false',text);self.assertIn(b'bpt_last_change_block: 0',text)
        self.assertEqual(c.legacy._protoc('encode',text),raw)
        f=c.build_fixture();self.assertEqual(f['authority'],'synthetic-bpt-state-wire-contract-no-native-execution-proof')
    def test_bpt_controls_are_required_and_old_profile_remains_isolated(self):
        c=codec()
        for key in ('join_exit_enabled','minimum_bpt_locked','bpt_last_change_block','allowlist_members'):
            for missing in (True,False):
                s=c.build_snapshot();b=s['pools'][0]['multi_asset']['managed_v2']['bpt']
                if missing:del b[key]
                else:b[key]=None
                with self.subTest(key=key,missing=missing),self.assertRaises(ValueError):c.encode_protobuf(s)
        for kind in range(7):
            s=c.build_snapshot();p=s['pools'][0];b=p['multi_asset']['managed_v2']['bpt']
            if kind==0:p['protocol']='balancer-v2-managed-scheduled'
            elif kind==1:b['minimum_bpt_locked']='0'
            elif kind==2:b['bpt_last_change_block']=2**32
            elif kind==3:b['join_exit_enabled']=False
            elif kind==4:b['allowlist_members']*=2
            elif kind==5:del b['allowlist_members'][0]['allowed']
            else:b['unknown']=False
            with self.subTest(kind=kind),self.assertRaises(ValueError):c.encode_protobuf(s)
