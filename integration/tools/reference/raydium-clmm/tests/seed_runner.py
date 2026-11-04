"""Mutation tests for native seed evidence audits (no service or business imports)."""
import copy
import importlib.util
import json
from pathlib import Path
import sys
import unittest

sys.dont_write_bytecode = True


def run_tests(runner, corpus, fixture, identities):
    class SeedAuditTests(unittest.TestCase):
        def valid(self):
            return next(c for c in corpus['cases'] if not c['expected'].get('reverted'))

        def test_report_execution_flags_are_explicit_false(self):
            evidence=runner.execution_evidence()
            self.assertEqual(set(evidence), {'sbf_transaction_executed','token_program_executed','anchor_instruction_executed'})
            runner.audit_execution_flags(evidence)
            for value in evidence.values():
                self.assertIs(value, False)

        def test_report_execution_flag_mutations_reject(self):
            evidence=runner.execution_evidence()
            for field in evidence:
                missing=dict(evidence)
                del missing[field]
                with self.subTest(field=field,mutation='missing'):
                    with self.assertRaises(AssertionError):runner.audit_execution_flags(missing)
                for value in [True,0,1,None,'false','true',[],{}]:
                    bad=dict(evidence)
                    bad[field]=value
                    with self.subTest(field=field,value=value):
                        with self.assertRaises(AssertionError):runner.audit_execution_flags(bad)

        def test_seed_poststate_and_pool_dependencies_are_immutable(self):
            c=self.valid()
            runner.audit_result(c['pool'],c['input'],c['zero_for_one'],c['expected'])
            for field,value in [('seed_index',256 if c['pool']['raydium_clmm']['seed_index']!=256 else 1),('vault0',corpus['base_pool']['token0']),('observation',corpus['base_pool']['token1'])]:
                bad=copy.deepcopy(c['expected']);bad['next_pool']['raydium_clmm'][field]=value
                with self.assertRaises(AssertionError):
                    runner.audit_result(c['pool'],c['input'],c['zero_for_one'],bad)

        def test_fee_token_and_vault_accounting_corruption_rejects(self):
            for mode in (0,1,2):
                c=next(c for c in corpus['cases'] if c['pool']['raydium_clmm'].get('collect_fee_on',0)==mode and not c['expected'].get('reverted'))
                fee_token=(0 if c['zero_for_one'] else 1) if mode==0 else mode-1
                for field in ['raw_vault0',f'fund_fees{1-fee_token}']:
                    bad=copy.deepcopy(c['expected']);bad['next_pool']['raydium_clmm'][field]+=1
                    with self.assertRaises(AssertionError):
                        runner.audit_result(c['pool'],c['input'],c['zero_for_one'],bad)

        def test_complete_extension_and_tick_proof_are_preserved(self):
            c=self.valid()
            for width in (None,0,111,113):
                bad=copy.deepcopy(c['expected'])
                if width is None: del bad['next_pool']['raydium_clmm']['bitmap_extension']
                else: bad['next_pool']['raydium_clmm']['bitmap_extension']['negative']=[0]*width
                with self.assertRaises(AssertionError):
                    runner.audit_result(c['pool'],c['input'],c['zero_for_one'],bad)
            bad=copy.deepcopy(c['expected']);bad['next_pool']['raydium_clmm']['tick_arrays'][0]['address']=corpus['base_pool']['token0']
            with self.assertRaises(AssertionError):runner.audit_result(c['pool'],c['input'],c['zero_for_one'],bad)

        def test_full_sequences_and_seed_cell_coverage(self):
            runner.audit(corpus,identities)
            bad=copy.deepcopy(corpus);bad['sequences'][0]['expected_final_pool']['raydium_clmm']['raw_vault0']+=1
            with self.assertRaises(AssertionError):runner.audit(bad,identities)
            bad=dict(corpus,independent_vectors=corpus['independent_vectors'][:-1])
            with self.assertRaises(AssertionError):runner.audit(bad,identities)
            bad=dict(corpus,cases=[c for c in corpus['cases'] if not (c['name'].startswith('seed1-next-bank-exact-fixture-') and c['input']=='1000000' and c['zero_for_one'] is False)])
            with self.assertRaises(AssertionError):runner.audit(bad,identities)

        def test_independent_seed_pool_final_states_reject_alias(self):
            bad=copy.deepcopy(corpus)
            bad['independence_cases'][0]['expected_final_pools'][1]=bad['independence_cases'][0]['expected_final_pools'][0]
            with self.assertRaises(AssertionError):runner.audit(bad,identities)

        def test_identity_endianness_bump_dependency_mutations_reject(self):
            runner.audit_identities(identities)
            for field,value in [('seed_le',[0,1]),('be_differs',False),('pool_bump',256)]:
                bad=copy.deepcopy(identities)
                v=next(v for v in bad['vectors'] if v['seed_index']==1);v[field]=value
                with self.assertRaises(AssertionError):runner.audit_identities(bad)
            bad=copy.deepcopy(identities);bad['vectors'][1]['dependencies']['vault0']['address']=bad['vectors'][0]['vault0']
            with self.assertRaises(AssertionError):runner.audit_identities(bad)

        def test_packed_seed_and_padding_mutations_reject(self):
            import base64
            runner.audit_fixture(fixture,corpus,identities)
            for offset in [391,392,393,396]:
                bad=copy.deepcopy(fixture);key=bad['pool']['pool_id'];a=bad['rpc_accounts'][key]
                raw=bytearray(base64.b64decode(a['data_base64']));raw[offset]^=1;a['data_base64']=base64.b64encode(raw).decode()
                # Refresh the RPC response too, to test packed bytes rather than map/list mismatch.
                bad['rpc_response']['result']['value'][bad['rpc_request_accounts'].index(key)]=a
                with self.assertRaises(AssertionError):runner.audit_fixture(bad,corpus,identities)

        def test_next_bank_cannot_mutate_seed_or_drop_scope_probe(self):
            bad=copy.deepcopy(fixture);bad['metadata']['next_bank_fixture']['pool']['raydium_clmm']['seed_index']=256
            with self.assertRaises(AssertionError):runner.audit_fixture(bad,corpus,identities)
            bad=copy.deepcopy(fixture);bad['metadata']['next_bank_scope_probe']['after_expected']={'amount_out':'1'}
            with self.assertRaises(AssertionError):runner.audit_fixture(bad,corpus,identities)

    suite=unittest.defaultTestLoader.loadTestsFromTestCase(SeedAuditTests)
    count=suite.countTestCases()
    result=unittest.TextTestRunner(verbosity=2).run(suite)
    assert result.wasSuccessful(), 'seed audit mutation tests failed'
    return count


if __name__ == '__main__':
    root=Path(__file__).resolve().parents[5]
    spec=importlib.util.spec_from_file_location('seed_golden',root/'integration/tools/ci/rayclmm-seed-golden.py')
    runner=importlib.util.module_from_spec(spec);spec.loader.exec_module(runner)
    run_tests(runner,*[json.loads(p.read_bytes()) for p in [root/'integration/test/contracts/rayclmm-seed-quotes.json',root/'integration/test/solana/rayclmm-seed-base.json',root/'integration/test/contracts/rayclmm-seed-identities.json']])
