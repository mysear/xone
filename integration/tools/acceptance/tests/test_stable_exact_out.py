import copy
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from verify_stable_exact_out import ROOT, validate_corpus

class StableOutputEvidence(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.original = json.loads((ROOT / 'integration/test/contracts/stable-exact-out.json').read_bytes())

    def rejected(self, mutate, reason):
        corpus = copy.deepcopy(self.original)
        mutate(corpus)
        with self.assertRaisesRegex(ValueError, reason):
            validate_corpus(corpus)

    def test_target_cannot_change_without_cash_movement(self):
        def mutate(c):
            p = next(x for x in c['cases'] if not x['expected'].get('reverted'))
            p['target'] = str(int(p['target']) + 1)
        self.rejected(mutate, 'TARGET_CASH_CONSERVATION')

    def test_reference_case_cannot_be_omitted(self):
        self.rejected(lambda c: c['cases'].pop(), 'CASE_COUNT')

    def test_sequence_cannot_change_amplification(self):
        self.rejected(lambda c: c['sequences'][0]['steps'][1]['pool']['stable'].update(amplification='1001'), 'SEQUENCE_FULL_STATE')

    def test_grid_cannot_claim_unexecuted_lower_input(self):
        self.rejected(lambda c: c['grids'][0].update(bestRequiredInput='1'), 'GRID_MINIMUM')

    def test_grid_cannot_change_target(self):
        self.rejected(lambda c: c['grids'][0].update(target='1'), 'GRID_TARGET_CONSERVATION')

    def test_same_pool_cannot_reuse_initial_balance(self):
        self.rejected(lambda c: c['ordered'][0]['steps'][1].update(pool=copy.deepcopy(c['ordered'][0]['pool'])), 'ORDERED_PHYSICAL_STATE')

    def test_original_failure_cannot_be_unknown_runtime_error(self):
        def mutate(c):
            next(x for x in c['cases'] if x['expected'].get('reverted'))['expected']['reason'] = 'RPC_TIMEOUT'
        self.rejected(mutate, 'UNEXPECTED_ORIGINAL_REVERT')

    def test_profile_cannot_be_relabelled(self):
        self.rejected(lambda c: c['cases'][0]['pool'].update(protocol='balancer-v2-composable-regular'), 'MODEL_IDENTITY')

if __name__ == '__main__':
    unittest.main()
