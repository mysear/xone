import copy
from contextlib import ExitStack
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch, PropertyMock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import prime_signed_v2 as m
import signed_rpc_capture as capture_module


ENDPOINTS = tuple('http://127.0.0.1:' + str(19001 + index) for index in range(3))


class Clock:
    def __init__(self):
        self.now = 0

    def monotonic(self):
        return self.now

    def sleep(self, seconds):
        self.now += seconds


class Capture:
    endpoints = ENDPOINTS

    def __init__(self, clock):
        self.clock = clock
        self.state = dict(pending=0, misses=0, enqueued=0, completed=0, errors=0,
                          bytes=0, closed=False, storage_failed=False)
        self.waits = []
        self.on_wait = None

    def stats(self):
        return dict(self.state)

    def wait_pending(self, timeout=30):
        self.waits.append(timeout)
        if self.on_wait:
            return self.on_wait(timeout)
        self.state['pending'] = 0
        self.state['completed'] += 1
        return True


class Child:
    def __init__(self, plan, stream, clock):
        self.plan, self.stream, self.clock = plan, stream, clock
        self.returncode = plan.get('exit')
        self.terminated = self.killed = self.waited = 0

    def poll(self):
        return self.returncode

    def terminate(self):
        self.terminated += 1
        if not self.plan.get('ignore_term'):
            self.returncode = -15

    def kill(self):
        self.killed += 1
        self.returncode = -9

    def wait(self, timeout):
        self.waited += 1
        if self.returncode is None:
            self.clock.sleep(timeout)
            raise subprocess.TimeoutExpired('owner', timeout)
        return self.returncode


class PrimeSignedV2(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='xone-prime-test-')
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.proof = self.root / 'input.json'
        self.original = json.dumps({'finality': {'data': {'finalized_header': {'execution': {
            'block_number': '100', 'block_hash': '0x' + '12' * 32}}}}}).encode()
        self.proof.write_bytes(self.original)
        self.clock = Clock()
        self.capture = Capture(self.clock)
        self.children, self.directories, self.configs = [], [], []
        self.plans = []
        self.on_http = None
        self.output = self.root / 'result'

    @staticmethod
    def baseline():
        return {'status': 200, 'body': {'reference': {'block': {
            'chain_key': 'evm:1', 'number_or_slot': 100, 'hash': '0x' + '12' * 32}}}}

    def spawn(self, args, **kwargs):
        self.assertEqual(args[1], '--config')
        self.assertEqual(len(args), 3)
        self.assertTrue(kwargs['start_new_session'])
        self.assertEqual(kwargs['cwd'], m.ROOT / 'chain-parser-go')
        directory = Path(args[2]).parent
        self.directories.append(directory)
        self.assertEqual(directory.stat().st_mode & 0o777, 0o700)
        self.assertEqual(Path(args[2]).stat().st_mode & 0o777, 0o600)
        cfg = json.loads(Path(args[2]).read_bytes())
        self.configs.append(cfg)
        state = Path(cfg['state']['path'])
        state.mkdir(parents=True)
        (state / 'failed-state').write_bytes(b'not reusable owner state')
        plan = self.plans[len(self.children)]
        if 'stats' in plan:
            self.capture.state.update(plan['stats'])
        if 'action' in plan:
            plan['action']()
        stream = kwargs['stdout']
        stream.write(b'bounded tail ignores preceding failure diagnostics\n' * 2000)
        if 'code' in plan:
            stream.write(json.dumps({'event': 'service_start_failed',
                'fields': {'error_code': plan['code']}}).encode() + b'\n')
        stream.flush()
        child = Child(plan, stream, self.clock)
        self.children.append(child)
        return child

    def http(self, url, token, timeout):
        self.assertTrue(url.endswith('/internal/chains/evm:1/baseline'))
        self.assertGreater(timeout, 0)
        if self.on_http:
            return self.on_http()
        return self.baseline()

    def prime(self, **kwargs):
        with ExitStack() as stack:
            stack.enter_context(patch.object(m, 'free_port', return_value=19999))
            stack.enter_context(patch.object(m.subprocess, 'Popen', side_effect=self.spawn))
            stack.enter_context(patch.object(m, 'http', side_effect=self.http))
            stack.enter_context(patch.object(m.time, 'monotonic', self.clock.monotonic))
            stack.enter_context(patch.object(m.time, 'sleep', self.clock.sleep))
            return m.prime_current_bank('owner-binary', self.capture, self.proof,
                '0x' + '34' * 32, self.output, **kwargs)

    def assert_cleaned(self):
        self.assertTrue(all(not path.exists() for path in self.directories))
        self.assertTrue(all(child.stream.closed and child.waited for child in self.children))
        self.assertEqual({p.name for p in self.output.iterdir()},
                         {'signed-proof.json', 'prime-report.json'})
        self.assertLess((self.output / 'prime-report.json').stat().st_size, 4096)

    def test_pending_wait_then_matching_normal_owner_is_the_only_success(self):
        self.plans = [dict(exit=1, code='RECEIPT_WITNESS_INVALID',
            stats=dict(pending=1, misses=1, enqueued=1),
            action=lambda: self.proof.write_bytes(b'external refresh changed the original')),
            dict(exit=None)]
        result = self.prime()
        self.assertTrue(result['primed'])
        self.assertEqual(result['code'], 'PRIMED')
        self.assertEqual(result['attempts'], [
            dict(attempt=1, code='RECEIPT_WITNESS_INVALID', pending=1),
            dict(attempt=2, code='PRIMED', pending=0)])
        self.assertEqual(self.capture.waits, [30])
        self.assertEqual(Path(result['proof_file']).read_bytes(), self.original)
        self.assertEqual(self.output.stat().st_mode & 0o777, 0o700)
        self.assertEqual(Path(result['proof_file']).stat().st_mode & 0o777, 0o600)
        self.assertEqual((self.output / 'prime-report.json').stat().st_mode & 0o777, 0o600)
        for cfg in self.configs:
            self.assertEqual(cfg['chains'][0]['rpc_urls'], list(ENDPOINTS[:2]))
            self.assertEqual(cfg['chains'][0]['proof_rpc_url'], ENDPOINTS[2])
            self.assertEqual(cfg['chains'][0]['beacon_finality']['proof_file'], result['proof_file'])
            self.assertEqual(cfg['startup']['max_baseline_age_ms'], 3600000)
            self.assertFalse(cfg['history']['enabled'])
            self.assertEqual(cfg['journal']['transport'], 'http-development')
            self.assertEqual(cfg['journal']['request_timeout_ms'], 60000)
            self.assertEqual(cfg['journal']['raw_evidence'], m.RAW_LIMITS)
        self.assert_cleaned()

    def test_completed_new_job_still_retries_when_pending_already_zero(self):
        self.plans = [dict(exit=1, code='RPC_ERROR_-32000',
                          stats=dict(pending=0, misses=1, enqueued=1, completed=1)),
                      dict(exit=None)]
        result = self.prime()
        self.assertTrue(result['primed'])
        self.assertEqual(len(self.children), 2)
        self.assertEqual(self.capture.waits, [])
        self.assert_cleaned()

    def test_existing_pending_job_completing_during_attempt_allows_one_retry(self):
        self.capture.state.update(pending=1, misses=1, enqueued=1)
        self.plans = [dict(exit=1, code='RPC_ERROR_-32000',
                          stats=dict(pending=0, misses=2, enqueued=1, completed=1)),
                      dict(exit=None)]
        result = self.prime()
        self.assertTrue(result['primed'])
        self.assertEqual(len(self.children), 2)
        self.assertEqual(self.capture.waits, [])
        self.assert_cleaned()

    def test_semantic_failure_without_new_jobs_returns_its_exact_code(self):
        self.plans = [dict(exit=1, code='PROVENANCE_BEACON_FINALITY_AGE')]
        result = self.prime()
        self.assertFalse(result['primed'])
        self.assertEqual(result['code'], 'PROVENANCE_BEACON_FINALITY_AGE')
        self.assertEqual(len(self.children), 1)
        self.assertEqual(self.capture.waits, [])
        self.assert_cleaned()

    def test_wrong_binding_is_not_success_even_with_unrelated_capture_work(self):
        for key, value in [('hash', '0x' + '56' * 32), ('number_or_slot', 99),
                           ('number_or_slot', True), ('chain_key', 'evm:10')]:
            with self.subTest(key=key, value=value):
                self.output = self.root / ('result-' + str(len(self.children)))
                reply = self.baseline()
                reply['body']['reference']['block'][key] = value
                self.on_http = lambda: copy.deepcopy(reply)
                self.plans.append(dict(exit=None, stats=dict(pending=1, misses=1, enqueued=1)))
                result = self.prime()
                self.assertFalse(result['primed'])
                self.assertEqual(result['code'], 'SIGNED_PRIME_EXECUTION_BINDING')
                self.assert_cleaned()
        self.assertEqual(self.capture.waits, [])

    def test_exit_zero_without_http_baseline_does_not_prime(self):
        self.plans = [dict(exit=0)]
        self.on_http = lambda: self.fail('exited owner must not be polled')
        result = self.prime()
        self.assertFalse(result['primed'])
        self.assertEqual(result['code'], 'SIGNED_PRIME_CHILD_EXIT_0')
        self.assert_cleaned()

    def test_capture_close_and_budget_exception_always_stop_active_owner(self):
        for closed in (True, False):
            with self.subTest(closed=closed):
                self.output = self.root / ('closed-' + str(closed))
                self.capture.state['closed'] = False
                self.plans.append(dict(exit=None))
                def fail():
                    if closed:
                        self.capture.state['closed'] = True
                        return {'status': 503}
                    raise capture_module.CaptureError('CAPTURE_TOTAL_BUDGET')
                self.on_http = fail
                result = self.prime()
                self.assertFalse(result['primed'])
                self.assertEqual(result['code'], 'CAPTURE_CLOSED' if closed else 'CAPTURE_TOTAL_BUDGET')
                self.assertEqual(result['attempts'][0]['code'], result['code'])
                self.assertEqual(self.children[-1].terminated, 1)
                self.assert_cleaned()

    def test_capture_wait_exception_preserves_failure_and_removes_attempt_state(self):
        self.plans = [dict(exit=1, code='RPC_ERROR_-32000',
                          stats=dict(pending=1, misses=1, enqueued=1))]
        def fail(_):
            raise capture_module.CaptureError('CAPTURE_TOTAL_BUDGET')
        self.capture.on_wait = fail
        result = self.prime()
        self.assertFalse(result['primed'])
        self.assertEqual(result['code'], 'CAPTURE_TOTAL_BUDGET')
        self.assert_cleaned()

    def test_total_budget_bounds_polling_and_escalates_only_the_owned_child(self):
        self.plans = [dict(exit=None, ignore_term=True)]
        self.on_http = lambda: {'status': 503}
        result = self.prime(budget_seconds=4)
        self.assertFalse(result['primed'])
        self.assertEqual(result['code'], 'SIGNED_PRIME_TIMEOUT')
        self.assertLessEqual(self.clock.now, 4)
        self.assertEqual((self.children[0].terminated, self.children[0].killed), (1, 1))
        self.assert_cleaned()

    def test_pending_wait_obeys_total_budget_and_thirty_second_wait_limit(self):
        self.plans = [dict(exit=1, code='RPC_ERROR_-32000',
                          stats=dict(pending=1, misses=1, enqueued=1))]
        def blocked(timeout):
            self.clock.sleep(timeout)
            return False
        self.capture.on_wait = blocked
        result = self.prime(budget_seconds=100)
        self.assertFalse(result['primed'])
        self.assertEqual(result['code'], 'SIGNED_PRIME_TIMEOUT')
        self.assertTrue(self.capture.waits)
        self.assertTrue(all(0 < timeout <= 30 for timeout in self.capture.waits))
        self.assertLessEqual(self.clock.now, 100)
        self.assert_cleaned()

    def test_attempt_limit_and_keyboard_interrupt_both_clean_all_children(self):
        self.plans = [dict(exit=1, code='RPC_ERROR_-32000', stats=dict(misses=1, enqueued=1)),
                      dict(exit=1, code='RPC_ERROR_-32000', stats=dict(misses=2, enqueued=2))]
        result = self.prime(max_attempts=2)
        self.assertFalse(result['primed'])
        self.assertEqual(result['code'], 'SIGNED_PRIME_ATTEMPTS')
        self.assert_cleaned()
        self.output = self.root / 'interrupt'
        self.plans.append(dict(exit=None))
        self.on_http = lambda: (_ for _ in ()).throw(KeyboardInterrupt())
        with self.assertRaises(KeyboardInterrupt):
            self.prime()
        self.assertEqual(json.loads((self.output / 'prime-report.json').read_bytes())['code'],
                         'SIGNED_PRIME_CANCELLED')
        self.assert_cleaned()

    def test_permanent_real_capture_failure_does_not_retry_repeated_misses(self):
        def fail(*_):
            raise capture_module.CaptureError('CAPTURE_FETCH_FAILED')
        actual = capture_module.SignedRPCCapture(self.root / 'capture',
            ('https://body-a.example', 'https://body-b.example', 'https://proof.example'), fail)
        actual.start(serve=False)
        self.addCleanup(actual.close)
        self.capture = actual
        request = json.dumps(dict(jsonrpc='2.0', id=1, method='eth_chainId', params=[])).encode()
        def miss():
            actual.response(0, request)
            self.assertTrue(actual.wait_pending(1))
        self.plans = [dict(exit=1, code='RPC_ERROR_-32000', action=miss),
                      dict(exit=1, code='RPC_ERROR_-32000', action=miss)]
        with patch.object(capture_module.SignedRPCCapture, 'endpoints',
                          new_callable=PropertyMock, return_value=ENDPOINTS):
            result = self.prime()
        self.assertFalse(result['primed'])
        self.assertEqual(result['code'], 'RPC_ERROR_-32000')
        self.assertEqual(len(self.children), 2)
        self.assertEqual((actual.stats()['misses'], actual.stats()['enqueued']), (2, 1))
        self.assertFalse(actual.stats()['closed'])
        self.assert_cleaned()

    def test_fixed_proof_growth_is_bounded_and_never_marks_owner_primed(self):
        def grow():
            path = self.output / 'signed-proof.json'
            with path.open('wb') as stream:
                stream.truncate(m.PROOF_BYTES + 1)
        self.plans = [dict(exit=None, action=grow)]
        result = self.prime()
        self.assertFalse(result['primed'])
        self.assertEqual(result['code'], 'SIGNED_PRIME_PROOF_CHANGED')
        self.assertEqual(result['attempts'][0]['code'], 'SIGNED_PRIME_PROOF_CHANGED')
        self.assert_cleaned()

    def test_child_exiting_during_http_response_cannot_prime(self):
        self.plans = [dict(exit=None)]
        def exited():
            self.children[-1].returncode = 0
            return self.baseline()
        self.on_http = exited
        result = self.prime()
        self.assertFalse(result['primed'])
        self.assertEqual(result['code'], 'SIGNED_PRIME_CHILD_EXITED')
        self.assert_cleaned()

    def test_malformed_proof_and_out_of_range_limits_never_spawn(self):
        self.proof.write_text('{}')
        result = self.prime()
        self.assertFalse(result['primed'])
        self.assertEqual(result['code'], 'SIGNED_PRIME_PROOF_ENCODING')
        self.assertEqual(self.children, [])
        self.assert_cleaned()
        for kwargs in [dict(budget_seconds=901), dict(budget_seconds=0),
                       dict(max_attempts=97), dict(max_attempts=True)]:
            with self.subTest(kwargs=kwargs), self.assertRaisesRegex(ValueError, 'SIGNED_PRIME_LIMITS'):
                self.prime(**kwargs)


if __name__ == '__main__':
    unittest.main()
