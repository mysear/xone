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
    def fetch_with_local_child(self, script, cancel=None, **patches):
        """Exercise real nonblocking pipes/child cancellation, never curl/network."""
        children = []
        original = m.subprocess.Popen
        def spawn(args, **kwargs):
            self.assertEqual(args[0:2], ['curl', '--disable'])
            self.assertEqual(args[-2:], ['--url', URLS[0]])
            self.assertIn('@-', args)
            self.assertTrue(kwargs['start_new_session'])
            child = original([sys.executable, '-B', '-c', script], **kwargs)
            children.append(child)
            return child
        constants = mock.patch.multiple(m, **patches) if patches else nullcontext()
        with mock.patch.object(m.subprocess, 'Popen', side_effect=spawn), constants:
            try:
                return m.curl_fetch(URLS[0], 'eth_chainId', [], cancel or threading.Event())
            finally:
                self.assertTrue(children)
                self.assertTrue(all(child.poll() is not None for child in children))

    def test_original_response_and_id_are_not_reserialized(self):
        script = 'import sys,json;r=json.load(sys.stdin);assert type(r["id"]) is int and r["id"]==1;sys.stdout.buffer.write(' + repr(RAW) + ')'
        self.assertEqual(self.fetch_with_local_child(script), RAW)

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


if __name__ == '__main__':
    unittest.main()
