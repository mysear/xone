import unittest
from pathlib import Path
import sys
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import verify_beacon_owner as owner


class BeaconAdvancement(unittest.TestCase):
    def check(self, change=None):
        start = dict(block=dict(chain_key='evm:1', number_or_slot=100, hash='a'), journal_seq=3,
                     online_generation=1, canonical_epoch=1, writer_epoch=1, scope_hash='scope')
        end = dict(start, journal_seq=4, block=dict(chain_key='evm:1', number_or_slot=101, hash='b', parent_hash='a'))
        record = dict(kind='BLOCK_COMMIT', reference=end, record_id='id')
        baseline = dict(reference=end)
        if change == 'generation':
            end['online_generation'] += 1
        if change == 'parent':
            end['block']['parent_hash'] = 'fork'
        if change == 'reanchor':
            record['kind'] = 'ONLINE_BASELINE_COMMIT'
        if change == 'baseline':
            baseline = dict(reference=start)
        replies = [dict(status=200, body=[record]), dict(status=200, body=baseline)]
        with patch.object(owner, 'http', side_effect=replies):
            return owner.verify_advancement('localhost', 'token', Mock(poll=lambda: None), start,
                dict(block_number='101', block_hash='b'), 1)

    def test_contiguous_commit(self):
        self.assertEqual(self.check()[0]['reference']['journal_seq'], 4)

    def test_rejects_discontinuity(self):
        for change in ('generation', 'parent', 'reanchor', 'baseline'):
            with self.subTest(change=change), self.assertRaisesRegex(ValueError, 'DISCONTINUITY|FINAL_BASELINE'):
                self.check(change)
