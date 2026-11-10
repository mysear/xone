"""Replay transport must preserve canonical source headers and fail closed."""
import importlib
import json
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / 'integration/tools/acceptance'))


class ScheduledReplayTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.module = importlib.import_module('managed_v2_scheduled_smoke')
        # The native validator is exercised by projection tests/the CLI. Here
        # inspect the replay of these bytes, without claiming native authority.
        cls.raw = json.loads((ROOT / 'integration/test/contracts/managed-v2-scheduled.json').read_bytes())
        cls.thin = json.loads((ROOT / 'integration/test/contracts/managed-v2-scheduled-pools.json').read_bytes())

    def chain(self, namespace='reschedule'):
        return self.module.ReplayChain(self.raw, self.thin, namespace)

    def test_each_source_reconstructs_its_original_canonical_ancestry(self):
        for ns, branch in self.raw['branches'].items():
            chain = self.chain(ns)
            for state in branch['states']:
                chain.select(state['id'])
                self.assertEqual(chain.rpc('eth_getBlockByNumber', ['latest', False]), state['header'])
                n = int(state['header']['number'], 16)
                self.assertEqual(set(chain.canonical), set(range(n + 1)))
                for i in range(1, n + 1):
                    self.assertEqual(chain.canonical[i]['parentHash'], chain.canonical[i-1]['hash'])
                self.assertEqual(chain.frame['pool'], self.thin['states'][ns + '/' + state['id']])
                for getter in state['getters']:
                    self.assertEqual(chain.rpc('eth_call', [{'to': getter['to'], 'data': getter['data']}, chain.anchor()]), getter['result'])

    def test_storage_requires_exact_canonical_anchor_and_uses_original_integer(self):
        chain = self.chain()
        state = self.raw['branches']['reschedule']['states'][0]
        pool = chain.frame['pool']['pool_id']
        expected = next(s['value'] for s in state['storage'] if s['address'] == pool and s['key'] == '0x9')
        self.assertEqual(int(chain.rpc('eth_getStorageAt', [pool, '0x9', chain.anchor()]), 16), int(expected, 16))
        for anchor in ('latest', {'blockHash': chain.anchor()['blockHash']},
                       {**chain.anchor(), 'requireCanonical': False},
                       {**chain.anchor(), 'extra': 1},
                       {**chain.anchor(), 'blockHash': '0x' + 'f'*64}):
            with self.assertRaises(ValueError):
                chain.rpc('eth_getStorageAt', [pool, '0x9', anchor])

    def test_faults_apply_to_private_serving_copy_and_recover(self):
        chain = self.chain()
        original = json.dumps(chain.frames, sort_keys=True)
        pool = chain.frame['pool']['pool_id']
        chain.select('S0', fault='missing-word')
        with self.assertRaises(ValueError):
            chain.rpc('eth_getStorageAt', [pool, '0x9', chain.anchor()])
        self.assertGreater(chain.fault_hits, 0)
        chain.select('S0', fault='reserved-token-bit')
        state = self.raw['branches']['reschedule']['states'][0]
        slot = state['packed']['tokens'][0]['key']
        self.assertTrue(int(chain.rpc('eth_getStorageAt', [pool, slot, chain.anchor()]), 16) & (1 << 133))
        chain.select('S0')
        self.assertEqual(json.dumps(chain.frames, sort_keys=True), original)
        for method, args in [('eth_getStorageAt', [pool, '0xdead', chain.anchor()]),
                             ('eth_call', [{'to': pool, 'data': '0x00000000'}, chain.anchor()])]:
            with self.assertRaises(ValueError):
                chain.rpc(method, args)
        with self.assertRaises(ValueError):
            chain.select('S0', fault='unknown-fault')


class ScheduledServiceGuardTests(unittest.TestCase):
    def setUp(self):
        self.module = importlib.import_module('managed_v2_scheduled_smoke')

    def test_quote_reference_rejects_earlier_generation_at_same_block(self):
        import copy
        ref = {'block': {'hash': 'same-H'}, 'online_generation': 4, 'scope_hash': 'scope',
               'canonical_epoch': 7, 'journal_seq': 11, 'writer_epoch': 2}
        q = {'snapshotRef': copy.deepcopy(ref), 'onlineGeneration': 4, 'scopeHash': 'scope'}
        self.module.validate_quote_reference(q, ref)
        for key, value in [('online_generation', 3), ('canonical_epoch', 6),
                           ('journal_seq', 10), ('writer_epoch', 1), ('scope_hash', 'old')]:
            changed = copy.deepcopy(q)
            changed['snapshotRef'][key] = value
            with self.assertRaises(ValueError):
                self.module.validate_quote_reference(changed, ref)
        for key, value in [('onlineGeneration', 3), ('scopeHash', 'old')]:
            changed = copy.deepcopy(q)
            changed[key] = value
            with self.assertRaises(ValueError):
                self.module.validate_quote_reference(changed, ref)

    def test_fault_window_must_reject_stale_fast_quote(self):
        for code, response in [(200, {'quoteId': 'stale'}), (503, {'quoteId': 'stale'}),
                               (422, {'grossBuyAmount': '1', 'routes': []}), (200, {})]:
            with self.assertRaises(ValueError):
                self.module.validate_withdrawn_quote(code, response)
        self.module.validate_withdrawn_quote(503, {'error': {'code': 'CHAIN_NOT_READY'}})

    def test_second_provider_failure_closes_first_provider(self):
        import tempfile
        from unittest import mock
        server = mock.Mock()
        chain = mock.Mock()
        chain.provider.side_effect = [server, OSError('second listener failed')]
        raw = {'branches': {'two': {}}}
        thin = {'cases': [], 'collections': []}
        report = {'namespaces': []}
        with tempfile.TemporaryDirectory() as folder, mock.patch.object(self.module, '_write'), \
             mock.patch.object(self.module, 'ReplayChain', return_value=chain), \
             mock.patch.object(self.module, 'OUT', Path(folder)):
            with self.assertRaisesRegex(OSError, 'second listener failed'):
                self.module.run_namespace(raw, thin, 'two', report)
        server.shutdown.assert_called_once()
        server.server_close.assert_called_once()
