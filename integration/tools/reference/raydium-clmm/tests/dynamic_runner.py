"""Mutation audits for original-native dynamic evidence; run by the generator."""
import copy
import unittest


def run_tests(runner, corpus, fixture):
    class EvidenceTests(unittest.TestCase):
        def test_accept_original_evidence(self):
            runner.audit(corpus)
            runner.audit_fixture(fixture, corpus)

        def test_field_omission_extra_width_and_bool(self):
            for field in runner.FIELDS:
                p = copy.deepcopy(corpus['base_pool'])
                del p['raydium_clmm']['dynamic_fee'][field]
                with self.assertRaises(AssertionError): runner.audit_pool(p)
            for key, value in [('filter_period', 65536), ('decay_period', 10), ('reduction_factor', 10000),
                               ('dynamic_fee_control', 100000), ('last_update_timestamp', 2**32),
                               ('volatility_reference', 1000001), ('volatility_accumulator', 1000001),
                               ('tick_spacing_index_reference', -(2**31)), ('filter_period', True), ('padding', 0)]:
                p = copy.deepcopy(corpus['base_pool']); p['raydium_clmm']['dynamic_fee'][key] = value
                with self.assertRaises(AssertionError): runner.audit_pool(p)

        def test_poststate_parameter_timestamp_reference_and_accumulator(self):
            c = next(c for c in corpus['cases'] if not c['expected'].get('reverted'))
            for field, value in [('dynamic_fee_control', 123), ('last_update_timestamp', 0),
                                 ('tick_spacing_index_reference', -5), ('volatility_reference', 123),
                                 ('volatility_accumulator', 2**32)]:
                e = copy.deepcopy(c['expected']); e['next_pool']['raydium_clmm']['dynamic_fee'][field] = value
                with self.assertRaises(AssertionError): runner.audit_result(c['pool'], c['input'], c['zero_for_one'], e)

        def test_full_pool_raw_fee_and_identity_mutations(self):
            c = next(c for c in corpus['cases'] if not c['expected'].get('reverted'))
            for field in ['raw_vault0','raw_vault1','bank_slot','max_steps','seed_index','tick_spacing']:
                e = copy.deepcopy(c['expected']); s = e['next_pool']['raydium_clmm']; s[field] = s.get(field, 0) + 1
                with self.assertRaises(AssertionError): runner.audit_result(c['pool'], c['input'], c['zero_for_one'], e)
            e = copy.deepcopy(c['expected']); e['amount_out'] = '0'
            with self.assertRaises(AssertionError): runner.audit_result(c['pool'], c['input'], c['zero_for_one'], e)

        def test_clock_probe_mutations(self):
            for mutation in ['amount','parameter','timestamp','observation','precondition']:
                c = copy.deepcopy(corpus); p = c['time_invariance_probes'][0]
                if mutation == 'amount': p['later_expected']['amount_out'] = '1'
                if mutation == 'parameter': p['later_expected']['next_pool']['raydium_clmm']['dynamic_fee']['dynamic_fee_control'] = 2
                if mutation == 'timestamp': p['later_expected']['next_pool']['raydium_clmm']['dynamic_fee']['last_update_timestamp'] = 0
                if mutation == 'observation': p['later_expected']['next_pool']['raydium_clmm']['observations'][0]['tick_cumulative'] += 1
                if mutation == 'precondition': p['later_pool']['raydium_clmm']['dynamic_fee']['last_update_timestamp'] = p['later_pool']['raydium_clmm']['bank_unix_timestamp']
                with self.assertRaises(AssertionError): runner.audit(c)

        def test_raw_pool_data_padding_response_and_next_bank(self):
            for mutation in ['field','padding','response','clock','next_bank','vectors','policy']:
                f = copy.deepcopy(fixture); key = f['pool']['pool_id']
                if mutation in ['field','padding']:
                    raw = bytearray(runner.raw(f,key)); raw[1096 if mutation == 'field' else 1130] ^= 1
                    runner.replace_raw(f,key,bytes(raw))
                if mutation == 'response': f['rpc_response']['result']['value'][0]['data_base64'] = ''
                if mutation == 'clock':
                    raw = bytearray(runner.raw(f,runner.CLOCK)); raw[32] ^= 1; runner.replace_raw(f,runner.CLOCK,bytes(raw))
                if mutation == 'next_bank': f['metadata']['next_bank_fixture']['pool']['pool_id'] = 'wrong'
                if mutation == 'vectors': f['metadata']['next_bank_vectors'][0]['expected']['amount_out'] = '1'
                if mutation == 'policy': f['metadata']['time_policy']['config_max_age_seconds'] = 2592000
                with self.assertRaises(AssertionError): runner.audit_fixture(f,corpus)

        def test_budget_admission_is_not_math_case(self):
            for mutation in ['case','relaxed','reason']:
                c = copy.deepcopy(corpus)
                if mutation == 'case': c['cases'][0] = {k:v for k,v in c['admission_probes'][0].items() if k in ['name','pool','input','zero_for_one','expected']}
                if mutation == 'relaxed': c['admission_probes'][0]['relaxed_expected'] = {'reverted':True}
                if mutation == 'reason': c['admission_probes'][0]['native_reason'] = 'bad'
                with self.assertRaises(AssertionError): runner.audit(c)

        def test_sequence_final_pool_and_missing_coverage(self):
            c = copy.deepcopy(corpus); c['sequences'][0]['expected_final_pool']['raydium_clmm']['dynamic_fee']['volatility_accumulator'] += 1
            with self.assertRaises(AssertionError): runner.audit(c)
            c = copy.deepcopy(corpus); c['cases'] = c['cases'][:200]
            with self.assertRaises(AssertionError): runner.audit(c)

        def test_execution_boundaries_explicit_bool_false(self):
            for field in runner.execution_evidence():
                for value in [True,0,None,'false']:
                    report = runner.execution_evidence(); report[field] = value
                    with self.assertRaises(AssertionError): runner.audit_execution_flags(report)

    result = unittest.TextTestRunner(verbosity=2).run(unittest.defaultTestLoader.loadTestsFromTestCase(EvidenceTests))
    assert result.wasSuccessful(), 'dynamic evidence mutation tests failed'
    return result.testsRun
