import copy
import json
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import signed_rpc_window as m
from signed_rpc_capture import rpc_error


def block_hash(number):
    return '0x' + format(number, '064x')


def envelope(result):
    return json.dumps(dict(jsonrpc='2.0', id=1, result=result)).encode()


class Clock:
    def __init__(self):
        self.now = 0

    def monotonic(self):
        return self.now


class Capture:
    def __init__(self, clock):
        self.clock = clock
        self.providers = ['rpc-' + format(index + 1, '024x') for index in range(3)]
        self.inventory = []
        self.records, self.pending, self.failures = {}, {}, {}
        self.calls, self.fetched, self.waits = [], [], []
        self.closed = self.storage_failed = False
        self.external_pending = self.maximum = self.inventory_calls = 0
        self.release_external = True
        self.reject = []
        self.on_wait = self.header_mutation = self.permanent = None
        self.on_reject = None
        self.instant = False

    def row(self, source, method, params):
        row = dict(source_index=source, provider_id=self.providers[source], method=method,
                   params=copy.deepcopy(params), response_sha256='ab' * 32, response_bytes=100)
        self.inventory.append(row)

    def request_inventory(self):
        self.inventory_calls += 1
        return copy.deepcopy(self.inventory)

    def stats(self):
        return dict(pending=len(self.pending) + self.external_pending, closed=self.closed,
                    storage_failed=self.storage_failed, provider_ids=list(self.providers))

    def key(self, source, method, params):
        return self.providers[source], method, json.dumps(params, sort_keys=True)

    def result(self, job):
        source, method, params = job
        if method == 'eth_getBlockByNumber' and params[1] is False:
            number = int(params[0], 16)
            value = dict(number=hex(number), hash=block_hash(number), parentHash=block_hash(number-1),
                         transactions=[block_hash(number + 1000)])
            if self.header_mutation:
                self.header_mutation(number, value)
            return value
        # Distinct fetched results for each new request. No template response is
        # ever supplied to the planner, or installed as a new block's response.
        if method == 'eth_getBlockReceipts':
            return []
        return dict(actual_source=source, actual_method=method, actual_params=params)

    def complete(self, key, job):
        self.fetched.append(copy.deepcopy(job))
        if self.permanent and self.permanent(job):
            self.failures[key] = 'CAPTURE_FETCH_FAILED'
        else:
            self.records[key] = envelope(self.result(job))

    def response(self, source, raw):
        request = json.loads(raw)
        assert request['jsonrpc'] == '2.0' and request['id'] == 1
        method, params = request['method'], request['params']
        job = (source, method, params)
        self.calls.append(copy.deepcopy(job))
        key = self.key(*job)
        if self.closed:
            return rpc_error('CAPTURE_CLOSED')
        if key in self.records:
            return self.records[key]
        if key in self.pending:
            return rpc_error('CAPTURE_PENDING')
        if key in self.failures:
            return rpc_error(self.failures[key])
        if self.reject:
            code = self.reject.pop(0)
            if self.on_reject:
                self.on_reject()
            return rpc_error(code)
        if self.instant:
            self.complete(key, job)
            return self.records[key]
        self.pending[key] = job
        self.maximum = max(self.maximum, len(self.pending) + self.external_pending)
        return rpc_error('CAPTURE_PENDING')

    def wait_pending(self, timeout=30):
        self.waits.append(timeout)
        if self.on_wait:
            return self.on_wait(timeout)
        for key, job in list(self.pending.items()):
            self.complete(key, job)
            del self.pending[key]
        if self.release_external:
            self.external_pending = 0
        self.clock.now += min(.01, timeout)
        return not self.pending and not self.external_pending

    def freeze(self):
        raise AssertionError('the planner does not own capture freeze')

    def close(self):
        raise AssertionError('the planner does not own capture close')


class SignedRPCWindow(unittest.TestCase):
    def setUp(self):
        self.clock = Clock()
        self.capture = Capture(self.clock)
        self.template = dict(chain_key='evm:1', number_or_slot=100, hash=block_hash(100))
        old, tag = block_hash(100), hex(100)
        self.capture.row(0, 'eth_getBlockByNumber', [tag, True])
        self.capture.row(0, 'eth_getBlockByNumber', [tag, False])
        self.capture.row(1, 'eth_getBlockByNumber', [tag, True])
        self.capture.row(0, 'eth_getBlockReceipts', [old])
        self.capture.row(1, 'eth_getBlockReceipts', [old])
        self.capture.row(0, 'eth_getCode', ['address-code', tag])
        self.capture.row(1, 'eth_call', [{'to': 'address-call', 'data': old, 'value': '0x0'},
                                      dict(blockHash=old, requireCanonical=True)])
        self.capture.row(1, 'eth_getStorageAt', ['address-storage', old, tag])
        self.capture.row(2, 'eth_getProof', ['address-proof', [old, '0x0'],
                                          dict(blockHash=old, requireCanonical=True)])
        self.capture.row(0, 'eth_getRawTransactionByHash', [old])
        self.capture.row(1, 'eth_getTransactionReceipt', [old])
        self.capture.row(2, 'eth_chainId', [])
        self.capture.row(0, 'eth_getBlockByNumber', ['0x0', False])

    def prefill(self, first=101, last=103, **kwargs):
        expected = kwargs.pop('expected_last_hash', block_hash(last))
        with patch.object(m.time, 'monotonic', self.clock.monotonic):
            return m.prefill_signed_range(self.capture, self.template, first, last, expected,
                                         **kwargs)

    def test_cold_identity_requires_both_sources_to_match_the_chain_and_genesis(self):
        genesis = block_hash(0)
        for field in ('valid', 'chainId', 'genesis', 'number'):
            capture = Capture(self.clock)
            def response(job):
                index, method, params = job
                if method == 'eth_chainId':
                    self.assertEqual(params, [])
                    return '0x5' if index == 1 and field == 'chainId' else '0x1'
                self.assertEqual((method, params), ('eth_getBlockByNumber', ['0x0', False]))
                return dict(number='0x1' if index == 1 and field == 'number' else '0x0',
                            hash=block_hash(1) if index == 1 and field == 'genesis' else genesis)
            capture.result = response
            with self.subTest(field=field):
                if field == 'valid':
                    self.assertEqual(m.prefill_signed_identities(capture, genesis, 60), 4)
                else:
                    with self.assertRaisesRegex(ValueError, 'IDENTITY_CONFLICT'):
                        m.prefill_signed_identities(capture, genesis, 60)

    def test_observed_window_replaces_only_each_method_block_selector_and_source(self):
        original = copy.deepcopy(self.capture.inventory)
        result = self.prefill(expected_parent=block_hash(100))
        self.assertEqual(result, dict(planned=27, completed=27, skipped=4))
        self.assertEqual(self.capture.inventory, original)
        self.assertEqual(self.capture.inventory_calls, 1)
        self.assertEqual(self.capture.maximum, 8)
        self.assertTrue(all(0 < timeout <= 30 for timeout in self.capture.waits))
        self.assertEqual(len(self.capture.fetched), 27)
        for number in range(101, 104):
            expected = [(0, 'eth_getBlockByNumber', [hex(number), False])]
            for source in (0, 1):
                expected.extend([(source, 'eth_getBlockByNumber', [hex(number), True]),
                                 (source, 'eth_getBlockReceipts', [block_hash(number)])])
            expected.extend([
                (0, 'eth_getCode', ['address-code', hex(number)]),
                (1, 'eth_call', [{'to': 'address-call', 'data': block_hash(100), 'value': '0x0'},
                                 dict(blockHash=block_hash(number), requireCanonical=True)]),
                (1, 'eth_getStorageAt', ['address-storage', block_hash(100), hex(number)]),
                (2, 'eth_getProof', ['address-proof', [block_hash(100), '0x0'],
                                     dict(blockHash=block_hash(number), requireCanonical=True)])])
            for job in expected:
                self.assertIn(job, self.capture.fetched)
        first_body = next(index for index, (_, method, params) in enumerate(self.capture.fetched)
                          if method != 'eth_getBlockByNumber' or params[1] is True)
        self.assertEqual(first_body, 3)
        self.assertFalse(self.capture.closed)

    def test_single_fresh_height_uses_old_authenticated_bank_only_as_request_template(self):
        result = self.prefill(first=150, last=150, expected_parent=block_hash(149))
        self.assertEqual((result['planned'], result['completed']), (9, 9))
        self.assertIn((2, 'eth_getProof', ['address-proof', [block_hash(100), '0x0'],
            dict(blockHash=block_hash(150), requireCanonical=True)]), self.capture.fetched)
        self.assertNotIn((0, 'eth_getBlockByNumber', [hex(100), True]), self.capture.fetched)

    def test_broken_chain_or_last_hash_stops_before_any_body_or_state_fetch(self):
        for fault in ('middle-parent', 'first-parent', 'last-hash'):
            with self.subTest(fault=fault):
                self.setUp()
                if fault == 'middle-parent':
                    self.capture.header_mutation = lambda n, v: v.update(parentHash=block_hash(1)) if n == 102 else None
                expected = block_hash(999) if fault == 'last-hash' else block_hash(103)
                parent = block_hash(99) if fault == 'first-parent' else block_hash(100)
                code = 'SIGNED_WINDOW_LAST_HASH' if fault == 'last-hash' else 'SIGNED_WINDOW_PARENT_CONFLICT'
                with self.assertRaisesRegex(ValueError, code):
                    self.prefill(expected_last_hash=expected, expected_parent=parent)
                self.assertTrue(all(method == 'eth_getBlockByNumber' and params[1] is False
                                    for _, method, params in self.capture.fetched))

    def test_header_shape_wrong_height_null_or_duplicate_transactions_is_rejected(self):
        mutations = [lambda v: v.update(number='0x65'), lambda v: v.update(hash='bad'),
                     lambda v: v.update(parentHash=None), lambda v: v.pop('transactions'),
                     lambda v: v.update(transactions=None),
                     lambda v: v.update(transactions=[block_hash(1)] * 2)]
        for mutate in mutations:
            with self.subTest(mutate=mutate):
                self.setUp()
                self.capture.header_mutation = lambda n, v: mutate(v) if n == 102 else None
                with self.assertRaisesRegex(ValueError, 'SIGNED_WINDOW_HEADER_SHAPE'):
                    self.prefill()

    def test_capacity_pressure_waits_only_while_pending_can_release_reservations(self):
        for code in ('CAPTURE_PENDING_BUDGET', 'CAPTURE_BYTES_BUDGET'):
            with self.subTest(code=code):
                self.setUp()
                self.capture.external_pending = 1
                self.capture.reject = [code]
                result = self.prefill(first=101, last=101)
                self.assertEqual(result['planned'], result['completed'])
                self.assertTrue(self.capture.waits)
                self.assertLessEqual(self.capture.maximum, 8)
                self.setUp()
                self.capture.reject = [code]
                with self.assertRaisesRegex(ValueError, code):
                    self.prefill(first=101, last=101)
                self.assertEqual(self.capture.waits, [])

    def test_existing_eight_global_jobs_do_not_allow_a_ninth_submission(self):
        self.capture.external_pending = 8
        result = self.prefill()
        self.assertEqual(result['planned'], result['completed'])
        self.assertEqual(self.capture.maximum, 8)
        self.assertTrue(self.capture.waits)

    def test_eight_unrelated_pending_jobs_do_not_block_completed_cache_hits(self):
        self.prefill(first=101, last=101)
        for source, method, params in self.capture.fetched:
            self.capture.row(source, method, params)
        self.capture.external_pending = 8
        self.capture.release_external = False
        self.capture.waits.clear()
        self.capture.on_wait = lambda _: self.fail('complete cache hits waited for unrelated jobs')
        result = self.prefill(first=101, last=101)
        self.assertEqual((result['planned'], result['completed']), (9, 9))
        self.assertEqual(self.capture.waits, [])

    def test_capacity_rejection_followed_by_reservation_release_retries_once(self):
        for code in ('CAPTURE_PENDING_BUDGET', 'CAPTURE_BYTES_BUDGET'):
            with self.subTest(code=code):
                self.setUp()
                self.capture.external_pending = 1
                self.capture.reject = [code]
                self.capture.on_reject = lambda: setattr(self.capture, 'external_pending', 0)
                result = self.prefill(first=101, last=101)
                self.assertEqual((result['planned'], result['completed']), (9, 9))

    def test_own_completed_requests_do_not_wait_for_unrelated_global_pending(self):
        self.capture.external_pending = 1
        self.capture.release_external = False
        result = self.prefill()
        self.assertEqual(result['planned'], result['completed'])
        self.assertEqual(self.capture.stats()['pending'], 1)

    def test_permanent_failure_and_close_after_wait_cannot_become_completion(self):
        self.capture.permanent = lambda job: job[2] == [hex(101), False]
        with self.assertRaisesRegex(ValueError, 'CAPTURE_FETCH_FAILED'):
            self.prefill()
        self.assertEqual(len([j for j in self.capture.fetched if j[2] == [hex(101), False]]), 1)
        self.setUp()
        def closed(_):
            self.capture.closed = True
            self.capture.pending.clear()
            return True
        self.capture.on_wait = closed
        with self.assertRaisesRegex(ValueError, 'CAPTURE_CLOSED'):
            self.prefill()
        self.assertEqual(self.capture.fetched, [])

    def test_timeout_and_keyboard_cancel_stop_planning_without_owning_capture_close(self):
        def blocked(timeout):
            self.clock.now += timeout
            return False
        self.capture.on_wait = blocked
        with self.assertRaisesRegex(ValueError, 'SIGNED_WINDOW_TIMEOUT'):
            self.prefill(budget_seconds=.1)
        self.assertEqual(self.clock.now, .1)
        self.assertLessEqual(len(self.capture.pending), 8)
        self.assertFalse(self.capture.closed)
        self.setUp()
        self.capture.on_wait = lambda _: (_ for _ in ()).throw(KeyboardInterrupt())
        with self.assertRaises(KeyboardInterrupt):
            self.prefill()
        self.assertFalse(self.capture.closed)
        self.assertLessEqual(len(self.capture.pending), 8)

    def test_provider_domain_duplicates_share_one_logical_request(self):
        self.capture.providers[2] = self.capture.providers[0]
        for row in self.capture.inventory:
            if row['source_index'] == 2:
                row['provider_id'] = self.capture.providers[0]
        self.capture.row(2, 'eth_getCode', ['address-code', hex(100)])
        result = self.prefill(first=101, last=101)
        self.assertEqual(result['planned'], 9)
        self.assertEqual(len([j for j in self.capture.fetched if j[1] == 'eth_getCode']), 1)

    def test_suspension_and_wall_clock_rollback_cannot_extend_planning_budget(self):
        for forward in (True, False):
            with self.subTest(forward=forward):
                self.setUp()
                wall = [1000.0]
                def blocked(timeout):
                    wall[0] += 121 if forward else -121
                    if not forward:
                        self.clock.now += timeout
                    return False
                self.capture.on_wait = blocked
                with patch.object(m.time, 'time', side_effect=lambda: wall[0]), \
                        self.assertRaisesRegex(ValueError, 'SIGNED_WINDOW_TIMEOUT'):
                    self.prefill(budget_seconds=1)
                self.assertEqual(len(self.capture.waits), 1)
                self.assertEqual(self.capture.fetched, [])
                self.assertFalse(self.capture.closed)

    def test_bounds_and_invalid_template_fail_before_capture_reads(self):
        cases = [dict(first=True), dict(first=-1), dict(last=100), dict(last=229),
                 dict(last=1 << 64), dict(budget_seconds=0), dict(budget_seconds=1801)]
        for kwargs in cases:
            with self.subTest(kwargs=kwargs), self.assertRaisesRegex(ValueError, 'SIGNED_WINDOW_LIMITS'):
                self.prefill(**kwargs)
        self.template['chain_key'] = 'evm:10'
        with self.assertRaisesRegex(ValueError, 'SIGNED_WINDOW_BLOCK_BINDING'):
            self.prefill()
        self.assertEqual(self.capture.calls, [])
        self.assertEqual(self.capture.inventory_calls, 0)

    def test_unknown_dynamic_and_nonfinal_selectors_are_skipped_without_rewriting(self):
        self.capture.row(0, 'eth_call', [{'to': 'a', 'data': block_hash(100)}, hex(99)])
        self.capture.row(0, 'eth_call', [{'to': 'a'}, hex(100), {'stateOverride': block_hash(100)}])
        self.capture.row(1, 'eth_getProof', ['a', [block_hash(100)], {'blockNumber': hex(100)}])
        self.capture.row(1, 'eth_getProof', ['a', [], {'blockHash': block_hash(100), 'blockNumber': hex(100)}])
        self.capture.row(0, 'eth_getCode', ['a', {'blockHash': block_hash(100), 'unknown': hex(100)}])
        before = copy.deepcopy(self.capture.inventory)
        result = self.prefill(first=101, last=101)
        self.assertEqual((result['planned'], result['skipped']), (9, 9))
        self.assertEqual(self.capture.inventory, before)
        self.assertFalse(any(method in ('eth_getRawTransactionByHash', 'eth_getTransactionReceipt')
                             for _, method, _ in self.capture.calls))

    def test_permanent_record_budget_or_frozen_unrecorded_response_is_not_retried(self):
        for code in ('CAPTURE_RECORD_BUDGET', 'UNRECORDED', 'CAPTURE_CANCELLED'):
            with self.subTest(code=code):
                self.setUp()
                self.capture.external_pending = 1
                self.capture.reject = [code]
                with self.assertRaisesRegex(ValueError, code):
                    self.prefill()
                self.assertEqual(self.capture.waits, [])


if __name__ == '__main__':
    unittest.main()
