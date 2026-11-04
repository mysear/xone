"""Regression tests for the independent token-fee corpus audit (no service builds)."""
import copy
import importlib.util
import json
from pathlib import Path
import sys
import unittest

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parents[5]
RUNNER = ROOT / 'integration/tools/ci/rayclmm-token-fee-golden.py'
spec = importlib.util.spec_from_file_location('token_fee_golden', RUNNER)
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
OLD = json.loads((ROOT / 'integration/test/contracts/rayclmm-quotes.json').read_bytes())


def pool(mode=1):
    value = copy.deepcopy(OLD['base_pool'])
    value['protocol'] = runner.PROFILE
    value['adapter_version'] = runner.ADAPTER
    value['raydium_clmm']['collect_fee_on'] = mode
    return value


class TokenFeeRunnerTests(unittest.TestCase):
    def test_profile_mode_and_integer_admission(self):
        for mode in (1, 2):
            runner.audit_pool(pool(mode))
        for mode in (0, 3, 255, True, '1'):
            bad = pool(mode)
            with self.subTest(mode=mode), self.assertRaises(AssertionError):
                runner.audit_pool(bad)
        for field, value in [('protocol', 'raydium-clmm-static'),
                             ('adapter_version', 'raydium-clmm/6d7a0f82-spl-static-1')]:
            bad = pool()
            bad[field] = value
            with self.subTest(field=field), self.assertRaises(AssertionError):
                runner.audit_pool(bad)
        bad = pool()
        bad['raydium_clmm']['liquidity'] = 1
        with self.assertRaises(AssertionError):
            runner.audit_pool(bad)

    def test_base_modes_are_only_identity_and_mode_changes(self):
        runner.audit_bases({'base_pool': pool(1), 'base_pool_mode2': pool(2)}, OLD['base_pool'])
        bad = pool(2)
        bad['raydium_clmm']['trade_fee_rate'] += 1
        with self.assertRaises(AssertionError):
            runner.audit_bases({'base_pool': pool(1), 'base_pool_mode2': bad}, OLD['base_pool'])

    def test_actual_fee_token_is_independent_of_direction(self):
        for mode in (1, 2):
            for direction in (True, False):
                before = pool(mode)
                after = copy.deepcopy(before)
                input_token, output_token = (0, 1) if direction else (1, 0)
                s = after['raydium_clmm']
                s[f'raw_vault{input_token}'] += 100
                s[f'raw_vault{output_token}'] -= 90
                s['recent_epoch'] = s['bank_epoch']
                s[f'protocol_fees{mode-1}'] += 1
                s[f'fund_fees{mode-1}'] += 1
                s[f'fee_growth_global{mode-1}'] = str(int(s[f'fee_growth_global{mode-1}']) + 1)
                expected = {'amount_out': '90', 'next_pool': after}
                runner.audit_result(before, '100', direction, expected)
                bad = copy.deepcopy(expected)
                bad['next_pool']['raydium_clmm'][f'fund_fees{2-mode}'] += 1
                with self.subTest(mode=mode, direction=direction), self.assertRaises(AssertionError):
                    runner.audit_result(before, '100', direction, bad)
                bad = copy.deepcopy(expected)
                bad['next_pool']['raydium_clmm'][f'raw_vault{output_token}'] -= 1
                with self.assertRaises(AssertionError):
                    runner.audit_result(before, '100', direction, bad)

    def test_reverts_and_amounts_are_strict(self):
        runner.audit_result(pool(), '0', True, {'reverted': True})
        for result in ({'reverted': True, 'reason': 'unexpected'}, {'reverted': 1}):
            with self.assertRaises(AssertionError):
                runner.audit_result(pool(), '0', True, result)
        with self.assertRaises(AssertionError):
            runner.audit_result(pool(), '18446744073709551616', True, {'reverted': True})

    def test_old_corpus_cannot_satisfy_token_fee_coverage(self):
        with self.assertRaises(AssertionError):
            runner.audit(OLD)

    def test_named_independent_vectors_are_complete_and_distinct(self):
        path = ROOT / 'integration/test/contracts/rayclmm-token-fee-quotes.json'
        if not path.exists():
            self.skipTest('first generation audits the newly emitted corpus directly')
        corpus = json.loads(path.read_bytes())
        counts = runner.audit(corpus)
        self.assertEqual(counts['independent_base_vectors'], 8)
        for names in ([], corpus['independent_vectors'][:-1], [corpus['independent_vectors'][0]] * 8):
            broken = dict(corpus, independent_vectors=names)
            with self.subTest(names=names), self.assertRaises(AssertionError):
                runner.audit(broken)

    def test_complete_sequence_final_pool_is_audited(self):
        path = ROOT / 'integration/test/contracts/rayclmm-token-fee-quotes.json'
        if not path.exists():
            self.skipTest('first generation audits the newly emitted corpus directly')
        corpus = json.loads(path.read_bytes())
        corpus['sequences'][0]['expected_final_pool']['raydium_clmm']['raw_vault0'] += 1
        with self.assertRaises(AssertionError):
            runner.audit(corpus)


if __name__ == '__main__':
    unittest.main()
