"""Small fixture/RPC tests; these never start parsers, routers, PG, or Kafka."""
import copy
import hashlib
import unittest

from curve_ng_smoke import (
    Chain, FRESHNESS_MS, NAMES, TEMPLATE_BYTES, TEMPLATE_SHA256,
    configure_freshness, check_fixtures, protobuf_text, bootstrap_snapshot,
)


class CurveNGReplayFixture(unittest.TestCase):
    def setUp(self):
        self.chain = Chain()

    def anchor(self, number=None):
        block = self.chain.block(number or self.chain.head)
        return {'blockHash': block['hash'], 'requireCanonical': True}

    def call(self, item, selector, index=None, number=None):
        data = selector + (f'{index:064x}' if index is not None else '')
        return self.chain.rpc('eth_call', [
            {'to': item['pool']['pool_id'], 'data': data}, self.anchor(number)])

    def storage(self, item, slot, number=None):
        return int(self.chain.rpc('eth_getStorageAt', [
            item['pool']['pool_id'], hex(slot), self.anchor(number)]), 16)

    def test_original_all_seven_runtimes_and_frozen_time(self):
        result = check_fixtures()
        self.assertEqual(result['coin_counts'], list(range(2, 9)))
        self.assertEqual(result['scenario_count'], 7)
        self.assertEqual(result['step_count'], 8)
        self.assertEqual(self.chain.timestamp, 1767225600)
        self.assertEqual(len(self.chain.items), 7)
        for item in self.chain.items:
            raw = bytes.fromhex(self.chain.rpc('eth_getCode', [
                item['pool']['pool_id'], self.anchor()])[2:])
            self.assertEqual(len(raw), TEMPLATE_BYTES + 2144)
            self.assertEqual(hashlib.sha256(raw[:TEMPLATE_BYTES]).hexdigest(), TEMPLATE_SHA256)
            self.assertEqual(hashlib.sha256(raw).hexdigest(),
                             item['config']['multi_asset']['expected_code_sha256'])

    def test_all_coins_getters_and_private_slots(self):
        for item in self.chain.items:
            state = item['pool']['multi_asset']
            ng = state['curve_ng']
            n = len(state['assets'])
            for slot, count in [(1, n), (16, n), (25, n - 1)]:
                self.assertEqual(self.storage(item, slot), count)
            for i, asset in enumerate(state['assets']):
                self.assertEqual(self.storage(item, 2 + i), int(ng['stored_balances'][i]))
                self.assertEqual(self.storage(item, 17 + i), int(ng['admin_balances'][i]))
                self.assertEqual(int(self.call(item, '0xe2e7d264', i), 16), int(ng['admin_balances'][i]))
                self.assertEqual(int(self.call(item, '0xc6610657', i), 16), int(asset['token'], 16))
                decimals = self.chain.rpc('eth_call', [
                    {'to': asset['token'], 'data': '0x313ce567'}, self.anchor()])
                self.assertEqual(10 ** (36 - int(decimals, 16)), int(asset['scaling']))
            for i, value in enumerate(ng['last_prices_packed']):
                self.assertEqual(self.storage(item, 26 + i), int(value))
            self.assertEqual(self.storage(item, 34), int(ng['last_d_packed']))
            self.assertEqual(self.storage(item, 37), int(ng['ma_last_time']))

    def test_past_hash_selects_past_bank_after_head_advances(self):
        number = self.chain.head
        item = self.chain.items[-1]
        before = self.storage(item, 2, number)
        self.chain.advance(step_index=1)
        self.assertEqual(self.storage(item, 2, number), before)
        self.assertEqual(self.storage(item, 2),
                         int(self.chain.items[-1]['pool']['multi_asset']['curve_ng']['stored_balances'][0]))
        self.assertNotEqual(self.chain.block(number)['hash'], self.chain.block(self.chain.head)['hash'])
        # Step one has the exact original Jan 1 timestamp, not the local date.
        self.assertEqual(self.chain.timestamp, 1767225600)
        self.chain.advance(step_index=2)
        self.assertEqual(self.chain.timestamp, 1767225601)

    def test_four_faults_change_only_eight_coin_target_and_recover(self):
        for fault in ('accounting', 'oracle', 'immutable', 'template'):
            self.chain.advance(fault=fault)
            item = self.chain.items[-1]
            ng = item['pool']['multi_asset']['curve_ng']
            if fault == 'accounting':
                self.assertEqual(self.storage(item, 9), int(ng['stored_balances'][-1]) + 1)
            elif fault == 'oracle':
                self.assertEqual(self.storage(item, 37) & ((1 << 128) - 1), self.chain.timestamp + 1)
            elif fault == 'immutable':
                raw = bytes.fromhex(self.call(item, '0xfd0684b1')[2:])
                self.assertEqual(int.from_bytes(raw[-32:], 'big'),
                                 int(item['pool']['multi_asset']['assets'][-1]['scaling']) + 1)
            else:
                raw = bytes.fromhex(self.chain.rpc('eth_getCode', [
                    item['pool']['pool_id'], self.anchor()])[2:])
                self.assertNotEqual(hashlib.sha256(raw).hexdigest(),
                                    item['config']['multi_asset']['expected_code_sha256'])
                self.assertNotEqual(hashlib.sha256(raw[:TEMPLATE_BYTES]).hexdigest(), TEMPLATE_SHA256)
            healthy = self.chain.items[0]
            self.assertEqual(self.storage(healthy, 2),
                             int(healthy['pool']['multi_asset']['curve_ng']['stored_balances'][0]))
            self.chain.advance()
            self.assertEqual(self.storage(self.chain.items[-1], 9), int(ng['stored_balances'][-1]))

    def test_bad_anchor_unknown_selector_and_unknown_storage_fail(self):
        target = self.chain.items[0]['pool']['pool_id']
        for anchor in ('latest', {'blockHash': self.anchor()['blockHash']},
                       {'blockHash': '0x' + 'ff' * 32, 'requireCanonical': True}):
            with self.assertRaises(ValueError):
                self.chain.rpc('eth_getCode', [target, anchor])
        with self.assertRaises(ValueError):
            self.chain.rpc('eth_getStorageAt', [target, '0xdead', self.anchor()])
        with self.assertRaises(ValueError):
            self.call(self.chain.items[0], '0xdeadbeef')

    def test_explicit_test_freshness_does_not_change_original_state(self):
        original = copy.deepcopy(self.chain.items)
        config = {'startup': {}, 'routing': {}}
        configure_freshness(config)
        self.assertEqual(config['startup']['max_baseline_age_ms'], FRESHNESS_MS)
        self.assertEqual(config['routing']['max_state_age_ms'], FRESHNESS_MS)
        self.assertGreater(FRESHNESS_MS, 366 * 24 * 60 * 60 * 1000)
        self.assertEqual(self.chain.items, original)

    def test_parser_freshness_keeps_router_only_section_absent(self):
        config = {'startup': {}}
        configure_freshness(config)
        self.assertEqual(config['startup']['max_baseline_age_ms'], FRESHNESS_MS)
        self.assertNotIn('routing', config)

    def test_protobuf_projection_and_bootstrap_envelope(self):
        text = protobuf_text(self.chain.items[0]['pool'])
        self.assertIn('curve_ng {', text)
        self.assertIn('stored_balances { uint256_be:', text)
        self.assertIn('block_hash:', text)
        self.assertIn('\\x00', text)
        # OwnerBootstrap.snapshot=1, cursor=2; extract bytes, not an inferred JSON state.
        self.assertEqual(bootstrap_snapshot(b'\x0a\x03abc\x12\x00'), b'abc')
        for raw in (b'', b'\x0a\x09abc', b'\x0a\x00', b'\x12\x00'):
            with self.assertRaises(ValueError):
                bootstrap_snapshot(raw)


if __name__ == '__main__':
    unittest.main()
