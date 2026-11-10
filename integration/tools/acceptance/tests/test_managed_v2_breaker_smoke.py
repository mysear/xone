"""Exact original breaker RPC plus strict business and first-poison deadlines."""
from pathlib import Path
import sys
import unittest
ROOT=Path(__file__).resolve().parents[4];sys.path.insert(0,str(ROOT/'integration/tools/acceptance'))
import managed_v2_bpt_smoke as old
import managed_v2_breaker_smoke as current
import importlib.util
spec=importlib.util.spec_from_file_location('prior_bpt_serving_contracts',Path(__file__).with_name('test_managed_v2_bpt_smoke.py'));prior=importlib.util.module_from_spec(spec);spec.loader.exec_module(prior)
class ManagedBreakerServingTests(prior.BptServingTests):
 @classmethod
 def setUpClass(cls):
  import json
  cls.s=current;cls.raw=json.loads((ROOT/'integration/test/contracts/managed-v2-breaker.json').read_bytes());cls.thin=json.loads((ROOT/'integration/test/contracts/managed-v2-breaker-pools.json').read_bytes())
 def test_exact_original_control_getters_and_canonical_anchor(self):
  chain=current.ReplayChain(self.raw,self.thin,'cached');ident=next(iter(chain.frames));chain.select(ident);frame=chain.frames[ident]
  for key,result in frame['getters'].items():self.assertEqual(chain.rpc('eth_call',[{'to':key[0],'data':key[1]},chain.anchor()]),result)
  self.assertEqual(chain.configs[0]['multi_asset']['token_policy'],'standard-breaker-bpt')
 def test_fault_serving_is_private_and_recovers(self):
  import copy
  chain=current.ReplayChain(self.raw,self.thin,'dynamic');ident=next(iter(chain.frames));before=copy.deepcopy(chain.frames);chain.select(ident,fault='missing-peer-word');f=chain.fault_evidence
  with self.assertRaises(ValueError):chain.rpc(f['method'],[f['key'][0],f['key'][1],chain.anchor()])
  self.assertGreater(chain.fault_hits,0);chain.select(ident);self.assertEqual(chain.frames,before)
 def test_source_fault_clock_starts_only_when_poison_is_actually_read(self):
  chain=current.ReplayChain(self.raw,self.thin,'dynamic');ident=next(iter(chain.frames));chain.select(ident,fault='missing-peer-word');self.assertIsNone(chain.fault_started_monotonic);f=chain.fault_evidence
  with self.assertRaises(ValueError):chain.rpc(f['method'],[f['key'][0],f['key'][1],chain.anchor()])
  self.assertIsNotNone(chain.fault_started_monotonic);started=chain.fault_started_monotonic
  with self.assertRaises(ValueError):chain.rpc(f['method'],[f['key'][0],f['key'][1],chain.anchor()])
  self.assertEqual(chain.fault_started_monotonic,started);chain.select(ident);self.assertIsNone(chain.fault_started_monotonic)
 def test_negative_quote_requires_expected_business_error_not_http_failure(self):
  case=next(c for c in self.raw['cases'] if c['expected_native'].get('rejection_reason')=='BAL#356')
  for code,response in [(500,{'error':'INTERNAL_SERVER_ERROR'}),(503,{'error':'CHAIN_NOT_READY'}),(504,{'error':'BUDGET_EXHAUSTED'}),(400,{'error':'INVALID_REQUEST'})]:
   with self.assertRaises(ValueError):current.validate_rejected_case(code,response,case)
  current.validate_rejected_case(404,{'error':'NO_ROUTE'},case)
  zero=next(c for c in self.raw['cases'] if c['label']=='zero-underlying-input');current.validate_rejected_case(400,{'error':'INVALID_REQUEST'},zero)

class ManagedBreakerSelectedModelWiringTests(unittest.TestCase):
 def test_actual_vector_consumer_receives_exact_declared_environment_key(self):
  from unittest import mock
  import subprocess
  path=ROOT/'integration/artifacts/local/managed-v2-breaker-actual-selected-vectors.json'
  with mock.patch.object(current.subprocess,'run',return_value=subprocess.CompletedProcess([],0,'1 passed','')) as run:
   result=current.compare_selected_vectors(path)
  self.assertEqual(result.returncode,0)
  args=run.call_args.args[0];self.assertIn('actual_api_selected_breaker_native_full_poststate',args);self.assertIn('--ignored',args)
  self.assertEqual(run.call_args.kwargs['env']['XONE_BREAKER_SELECTED_VECTORS'],str(path))
  self.assertNotIn('XONE_breaker_SELECTED_VECTORS',run.call_args.kwargs['env'])
