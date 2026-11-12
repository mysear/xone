"""Tampering controls for the optional local normal-process acceptance receipts."""
from pathlib import Path
import copy
import json
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import family_expansion_acceptance as acceptance


class FamilyAcceptanceControls(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.receipts = {}
        for family, name in [('pump', 'pump-bonding'), ('only-y', 'meteora-only-y')]:
            path = acceptance.OUT / ('core-' + name + '-normal-process.json')
            if not path.is_file() or json.loads(path.read_bytes()).get('passed') is not True:
                raise unittest.SkipTest('Run family-expansion-current-check normal processes first')
            cls.receipts[family] = json.loads(path.read_bytes())
        cls.source = acceptance.source_hash()

    def rejected_capture(self, family, key, mutate, message):
        receipt = copy.deepcopy(self.receipts[family])
        capture = json.loads(acceptance.artifact(receipt[key]).read_bytes())
        mutate(capture)
        with tempfile.TemporaryDirectory(prefix='family-tamper-', dir=acceptance.OUT) as folder:
            path = Path(folder) / 'capture.json'
            path.write_text(json.dumps(capture) + '\n')
            receipt[key] = dict(path=str(path.relative_to(acceptance.ROOT)), sha256=acceptance.sha(path))
            selected = Path(folder) / 'receipt.json'
            selected.write_text(json.dumps(receipt) + '\n')
            count, faults = (5, 7) if family == 'pump' else (33, 9)
            with self.assertRaisesRegex(ValueError, message):
                acceptance.process_receipt(selected, self.source, family, count, faults)

    def test_rehashed_pump_fee_ledger_change_is_rejected(self):
        def mutate(capture):
            capture['records'][0]['steps'][0]['after']['pump']['accounts'][0]['lamports'] += 1
        self.rejected_capture('pump', 'model_projection', mutate, 'pump:complete-model-ledger')

    def test_rehashed_only_y_original_account_change_is_rejected(self):
        def mutate(capture):
            bank = capture['records'][0]['native']['after']
            bank[next(iter(bank))]['lamports'] += 1
        self.rejected_capture('only-y', 'original_capture', mutate, 'only-y:original-complete-bank')

    def test_reported_pump_income_cannot_override_actual_user_balance(self):
        def mutate(capture):
            capture['records'][0]['native']['user_sol_delta'] += 1
        self.rejected_capture('pump', 'original_capture', mutate, 'pump:reported-funds')

    def test_same_count_duplicate_bank_cannot_replace_missing_coverage(self):
        def mutate(capture):
            capture['records'][0] = copy.deepcopy(capture['records'][2])
        self.rejected_capture('only-y', 'captured_plans', mutate, 'only-y:distinct-bank-coverage')


if __name__ == '__main__':
    unittest.main()
