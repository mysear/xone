"""Strict replay tests only; these never constitute a three-service pass."""
import copy
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest import mock

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / 'integration/tools/acceptance'))
import managed_v2_multi_smoke as smoke


class MultiSmokeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.prefix = json.loads((ROOT / 'integration/test/contracts/managed-v2-aum.json').read_bytes())
        cls.raw = {s: json.loads(smoke.REFERENCE[s].read_bytes()) for s in smoke.SCOPES}

    def chain(self, scope='shared'):
        return smoke.ReplayChain(scope, self.raw[scope], self.prefix)

    def test_strict_canonical_published_anchors(self):
        c = self.chain()
        getter = next(iter(c.frame['getters']))
        body = dict(to=getter[0], data=getter[1])
        for anchor in ('latest', c.frame['header']['number'], {'blockHash': c.frame['header']['hash']},
                       dict(c.anchor(), requireCanonical=False), dict(c.anchor(), extra=True),
                       {'blockHash': c.frames[1]['header']['hash'], 'requireCanonical': True}):
            with self.subTest(anchor=anchor), self.assertRaises(ValueError):
                c.rpc('eth_call', [body, anchor])
        self.assertEqual(c.rpc('eth_call', [body, c.anchor()]), c.frame['getters'][getter])
        c.select(1)
        old = {'blockHash': c.frames[0]['header']['hash'], 'requireCanonical': True}
        self.assertEqual(c.rpc('eth_call', [body, old]), c.frames[0]['getters'][getter])

    def test_original_headers_receipts_code_and_getters_exact(self):
        for scope in smoke.SCOPES:
            c = self.chain(scope)
            for i, frame in enumerate(c.frames):
                c.select(i)
                for key, value in frame['getters'].items():
                    self.assertEqual(c.rpc('eth_call', [dict(to=key[0], data=key[1]), c.anchor()]), value)
                for address, value in c.codes.items():
                    self.assertEqual(c.rpc('eth_getCode', [address, c.anchor()]), value)
                self.assertEqual(c.rpc('eth_getBlockByNumber', ['latest', False]), frame['header'])
            for n, h in c.headers.items():
                self.assertEqual(c.rpc('eth_getBlockByNumber', [hex(n), False]), h)
            for tx, receipt in c.receipts.items():
                self.assertEqual(c.rpc('eth_getTransactionReceipt', [tx]), receipt)
            self.assertEqual(sorted(c.headers), list(range(c.head + 1)))

    def test_shared_global_duplicates_must_agree(self):
        raw = copy.deepcopy(self.raw['shared'])
        bank = next(iter(raw['banks'].values()))
        a, b = bank['pools']
        keys = {(g['to'], g['data']): g for g in a['getters']}
        duplicate = next(g for g in b['getters'] if (g['to'], g['data']) in keys)
        duplicate['result'] = '0x' + smoke.base.word(2)
        with self.assertRaisesRegex(ValueError, 'conflicting shared getter'):
            smoke.ReplayChain('shared', raw, self.prefix)
        c = self.chain()
        self.assertLess(len(c.frame['getters']), sum(p['getter_count'] for p in next(iter(c.raw['banks'].values()))['pools']))

    def test_unknown_getter_body_code_receipt_and_block_fail(self):
        c = self.chain()
        for method, args in [('eth_call', [dict(to='0x'+'00'*20, data='0x12345678'), c.anchor()]),
                             ('eth_call', [dict(to=next(iter(c.codes)), data='0x12345678', value='0x0'), c.anchor()]),
                             ('eth_getCode', ['0x'+'00'*20, c.anchor()]),
                             ('eth_getTransactionReceipt', ['0x'+'ff'*32]),
                             ('eth_getBlockByNumber', [hex(c.head+1), False]), ('eth_getLogs', [{}])]:
            with self.subTest(method=method, args=args), self.assertRaises(ValueError):
                c.rpc(method, args)

    def test_full_inventory_and_native_requests(self):
        for scope, count in [('catalog', 10), ('shared', 7)]:
            c = self.chain(scope)
            self.assertEqual(len(c.frames), count)
            self.assertEqual(len(c.cases), 8 if scope == 'catalog' else 5)
            swaps = [case for case in c.cases if case['operation'] == 'swap']
            self.assertEqual(len(swaps), 4)
            for case in swaps:
                req = smoke.native_request(case)
                self.assertEqual(req['sellAmount'], case['expected_native']['swap_event']['amount_in'])
                self.assertEqual(req['caller'], case['execution']['transaction']['from'])
                self.assertEqual(req['recipient'], case['execution']['args'][1]['recipient'].lower())
                self.assertEqual(req['constraints'], {'maxHops': 1})
            self.assertEqual([p['id'] for p in c.configs], [p['pool_id'] for p in c.frame['pools']])
            self.assertEqual(len(c.configs), 4 if scope == 'catalog' else 2)

    def test_faults_change_real_next_bank_and_preserve_original(self):
        for scope in smoke.SCOPES:
            c = self.chain(scope)
            frozen = copy.deepcopy(c.frames)
            for fault in smoke.faults_for(scope):
                c.select(0)
                c.select(1, fault)
                evidence = c.fault_evidence
                self.assertEqual(evidence['source_hash'], frozen[1]['header']['hash'])
                self.assertNotEqual(evidence['before'], evidence['after'])
                if evidence['method'] == 'eth_getBlockByNumber':
                    actual = c.rpc(evidence['method'], ['latest', False])['timestamp']
                else:
                    args = [evidence['body'], c.anchor()]
                    try:
                        actual = c.rpc(evidence['method'], args)
                    except ValueError:
                        actual = None
                self.assertEqual(actual, evidence['after'])
                self.assertEqual(c.frames, frozen)
                c.select(1)
                self.assertEqual(c.frame, frozen[1])
            self.assertGreaterEqual(len(smoke.faults_for(scope)), 4)

    def test_fixed_header_getter_and_parent_gaps_rejected(self):
        for edit in ('getter', 'pool', 'parent'):
            raw = copy.deepcopy(self.raw['shared'])
            bank = next(iter(raw['banks'].values()))
            if edit == 'getter':
                bank['pools'][0]['getters'][0]['block_hash'] = '0x'+'ff'*32
            elif edit == 'pool':
                bank['pools'][0]['pool']['multi_asset']['managed_v2']['timestamp'] += 1
            else:
                raw['headers'] = [h for h in raw['headers'] if int(h['number'], 16) != 30]
            with self.subTest(edit=edit), self.assertRaises(ValueError):
                smoke.ReplayChain('shared', raw, self.prefix)

    def test_partial_provider_and_infrastructure_startup_cleanup(self):
        server = mock.Mock()
        chain = mock.Mock()
        chain.provider.side_effect = [server, RuntimeError('second provider failed')]
        with self.assertRaisesRegex(RuntimeError, 'second provider'):
            with smoke.resources(chain):
                self.fail('startup should fail')
        server.shutdown.assert_called_once()
        server.server_close.assert_called_once()
        chain.provider.side_effect = None
        chain.provider.return_value = server
        infra = mock.Mock()
        infra.start.side_effect = RuntimeError('postgres started then kafka failed')
        with mock.patch.object(smoke.base, 'Infrastructure', return_value=infra):
            with self.assertRaisesRegex(RuntimeError, 'kafka failed'):
                with smoke.resources(chain):
                    self.fail('startup should fail')
        infra.close.assert_called_once()
        with mock.patch.object(smoke.subprocess, 'Popen', side_effect=OSError('exec failed')):
            log = mock.mock_open()
            with mock.patch.object(Path, 'open', log):
                with self.assertRaisesRegex(OSError, 'exec failed'):
                    with smoke.Processes() as processes:
                        processes.start('go', ['missing'], ROOT, ROOT/'unused.log')
            log().close.assert_called_once()
        with tempfile.TemporaryDirectory() as folder:
            targets = {scope: Path(folder)/(scope+'.json') for scope in smoke.SCOPES}
            for target in targets.values():
                target.write_text(json.dumps({'passed': True, 'state': 'PASSED'}))
            observed = []
            def fail_startup(scope, report):
                if not observed:
                    self.assertTrue(all(json.loads(p.read_text())['state'] == 'RUNNING' for p in targets.values()))
                observed.append(scope)
                raise RuntimeError('deliberate setup failure')
            with mock.patch.dict(smoke.REPORTS, targets), mock.patch.object(smoke, 'run_scope', side_effect=fail_startup), \
                    mock.patch('sys.stdout'), mock.patch('sys.stderr'):
                self.assertEqual(smoke.main([]), 1)
            self.assertEqual(observed, list(smoke.SCOPES))
            for target in targets.values():
                report = json.loads(target.read_text())
                self.assertEqual(report['state'], 'FAILED')
                self.assertIs(report['passed'], False)
                self.assertIs(report['full_service_run'], False)


if __name__ == '__main__':
    unittest.main()
