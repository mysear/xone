import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import verify_beacon_owner as owner


class Clock:
    def __init__(self):
        self.now = 0
        self.sleeps = []

    def monotonic(self):
        return self.now

    def sleep(self, seconds):
        self.sleeps.append(seconds)
        self.now += seconds


class BeaconRefreshRetry(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory()
        self.addCleanup(self.directory.cleanup)
        self.path = Path(self.directory.name) / 'proof.json'
        self.path.write_bytes(self.proof(100))
        self.original = self.path.read_bytes()
        self.child = Mock()
        self.child.poll.return_value = None
        self.clock = Clock()
        self.retries = []

    @staticmethod
    def proof(number):
        return json.dumps({'finality': {'data': {'finalized_header': {'execution': {
            'block_number': str(number), 'block_hash': '0x' + format(number, '064x')}}}}}).encode()

    @staticmethod
    def declined(code):
        error = {'event': 'service_start_failed', 'fields': {'error_code': code}}
        return subprocess.CompletedProcess(['owner'], 1, b'', json.dumps(error).encode())

    def advanced(self, number=101):
        self.path.write_bytes(self.proof(number))
        return subprocess.CompletedProcess(['owner'], 0, json.dumps({
            'execution_number': number, 'execution_hash': '0x' + format(number, '064x')}).encode(), b'')

    def wait(self, run):
        with patch.object(owner.subprocess, 'run', side_effect=run) as invoked, \
                patch.object(owner.time, 'monotonic', self.clock.monotonic), \
                patch.object(owner.time, 'sleep', self.clock.sleep):
            result = owner.wait_for_next_finality('owner', 'config', self.path,
                'https://beacon.example', self.child, 100, self.retries.append)
        return result, invoked

    def test_network_then_participation_rejection_preserve_proof_until_verified_advance(self):
        codes = ['PROVENANCE_BEACON_REFRESH_NETWORK', 'PROVENANCE_BEACON_UPDATE_PARTICIPATION']
        calls = []
        def run(*args, **kwargs):
            self.assertEqual(self.path.read_bytes(), self.original)
            calls.append(kwargs)
            return self.declined(codes[len(calls)-1]) if len(calls) <= 2 else self.advanced()
        following, invoked = self.wait(run)
        self.assertEqual(following, json.loads(self.proof(101)))
        self.assertEqual(self.retries, codes)
        self.assertEqual(self.clock.sleeps, [12, 12])
        self.assertEqual(invoked.call_count, 3)
        self.assertTrue(all(call['timeout'] == 70 and call['check'] is False for call in calls))

    def test_same_verified_head_waits_then_actual_advance_returns_installed_proof(self):
        numbers = iter([100, 101])
        following, invoked = self.wait(lambda *a, **k: self.advanced(next(numbers)))
        self.assertEqual(following, json.loads(self.path.read_bytes()))
        self.assertEqual(self.clock.sleeps, [12])
        self.assertEqual(self.retries, [])
        self.assertEqual(invoked.call_count, 2)

    def test_invalid_signature_or_similar_error_codes_fail_immediately(self):
        for code in ['PROVENANCE_BEACON_UPDATE_SIGNATURE',
                     'PROVENANCE_BEACON_REFRESH_NETWORK_EXTRA',
                     'prefix PROVENANCE_BEACON_UPDATE_PARTICIPATION']:
            with self.subTest(code=code), self.assertRaises(subprocess.CalledProcessError):
                self.wait(lambda *a, **k: self.declined(code))
            self.assertEqual(self.path.read_bytes(), self.original)
            self.assertEqual(self.retries, [])
            self.assertEqual(self.clock.sleeps, [])

    def test_retryable_error_cannot_hide_replaced_proof(self):
        def run(*args, **kwargs):
            self.path.write_bytes(self.proof(101))
            return self.declined('PROVENANCE_BEACON_REFRESH_NETWORK')
        with self.assertRaisesRegex(ValueError, 'OWNER_REFRESH_REJECTED_PROOF_CHANGED'):
            self.wait(run)
        self.assertEqual(self.retries, [])
        self.assertEqual(self.clock.sleeps, [])

    def test_successful_cli_must_match_installed_hash_and_height_even_without_advance(self):
        for number in [100, 101]:
            for field in ['execution_number', 'execution_hash']:
                with self.subTest(number=number, field=field):
                    def run(*args, **kwargs):
                        reply = self.advanced(number)
                        binding = json.loads(reply.stdout)
                        binding[field] = number+1 if field == 'execution_number' else '0x' + 'ff'*32
                        reply.stdout = json.dumps(binding).encode()
                        return reply
                    with self.assertRaisesRegex(ValueError, 'OWNER_REFRESH_EXECUTION_BINDING'):
                        self.wait(run)
                    self.assertEqual(self.clock.sleeps, [])

    def test_owner_exit_before_or_during_cli_never_accepts_advance(self):
        self.child.poll.return_value = 1
        run = Mock()
        with self.assertRaisesRegex(ValueError, 'OWNER_NEXT_FINALITY_CHILD_EXITED'):
            self.wait(run)
        run.assert_not_called()
        self.child.poll.return_value = None
        def exited(*args, **kwargs):
            reply = self.advanced()
            self.child.poll.return_value = 1
            return reply
        with self.assertRaisesRegex(ValueError, 'OWNER_NEXT_FINALITY_CHILD_EXITED'):
            self.wait(exited)
        self.assertEqual(self.clock.sleeps, [])

    def test_malformed_success_binding_is_not_treated_as_no_advance(self):
        for output in [b'{}', b'[]', b'not-json', b'{"execution_number":101,"execution_number":100}']:
            with self.subTest(output=output):
                def run(*args, **kwargs):
                    reply = self.advanced()
                    reply.stdout = output
                    return reply
                with self.assertRaisesRegex(ValueError, 'OWNER_REFRESH_EXECUTION_BINDING'):
                    self.wait(run)
        self.assertEqual(self.clock.sleeps, [])

    def test_total_deadline_bounds_retries_and_last_cli_timeout(self):
        calls = []
        def run(*args, **kwargs):
            calls.append(kwargs['timeout'])
            return self.declined('PROVENANCE_BEACON_UPDATE_PARTICIPATION')
        with self.assertRaisesRegex(ValueError, 'OWNER_NEXT_FINALITY_TIMEOUT'):
            self.wait(run)
        self.assertEqual(self.clock.now, 900)
        self.assertEqual(len(calls), 75)
        self.assertEqual(calls[-1], 12)
        self.assertEqual(self.path.read_bytes(), self.original)

    def test_advance_after_deadline_and_unclassified_process_timeout_fail(self):
        def late(*args, **kwargs):
            self.clock.now = 901
            return self.advanced()
        with self.assertRaisesRegex(ValueError, 'OWNER_NEXT_FINALITY_TIMEOUT'):
            self.wait(late)
        self.clock.now = 0
        with self.assertRaises(subprocess.TimeoutExpired):
            self.wait(Mock(side_effect=subprocess.TimeoutExpired(['owner'], 70)))
        self.assertEqual(self.retries, [])


if __name__ == '__main__':
    unittest.main()
