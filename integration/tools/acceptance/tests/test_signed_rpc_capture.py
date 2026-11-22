import concurrent.futures
from contextlib import nullcontext
import gzip
import json
import os
from pathlib import Path
import socket
import struct
import sys
import tempfile
import threading
import time
import types
import unittest
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import signed_rpc_capture as m


URLS = ('https://body.example/rpc?private=secret', 'https://proof.example/rpc')
RAW = b' { "result" : {"value": "0x01"}, "id" : 1, "jsonrpc" : "2.0" }\n'


def request(method='eth_chainId', params=None, **changes):
    value = dict(jsonrpc='2.0', id=1, method=method, params=[] if params is None else params)
    value.update(changes)
    return json.dumps(value, separators=(',', ':')).encode()


def message(raw):
    return json.loads(raw)['error']['message']


class CaptureTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory(prefix='xone-rpc-capture-test-')
        self.addCleanup(self.temporary.cleanup)
        self.directory = Path(self.temporary.name)/'cache'

    def capture(self, fetcher=lambda *_: RAW, upstreams=URLS, **limits):
        capture = m.SignedRPCCapture(self.directory, upstreams, fetcher)
        for key, value in limits.items():
            setattr(capture, key, value)
        capture.start(serve=False)  # Tests never open a socket or use HTTPS.
        self.addCleanup(capture.close)
        return capture

    def test_exact_bytes_are_durable_private_and_reopened_without_fetch(self):
        calls = []
        def fetch(url, method, params, cancel):
            calls.append((url, method, params, cancel.is_set()))
            return RAW
        capture = self.capture(fetch)
        self.assertEqual(message(capture.response(0, request())), 'CAPTURE_PENDING')
        self.assertTrue(capture.wait_pending(3))
        self.assertEqual(capture.response(0, request()), RAW)
        self.assertEqual(calls, [(URLS[0], 'eth_chainId', [], False)])
        stats = capture.stats()
        self.assertEqual((stats['pending'], stats['completed'], stats['enqueued'], stats['bytes']),
                         (0, 1, 1, len(RAW)))
        self.assertEqual(stats['reserved_bytes'], 0)
        self.assertEqual(os.stat(self.directory).st_mode & 0o777, 0o700)
        files = list(self.directory.iterdir())
        self.assertEqual(len(files), 1)
        self.assertEqual(os.stat(files[0]).st_mode & 0o777, 0o600)
        encoded = files[0].read_bytes()
        self.assertNotIn(b'secret', encoded)
        capture.close()
        reopened = self.capture(lambda *_: self.fail('reopened hit performed network acquisition'))
        reopened.freeze()
        self.assertEqual(reopened.response(0, request()), RAW)
        self.assertEqual(reopened.stats()['enqueued'], 0)

    def test_pending_requests_deduplicate_and_keep_original_provider_domains(self):
        release, entered = threading.Event(), threading.Event()
        calls = []
        def fetch(url, method, params, cancel):
            calls.append(url)
            entered.set()
            release.wait(2)
            return RAW
        capture = self.capture(fetch, upstreams=(URLS[0], URLS[1], URLS[0]))
        self.assertEqual(message(capture.response(0, request())), 'CAPTURE_PENDING')
        self.assertTrue(entered.wait(1))
        with concurrent.futures.ThreadPoolExecutor(8) as pool:
            answers = list(pool.map(lambda _: capture.response(2, request()), range(20)))
        self.assertTrue(all(message(answer) == 'CAPTURE_PENDING' for answer in answers))
        self.assertEqual(capture.stats()['enqueued'], 1)
        self.assertEqual(capture.stats()['misses'], 21)
        with self.assertRaisesRegex(m.CaptureError, 'CAPTURE_PENDING'):
            capture.freeze()
        release.set()
        self.assertTrue(capture.wait_pending(3))
        self.assertEqual(capture.response(2, request()), RAW)
        self.assertEqual(capture.providers[0], capture.providers[2])
        self.assertNotEqual(capture.providers[0], capture.providers[1])
        self.assertEqual(calls, [URLS[0]])

    def test_invalid_ids_write_methods_and_large_requests_never_enqueue(self):
        capture = self.capture(lambda *_: self.fail('invalid request reached fetcher'))
        rows = [request(id=2), request(id='1'), request(id=True), request(id=1.0),
                request(jsonrpc='1.0'), request('eth_sendRawTransaction', ['0x00']),
                b'{"jsonrpc":"2.0","id":1,"id":1,"method":"eth_chainId","params":[]}',
                b'x'*(m.REQUEST_BYTES+1), request(params='invalid')]
        for raw in rows:
            with self.subTest(raw=raw[:70]):
                self.assertIn('error', json.loads(capture.response(0, raw)))
        self.assertEqual(capture.stats()['enqueued'], 0)

    def test_response_id_errors_null_duplicate_json_and_size_are_never_cached(self):
        rows = [b'{"jsonrpc":"2.0","id":2,"result":"0x1"}',
                b'{"jsonrpc":"2.0","id":true,"result":"0x1"}',
                b'{"jsonrpc":"2.0","id":1,"result":null}',
                b'{"jsonrpc":"2.0","id":1,"result":1,"error":{}}',
                b'{"jsonrpc":"2.0","id":1,"result":1,"result":2}', b'not-json',
                b'x'*1025]
        capture = self.capture(lambda url, method, params, cancel: rows[params[0]],
                               MAX_RESPONSE_BYTES=1024)
        for index in range(len(rows)):
            self.assertEqual(message(capture.response(0, request(params=[index]))), 'CAPTURE_PENDING')
        self.assertTrue(capture.wait_pending(3))
        self.assertEqual(capture.stats()['completed'], 0)
        self.assertEqual(capture.stats()['errors'], len(rows))
        self.assertEqual(list(self.directory.iterdir()), [])
        self.assertNotEqual(message(capture.response(0, request(params=[0]))), 'CAPTURE_PENDING')

    def test_reservations_enforce_pending_record_decoded_and_disk_budgets(self):
        entered, release = threading.Event(), threading.Event()
        def fetch(*_):
            entered.set()
            release.wait(2)
            return RAW
        capture = self.capture(fetch, MAX_RESPONSE_BYTES=1024, MAX_PENDING=1,
                               MAX_DECODED_BYTES=1024)
        self.assertEqual(message(capture.response(0, request())), 'CAPTURE_PENDING')
        self.assertTrue(entered.wait(1))
        self.assertEqual(capture.stats()['reserved_bytes'], 1024)
        self.assertEqual(message(capture.response(0, request(params=[1]))), 'CAPTURE_PENDING_BUDGET')
        capture.MAX_PENDING = 64
        self.assertEqual(message(capture.response(0, request(params=[1]))), 'CAPTURE_BYTES_BUDGET')
        release.set()
        self.assertTrue(capture.wait_pending(3))
        capture.MAX_DECODED_BYTES = 4096
        capture.MAX_RECORDS = 1
        self.assertEqual(message(capture.response(0, request(params=[2]))), 'CAPTURE_RECORD_BUDGET')
        capture.MAX_RECORDS = 10
        capture.MAX_DISK_BYTES = capture.stats()['disk_bytes']
        self.assertEqual(message(capture.response(0, request(params=[3]))), 'CAPTURE_BYTES_BUDGET')
        self.assertEqual(capture.stats()['enqueued'], 1)

    def test_freeze_miss_never_fetches_and_wait_is_bounded(self):
        capture = self.capture()
        capture.response(0, request())
        self.assertTrue(capture.wait_pending(3))
        capture.freeze()
        self.assertEqual(capture.response(0, request()), RAW)
        self.assertEqual(message(capture.response(1, request())), 'UNRECORDED')
        self.assertEqual(capture.stats()['enqueued'], 1)
        for timeout in [-1, 31, True]:
            with self.assertRaisesRegex(m.CaptureError, 'CAPTURE_WAIT_BUDGET'):
                capture.wait_pending(timeout)

    def test_corruption_wrong_filename_and_gzip_bomb_fail_closed(self):
        capture = self.capture(MAX_RESPONSE_BYTES=1024)
        capture.response(0, request())
        self.assertTrue(capture.wait_pending(3))
        path = next(self.directory.glob('*.rpc'))
        original = path.read_bytes()
        damaged = bytearray(original)
        damaged[-1] ^= 1
        path.write_bytes(damaged)
        self.assertEqual(message(capture.response(0, request())), 'CAPTURE_GZIP_HASH')
        path.write_bytes(original)
        wrong = path.with_name('f'*64+'.rpc')
        path.rename(wrong)
        with self.assertRaisesRegex(m.CaptureError, 'CAPTURE_RECORD_BINDING'):
            capture._read_file(wrong)
        wrong.rename(path)
        prefix = len(m.MAGIC)
        length = struct.unpack('>I', original[prefix:prefix+4])[0]
        envelope = json.loads(original[prefix+4:prefix+4+length])
        compressed = gzip.compress(b'x'*2048, mtime=0)
        metadata = envelope['metadata']
        metadata.update(gzip_bytes=len(compressed), gzip_sha256=m.digest(compressed))
        encoded = m.canonical(dict(metadata=metadata, sha256=m.digest(m.canonical(metadata).encode()))).encode()
        path.write_bytes(m.MAGIC+struct.pack('>I', len(encoded))+encoded+compressed)
        self.assertEqual(message(capture.response(0, request())), 'CAPTURE_RESPONSE_HASH')
        self.assertEqual(capture.stats()['enqueued'], 1)

    def test_close_signals_inflight_fetcher_and_never_publishes_late_response(self):
        entered, cancelled = threading.Event(), threading.Event()
        def fetch(url, method, params, cancel):
            entered.set()
            if cancel.wait(2):
                cancelled.set()
            return RAW
        capture = self.capture(fetch)
        capture.response(0, request())
        self.assertTrue(entered.wait(1))
        capture.close()
        self.assertTrue(cancelled.is_set())
        self.assertTrue(all(not worker.is_alive() for worker in capture.workers))
        self.assertEqual(capture.stats()['pending'], 0)
        self.assertEqual(capture.stats()['reserved_bytes'], 0)
        self.assertEqual(capture.stats()['completed'], 0)
        self.assertEqual(list(self.directory.iterdir()), [])
        self.assertEqual(message(capture.response(0, request())), 'CAPTURE_CLOSED')

    def test_fetcher_diagnostics_are_hashed_without_credentials(self):
        def fetch(*_):
            raise RuntimeError('https://private.example/?key=private-secret')
        capture = self.capture(fetch)
        capture.response(0, request())
        self.assertTrue(capture.wait_pending(3))
        self.assertNotIn('private-secret', json.dumps(capture.stats()))
        self.assertEqual(capture.stats()['error_codes'], {'CAPTURE_FETCH_INVALID': 1})

    def test_mapping_uses_separate_path_free_ports_and_original_provider_ids(self):
        capture = self.capture(upstreams=(URLS[0], URLS[1], URLS[0]))
        capture.servers = [types.SimpleNamespace(server_port=port) for port in (30001, 30002, 30003)]
        try:
            self.assertEqual(capture.endpoints,
                             ('http://127.0.0.1:30001', 'http://127.0.0.1:30002', 'http://127.0.0.1:30003'))
            self.assertEqual([row['provider_id'] for row in capture.mapping()], list(capture.providers))
            self.assertNotIn('secret', json.dumps(capture.mapping()))
        finally:
            capture.servers = []

    def test_request_inventory_validates_completed_records_and_copies_params(self):
        capture = self.capture(upstreams=(URLS[0], URLS[1], URLS[0]))
        params = [{'blockHash': '0x1234', 'keys': ['0x0']}]
        capture.response(2, request('eth_getProof', params))
        self.assertTrue(capture.wait_pending(3))
        capture.freeze()
        before_stats = capture.stats()
        before_files = {path.name: path.read_bytes() for path in self.directory.iterdir()}
        rows = capture.request_inventory()
        self.assertEqual(rows, [{
            'provider_id': capture.providers[0], 'source_index': 0,
            'method': 'eth_getProof', 'params': params,
            'response_sha256': m.digest(RAW), 'response_bytes': len(RAW),
        }])
        self.assertNotIn('secret', json.dumps(rows))
        rows[0]['params'][0]['keys'].append('changed')
        rows[0]['method'] = 'changed'
        self.assertEqual(capture.request_inventory()[0]['params'], params)
        self.assertEqual(capture.stats(), before_stats)
        self.assertEqual({path.name: path.read_bytes() for path in self.directory.iterdir()}, before_files)

    def test_request_inventory_excludes_pending_never_fetches_and_rejects_closed_or_failed(self):
        entered, release = threading.Event(), threading.Event()
        def fetch(*_):
            entered.set()
            release.wait(2)
            return RAW
        capture = self.capture(fetch)
        capture.response(0, request())
        self.assertTrue(entered.wait(1))
        before = capture.stats()
        self.assertEqual(capture.request_inventory(), [])
        self.assertEqual(capture.stats(), before)
        release.set()
        self.assertTrue(capture.wait_pending(3))
        capture.storage_failed = True
        with self.assertRaisesRegex(m.CaptureError, 'CAPTURE_STORAGE_FAILED'):
            capture.request_inventory()
        capture.storage_failed = False
        capture.close()
        with self.assertRaisesRegex(m.CaptureError, 'CAPTURE_CLOSED'):
            capture.request_inventory()

    def test_request_inventory_rejects_corrupt_retained_response_without_fetch(self):
        capture = self.capture()
        capture.response(0, request())
        self.assertTrue(capture.wait_pending(3))
        path = next(self.directory.glob('*.rpc'))
        damaged = bytearray(path.read_bytes())
        damaged[-1] ^= 1
        path.write_bytes(damaged)
        before = capture.stats()
        with self.assertRaisesRegex(m.CaptureError, 'CAPTURE_GZIP_HASH'):
            capture.request_inventory()
        self.assertEqual(capture.stats(), before)

    def test_failed_keys_hold_record_slots_and_storage_failure_stops_new_writes(self):
        capture = self.capture(lambda *_: b'bad-json', MAX_RECORDS=1)
        capture.response(0, request())
        self.assertTrue(capture.wait_pending(3))
        self.assertEqual(message(capture.response(0, request(params=[1]))), 'CAPTURE_RECORD_BUDGET')
        capture.close()
        capture = self.capture()
        with mock.patch.object(m.os, 'fsync', side_effect=OSError('private-storage-detail')):
            capture.response(0, request())
            self.assertTrue(capture.wait_pending(3))
        self.assertTrue(capture.stats()['storage_failed'])
        self.assertEqual(message(capture.response(0, request(params=[2]))), 'CAPTURE_STORAGE_FAILED')
        self.assertEqual(list(self.directory.iterdir()), [])
        self.assertNotIn('private-storage-detail', json.dumps(capture.stats()))

    def test_invalid_json_constants_are_not_valid_rpc_bytes(self):
        capture = self.capture(lambda *_: b'{"jsonrpc":"2.0","id":1,"result":NaN}')
        self.assertEqual(message(capture.response(0, request(params=[float('nan')]))), 'CAPTURE_JSON_ENCODING')
        capture.response(0, request())
        self.assertTrue(capture.wait_pending(3))
        self.assertEqual(capture.stats()['completed'], 0)

    def test_empty_and_partial_get_headers_cannot_block_close(self):
        capture = m.SignedRPCCapture(self.directory, URLS, lambda *_: RAW).start()
        self.addCleanup(capture.close)
        entered = [threading.Event(), threading.Event()]
        sockets = []
        with __import__('contextlib').ExitStack() as stack:
            for server, event in zip(capture.servers, entered):
                handler = server.RequestHandlerClass
                original = handler.setup
                def setup(instance, original=original, event=event):
                    original(instance)
                    event.set()
                stack.enter_context(mock.patch.object(handler, 'setup', setup))
                connection = socket.create_connection(('127.0.0.1', server.server_port), timeout=1)
                sockets.append(connection)
                stack.callback(connection.close)
            # Both are real accepted connections. The first sends no request
            # line; the second stalls while parsing an unsupported GET header.
            sockets[1].sendall(b'GET / HTTP/1.1\r\nHost: localhost\r\nX-Pending:')
            self.assertTrue(all(event.wait(1) for event in entered))
            started = time.monotonic()
            capture.close()
            self.assertLess(time.monotonic() - started, 3.5)
        self.assertTrue(all(not thread.is_alive() for thread in capture.server_threads))
        self.assertTrue(all(not worker.is_alive() for worker in capture.workers))
        self.assertEqual(capture.stats()['fetches'], 0)


class CurlPipeTests(unittest.TestCase):
    def fetch_with_local_child(self, script, cancel=None, after_spawn=None, pace=None, **patches):
        """Exercise real nonblocking pipes/child cancellation, never curl/network."""
        children = []
        self.last_fetch_children = children
        self.last_curl_args = []
        scripts = None if isinstance(script, str) else list(script)
        original = m.subprocess.Popen
        def spawn(args, **kwargs):
            self.assertEqual(args[0:2], ['curl', '--disable'])
            self.assertEqual(args[-2:], ['--url', URLS[0]])
            self.assertIn('@-', args)
            self.assertTrue(kwargs['start_new_session'])
            self.last_curl_args.append(list(args))
            if scripts is not None:
                self.assertLess(len(children), len(scripts), 'unexpected extra transport retry')
            selected = script if scripts is None else scripts[len(children)]
            child = original([sys.executable, '-B', '-c', selected], **kwargs)
            children.append(child)
            if after_spawn is not None:
                after_spawn(child)
            return child
        constants = mock.patch.multiple(m, **patches) if patches else nullcontext()
        with mock.patch.object(m.subprocess, 'Popen', side_effect=spawn), constants:
            try:
                return m.curl_fetch(URLS[0], 'eth_chainId', [], cancel or threading.Event(), pace)
            finally:
                self.assertTrue(children)
                self.assertTrue(all(child.poll() is not None for child in children))

    def test_original_response_and_id_are_not_reserialized(self):
        script = 'import sys,json;r=json.load(sys.stdin);assert type(r["id"]) is int and r["id"]==1;sys.stdout.buffer.write(' + repr(RAW) + ')'
        self.assertEqual(self.fetch_with_local_child(script), RAW)

    def test_rate_response_defers_same_host_using_header_and_keeps_original_success_bytes(self):
        pace = mock.Mock(wraps=m.ProviderPace(interval=0))
        pace.defer.side_effect = lambda _: None
        limited = 'import sys;sys.stdin.read();sys.stderr.write("\\nXONE-HTTP 429\\n45\\n");sys.exit(22)'
        self.assertEqual(self.fetch_with_local_child([limited, self.successful_response()], pace=pace), RAW)
        pace.defer.assert_called_once_with(45)
        self.assertEqual(pace.acquire.call_count, 2)
        self.assertIn('%header{retry-after}', self.last_curl_args[0][self.last_curl_args[0].index('--write-out') + 1])
        with mock.patch.object(m.time, 'time', return_value=0):
            self.assertEqual(m._retry_seconds(b'Thu, 01 Jan 1970 00:01:05 GMT'), 65)
        self.assertEqual(m._retry_seconds(b'99999999999999999999'), float('inf'))

    def test_shared_provider_cooldown_never_extends_fetch_deadline_or_delays_another_host(self):
        capture = m.SignedRPCCapture(Path('unused'), (URLS[0], URLS[1], URLS[0]+'/proof'))
        lanes = []
        with mock.patch.object(m, 'curl_fetch', side_effect=lambda *args: lanes.append(args[-1])):
            for url in capture.upstreams:
                capture.fetcher(url, 'eth_chainId', [], threading.Event())
        self.assertIs(lanes[0], lanes[2])
        self.assertIsNot(lanes[0], lanes[1])
        lanes[0].defer(60)
        with self.assertRaisesRegex(m.CaptureError, 'CAPTURE_FETCH_TIMEOUT'):
            lanes[2].acquire(threading.Event(), time.monotonic()+.05, time.time()+.05)
        lanes[1].acquire(threading.Event(), time.monotonic()+1, time.time()+1)
        cancel = threading.Event()
        cancel.set()
        with self.assertRaisesRegex(m.CaptureError, 'CAPTURE_CANCELLED'):
            lanes[2].acquire(cancel, time.monotonic()+60, time.time()+60)

    def test_forbidden_http_response_is_terminal(self):
        forbidden = 'import sys;sys.stdin.read();sys.stderr.write("\\nXONE-HTTP 403\\n\\n");sys.exit(22)'
        with self.assertRaisesRegex(m.CaptureError, 'CAPTURE_HTTP_403'):
            self.fetch_with_local_child([forbidden])

    def test_decoded_stdout_stderr_and_wall_time_are_bounded(self):
        for script, patches, code in [
            ('import sys;sys.stdin.read();sys.stdout.buffer.write(b"x"*8192)',
             {'RESPONSE_BYTES': 128}, 'CAPTURE_RESPONSE_BUDGET'),
            ('import sys;sys.stdin.read();sys.stderr.buffer.write(b"secret"*8192)',
             {'STDERR_BYTES': 128}, 'CAPTURE_STDERR_BUDGET'),
            ('import sys,time;sys.stdin.read();time.sleep(10)',
             {'FETCH_SECONDS': .15}, 'CAPTURE_FETCH_TIMEOUT'),
        ]:
            with self.subTest(code=code), self.assertRaises(m.CaptureError) as raised:
                self.fetch_with_local_child(script, **patches)
            self.assertEqual(raised.exception.code, code)
            self.assertEqual(len(raised.exception.detail_hash), 64)
            self.assertNotIn('secret', str(raised.exception))

    def test_cancel_stops_only_its_own_child(self):
        cancel = threading.Event()
        timer = threading.Timer(.15, cancel.set)
        timer.start()
        try:
            with self.assertRaisesRegex(m.CaptureError, 'CAPTURE_CANCELLED'):
                self.fetch_with_local_child('import sys,time;sys.stdin.read();time.sleep(10)', cancel)
        finally:
            timer.cancel()

    @staticmethod
    def failed_transport(stdout=b'', stderr=b'upstream HTTP 504', delay=0):
        return ('import sys,time;sys.stdin.buffer.read();time.sleep(' + repr(delay) + ');'
                'sys.stdout.buffer.write(' + repr(stdout) + ');sys.stdout.buffer.flush();'
                'sys.stderr.buffer.write(' + repr(stderr) + ');sys.stderr.buffer.flush();sys.exit(56)')

    @staticmethod
    def successful_response(body=RAW):
        return 'import sys;sys.stdin.buffer.read();sys.stdout.buffer.write(' + repr(body) + ')'

    def test_two_transport_failures_then_third_success_preserve_only_success_bytes(self):
        scripts = [self.failed_transport(b'failed-prefix-one'),
                   self.failed_transport(b'failed-prefix-two'), self.successful_response()]
        self.assertEqual(self.fetch_with_local_child(scripts), RAW)
        self.assertEqual(len(self.last_fetch_children), 3)

    def test_three_transport_failures_exhaust_attempt_limit_and_hash_diagnostics(self):
        script = self.failed_transport(b'partial', b'private-upstream-504')
        with self.assertRaises(m.CaptureError) as raised:
            self.fetch_with_local_child([script, script, script])
        self.assertEqual(raised.exception.code, 'CAPTURE_FETCH_FAILED')
        self.assertEqual(len(self.last_fetch_children), 3)
        self.assertEqual(raised.exception.detail_hash, m.digest(b'private-upstream-504'))
        self.assertNotIn('private-upstream', str(raised.exception))

    def test_partial_failed_outputs_share_one_response_budget_with_later_attempts(self):
        cases = [([self.failed_transport(b'a'*5, b''), self.failed_transport(b'b'*5, b''),
                   self.successful_response()], 9),
                 ([self.failed_transport(b'a'*4, b''), self.successful_response()], len(RAW)+3)]
        for scripts, budget in cases:
            with self.subTest(budget=budget), self.assertRaises(m.CaptureError) as raised:
                self.fetch_with_local_child(scripts, RESPONSE_BYTES=budget)
            self.assertEqual(raised.exception.code, 'CAPTURE_RESPONSE_BUDGET')
            self.assertEqual(len(self.last_fetch_children), 2)

    def test_failed_attempt_stderr_shares_one_budget_and_cannot_trigger_more_retries(self):
        script = self.failed_transport(stderr=b'e'*5)
        with self.assertRaises(m.CaptureError) as raised:
            self.fetch_with_local_child([script, script, self.successful_response()], STDERR_BYTES=9)
        self.assertEqual(raised.exception.code, 'CAPTURE_STDERR_BUDGET')
        self.assertEqual(len(self.last_fetch_children), 2)

    def test_retries_share_original_wall_deadline_and_curl_receives_only_remaining_time(self):
        started = time.monotonic()
        real_wall, backward = time.time, [0]
        scripts = [self.failed_transport(delay=.05),
                   'import sys,time;sys.stdin.buffer.read();time.sleep(10)']
        # A wall-clock rollback after launch cannot extend the monotonic bound.
        with mock.patch.object(m.time, 'time', side_effect=lambda: real_wall()-backward[0]), \
                self.assertRaises(m.CaptureError) as raised:
            self.fetch_with_local_child(scripts, FETCH_SECONDS=1.2,
                after_spawn=lambda _: backward.__setitem__(0, 3600))
        self.assertEqual(raised.exception.code, 'CAPTURE_FETCH_TIMEOUT')
        self.assertEqual(len(self.last_fetch_children), 2)
        limits = [float(args[args.index('--max-time')+1]) for args in self.last_curl_args]
        self.assertLessEqual(limits[0], 1.2)
        self.assertLess(limits[1], limits[0]-.2)
        self.assertLess(time.monotonic()-started, 2.5)

    def test_wall_forward_jump_after_real_child_launch_times_out_without_retry(self):
        real_wall, forward = time.time, [0]
        cancel = threading.Event()
        # Bound an intentionally broken implementation too; cancellation would
        # produce the wrong error and fail the assertion rather than hang.
        fallback = threading.Timer(1.5, cancel.set)
        fallback.start()
        started = time.monotonic()
        try:
            with mock.patch.object(m.time, 'time', side_effect=lambda: real_wall()+forward[0]), \
                    self.assertRaises(m.CaptureError) as raised:
                self.fetch_with_local_child('import sys,time;sys.stdin.buffer.read();time.sleep(10)',
                    cancel, after_spawn=lambda _: forward.__setitem__(0, m.FETCH_SECONDS+1))
            self.assertEqual(raised.exception.code, 'CAPTURE_FETCH_TIMEOUT')
            self.assertEqual(len(self.last_fetch_children), 1)
            self.assertLess(time.monotonic()-started, 1.5)
        finally:
            fallback.cancel()
            fallback.join(timeout=2)

    def test_cancellation_during_transport_backoff_prevents_next_child(self):
        cancel, backoff = threading.Event(), threading.Event()
        real_wait = cancel.wait
        def observed_wait(seconds=None):
            if seconds is not None and seconds >= .2:
                backoff.set()
            return real_wait(seconds)
        def cancel_backoff():
            if backoff.wait(2):
                cancel.set()
        worker = threading.Thread(target=cancel_backoff)
        worker.start()
        try:
            with mock.patch.object(cancel, 'wait', side_effect=observed_wait), \
                    self.assertRaises(m.CaptureError) as raised:
                self.fetch_with_local_child([self.failed_transport(), self.successful_response()], cancel)
            self.assertEqual(raised.exception.code, 'CAPTURE_CANCELLED')
            self.assertTrue(backoff.is_set())
            self.assertEqual(len(self.last_fetch_children), 1)
        finally:
            cancel.set()
            worker.join(timeout=3)
            self.assertFalse(worker.is_alive())

    def test_successful_http_jsonrpc_semantic_error_is_returned_once_without_transport_retry(self):
        semantic = b'{"jsonrpc":"2.0","id":1,"error":{"code":-32000,"message":"historical state unavailable"}}\n'
        self.assertEqual(self.fetch_with_local_child([
            self.successful_response(semantic), self.successful_response()]), semantic)
        self.assertEqual(len(self.last_fetch_children), 1)


if __name__ == '__main__':
    unittest.main()
