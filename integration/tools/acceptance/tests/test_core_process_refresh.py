"""A successful process exit alone cannot qualify a calculation receipt."""
import copy
from pathlib import Path
import sys
import unittest
import json
import tempfile
from unittest.mock import patch
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from core_process_refresh import validate_gate_receipt, public_gate_destination, reusable_gate
import core_process_refresh as refresh

class RefreshReceiptTests(unittest.TestCase):
    def public(self):
        return dict(status='QUALIFIED_FINITE_NORMAL_REFERENCE_DIFFERENTIAL',
                    gross_matches_original_VM_token_delta=True, normal_quote_observed=True,
                    normal_parser_typed_baseline_injected=False, normal_replay_public_network_requests=0,
                    quantities=dict(router_pool_gross='9', reference_recipient_token_delta='9'))

    def test_actual_public_schema_without_generic_pass_flag(self):
        for name in ['polygon', 'arbitrum']:
            validate_gate_receipt(name, self.public(), 'source')

    def test_exit_or_unrelated_pass_flag_cannot_replace_public_authority(self):
        for key in self.public():
            value = self.public()
            del value[key]
            value['passed'] = True
            with self.subTest(key=key), self.assertRaises(ValueError):
                validate_gate_receipt('polygon', value, 'source')
        value = self.public()
        value['quantities']['reference_recipient_token_delta'] = '10'
        with self.assertRaises(ValueError):
            validate_gate_receipt('polygon', value, 'source')

    def test_local_receipts_require_current_source_and_boolean_pass(self):
        for name in ['slipstream', 'velodrome', 'lfj', 'pancake']:
            validate_gate_receipt(name, dict(passed=True, source_tree_sha256='source'), 'source')
            for value in [dict(passed=True, source_tree_sha256='old'),
                          dict(passed=1, source_tree_sha256='source')]:
                with self.assertRaises(ValueError):
                    validate_gate_receipt(name, value, 'source')


class RefreshContinuationTests(unittest.TestCase):
    def test_public_destinations_satisfy_normal_runner_ownership(self):
        for name in ['polygon', 'arbitrum']:
            target = public_gate_destination('core-stage39-final', name)
            self.assertTrue(target.name.startswith('r01-public-'))
            self.assertEqual(target.parent, refresh.OUT)
        for stem, name in [('other', 'polygon'), ('core-valid', '../escape')]:
            with self.assertRaises(ValueError):
                public_gate_destination(stem, name)

    def test_reuse_requires_unchanged_runtime_and_artifacts(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            out = root / 'out'
            out.mkdir()
            log, receipt = out / 'run.log', out / 'receipt.json'
            log.write_text('actual execution')
            receipt.write_text(json.dumps(dict(passed=True, source_tree_sha256='source')))
            fingerprint = dict(source_tree_sha256='source', binaries={'router': 'binary'})
            entry = dict(name='slipstream', passed=True, observed_exit_code=0,
                         log='out/run.log', log_sha256=refresh.sha(log),
                         report='out/receipt.json', report_sha256=refresh.sha(receipt))
            previous = dict(source_tree_sha256='source', runtime_fingerprint=fingerprint,
                            checks=[entry])
            with patch.object(refresh, 'ROOT', root), patch.object(refresh, 'OUT', out):
                self.assertEqual(reusable_gate(previous, 'slipstream', fingerprint), entry)
                self.assertIsNone(reusable_gate(previous, 'polygon', fingerprint))
                changed = copy.deepcopy(fingerprint)
                changed['binaries']['router'] = 'changed'
                with self.assertRaises(ValueError):
                    reusable_gate(previous, 'slipstream', changed)
                log.write_text('modified')
                with self.assertRaises(ValueError):
                    reusable_gate(previous, 'slipstream', fingerprint)
