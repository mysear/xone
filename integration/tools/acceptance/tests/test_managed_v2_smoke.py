"""Original-byte RPC replay and source admission contract; run under -OO too."""
import copy
import importlib
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / 'integration/tools/acceptance'))


class ManagedV2SmokeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.module = importlib.import_module('managed_v2_smoke')
        cls.chain = cls.module.Chain(now=1793714790)

    def setUp(self):
        self.chain.select(0, fault=None)

    def test_original_headers_and_getters_are_returned_without_padding(self):
        chain = self.chain
        self.assertEqual(chain.rpc('eth_getBlockByNumber', ['latest', False]), chain.frame['header'])
        for (address, data), value in chain.frame['getters'].items():
            actual = chain.rpc('eth_call', [{'to': address, 'data': data}, chain.anchor()])
            self.assertEqual(actual, value)
        self.assertEqual(chain.frame['pool'], self.module.projection.project_snapshot(chain.bank, chain.frame['ref'])['pool'])

    def test_every_read_requires_exact_canonical_hash(self):
        chain = self.chain
        key = next(iter(chain.frame['getters']))
        for anchor in ('latest', hex(chain.head), {'blockHash': chain.frame['header']['hash']},
                       {'blockHash': '0x' + 'ff' * 32, 'requireCanonical': True}):
            with self.subTest(anchor=anchor), self.assertRaises(ValueError):
                chain.rpc('eth_call', [{'to': key[0], 'data': key[1]}, anchor])
            with self.subTest(anchor=anchor), self.assertRaises(ValueError):
                chain.rpc('eth_getCode', [key[0], anchor])

    def test_unknown_getter_has_no_synthetic_fallback(self):
        with self.assertRaises(ValueError):
            self.chain.rpc('eth_call', [{'to': self.chain.frame['pool']['pool_id'], 'data': '0x00000000'}, self.chain.anchor()])
        with self.assertRaises(ValueError):
            self.chain.rpc('eth_getStorageAt', [self.chain.frame['pool']['pool_id'], '0x0', self.chain.anchor()])

    def test_native_receipts_and_responses_are_detached(self):
        chain = self.chain
        block = chain.rpc('eth_getBlockByNumber', ['latest', False])
        tx = block['transactions'][0]
        receipt = chain.rpc('eth_getTransactionReceipt', [tx])
        self.assertEqual(receipt, chain.receipts[tx])
        block['timestamp'] = '0x0'
        receipt['logs'].clear()
        self.assertNotEqual(chain.rpc('eth_getBlockByNumber', ['latest', False])['timestamp'], '0x0')
        self.assertEqual(chain.rpc('eth_getTransactionReceipt', [tx]), chain.receipts[tx])

    def test_service_inventory_keeps_future_original_frames_separate(self):
        chain = self.chain
        self.assertGreaterEqual(len(chain.frames), 6)
        heights = [int(f['header']['number'], 16) for f in chain.frames]
        self.assertEqual(heights, sorted(set(heights)))
        self.assertTrue(all(self.module.projection.service_eligible(f['pool'], now=1793714790) for f in chain.frames))
        self.assertTrue(chain.future_cases)
        self.assertTrue(all(not self.module.projection.service_eligible(c['source_pool'], now=1793714790)
                            for c in chain.future_cases))

    def test_startup_fingerprint_failure_closes_both_rpc_servers(self):
        from unittest.mock import MagicMock, patch
        servers = [MagicMock(), MagicMock()]
        chain = MagicMock()
        chain.provider.side_effect = servers
        with patch.object(self.module, 'Chain', return_value=chain), patch.object(self.module.base, 'runtime_fingerprint', side_effect=RuntimeError('fingerprint read failed')):
            with self.assertRaisesRegex(RuntimeError, 'fingerprint read failed'):
                self.module.run()
        for server in servers:
            server.shutdown.assert_called_once()
            server.server_close.assert_called_once()

    def test_second_rpc_startup_failure_closes_first_server(self):
        from unittest.mock import MagicMock, patch
        server = MagicMock()
        chain = MagicMock()
        chain.provider.side_effect = [server, RuntimeError('provider startup failed')]
        with patch.object(self.module, 'Chain', return_value=chain):
            with self.assertRaisesRegex(RuntimeError, 'provider startup failed'):
                self.module.run()
        server.shutdown.assert_called_once()
        server.server_close.assert_called_once()

    def test_each_fault_changes_an_actual_recorded_dependency_response(self):
        chain = self.chain
        frame = chain.frame
        for fault in self.module.FAULTS:
            chain.select(0, fault=fault)
            observed = chain.rpc('eth_getBlockByNumber', ['latest', False]) != frame['header']
            for (address, data), expected in frame['getters'].items():
                try:
                    observed |= chain.rpc('eth_call', [{'to': address, 'data': data}, chain.anchor()]) != expected
                except ValueError:
                    observed = True
            for label in ('ManagedPoolFactory', 'RecoveryModeHelper'):
                address = chain.identities[label]['address']
                observed |= chain.rpc('eth_getCode', [address, chain.anchor()]) != chain.codes[address]
            self.assertTrue(observed, fault + ' has no effect on any original dependency')

    def test_paused_original_branch_uses_its_own_headers_and_expiry_source(self):
        chain = self.module.Chain(now=1793714790, paused_branch=True)
        self.assertEqual(len(chain.frames), 15)
        paused = [f for f in chain.frames if f['pool']['multi_asset']['managed_v2']['vault_paused']]
        self.assertEqual(len(paused), 3)
        self.assertTrue(all(not f['pool']['multi_asset']['managed_v2']['paused'] for f in paused))
        quoted = next(c for c in chain.cases if c['name'] == 'swap-after-vault-buffer')
        frame = next(f for f in chain.frames if f['ref'] == quoted['source_refs']['before'])
        m = frame['pool']['multi_asset']['managed_v2']
        self.assertGreater(m['timestamp'],m['vault_buffer_period_end'])
        self.assertLess(m['timestamp'],m['buffer_period_end'])
        self.assertFalse(m['vault_paused'])
        self.assertEqual(quoted['expected_native']['quantity'],'370247922332383787644')
        recovery = [f for f in chain.frames if f['pool']['multi_asset']['managed_v2']['recovery_mode']]
        self.assertEqual(len(recovery),5)
        self.assertTrue(all(f['pool']['multi_asset']['managed_v2']['management_aum_fee']=='0' for f in recovery))
        self.assertFalse(chain.frames[-1]['pool']['multi_asset']['managed_v2']['recovery_mode'])
        self.assertEqual(chain.frames[-1]['pool']['multi_asset']['managed_v2']['management_aum_fee'],'10000000000000000')
        for index, frame in enumerate(chain.frames):
            chain.select(index)
            self.assertEqual(chain.rpc('eth_getBlockByNumber',['latest',False]),frame['header'])
            for (address,data),expected in frame['getters'].items():
                self.assertEqual(chain.rpc('eth_call',[{'to':address,'data':data},chain.anchor()]),expected)

    def test_fault_is_reversible_and_never_mutates_original_evidence(self):
        chain = self.chain
        original = copy.deepcopy(chain.frame)
        for fault in self.module.FAULTS:
            chain.select(0, fault=fault)
            self.assertEqual(chain.frame, original)
        chain.select(0, fault=None)
        self.assertEqual(chain.rpc('eth_getBlockByNumber', ['latest', False]), original['header'])


if __name__ == '__main__':
    unittest.main()
