"""Independent C++ field42 syntax, deliberately synthetic authority."""
import importlib.util
from pathlib import Path
import unittest
ROOT=Path(__file__).resolve().parents[4]
def codec():
 spec=importlib.util.spec_from_file_location('breaker_wire',ROOT/'integration/tools/ci/managed-v2-breaker-wire-golden.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m);return m
class ManagedBreakerWireTests(unittest.TestCase):
 def test_field42_full_words_known_zero_and_roundtrip(self):
  c=codec();v=c.build_fixture();raw=bytes.fromhex(v['protobuf_hex']);text=c.legacy._protoc('decode',raw)
  self.assertIn(b'circuit_breakers {',text);self.assertIn(b'lower_bpt_price_bound {',text)
  self.assertEqual(c.legacy._protoc('encode',text),raw);self.assertEqual(v['authority'],'synthetic-breaker-state-wire-contract-no-native-execution-proof')
 def test_missing_null_old_profile_and_raw_getter_contradictions_reject(self):
  c=codec()
  for key in ('state_word','reference_bpt_price','reference_weight','lower_percentage','upper_percentage','lower_bpt_price_bound','upper_bpt_price_bound'):
   for missing in (True,False):
    s=c.build_snapshot();t=s['pools'][0]['multi_asset']['managed_v2']['circuit_breakers']['tokens'][0]
    if missing:del t[key]
    else:t[key]=None
    with self.subTest(key=key,missing=missing),self.assertRaises(ValueError):c.encode_protobuf(s)
  for kind in range(5):
   s=c.build_snapshot();p=s['pools'][0];m=p['multi_asset']['managed_v2'];tokens=m['circuit_breakers']['tokens']
   if kind==0:p['protocol']='balancer-v2-managed-bpt'
   elif kind==1:tokens.reverse()
   elif kind==2:tokens[0]['reference_weight']='1'
   elif kind==3:m['token_info'][0]['circuit_breaker_enabled']=False
   else:tokens[0]['lower_bpt_price_bound']='0'
   with self.subTest(kind=kind),self.assertRaises(ValueError):c.encode_protobuf(s)
