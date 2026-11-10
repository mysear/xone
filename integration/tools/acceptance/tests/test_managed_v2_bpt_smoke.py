"""Original BPT RPC serving preserves exact H and private fault injection."""
from pathlib import Path
import importlib
import copy
import json
import sys
import unittest
ROOT=Path(__file__).resolve().parents[4];sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
class BptServingTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.s=importlib.import_module('managed_v2_bpt_smoke')
        cls.raw=json.loads((ROOT/'integration/test/contracts/managed-v2-bpt.json').read_bytes())
        cls.thin=json.loads((ROOT/'integration/test/contracts/managed-v2-bpt-pools.json').read_bytes())
    def test_exact_original_control_getters_and_canonical_anchor(self):
        chain=self.s.ReplayChain(self.raw,self.thin,'settled');ident=next(iter(chain.frames));chain.select(ident);frame=chain.frames[ident]
        for key,result in frame['getters'].items():
            self.assertEqual(chain.rpc('eth_call',[{'to':key[0],'data':key[1]},chain.anchor()]),result)
        for key,result in frame['storage'].items():
            self.assertEqual(chain.rpc('eth_getStorageAt',[key[0],key[1],chain.anchor()]),result)
        with self.assertRaises(ValueError):chain.rpc('eth_getStorageAt',[next(iter(frame['storage']))[0],'0x9','latest'])
        self.assertEqual(chain.configs[0]['multi_asset']['token_policy'],'standard-bpt')
        self.assertIn('bpt',chain.configs[0]['multi_asset']['managed_v2'])
    def test_fault_serving_is_private_and_recovers(self):
        chain=self.s.ReplayChain(self.raw,self.thin,'scheduled');ident=next(iter(chain.frames));before=copy.deepcopy(chain.frames)
        chain.select(ident,fault='missing-peer-word');target=chain.fault_evidence
        with self.assertRaises(ValueError):chain.rpc(target['method'],[target['key'][0],target['key'][1],chain.anchor()])
        self.assertGreater(chain.fault_hits,0);chain.select(ident);self.assertEqual(before,chain.frames)

    def test_source_fault_clock_starts_only_when_poison_is_actually_read(self):
        chain=self.s.ReplayChain(self.raw,self.thin,'scheduled');ident=next(iter(chain.frames))
        chain.select(ident,fault='missing-peer-word')
        self.assertIsNone(chain.fault_started_at_unix_ms)
        target=chain.fault_evidence
        with self.assertRaises(ValueError):chain.rpc(target['method'],[target['key'][0],target['key'][1],chain.anchor()])
        self.assertIsInstance(chain.fault_started_at_unix_ms,int)
        started=chain.fault_started_at_unix_ms
        with self.assertRaises(ValueError):chain.rpc(target['method'],[target['key'][0],target['key'][1],chain.anchor()])
        self.assertEqual(chain.fault_started_at_unix_ms,started)
        chain.select(ident);self.assertIsNone(chain.fault_started_at_unix_ms)

    def test_late_503_is_rejected_even_if_query_started_within_deadline(self):
        from unittest import mock
        clock={'now':124.0};budgets=[]
        def fetch(remaining):
            budgets.append(remaining);clock['now']=126.0;return 503,{'error':'CHAIN_NOT_READY'}
        with mock.patch.object(self.s.time,'monotonic',side_effect=lambda:clock['now']),mock.patch.object(self.s.time,'sleep'):
            with self.assertRaises(ValueError):self.s.wait_for_withdrawal(fetch,100.0,25)
        self.assertEqual(budgets,[1.0])

    def test_withdrawal_deadline_uses_first_poison_monotonic_clock(self):
        from unittest import mock
        clock={'now':102.0}
        def fetch(remaining):
            self.assertEqual(remaining,23.0);clock['now']=103.0;return 503,{'error':'CHAIN_NOT_READY'}
        with mock.patch.object(self.s.time,'monotonic',side_effect=lambda:clock['now']):
            event=self.s.wait_for_withdrawal(fetch,100.0,25)
        self.assertEqual(event['latency_ms'],3000)

    def test_negative_quote_requires_expected_business_error_not_http_failure(self):
        case=next(c for c in self.raw['cases'] if c['id']=='controls/C1')
        for code,body in [(500,{'error':'INTERNAL_SERVER_ERROR'}),(503,{'error':'CHAIN_NOT_READY'}),(504,{'error':'BUDGET_EXHAUSTED'}),(429,{'error':'RATE_LIMITED'}),(400,{'error':'INVALID_REQUEST'}),(200,{'error':'NO_ROUTE'}),(404,{'error':'NO_ROUTE','quoteId':'unexpected'})]:
            with self.subTest(code=code,body=body),self.assertRaises(ValueError):self.s.validate_rejected_case(code,body,case)
        self.s.validate_rejected_case(404,{'error':'NO_ROUTE'},case)
        self.s.validate_rejected_case(400,{'error':'NO_EXECUTABLE_ALLOCATION'},case)
        zero=next(c for c in self.raw['cases'] if c['label']=='zero-underlying-input')
        self.s.validate_rejected_case(400,{'error':'INVALID_REQUEST'},zero)
