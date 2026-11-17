import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

MODULE = Path(__file__).resolve().parents[1]/'design_delivery_audit.py'
spec = importlib.util.spec_from_file_location('design_delivery_audit', MODULE)
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


class DeliveryAudit(unittest.TestCase):
    def facts(self, rpc=True, broker=True):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory)/'config.json'
            value = {
                'chains': [{'key': 'evm:1', 'genesis_hash': '0x'+'ab'*32,
                            'rpc_urls': ['https://provider.invalid/private-secret'] if rpc else []}],
                'journal': {'kafka': {'brokers': ['broker.invalid:9093'] if broker else [],
                                      'security_protocol': 'SASL_SSL'}},
                'control': {'backend': 'postgres', 'dsn_file': '/secret/private-dsn'},
            }
            path.write_text(json.dumps(value))
            return audit.config_facts(path)

    def test_configuration_fact_projection_never_copies_urls_dsns_or_credentials(self):
        facts = self.facts()
        text = json.dumps(facts)
        self.assertNotIn('private-secret', text)
        self.assertNotIn('private-dsn', text)
        self.assertNotIn('broker.invalid', text)
        self.assertEqual(facts['kafka_broker_count'], 1)
        self.assertTrue(facts['control_dsn_declared'])

    def test_rpc_missing_and_deployment_missing_are_distinct_external_inputs(self):
        blockers = audit.external_blockers({'rpc-only': self.facts(False, True),
                                             'infra-only': self.facts(True, False)})
        by_id = {b['id']: b for b in blockers}
        self.assertEqual(by_id['EXTERNAL_CHAIN_ACCEPTANCE_INPUTS']['affected_services'], ['rpc-only'])
        self.assertEqual(by_id['EXTERNAL_DEPLOYMENT_ACCEPTANCE_ENVIRONMENT']['affected_services'], ['infra-only'])
        for blocker in blockers:
            self.assertIn('blocked_scope', blocker)
            self.assertIn('required_input', blocker)
            self.assertNotIn('all_requested_features_complete', blocker)

    def test_declared_external_inputs_do_not_create_a_blocker_or_a_completion_claim(self):
        facts = self.facts()
        self.assertEqual(audit.external_blockers({'configured': facts}), [])
        self.assertNotIn('production_ready', facts)
        self.assertNotIn('all_requested_features_complete', facts)

    def test_placeholder_identity_is_missing_authority_even_with_a_url(self):
        facts = self.facts()
        facts['placeholder_genesis_count'] = 1
        blockers = audit.external_blockers({'placeholder': facts})
        self.assertEqual([b['id'] for b in blockers], ['EXTERNAL_CHAIN_ACCEPTANCE_INPUTS'])

    def test_absent_local_test_topology_does_not_assert_external_service_outage(self):
        with tempfile.TemporaryDirectory() as directory:
            self.assertEqual(audit.local_kafka_facts(Path(directory)), [])

    def test_single_host_topology_retains_local_authority_and_does_not_expose_listeners(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root/'integration/.runtime/infrastructure/example/kafka.properties'
            path.parent.mkdir(parents=True)
            path.write_text('process.roles=broker,controller\ncontroller.quorum.voters=1@127.0.0.1:9999\n'
                            'listeners=PLAINTEXT://127.0.0.1:9092\n'
                            'transaction.state.log.replication.factor=1\ntransaction.state.log.min.isr=1\n')
            facts = audit.local_kafka_facts(root)
            self.assertEqual(len(facts), 1)
            self.assertEqual(facts[0]['authority'], 'LOCAL_TEST_CONFIGURATION_ONLY')
            self.assertEqual(facts[0]['controller_voter_count'], 1)
            self.assertEqual(facts[0]['transaction_replication_factor'], '1')
            self.assertTrue(facts[0]['loopback_listeners'])
            self.assertNotIn('127.0.0.1', json.dumps(facts))
            self.assertNotIn('production_ready', facts[0])


if __name__ == '__main__':
    unittest.main()
