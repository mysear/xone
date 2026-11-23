import copy
import json
import sys
import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from replay_beacon_window import ArchiveRPC, BATCH_METHOD, STAGED_V1, STAGED_V2, check_rows, provider_id, read_window


class HistoricalWindow(unittest.TestCase):
    def test_staged_decoder_versions_select_distinct_content_at_the_same_block(self):
        endpoint = 'https://source.example'
        block = dict(chain_key='evm:56', number_or_slot=7, hash='block')
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            responses = {}
            for version, digest in ((STAGED_V1, 'old'), (STAGED_V2, 'new'), (provider_id(endpoint), 'source')):
                path = directory / f'{digest}.raw'
                path.touch()
                owner = dict(Chain='evm:56', Height=7, Hash='block', ProviderID=version)
                responses[path] = owner, json.dumps(dict(block=block)).encode(), dict(data_sha256=digest)
            with patch('replay_beacon_window.sealed_payload', side_effect=lambda path, *_: responses[path]), \
                    patch('replay_beacon_window.rpc_observation', return_value=(
                        'eth_chainId', [], b'{"jsonrpc":"2.0","id":1,"result":"0x38"}')):
                _, legacy = read_window(directory, [endpoint], 'evm:56')
                _, current = read_window(directory, [endpoint], 'evm:56', staged_provider=STAGED_V2)
                self.assertEqual((legacy, current), ({(7, 'block'): 'old'}, {(7, 'block'): 'new'}))

    def test_provider_batch_matches_complete_requests_and_preserves_wire_order(self):
        source = ArchiveRPC('source')
        calls = [dict(id=i, jsonrpc='2.0', method='eth_call', params=[{'data': hex(i)}, '0x50'])
                 for i in range(1, 17)]
        replies = [dict(id=i, jsonrpc='2.0', result=hex(i)) for i in range(16, 0, -1)]
        raw = json.dumps(replies).encode()
        path = Path('record.raw')
        for invalid in [replies[:-1], replies[:-1] + replies[:1], [dict(replies[0], error={})] + replies[1:]]:
            source.add(path, BATCH_METHOD, calls, json.dumps(invalid).encode())
        self.assertEqual((source.index, source.errors), ({}, 3))
        source.add(path, BATCH_METHOD, calls, raw)
        source.add(path, BATCH_METHOD, calls, json.dumps(list(reversed(replies))).encode())
        with patch('replay_beacon_window.sealed_payload', return_value=({'ProviderID':'source'}, b'', {})), \
                patch('replay_beacon_window.rpc_observation', return_value=(BATCH_METHOD, calls, raw)) as observation:
            self.assertEqual(source.dispatch(calls), raw)
            self.assertEqual((source.served, source.served_batches), (1, 1))
            observation.return_value = (BATCH_METHOD, calls, raw + b' ')
            with self.assertRaisesRegex(ValueError, 'REPLAY_RESPONSE_CHANGED'):
                source.dispatch(calls)
            self.assertEqual((source.served, source.served_batches), (1, 1))
        changed = copy.deepcopy(calls)
        changed[0]['params'][1] = '0x51'
        with self.assertRaisesRegex(ValueError, 'UNRECORDED_RPC_REQUEST'):
            source.dispatch(changed)
        for invalid in [[], calls + [calls[-1]], [calls[-1]], [dict(calls[0], method='eth_chainId')],
                        dict(calls[0], method=BATCH_METHOD, params=calls)]:
            with self.assertRaisesRegex(ValueError, 'REPLAY_INVALID_CALL_BATCH'):
                source.dispatch(invalid)

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
