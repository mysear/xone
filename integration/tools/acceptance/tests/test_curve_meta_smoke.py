"""Frozen MetaUSD RPC and wire checks; no parser/router/PG/Kafka processes."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

from curve_meta_fixture import code_bytes, dependency_code, implementation_code, profile
from curve_meta_smoke import (
    BASE_LP, BASE_POOL, Chain, FAULTS, FRESHNESS_MS, NAMES, PROCESS_CHECKS,
    TEMPLATE_BYTES, TEMPLATE_SHA256, bootstrap_snapshot, check_fixtures,
    configure_freshness, initial_report, protobuf_text, quote_request, require, word,
    validate_quote_result,
)


class CurveMetaReplayFixture(unittest.TestCase):
    def setUp(self):
        self.chain = Chain()

    def anchor(self, number=None):
        return {'blockHash': self.chain.block(self.chain.head if number is None else number)['hash'],
                'requireCanonical': True}

    def call(self, target, selector, arg=None, number=None):
        data = selector + (word(arg) if arg is not None else '')
        return self.chain.rpc('eth_call', [{'to': target, 'data': data}, self.anchor(number)])

    def code(self, target, number=None):
        return bytes.fromhex(self.chain.rpc('eth_getCode', [target, self.anchor(number)])[2:])

    def test_actual_corpus_four_pools_eight_historical_phases(self):
        facts = check_fixtures()
        self.assertEqual(NAMES, ['meta-0', 'meta-6', 'meta-8', 'meta-18'])
        self.assertEqual(facts['scenario_count'], 4)
        self.assertEqual(facts['step_count'], 8)
        self.assertEqual(facts['timestamps'], list(range(1767225600, 1767225608)))
        self.assertEqual(facts['coin_counts'], [2] * 4)
        self.assertEqual(TEMPLATE_BYTES, 20060)
        self.assertLessEqual(TEMPLATE_BYTES, 24576)
        self.assertEqual(hashlib.sha256(implementation_code()).hexdigest(), TEMPLATE_SHA256)
        for item in self.chain.items:
            p, m = item['pool'], item['pool']['multi_asset']['curve_meta']
            self.assertEqual(self.code(p['pool_id']), code_bytes(item['name']))
            self.assertEqual(self.code(m['implementation']), implementation_code())
            self.assertEqual(len(self.code(p['pool_id'])), 45)
            self.assertEqual(m['base_pool'], BASE_POOL)
            self.assertEqual(p['token1'], BASE_LP)
            self.assertEqual(p['multi_asset']['assets'][1]['scaling'], str(10**18))

    def test_all_32_banks_expose_original_getters_raw_and_effective_accounting(self):
        for phase in range(8):
            if phase:
                self.chain.advance(step_index=phase)
            for item in self.chain.items:
                pool = item['pool']; state = pool['multi_asset']; meta = state['curve_meta']
                address = pool['pool_id']
                for selector, value in [('0x76a2f0f0', state['amplification']),
                                        ('0xddca3f43', pool['fee_numerator']),
                                        ('0xfee3f7f9', state['admin_fee'])]:
                    self.assertEqual(int(self.call(address, selector), 16), int(value))
                for index, asset in enumerate(state['assets']):
                    self.assertEqual(int(self.call(address, '0xc6610657', index), 16), int(asset['token'], 16))
                    self.assertEqual(int(self.call(address, '0x4903b0d1', index), 16), int(asset['balance']))
                    self.assertEqual(int(self.call(address, '0xe2e7d264', index), 16), int(meta['admin_balances'][index]))
                    raw = int(self.call(asset['token'], '0x70a08231', int(address, 16)), 16)
                    self.assertEqual(raw, int(item['original']['stored_balances'][index]))
                    self.assertEqual(raw - int(meta['admin_balances'][index]), int(asset['balance']))
                    decimals = int(self.call(asset['token'], '0x313ce567'), 16)
                    self.assertEqual(decimals, 18 if index else [0, 6, 8, 18][NAMES.index(item['name'])])
                    self.assertEqual(self.code(asset['token']), dependency_code(item['name'], asset['token']))
                rate = self.chain.rpc('eth_getStorageAt', [address, '0xb', self.anchor()])
                self.assertEqual(int(rate, 16), int(state['assets'][0]['scaling']))
                self.assertEqual(int(self.call(BASE_POOL, '0xbb7b8b80'), 16), int(meta['base_virtual_price']))

    def test_post_fee_public_balances_are_effective_not_raw_token_balances(self):
        self.chain.advance(step_index=1)
        item = self.chain.items[0]
        pool = item['pool']
        index = 1
        asset = pool['multi_asset']['assets'][index]
        meta = pool['multi_asset']['curve_meta']
        raw = int(self.call(asset['token'], '0x70a08231', int(pool['pool_id'], 16)), 16)
        effective = int(asset['balance'])
        admin = int(meta['admin_balances'][index])
        self.assertGreater(admin, 0)
        self.assertNotEqual(raw, effective)
        self.assertEqual(raw, int(meta['stored_balances'][index]))
        self.assertEqual(raw - admin, effective)
        self.assertEqual(int(self.call(pool['pool_id'], '0x4903b0d1', index), 16), effective)

    def test_shared_lp_balanceof_selects_owner_from_calldata(self):
        self.chain.advance(step_index=3)
        balances = []
        for item in self.chain.items:
            value = int(self.call(BASE_LP, '0x70a08231', int(item['pool']['pool_id'], 16)), 16)
            self.assertEqual(value, int(item['original']['stored_balances'][1]))
            balances.append(value)
        self.assertGreater(len(set(balances)), 1)
        with self.assertRaises(ValueError):
            self.call(BASE_LP, '0x70a08231', 1234)

    def test_old_hash_uses_old_phase_and_old_fault_not_current_bank(self):
        number = self.chain.head
        target = self.chain.items[-1]['pool']['pool_id']
        raw = self.call(BASE_LP, '0x70a08231', int(target, 16))
        old_code = self.code(BASE_POOL)
        self.chain.advance(step_index=2, fault='base-code')
        self.assertEqual(self.call(BASE_LP, '0x70a08231', int(target, 16), number), raw)
        self.assertEqual(self.code(BASE_POOL, number), old_code)
        self.assertNotEqual(self.code(BASE_POOL), old_code)
        self.assertEqual(int(self.call(BASE_POOL, '0xbb7b8b80', number=number), 16), 10**18)
        self.assertEqual(self.chain.block(number)['timestamp'], hex(1767225600))
        fault_number = self.chain.head
        self.chain.advance()
        self.assertNotEqual(self.code(BASE_POOL, fault_number), self.code(BASE_POOL))
        self.assertEqual(self.chain.timestamp, 1767225602)
        self.chain.advance(step_index=3)
        self.assertEqual(self.chain.timestamp, 1767225603)

    def test_all_five_faults_and_recovery(self):
        self.assertEqual(FAULTS, ['base-code', 'virtual-price', 'accounting', 'implementation', 'lp-decimals'])
        for fault in FAULTS:
            self.chain.advance(step_index=3, fault=fault)
            item = self.chain.items[-1]; pool = item['pool']; meta = pool['multi_asset']['curve_meta']
            if fault == 'base-code':
                self.assertNotEqual(hashlib.sha256(self.code(BASE_POOL)).hexdigest(), meta['base_pool_code_sha256'])
            elif fault == 'virtual-price':
                self.assertEqual(int(self.call(BASE_POOL, '0xbb7b8b80'), 16), 0)
            elif fault == 'accounting':
                raw = int(self.call(pool['token0'], '0x70a08231', int(pool['pool_id'], 16)), 16)
                self.assertEqual(raw, int(meta['stored_balances'][0]) + 1)
                healthy = self.chain.items[0]
                self.assertEqual(int(self.call(healthy['pool']['token0'], '0x70a08231', int(healthy['pool']['pool_id'], 16)), 16),
                                 int(healthy['original']['stored_balances'][0]))
            elif fault == 'implementation':
                self.assertNotEqual(hashlib.sha256(self.code(meta['implementation'])).hexdigest(), meta['implementation_code_sha256'])
                self.assertEqual(self.code(pool['pool_id']), code_bytes(item['name']))
            else:
                self.assertEqual(int(self.call(BASE_LP, '0x313ce567'), 16), 17)
            self.chain.advance()
            self.assertEqual(self.code(BASE_POOL), dependency_code(item['name'], BASE_POOL))
            self.assertEqual(self.code(meta['implementation']), implementation_code())
            self.assertEqual(int(self.call(BASE_LP, '0x313ce567'), 16), 18)
            self.assertEqual(int(self.call(BASE_POOL, '0xbb7b8b80'), 16), int(meta['base_virtual_price']))

    def test_canonical_pin_and_strict_abi_reject_unknown_reads(self):
        target = self.chain.items[0]['pool']['pool_id']
        for anchor in ['latest', {'blockHash': self.anchor()['blockHash']},
                       {'blockHash': '0x' + 'ff'*32, 'requireCanonical': True},
                       {'blockHash': self.anchor()['blockHash'], 'requireCanonical': 1}]:
            with self.assertRaises(ValueError):
                self.chain.rpc('eth_getCode', [target, anchor])
        for selector, arg in [('0xdeadbeef', None), ('0xc6610657', 2), ('0x4903b0d1', None)]:
            with self.assertRaises(ValueError):
                self.call(target, selector, arg)
        with self.assertRaises(ValueError):
            self.chain.rpc('eth_getStorageAt', [target, '0xc', self.anchor()])
        with self.assertRaises(ValueError):
            self.chain.advance(step_index=8)
        with self.assertRaises(ValueError):
            self.chain.advance(fault='unknown')

    def test_receipts_pin_source_and_count_reads(self):
        block = self.chain.block(self.chain.head)
        receipt = self.chain.rpc('eth_getTransactionReceipt', [block['transactions'][0]])
        self.assertEqual(receipt['blockHash'], block['hash'])
        self.assertEqual(receipt['logs'], [])
        self.assertEqual(self.chain.receipts, 1)
        self.code(BASE_POOL)
        self.assertEqual(self.chain.pinned, 1)
        self.assertEqual(self.chain.read_counts['eth_getCode'], 1)

    def test_freshness_is_explicit_test_config_only(self):
        before = copy.deepcopy(self.chain.items)
        for routing in [False, True]:
            config = {'startup': {}}
            if routing:
                config['routing'] = {}
            configure_freshness(config)
            self.assertEqual(config['startup']['max_baseline_age_ms'], 315360000000)
            self.assertEqual('routing' in config, routing)
            if routing:
                self.assertEqual(config['routing']['max_state_age_ms'], FRESHNESS_MS)
        self.assertEqual(before, self.chain.items)

    def test_independent_protoc_encodes_all_meta_amounts_and_digests(self):
        item = self.chain.items[0]
        text = protobuf_text(item['pool'])
        self.assertIn('curve_meta {', text)
        self.assertIn('base_virtual_price { uint256_be:', text)
        self.assertIn('stored_balances { uint256_be:', text)
        for name in ['base_pool_code_sha256', 'implementation_code_sha256', 'block_hash']:
            self.assertIn(name + ': "\\x', text)
        root = Path(__file__).resolve().parents[4]
        result = subprocess.run(['protoc', '-I', str(root/'common/protoc/proto'),
                                 '--encode=dex.v1.PoolState', str(root/'common/protoc/proto/dex/v1/state.proto')],
                                input=text.encode(), capture_output=True)
        self.assertEqual(result.returncode, 0, result.stderr.decode())
        self.assertIn(int(item['pool']['multi_asset']['curve_meta']['base_virtual_price']).to_bytes(32, 'big'), result.stdout)
        self.assertIn(bytes.fromhex(TEMPLATE_SHA256), result.stdout)
        self.assertEqual(bootstrap_snapshot(b'\x0a\x03abc\x12\x00'), b'abc')
        for raw in [b'', b'\x0a\x09abc', b'\x0a\x00', b'\x12\x00', b'\x0a\x01a\x0a\x01b']:
            with self.assertRaises(ValueError):
                bootstrap_snapshot(raw)

    def test_quote_request_covers_each_original_direction(self):
        directions = set()
        for phase in range(8):
            for name in NAMES:
                item = profile(name, phase)
                request = quote_request(item); native = item['original']
                assets = item['pool']['multi_asset']['assets']
                self.assertEqual(request['sellToken'], assets[native['index_in']]['token'])
                self.assertEqual(request['buyToken'], assets[native['index_out']]['token'])
                self.assertEqual(request['sellAmount'], native['input'])
                self.assertEqual(request['verification'], 'FAST')
                directions.add((native['index_in'], native['index_out']))
        self.assertEqual(directions, {(0, 1), (1, 0)})

    def test_report_cannot_claim_process_success_from_fixture_checks(self):
        report = initial_report()
        for field in ['passed', 'production_ready', 'verified', 'all_requested_features_complete']:
            self.assertIs(report[field], False)
        self.assertTrue(report['historical_fixture_replay'])
        self.assertEqual(report['checks'], [])
        self.assertEqual(report['pool_count'], 4)
        self.assertEqual(report['service_steps'], 32)
        self.assertEqual(report['quote_checks'], 0)
        self.assertEqual(report['fault_names'], FAULTS)
        self.assertEqual(len(PROCESS_CHECKS), 9)
        self.assertIn('native-dust-explicitly-rejected', PROCESS_CHECKS)

    def test_cli_invalidates_stale_success_before_fallible_imports(self):
        source = Path(sys.modules['curve_meta_smoke'].__file__).read_text()
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            script = root/'integration/tools/acceptance/curve_meta_smoke.py'
            script.parent.mkdir(parents=True)
            script.write_text(source)
            target = root/'integration/artifacts/local/curve-meta-three-service-smoke.json'
            target.parent.mkdir(parents=True)
            target.write_text('{"passed": true}')
            (script.parent/'infrastructure.py').write_text('raise RuntimeError("blocked helper import")\n')
            result = subprocess.run([sys.executable, str(script)], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            report = json.loads(target.read_text())
            self.assertIs(report['passed'], False)
            self.assertIs(report['production_ready'], False)
            self.assertIs(report['verified'], False)
            self.assertIs(report['all_requested_features_complete'], False)

    def test_native_dust_requires_404_no_route_and_no_quote_id(self):
        item = profile('meta-0', 5)
        self.assertEqual(item['original']['expected']['amount_out'], '0')
        self.assertEqual(item['original']['input'], str(10**18))
        self.assertEqual(validate_quote_result(item, 404, {'error': 'NO_ROUTE'}), 'dust')
        for status, response in [(200, {'grossBuyAmount': '0', 'quoteId': 'fake'}),
                                 (404, {'error': 'NO_ROUTE', 'quoteId': 'fake'}),
                                 (404, {'error': 'NO_ROUTE', 'quote_id': 'fake'}),
                                 (422, {'error': 'NO_ROUTE'}), (422, {'error': 'INSUFFICIENT_OUTPUT'}),
                                 (404, {'error': 'INSUFFICIENT_OUTPUT'}),
                                 (503, {'error': 'NO_ROUTE'}), (404, {'error': 'OTHER'})]:
            with self.assertRaises(ValueError):
                validate_quote_result(item, status, response)
        positive = profile('meta-6', 5)
        with self.assertRaises(ValueError):
            validate_quote_result(positive, 404, {'error': 'NO_ROUTE'})
        value = {'grossBuyAmount': positive['original']['expected']['amount_out'],
                 'consumedIn': positive['original']['input'], 'refund': '0'}
        self.assertEqual(validate_quote_result(positive, 200, value), 'success')

    def test_main_report_is_fail_closed_during_and_after_process_failure(self):
        import curve_meta_smoke as module
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory)/'curve-meta-three-service-smoke.json'
            def fail():
                running = json.loads(target.read_text())
                self.assertEqual(running, initial_report())
                raise RuntimeError('process gate remains closed')
            with patch.object(module, 'OUT', Path(directory)), patch.object(module, 'run', side_effect=fail):
                with self.assertRaisesRegex(RuntimeError, 'process gate remains closed'):
                    module.main()
            report = json.loads(target.read_text())
            self.assertEqual(report['state'], 'FAILED')
            for key in ['passed', 'production_ready', 'verified', 'all_requested_features_complete']:
                self.assertIs(report[key], False)
            self.assertEqual(report['successful_quotes'], 0)
            self.assertEqual(report['dust_rejections'], 0)

    def test_require_and_word_work_with_assertions_disabled(self):
        with self.assertRaises(ValueError):
            require(False, 'explicit check')
        for value in [-1, 1 << 256]:
            with self.assertRaises(ValueError):
                word(value)
        self.assertEqual(len(word((1 << 256) - 1)), 64)


if __name__ == '__main__':
    unittest.main()
