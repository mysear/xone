import copy
import json
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from verify_managed_snapshot_exact_out import ROOT, validate_corpus

class ManagedOutputEvidence(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.original = json.loads((ROOT / 'integration/test/contracts/managed-snapshot-exact-out.json').read_bytes())

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

    def test_sequence_cannot_change_observed_weights(self):
        self.rejected(lambda c:c['sequences'][0]['steps'][1]['pool']['multi_asset']['weights'].__setitem__(0,'1'),'SEQUENCE_FULL_STATE')

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
        self.rejected(lambda c: c['cases'][0]['pool'].update(protocol='balancer-v2-managed-fees'), 'MODEL_IDENTITY')


    def test_normalizing_weights_is_rejected(self):
        def mutate(c):
            row=c['cases'][0];w=row['pool']['multi_asset']['weights'];w[0]=str(int(w[0])+10**18-sum(map(int,w)))
        self.rejected(mutate,'OBSERVED_WEIGHT_AND_FEE')

    def test_fee_from_another_clock_is_rejected(self):
        self.rejected(lambda c:c['cases'][0]['pool'].update(fee_numerator=10**17),'OBSERVED_WEIGHT_AND_FEE')

if __name__ == '__main__':
    unittest.main()
