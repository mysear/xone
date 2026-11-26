import base64
import copy
from contextlib import contextmanager
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from current_v2_evidence import provider_id, sha
from public_evm_window import (BSC_STATE_WITNESS, BSC_SWAP_WITNESS, PANCAKE_SWAP_TOPIC, FinalizedObservations,
                               NumberedArchiveRPC, capture, capture_time_key, state_witness, verify_journal,
                               verify_replay, witness_bsc_swaps)


def encoded(value):
    return json.dumps(value, separators=(',', ':')).encode()


def block(height, chain='evm:56'):
    return dict(chain_key=chain, number_or_slot=height, hash=f'0x{height:064x}',
                parent_hash=f'0x{height - 1:064x}', timestamp_ms=height * 1000)


def snapshot(ref, sequence):
    reference = dict(block=ref, journal_seq=sequence, online_generation=1, writer_epoch=1, canonical_epoch=1)
    return dict(reference=reference, pools=[dict(id='pool', reserve0=str(sequence))], source='rpc', history_gaps=[])


class PublicWindow(unittest.TestCase):
    def test_capture_order_retains_nanoseconds_and_requires_utc(self):
        prefix = '2026-11-23T05:00:00'
        times = [capture_time_key(prefix + suffix) for suffix in ('Z', '.000000001Z', '.000000002Z', '.1Z')]
        self.assertTrue(all(a < b for a, b in zip(times, times[1:])))
        self.assertEqual(capture_time_key(prefix + '.100000000Z'), times[-1])
        with self.assertRaisesRegex(ValueError, 'PUBLIC_WINDOW_CAPTURE_TIME'):
            capture_time_key(prefix + '.0000000001Z')

    def test_bsc_state_check_can_use_distinct_fallback_without_primary(self):
        primary, secondary = 'https://one.example', 'https://two.example'
        chain = dict(key='evm:56', rpc_urls=[primary, secondary])
        self.assertEqual(state_witness(chain)['rpc_urls'], [BSC_STATE_WITNESS, secondary])
        self.assertEqual(state_witness(dict(chain, rpc_urls=[BSC_STATE_WITNESS, secondary]))['rpc_urls'], [secondary])
        self.assertEqual(state_witness(dict(chain, key='evm:137'))['rpc_urls'], [secondary])

    def write_header(self, directory, ref, url, *, tag=None, captured_at='2026-11-23T05:00:00.1Z', **fields):
        height = ref['number_or_slot']
        header = dict(number=hex(height), hash=ref['hash'], parentHash=ref['parent_hash'],
                      timestamp=hex(ref['timestamp_ms'] // 1000), **fields)
        params = [tag or hex(height), False]
        reply = encoded(dict(jsonrpc='2.0', id=1, result=header))
        raw = encoded(dict(source_kind='raw-json-rpc-response', method='eth_getBlockByNumber', params=params,
                           response=base64.b64encode(reply).decode()))
        owner = dict(Chain=ref['chain_key'], Height=0, Hash=sha(raw), ProviderID=provider_id(url))
        identifier = sha(encoded(owner))
        sealed = dict(Ref=dict(ID=identifier, Digest=sha(raw), Owner=owner, Bytes=len(raw)),
                      Data=base64.b64encode(raw).decode(), CapturedAt=captured_at)
        path = directory / f'{identifier}.raw'
        path.write_bytes(encoded(dict(Digest=sha(encoded(sealed)), Payload=sealed)))
        return path, params, reply

    def test_moving_heads_are_exact_sealed_observations_and_never_served(self):
        first, last = block(10), block(13)
        urls = ['https://one.example', 'https://two.example']
        chain = dict(key='evm:56', rpc_urls=urls, native_proof=dict(max_lookahead=8))
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            self.write_header(directory, first, urls[0], tag='finalized', captured_at='2026-11-23T05:00:00.2Z')
            self.write_header(directory, first, urls[1], tag='finalized')
            path, params, reply = self.write_header(directory, last, urls[0], tag='finalized')
            archive = FinalizedObservations(directory, chain, dict(max_records=3, max_bytes=10000))
            archive.scan()
            archive.scan()
            self.assertEqual(len(archive.files), 3)
            self.assertEqual(archive.find(snapshot(first, 1))[0]['provider_id'], provider_id(urls[1]))
            self.assertEqual(archive.find(snapshot(last, 4))[0]['response_sha256'], sha(reply))
            self.assertIsNone(archive.find(snapshot(dict(last, parent_hash=last['hash']), 4)))
            source = NumberedArchiveRPC(provider_id(urls[0]), chain['key'])
            source.add(path, 'eth_getBlockByNumber', params, reply)
            with self.assertRaisesRegex(ValueError, 'UNRECORDED_RPC_REQUEST'):
                source.dispatch(dict(jsonrpc='2.0', id=1, method='eth_getBlockByNumber', params=params))

    def test_avalanche_binding_requires_all_same_provider_ancestors(self):
        key = 'evm:43114'
        head = block(13, key)
        target = snapshot(block(10, key), 1)
        urls = ['https://one.example', 'https://two.example']
        chain = dict(key=key, rpc_urls=urls, native_proof=dict(max_lookahead=8))
        with tempfile.TemporaryDirectory() as tmp:
            directory = Path(tmp)
            self.write_header(directory, head, urls[0], tag='finalized', settledHeight='0xa')
            self.write_header(directory, block(12, key), urls[0])
            self.write_header(directory, block(11, key), urls[1])
            self.write_header(directory, block(10, key), urls[0])
            archive = FinalizedObservations(directory, chain, dict(max_records=5, max_bytes=10000))
            archive.scan()
            self.assertIsNone(archive.find(target))
            self.write_header(directory, block(11, key), urls[0])
            archive.scan()
            observations = archive.find(target)
            self.assertEqual([row['block']['number_or_slot'] for row in observations], [13, 12, 11, 10])

    def test_journal_and_replay_reject_changed_pools_lineage_and_facts(self):
        snapshots = [snapshot(block(height), num) for num, height in enumerate(range(10, 13), 1)]
        first, _, last = snapshots
        records = [dict(reference=snap['reference'], pools=snap['pools'], payload_hash='a' * 64,
                        kind='ONLINE_BASELINE_COMMIT' if seq == 1 else 'BLOCK_COMMIT', record_id=f'evm:56/1/{seq}')
                   for seq, snap in enumerate(snapshots, 1)]
        verify_journal(first, last, records, 2)
        for field, value in [('parent_hash', first['reference']['block']['hash']), ('number_or_slot', 15)]:
            broken = copy.deepcopy(records)
            broken[-1]['reference']['block'][field] = value
            with self.assertRaisesRegex(ValueError, 'JOURNAL_DISCONTINUITY'):
                verify_journal(first, last, broken, 2)
        partial = copy.deepcopy(last)
        partial['pools'][0]['reserve0'] = '0'
        with self.assertRaisesRegex(ValueError, 'FINAL_SNAPSHOT'):
            verify_journal(first, partial, records, 2)
        rows = [dict(reference=record['reference'], block=record['reference']['block'], kind=record['kind'],
                     record_id=record['record_id'], payload_hash=record['payload_hash'],
                     staged_sha256='b' * 64, swap_count=1) for record in records[1:]]
        staged = {(row['block']['number_or_slot'], row['block']['hash']): row['staged_sha256'] for row in rows}
        decoded = dict(initial_snapshot=first, final_snapshot=last, initial_record=records[0], rows=rows)
        self.assertEqual(verify_replay(decoded, first, last, records, staged), rows)
        changed_baseline = dict(decoded, initial_record=dict(records[0], payload_hash='0' * 64))
        with self.assertRaisesRegex(ValueError, 'REPLAY_BASELINE_RECORD'):
            verify_replay(changed_baseline, first, last, records, staged)
        for field, value, reason in [('payload_hash', '0' * 64, 'REPLAY_JOURNAL'),
                                     ('record_id', 'evm:56/2/3', 'REPLAY_JOURNAL'),
                                     ('normalized_facts', {'blocks': []}, 'REPLAY_FACT_BATCH'),
                                     ('staged_sha256', '0' * 64, 'REPLAY_STAGED_FACTS')]:
            with self.subTest(field=field):
                broken = copy.deepcopy(decoded)
                broken['rows'][-1][field] = value
                with self.assertRaisesRegex(ValueError, reason):
                    verify_replay(broken, first, last, records, staged)

    def test_capture_checks_initial_pool_before_awaiting_a_tail(self):
        first, middle, last = [snapshot(block(n), seq) for seq, n in enumerate(range(10, 13), 1)]
        chain = dict(key='evm:56', rpc_urls=['https://one.example', 'https://two.example'],
                     native_proof=dict(max_lookahead=8))
        config = dict(chains=[chain], state=dict(path='/private/state'), journal=dict(raw_evidence={}),
                      startup=dict(max_tail_blocks=8, max_baseline_age_ms=1000))
        records = [dict(reference=snap['reference'], pools=snap['pools'], payload_hash='a' * 64,
                        kind='ONLINE_BASELINE_COMMIT' if seq == 1 else 'BLOCK_COMMIT', record_id=f'evm:56/1/{seq}')
                   for seq, snap in enumerate((first, middle, last), 1)]
        events = []
        samples = iter((first, last))

        def read(path):
            if path.endswith('/baseline'):
                snap = next(samples)
                events.append(('sample', snap['reference']['journal_seq']))
                return snap
            return records

        @contextmanager
        def owner(*_):
            process = Mock()
            process.poll.return_value = None
            yield process, read, None

        def check(_target, _chain, snap, _age):
            self.assertEqual(_chain['rpc_urls'], [BSC_STATE_WITNESS, chain['rpc_urls'][1]])
            events.append(('check', snap['reference']['journal_seq']))
            return {'verified': True}

        with patch('public_evm_window.owner_process', owner), patch('public_evm_window.check_snapshot', check), \
                patch.object(FinalizedObservations, 'scan'), patch.object(FinalizedObservations, 'find', return_value=[True]), \
                patch('public_evm_window.time.sleep'):
            self.assertEqual(capture(Path('parser'), config, {}, Path('/private'), 2, 30),
                             (first, last, records, {'verified': True}))
        self.assertEqual(events, [('sample', 1), ('check', 1), ('sample', 3)])

    def test_bsc_swap_witness_compares_all_event_amounts_and_rejects_foreign_logs(self):
        pool, token0, token1 = ('0x' + value * 40 for value in ('a', 'b', 'c'))
        probe = dict(pool=pool, token0=token0, token1=token1, protocol='pancake-v2')
        target = dict(chain_key='evm:56', public_pool_probe=probe)
        first, traded, last = (block(h) for h in range(10, 13))
        swap = dict(event_id=f"evm:56/{traded['hash']}/2/3", tx_hash='0x' + 'e' * 64,
                    tx_index=2, log_index=3, pool_id=pool, token0=token0, token1=token1,
                    amount0_in=str(2**128), amount1_in='0', amount0_out='0', amount1_out='17')
        records = [dict(reference=dict(block=first))]
        for ref, swaps in ((traded, [swap]), (last, [])):
            records.append(dict(reference=dict(block=ref), normalized_facts=dict(
                schema_version=1, decoder_version='evm-v2-swap/2', blocks=[dict(block=ref, swaps=swaps)])))
        reply = dict(address=pool, removed=False, topics=[PANCAKE_SWAP_TOPIC, '0x' + '0' * 64, '0x' + '0' * 64],
                     transactionHash=swap['tx_hash'], blockHash=traded['hash'], blockNumber='0xb',
                     transactionIndex='0x2', logIndex='0x3',
                     data='0x' + ''.join(f'{value:064x}' for value in (2**128, 0, 0, 17)))
        replies = [reply]

        def read_witness(url, method, params):
            self.assertEqual(url, BSC_SWAP_WITNESS)
            if method == 'eth_getLogs':
                self.assertEqual(params, [dict(address=pool, topics=[PANCAKE_SWAP_TOPIC], fromBlock='0xb', toBlock='0xc')])
                return replies
            if method == 'eth_chainId':
                return '0x38'
            self.assertEqual(method, 'eth_getBlockByNumber')
            ref = block(int(params[0], 16))
            return dict(number=params[0], hash=ref['hash'], parentHash=ref['parent_hash'],
                        timestamp=hex(ref['timestamp_ms']//1000))

        with patch('public_evm_window.rpc', side_effect=read_witness):
            observed = witness_bsc_swaps(target, records)
            self.assertEqual((observed['count'], observed['first_swap']), (1, swap))
            replies[:] = [dict(reply, blockHash=last['hash'])]
            with self.assertRaisesRegex(ValueError, 'WITNESS_LOG_IDENTITY'):
                witness_bsc_swaps(target, records)
            replies[:] = [reply, reply]
            with self.assertRaisesRegex(ValueError, 'WITNESS_LOG_IDENTITY'):
                witness_bsc_swaps(target, records)
            replies[:] = [reply]
            broken = copy.deepcopy(records)
            broken[1]['normalized_facts']['blocks'][0]['swaps'][0]['amount1_out'] = '18'
            with self.assertRaisesRegex(ValueError, 'WITNESS_FACTS'):
                witness_bsc_swaps(target, broken)
