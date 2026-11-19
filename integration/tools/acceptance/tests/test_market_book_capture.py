import copy
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch

ACCEPTANCE = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ACCEPTANCE))
sys.path.insert(0, str(ACCEPTANCE.parent / 'market'))
import capture_chainlink_book as book
from market_feed_vectors import synthetic, digest


class MarketBookTests(unittest.TestCase):
    def setUp(self):
        temp = tempfile.TemporaryDirectory()
        self.addCleanup(temp.cleanup)
        self.root = Path(temp.name)
        c = synthetic()
        self.p, self.read = c['policy'], c['market_feed']
        self.p2 = dict(self.p, token='0x' + '11' * 20)
        a = c['anchor']
        self.at = a['timestamp_ms']
        self.header = dict(hash=a['block_hash'], number=hex(a['block_number']),
                           parentHash=a['parent_hash'], timestamp=hex(self.at // 1000),
                           stateRoot='0x' + '44' * 32)
        self.calls = []
        self.mode = 'ok'
        owner = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *args):
                pass

            def do_POST(self):
                req = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                result = owner.answer(req['method'], req['params'])
                raw = json.dumps(dict(jsonrpc='2.0', id=req['id'], result=result)).encode()
                self.send_response(200)
                self.send_header('Content-Length', str(len(raw)))
                self.end_headers()
                self.wfile.write(raw)

        server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        self.url = 'http://127.0.0.1:' + str(server.server_port)
        self.binary = os.environ.get('XONE_MARKET_VALIDATOR', ACCEPTANCE.parents[2] /
                                     'integration/.runtime/design-review-target/debug/xone-market-price')

    def answer(self, method, params):
        self.calls.append((method, params))
        if method == 'eth_chainId':
            return '0x1'
        if method == 'eth_getBlockByNumber' and params[0] == '0x0':
            return dict(number='0x0', hash=book.GENESIS)
        if method == 'eth_getBlockByHash' and self.mode == 'changed':
            return dict(self.header, hash='0x' + 'ff' * 32)
        if method.startswith('eth_getBlockBy'):
            return self.header
        address = params[0] if method == 'eth_getCode' else params[0]['to']
        if address in [self.p['token'], self.p2['token']]:
            if address == self.p2['token'] and self.mode == 'invalid':
                return '0x'
            return self.read['token_decimals_return']
        for side in ['native_usd', 'token_usd']:
            feed, read = self.p['market_feed'][side], self.read[side]
            if address == feed['aggregator']:
                return read['aggregator_code']
            if address == feed['proxy']:
                if method == 'eth_getCode':
                    return read['proxy_code']
                name = {'0x245a7bfc':'aggregator', '0x313ce567':'decimals',
                        '0x7284e416':'description', '0xfeaf968c':'round'}[params[0]['data']]
                return read[name + '_return']
        raise AssertionError('unexpected RPC request')

    def run_book(self, policies=None, revision=7, end=None, label='result', **config_fields):
        config = self.root / 'config.json'
        config.write_text(json.dumps(dict(policies=[self.p, self.p2] if policies is None else policies, **config_fields)))
        with patch('time.time', return_value=(self.at + 1000) / 1000), \
             patch('time.time_ns', return_value=(end if end is not None else self.at + 2000) * 1_000_000):
            return book.capture_book(config, self.url, self.binary, self.root / label,
                                     revision, allow_loopback=True)

    def test_complete_request_and_independent_signing_messages(self):
        bundle = self.run_book()
        self.assertFalse(bundle['authorized'])
        self.assertEqual(bundle, json.loads((self.root/'result/signing-request.json').read_bytes()))
        self.assertEqual([r['observation']['token'] for r in bundle['signing_requests']],
                         [self.p['token'], self.p2['token']])
        for i, r in enumerate(bundle['signing_requests']):
            o = r['observation']
            self.assertEqual(o['publication_revision'], 7)
            self.assertEqual(o['anchor'], bundle['anchor'])
            self.assertEqual(o['policy_hash'], digest([self.p, self.p2][i]))
            self.assertEqual(r['signing_message_hex'], ('xone-native-token-price/1:' + digest(o)).encode().hex())
            self.assertEqual(r['capture_report_sha256'], book.file_digest(self.root/'result'/r['capture_report']))
        self.assertEqual(self.calls.count(('eth_getBlockByNumber', ['finalized', False])), 1)
        self.assertEqual(self.calls.count(('eth_getBlockByHash', [self.header['hash'], False])), 1)
        self.assertEqual(len(self.calls), 34)

    def test_changed_block_or_invalid_second_token_cannot_emit_partial_request(self):
        for mode, error in [('changed','MARKET_BLOCK_CHANGED'), ('invalid','MARKET_PRODUCT_VALIDATION_FAILED')]:
            self.mode = mode
            with self.subTest(mode=mode), self.assertRaisesRegex(ValueError, error):
                self.run_book(label=mode)
            self.assertTrue((self.root/mode/'capture-0/unsigned.json').is_file())
            self.assertFalse((self.root/mode/'signing-request.json').exists())

    def test_expiry_and_wall_clock_regression_at_handoff(self):
        for label, end in [('expired', self.at + 3599000), ('clock', self.at + 999)]:
            with self.subTest(label=label), self.assertRaisesRegex(ValueError, 'MARKET_BOOK_NOT_CURRENT'):
                self.run_book(policies=[self.p], label=label, end=end)
            self.assertFalse((self.root/label/'signing-request.json').exists())

    def test_invalid_policy_set_or_revision_stops_before_rpc(self):
        bad = copy.deepcopy(self.p2)
        bad['chain_key'] = 'evm:10'
        for rows, revision in [([],1), ([self.p]*2,1), ([self.p,bad],1), ([self.p],0),
                               ([self.p],True), ([self.p],2**64)]:
            with self.subTest(revision=revision), self.assertRaises(ValueError):
                self.run_book(rows, revision)
        self.assertEqual(self.calls, [])


if __name__ == '__main__':
    unittest.main()
