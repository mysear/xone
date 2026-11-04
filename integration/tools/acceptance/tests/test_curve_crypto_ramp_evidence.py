import copy
import sys
import unittest
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import curve_crypto_ramp_evidence as m

class CryptoRampEvidence(unittest.TestCase):
    def report(self):
        return {'passed': True, 'state': 'PASSED', 'source_binding': {'s': 'current'}, 'input_hashes': {'fixture': 'current'}, 'inputs_unchanged': True, 'started_at_unix_ms': 1000, 'completed_at_unix_ms': 2000, 'checks': m.CHECKS, 'quote_checks': 56, 'successful_quotes': 56, 'native_revert_rejections': 0, 'phase_count': 14, 'pool_count': 2, 'receipt_reads': 1, 'read_counts': {'ramp-origin': 2, 'eth_getCode': 1, 'eth_getStorageAt': 1, 'eth_call': 1}, 'profile': m.PROFILE, 'verified': False, 'production_ready': False, 'all_requested_features_complete': False, 'trust': 'SYNTHETIC_RPC_NOT_DEPLOYMENT_PROOF'}

    def test_current_ramp_process_requires_all_banks_faults_and_execution_inputs(self):
        m.validate_process(self.report(), {'s': 'current'}, {'fixture': 'current'}, 3000)
        for key, value in [('checks', m.CHECKS[:-1]), ('input_hashes', {'fixture': 'old'}), ('inputs_unchanged', False), ('source_binding', {}), ('quote_checks', 52), ('successful_quotes', 0), ('read_counts', {}), ('verified', True), ('all_requested_features_complete', True), ('profile', 'curve-crypto2-full'), ('trust', 'LIVE_DEPLOYMENT')]:
            report = self.report()
            report[key] = value
            with self.assertRaises(ValueError, msg=key):
                m.validate_process(report, {'s': 'current'}, {'fixture': 'current'}, 3000)

    def test_process_times_cannot_be_stale_future_or_missing(self):
        for key, value in [('started_at_unix_ms', 0), ('started_at_unix_ms', 3000), ('completed_at_unix_ms', 4000), ('completed_at_unix_ms', '2000')]:
            report = self.report()
            report[key] = value
            with self.assertRaises(ValueError, msg=key):
                m.validate_process(report, {'s': 'current'}, {'fixture': 'current'}, 3000)
        with self.assertRaises(ValueError):
            m.validate_process(self.report(), {'s': 'current'}, {'fixture': 'current'}, 3_603_000)

    def test_native_public_sequence_inventory_and_complete_projection(self):
        m.check_projection()

    def test_review_approval_cannot_be_reused_without_integration_input_binding(self):
        value = {'source_tree_sha256': 'current', 'scope': m.SCOPE, 'reviews': {key: {'passed': True, 'open_p1_p2': 0, 'agent_id': key} for key in ['spec', 'quality']}}
        with self.assertRaises(ValueError):
            m.validate_review(value, {'source_tree_sha256': 'current'}, {'fixture': 'current'})

    def test_current_independent_spec_and_quality_reviews_required(self):
        report = {'source_tree_sha256': 'current', 'scope': m.SCOPE, 'reviews': {key: {'passed': True, 'open_p1_p2': 0, 'agent_id': key} for key in ['spec', 'quality']}}
        report.update(input_sha256=m.input_digest({'fixture': 'current'}), input_files=1)
        m.validate_review(report, {'source_tree_sha256': 'current'}, {'fixture': 'current'})
        for bad in ['source', 'missing', 'same-agent', 'open', 'input-missing', 'input-stale', 'input-count']:
            changed = copy.deepcopy(report)
            if bad == 'source':
                changed['source_tree_sha256'] = 'old'
            elif bad == 'missing':
                del changed['reviews']['quality']
            elif bad == 'same-agent':
                changed['reviews']['quality']['agent_id'] = 'spec'
            elif bad == 'open':
                changed['reviews']['spec']['open_p1_p2'] = 1
            elif bad == 'input-missing':
                del changed['input_sha256']
            elif bad == 'input-stale':
                changed['input_sha256'] = m.input_digest({'fixture': 'old'})
            else:
                changed['input_files'] = 0
            with self.assertRaises(ValueError):
                m.validate_review(changed, {'source_tree_sha256': 'current'}, {'fixture': 'current'})

if __name__ == '__main__':
    unittest.main()
