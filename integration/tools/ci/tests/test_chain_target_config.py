from pathlib import Path
import copy
import importlib.util
import json
import sys
import time
import unittest
from tempfile import TemporaryDirectory

ROOT = Path(__file__).resolve().parents[4]
MODULE = ROOT / 'integration/tools/configuration/chain_target_config.py'

class ChainTargetConfigTests(unittest.TestCase):
    def setUp(self):
        self.assertTrue(MODULE.is_file(), 'missing executable target-chain configuration generator')
        spec = importlib.util.spec_from_file_location('chain_target_config', MODULE)
        self.tool = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(self.tool)
        self.targets = self.tool.load_targets()

    def base(self, family='evm'):
        owner = 'chain-parser-go' if family == 'evm' else 'chain-parser-rust'
        return json.loads((ROOT / owner / 'config/config-test.yaml').read_text())

    def rpc(self, target, chain_id=None, genesis=None):
        def call(method, params):
            if method == 'eth_chainId': return chain_id or hex(target['chain_id'])
            if method == 'eth_getBlockByNumber':
                self.assertEqual(params, ['0x0', False])
                return {'number':'0x0', 'hash':genesis or target.get('genesis_hash') or '0x'+'ab'*32}
            if method == 'getGenesisHash': return genesis or target['genesis_hash']
            self.fail('unexpected source request')
        return call

    def test_design_target_matrix_is_exact_and_no_target_grants_protocol_admission(self):
        self.assertEqual({t['chain_key'] for t in self.targets.values()}, {'evm:1','evm:8453','evm:56','solana:mainnet-beta','evm:42161','evm:10','evm:137','evm:43114'})
        self.assertEqual(len(self.targets), 8)
        for key, target in self.targets.items():
            with self.subTest(key=key):
                config, report = self.tool.render(self.base(target['family']), target, self.rpc(target), self.tool.ROOT)
                self.assertEqual(config['chains'][0]['key'], target['chain_key'])
                self.assertEqual(config['chains'][0]['pools'], [])
                self.assertEqual(config['startup']['mode'], 'HEAD_FIRST')
                self.assertLessEqual(config['startup']['max_baseline_age_ms'], 120000)
                self.assertLessEqual(config['startup']['max_tail_blocks'], 128)
                self.assertFalse(config['history']['online_write_permission'])
                self.assertEqual(config['history']['namespace'], 'history')
                self.assertFalse(report['production_ready'])
                self.assertFalse(report['consensus_verified'])
                self.assertFalse(report['protocol_admission_granted'])
                self.assertEqual(config['chains'][0]['confirmation_tag'], 'finalized')
                if target['family'] == 'evm':
                    self.assertEqual(config['chains'][0]['block_integrity'], 'known-evm-native-commitments-v2')
                    proof = config['chains'][0]['native_proof']
                    self.assertEqual(proof, {'max_bytes':16777216, 'max_headers':64,
                        'max_window_blocks':16, 'max_transactions':4096, 'max_lookahead':32})
                else:
                    self.assertNotIn('block_integrity', config['chains'][0])
                    self.assertNotIn('native_proof', config['chains'][0])

    def test_go_defaults_are_consumed_bounded_and_require_no_external_secret(self):
        for key, target in self.targets.items():
            if target['owner'] != 'chain-parser-go':
                continue
            with self.subTest(key=key):
                base = self.base(); base['api']['auth_token'] = ''
                config, report = self.tool.render(base, target, self.rpc(target), self.tool.ROOT)
                chain = config['chains'][0]
                probe = chain['provider_probe']
                self.assertEqual(probe['historical_height'], 0)
                self.assertEqual(probe['historical_hash'], target['genesis_hash'])
                self.assertGreaterEqual(probe['max_age_ms'], probe['interval_ms'])
                self.assertGreaterEqual(probe['max_backoff_ms'], probe['interval_ms'])
                self.assertEqual(chain['receipts_mode'], 'bulk-auto')
                maintenance = config['journal']['raw_maintenance']
                raw = config['journal']['raw_evidence']
                self.assertTrue(Path(maintenance['sink_path']).is_absolute())
                self.assertNotEqual(maintenance['sink_path'], config['state']['path'])
                self.assertLess(maintenance['target_bytes'], raw['max_bytes'])
                self.assertLessEqual(maintenance['max_raw_bytes'], raw['max_record_bytes'])
                self.assertLessEqual(maintenance['max_archive'], maintenance['max_scan'])
                self.assertLessEqual(maintenance['max_reclaims'], maintenance['max_scan'])
                self.assertGreaterEqual(len(config['api']['auth_token']), 24)
                self.assertTrue(report['management_token_generated'])
                self.assertNotIn(config['api']['auth_token'], json.dumps(report))
                self.assertNotIn('dynamic_catalog', chain)
                self.assertEqual(report['dynamic_catalog_default'], 'qualified-deployment-required')

    def test_go_defaults_preserve_explicit_runtime_settings_and_sink_rebasing(self):
        target = self.targets['ethereum']; base = self.base()
        base['chains'][0].update(key=target['chain_key'], genesis_hash=target['genesis_hash'],
                                 receipts_mode='per-tx', block_integrity='known-evm-header-raw-transactions-v1',
                                 provider_probe={'explicit': 'preserved'},
                                 dynamic_catalog={'explicit': 'preserved'})
        base['api']['auth_token'] = 'configured-local-management-token'
        base['journal']['raw_maintenance'] = {'sink_path':'../custom-archive', 'explicit': 'preserved'}
        config, report = self.tool.render(base, target, self.rpc(target), self.tool.ROOT / 'chain-parser-go/config')
        chain = config['chains'][0]
        self.assertEqual(chain['receipts_mode'], 'per-tx')
        self.assertEqual(chain['block_integrity'], 'known-evm-header-raw-transactions-v1')
        self.assertNotIn('native_proof', chain)
        self.assertEqual(chain['provider_probe'], base['chains'][0]['provider_probe'])
        self.assertEqual(chain['dynamic_catalog'], base['chains'][0]['dynamic_catalog'])
        self.assertEqual(config['journal']['raw_maintenance']['explicit'], 'preserved')
        self.assertEqual(Path(config['journal']['raw_maintenance']['sink_path']), (self.tool.ROOT / 'chain-parser-go/custom-archive').resolve())
        self.assertEqual(config['api']['auth_token'], base['api']['auth_token'])
        self.assertFalse(report['management_token_generated'])

    def test_solana_receives_no_go_runtime_fields_or_generated_management_token(self):
        target = self.targets['solana']; base = self.base('solana')
        config, report = self.tool.render(base, target, self.rpc(target), self.tool.ROOT)
        for field in ['provider_probe', 'dynamic_catalog', 'receipts_mode', 'block_integrity', 'native_proof']:
            self.assertNotIn(field, config['chains'][0])
        self.assertNotIn('raw_maintenance', config['journal'])
        self.assertEqual(config['api'], base['api'])
        self.assertFalse(report['management_token_generated'])

    def test_solana_pins_complete_genesis_hash_instead_of_caip_prefix(self):
        self.assertEqual(self.targets['solana']['genesis_hash'], '5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d')

    def test_every_mainnet_target_rejects_same_chain_id_with_another_genesis(self):
        for name, target in self.targets.items():
            with self.subTest(name=name):
                self.assertTrue(target.get('genesis_hash'), 'target requires a frozen full genesis identity')
                with self.assertRaises(ValueError):
                    self.tool.render(self.base(target['family']), target, self.rpc(target, genesis='0x'+'cd'*32), self.tool.ROOT)

    def test_state_and_default_topic_include_complete_genesis_identity(self):
        target = self.targets['bsc']; alternative = copy.deepcopy(target)
        alternative['genesis_hash'] = '0x'+'cd'*32
        first, _ = self.tool.render(self.base(), target, self.rpc(target), self.tool.ROOT)
        second, _ = self.tool.render(self.base(), alternative, self.rpc(alternative), self.tool.ROOT)
        self.assertNotEqual(first['state']['path'], second['state']['path'])
        self.assertNotEqual(first['journal']['kafka']['topics'], second['journal']['kafka']['topics'])
        self.assertIn(target['genesis_hash'][2:], first['state']['path'])
        missing = copy.deepcopy(target); missing.pop('genesis_hash')
        with self.assertRaisesRegex(ValueError, 'TARGET_GENESIS_PIN_REQUIRED'):
            self.tool.render(self.base(), missing, self.rpc(target), self.tool.ROOT)

    def test_frozen_genesis_pins_match_retained_identity_observations(self):
        evidence = json.loads((ROOT/'integration/test/source/chain-genesis-observations.json').read_text())
        self.assertFalse(evidence['consensus_verified'])
        self.assertFalse(evidence['independent_failure_domains_verified'])
        for name, target in self.targets.items():
            self.assertEqual(evidence['targets'][name]['chain_key'], target['chain_key'])
            self.assertEqual(evidence['targets'][name]['genesis_hash'], target['genesis_hash'])
            rows = [r for r in evidence['rows'] if r['target'] == name]
            self.assertTrue(rows)
            for row in rows:
                observation = row['observations'][-1]
                response = observation.get('raw_response', {}).get('result', observation.get('parsed_response_result'))
                self.assertEqual(response.get('hash') if isinstance(response, dict) else response, target['genesis_hash'])

    def test_retained_real_genesis_evidence_can_generate_every_target_without_live_rpc(self):
        path = ROOT/'integration/test/source/chain-genesis-observations.json'
        for target in self.targets.values():
            config, _ = self.tool.render(self.base(target['family']), target, self.tool.retained_call(path, target), self.tool.ROOT)
            self.assertEqual(config['chains'][0]['genesis_hash'], target['genesis_hash'])

    def test_retained_identity_evidence_requires_the_frozen_full_file_digest(self):
        source = ROOT/'integration/test/source/chain-genesis-observations.json'
        with TemporaryDirectory() as directory:
            path = Path(directory)/'changed.json'; path.write_bytes(source.read_bytes()+b' ')
            with self.assertRaisesRegex(ValueError, 'TARGET_EVIDENCE_DIGEST'):
                self.tool.retained_call(path, self.targets['ethereum'])

    def test_same_chain_rpc_infrastructure_precedes_public_defaults(self):
        target = self.targets['ethereum']; base = self.base()
        base['chains'][0].update(key=target['chain_key'], genesis_hash=target['genesis_hash'], rpc_urls=['https://owned-source-a.example','https://owned-source-b.example'])
        config, _ = self.tool.render(base, target, self.rpc(target), self.tool.ROOT)
        self.assertEqual(config['chains'][0]['rpc_urls'], base['chains'][0]['rpc_urls'])
        self.assertEqual(self.tool.choose_rpc_urls(base, target, ['https://explicit-source.example']), ['https://explicit-source.example'])

    def test_automatic_raw_evidence_limits_cannot_emit_an_invalid_config(self):
        target = self.targets['ethereum']; base = self.base()
        base['journal'].pop('raw_evidence', None)
        base['journal']['max_payload_bytes'] = 32*1024*1024
        with self.assertRaisesRegex(ValueError, 'TARGET_RAW_BUDGET_INCOMPATIBLE'):
            self.tool.render(base, target, self.rpc(target), self.tool.ROOT)

    def test_probe_worker_has_an_absolute_deadline_even_while_making_progress(self):
        before = time.monotonic()
        with self.assertRaisesRegex(ValueError, 'TARGET_RPC_DEADLINE'):
            self.tool.run_worker([sys.executable, '-c', 'import time; [time.sleep(.1) for _ in range(30)]'], b'{}', .05)
        self.assertLess(time.monotonic()-before, 1.5)

    def test_wrong_chain_genesis_or_missing_rpc_evidence_fails_before_config(self):
        target = self.targets['ethereum']
        for rpc in [self.rpc(target, chain_id='0x2105'), self.rpc(target, genesis='0x'+'00'*32), lambda *_: None]:
            with self.assertRaises(ValueError): self.tool.render(self.base(), target, rpc, self.tool.ROOT)
        target = self.targets['solana']
        with self.assertRaises(ValueError): self.tool.render(self.base('solana'), target, self.rpc(target, genesis='localnet'), self.tool.ROOT)

    def test_existing_same_chain_genesis_pin_cannot_be_replaced_by_new_provider(self):
        target = self.targets['bsc']
        base = self.base(); base['chains'][0]['key'] = target['chain_key']
        base['chains'][0]['genesis_hash'] = '0x'+'11'*32
        with self.assertRaises(ValueError): self.tool.render(base, target, self.rpc(target), self.tool.ROOT)

    def test_existing_foreign_pool_catalog_and_wrong_owner_cannot_be_relabelled(self):
        target = self.targets['ethereum']
        base = self.base(); base['chains'][0]['pools'] = [{'id':'local-pool'}]
        with self.assertRaises(ValueError): self.tool.render(base, target, self.rpc(target), self.tool.ROOT)
        with self.assertRaises(ValueError): self.tool.render(self.base('solana'), target, self.rpc(target), self.tool.ROOT)

    def test_local_runtime_paths_are_rebased_and_networks_do_not_share_state_or_topic(self):
        configs = []
        for key in ['ethereum', 'base']:
            target = self.targets[key]
            base = self.base(); original = copy.deepcopy(base)
            config, report = self.tool.render(base, target, self.rpc(target), self.tool.ROOT / 'chain-parser-go/config')
            self.assertEqual(base, original)
            self.assertTrue(Path(config['state']['path']).is_absolute())
            self.assertEqual(config['journal']['source_urls'], base['journal']['source_urls'])
            self.assertEqual(config['control']['backend'], base['control']['backend'])
            self.assertEqual(config['journal']['transport'], base['journal']['transport'])
            configs.append(config)
        self.assertNotEqual(configs[0]['state']['path'], configs[1]['state']['path'])
        self.assertNotEqual(configs[0]['journal']['kafka']['topics'], configs[1]['journal']['kafka']['topics'])

    def test_unbounded_recovery_and_online_history_cannot_be_inherited(self):
        target = self.targets['ethereum']
        base = self.base()
        base['startup']['mode'] = 'REPLAY_ALL'; base['history']['online_write_permission'] = True
        with self.assertRaises(ValueError): self.tool.render(base, target, self.rpc(target), self.tool.ROOT)

if __name__ == '__main__': unittest.main()
