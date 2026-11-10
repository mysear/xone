import copy
from pathlib import Path
import sys
import unittest
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from pancake_bsc4_acceptance import runtime_match


class OriginalRuntimeBindingTests(unittest.TestCase):
    def original(self):
        # A full immutable word is rebased for a controlled reference address.
        return {'evm': {'deployedBytecode': {'object': '60' + '00' * 32 + '6001',
                'immutableReferences': {'1': [{'start': 1, 'length': 32}]}}}}

    def test_rebased_constructor_word_preserves_the_original_program(self):
        runtime_match('0x60' + 'ab' * 32 + '6001', self.original())

    def test_executable_and_trailer_changes_cannot_hide_as_immutables(self):
        for code in ['0x61' + 'ab' * 32 + '6001',
                     '0x60' + 'ab' * 32 + '6002',
                     '0x60' + 'ab' * 32 + '600100']:
            with self.subTest(code=code), self.assertRaises(ValueError):
                runtime_match(code, self.original())

    def test_malformed_immutable_ranges_reject(self):
        for start, length in [(-1, 32), (3, 32), (1, 33)]:
            artifact = copy.deepcopy(self.original())
            artifact['evm']['deployedBytecode']['immutableReferences']['1'][0] = {
                'start': start, 'length': length}
            with self.subTest(start=start, length=length), self.assertRaises(ValueError):
                runtime_match('0x60' + 'ab' * 32 + '6001', artifact)
