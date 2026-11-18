import importlib.util
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
from pathlib import Path
import tempfile
import threading
import time
import unittest

PATH = Path(__file__).resolve().parents[2] / 'market' / 'capture_chainlink.py'
spec = importlib.util.spec_from_file_location('approved_feed_capture', PATH)
capture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(capture)


class CollectorTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.directory = Path(self.temporary.name)
        self.mode = 'success'
        self.requests = []
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_POST(self):
                request = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                owner.requests.append(request)
                status = 200
                if owner.mode == 'slow':
                    time.sleep(.4)
                if owner.mode == 'duplicate':
                    raw = b'{"jsonrpc":"2.0","id":1,"id":1,"result":"0x1"}'
                elif owner.mode == 'error':
                    raw = json.dumps(dict(jsonrpc='2.0', id=request['id'], error=dict(code=-32000, message='NOT_TO_BE_ECHOED'))).encode()
                elif owner.mode == 'oversize':
                    raw = b'x' * ((256 << 10) + 2)
                elif owner.mode == 'redirect':
                    status, raw = 302, b''
                elif owner.mode == 'http-error':
                    status, raw = 429, b'PUBLIC_PROVIDER_RATE_LIMIT'
                else:
                    raw = json.dumps(dict(jsonrpc='2.0', id=request['id'], result='0x1')).encode()
                self.send_response(status)
                if status == 302:
                    self.send_header('Location', 'http://127.0.0.1:1/unexpected')
                self.send_header('Content-Length', str(len(raw)))
                self.end_headers()
                try:
                    self.wfile.write(raw)
                except (BrokenPipeError, ConnectionResetError):
                    pass

        self.server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        threading.Thread(target=self.server.serve_forever, daemon=True).start()
        self.url = 'http://127.0.0.1:' + str(self.server.server_port)

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()
        self.temporary.cleanup()

    def rpc(self, name='rpc'):
        return capture.RPC(self.url, self.directory / name, allow_loopback=True)

    def test_success_records_exact_raw_response_request_and_provider_hash(self):
        rpc = self.rpc()
        self.assertEqual(rpc.call('eth_chainId', []), '0x1')
        record = rpc.records[0]
        raw = (rpc.directory / record['response_file']).read_bytes()
        self.assertEqual(capture.digest(raw), record['response_sha256'])
        self.assertEqual(record['request'], self.requests[0])
        self.assertEqual(record['http_status'], 200)
        self.assertEqual(rpc.provider_id, 'rpc-' + capture.digest(self.url.encode()))

    def test_duplicate_error_oversize_and_redirect_do_not_become_zero_prices(self):
        for mode in ['duplicate', 'error', 'oversize', 'redirect']:
            self.mode = mode
            with self.subTest(mode=mode), self.assertRaises(ValueError):
                self.rpc(mode).call('eth_chainId', [])

    def test_http_failure_preserves_bounded_original_response_without_exception_text(self):
        self.mode = 'http-error'
        rpc = self.rpc()
        with self.assertRaisesRegex(ValueError, '^MARKET_RPC_HTTP_429$'):
            rpc.call('eth_chainId', [])
        self.assertEqual(rpc.records[0]['http_status'], 429)
        self.assertEqual((rpc.directory / '00.json').read_bytes(), b'PUBLIC_PROVIDER_RATE_LIMIT')

    def test_absolute_deadline_owns_and_reaps_slow_rpc_worker(self):
        self.mode = 'slow'
        rpc = self.rpc()
        rpc.deadline = time.monotonic() + .15
        before = time.monotonic()
        with self.assertRaisesRegex(ValueError, '^MARKET_RPC_TOTAL_DEADLINE$'):
            rpc.call('eth_chainId', [])
        self.assertLess(time.monotonic() - before, 1)
        self.assertEqual(rpc.records, [])

    def test_call_bytes_deadline_and_method_budgets_stop_before_extra_network_reads(self):
        rpc = self.rpc()
        for method in ['eth_sendRawTransaction', 'eth_estimateGas', 'debug_traceCall']:
            with self.assertRaisesRegex(ValueError, 'MARKET_RPC_CALL_BUDGET'):
                rpc.call(method, [])
        self.assertEqual(self.requests, [])
        rpc.records = [{}] * 32
        with self.assertRaises(ValueError):
            rpc.call('eth_chainId', [])
        rpc.records = []
        rpc.deadline = time.monotonic() - 1
        with self.assertRaisesRegex(ValueError, 'MARKET_RPC_TOTAL_DEADLINE'):
            rpc.call('eth_chainId', [])
        rpc.deadline = time.monotonic() + 2
        rpc.bytes = 2 << 20
        with self.assertRaisesRegex(ValueError, 'MARKET_RPC_TOTAL_BYTES'):
            rpc.call('eth_chainId', [])
        self.assertEqual(len(self.requests), 1)

    def test_endpoint_and_header_inputs_are_strict_and_never_expose_secrets(self):
        for url in ['http://public.example', self.url, 'https://u:password@example.com',
                    'https://example.com/?token=secret', 'https://example.com/#secret']:
            with self.assertRaises(ValueError) as caught:
                capture.RPC(url, self.directory / 'invalid')
            self.assertNotIn('secret', str(caught.exception))
            self.assertNotIn('password', str(caught.exception))
        for q in ['0x00', '0x01', '-1', '1', '0X1', '0xAB', 1, None]:
            with self.assertRaises(ValueError):
                capture.quantity(q)
        with self.assertRaises(ValueError):
            capture.header(dict(number='0x1', timestamp='0x1', hash='invalid'))


if __name__ == '__main__':
    unittest.main()
