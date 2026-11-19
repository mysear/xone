import copy
from pathlib import Path
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from verify_beacon_rotation import verify_switch


class RotationSwitch(unittest.TestCase):
    def test_old_generation_fork_and_changed_pool_are_rejected(self):
        binding = dict(execution_number=1, execution_hash='a')
        ref = dict(block=dict(number_or_slot=1, hash='a'), online_generation=1, writer_epoch=1, journal_seq=1)
        old = dict(reference=ref, pools=[{'reserve0': '15'}])
        new = dict(reference=dict(ref, online_generation=2, writer_epoch=2, journal_seq=2), pools=old['pools'])
        verify_switch(old, new, binding)
        for key, change in [('reference', ref), ('reference', dict(new['reference'], block={'hash': 'b'})),
                            ('pools', [{'reserve0': '14'}])]:
            with self.subTest(key=key), self.assertRaises(ValueError):
                verify_switch(old, dict(copy.deepcopy(new), **{key: change}), binding)
