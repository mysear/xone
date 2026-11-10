"""The original VM bank must carry the ordinary reader's complete source closure."""
import json
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[4]


class ScheduledSourceTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.bank = json.loads((ROOT / 'integration/test/contracts/managed-v2-scheduled.json').read_bytes())

    def test_all_original_states_have_service_identity_pause_and_breaker_reads(self):
        checked = 0
        for namespace, branch in self.bank['branches'].items():
            for state in branch['states']:
                pool = branch.get('pool') or branch['configuration']['pool']
                vault = branch.get('vault') or branch['configuration']['vault']
                observations = {(g['to'], g['method'], tuple(map(str, g['args'])))
                                for g in state['getters']}
                for address, method in ((pool, 'getVault'), (pool, 'getProtocolFeesCollector'),
                                        (pool, 'getPausedState'), (pool, 'name'),
                                        (vault, 'getPausedState'), (vault, 'WETH')):
                    self.assertIn((address, method, ()), observations,
                                  f'{namespace}/{state["id"]} lacks ordinary source {method}')
                factory = (branch.get('configuration', {}).get('factory') or
                           next(i['address'] for i in branch['identities'] if i['label'] == 'ManagedPoolFactory'))
                for method in ('getVault', 'getWeightedMath', 'getRecoveryModeHelper',
                               'getProtocolFeePercentagesProvider', 'getCreationCodeContracts', 'getPauseConfiguration'):
                    self.assertIn((factory, method, ()), observations,
                                  f'{namespace}/{state["id"]} lacks factory {method}')
                tokens = [t['token'] for t in state['packed']['tokens']]
                for token in tokens:
                    self.assertIn((pool, 'getCircuitBreakerState', (token,)), observations)
                for g in state['getters']:
                    self.assertEqual((g['block_number'], g['block_hash']),
                                     (state['header']['number'], state['header']['hash']))
                checked += 1
        self.assertEqual(checked, 234)
