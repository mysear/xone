import copy
import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from replay_beacon_window import ArchiveRPC, check_rows


class HistoricalWindow(unittest.TestCase):
    def test_conflicting_successes_rejected_and_transient_errors_counted(self):
        rpc = ArchiveRPC('source')
        params = ['0x1', False]
        add = lambda body: rpc.add(Path('record.raw'), 'eth_getBlockByNumber', params, json.dumps(body).encode())
        reply = dict(jsonrpc='2.0', id=1, result=dict(hash='a', number='0x1'))
        add(reply)
        add(dict(reply, result=dict(number='0x1', hash='a')))
        add(dict(jsonrpc='2.0', id=1, error=dict(code=15)))
        self.assertEqual((len(rpc.index), rpc.errors), (1, 1))
        with self.assertRaisesRegex(ValueError, 'AMBIGUOUS_CAPTURE_RESULT'):
            add(dict(reply, result=dict(hash='b', number='0x1')))
        with self.assertRaisesRegex(ValueError, 'UNRECORDED_RPC_REQUEST'):
            rpc.response('eth_getProof', [])

    def test_checks_stage_digest_and_original_journal_lineage(self):
        start = dict(block=dict(number_or_slot=7, hash='a'), online_generation=1, journal_seq=1)
        end = dict(start, journal_seq=2, block=dict(number_or_slot=8, hash='b', parent_hash='a'))
        report = dict(first=start, commits=[dict(reference=end, kind='BLOCK_COMMIT')])
        rows = [dict(block=r['block'], staged_sha256='digest') for r in [start, end]]
        check_rows(rows, report, {(8, 'b'): 'digest'})
        with self.assertRaisesRegex(ValueError, 'STATE_OR_FACTS_CHANGED'):
            check_rows(rows, report, {(8, 'b'): 'other'})
        changed = copy.deepcopy(report)
        changed['commits'][0]['reference']['online_generation'] = 2
        with self.assertRaisesRegex(ValueError, 'REPLAY_JOURNAL'):
            check_rows(rows, changed, {(8, 'b'): 'digest'})
