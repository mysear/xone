"""Complete native shared banks projected without a local pricing oracle."""
import copy
import importlib
import sys
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT/'integration/tools/acceptance'))


class ScheduledSharedProjectionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.module = importlib.import_module('managed_v2_scheduled_shared_projection')
        cls.raw = cls.module.load(ROOT/'integration/test/contracts/managed-v2-scheduled-shared.json')
        cls.thin = cls.module.project(cls.raw)

    def test_72_banks_144_physical_pools_retain_all_source_headers_and_profiles(self):
        self.assertEqual(len(self.thin['banks']), 72)
        self.assertEqual(len(self.thin['cases']), 52)
        for bank in self.raw['banks']:
            projected = self.thin['banks'][bank['id']]
            self.assertEqual(projected['header'], bank['header'])
            self.assertEqual(len(projected['pools']), 2)
            branch = self.raw['branches'][bank['namespace']]
            for row, config in zip(projected['pools'], branch['pools']):
                p = row['pool']
                m = p['multi_asset']['managed_v2']
                self.assertEqual(p['protocol'], config['requested_protocol'])
                self.assertEqual(m['block_hash'], bank['header']['hash'])
                self.assertEqual(m['block_number'], int(bank['header']['number'], 16))
                self.assertEqual(m['timestamp'], int(bank['header']['timestamp'], 16))
                self.assertNotIn('execution_context', m)
                self.assertEqual('schedule' in m, p['protocol']=='balancer-v2-managed-scheduled')
                self.assertEqual(row['config']['protocol'], p['protocol'])

    def test_all_52_expected_transitions_are_only_observed_after_with_source_retained(self):
        indexed = {c['id']: c for c in self.raw['cases']}
        for case in self.thin['cases']:
            native = indexed[case['id']]
            self.assertEqual(case['expected_native'], native['expected_native'])
            before = self.thin['banks'][case['before']]['pools']
            observed = self.thin['banks'][case['after']]['pools']
            expected = copy.deepcopy([row['pool'] for row in observed])
            for origin, p in zip(before, expected):
                m = p['multi_asset']['managed_v2']
                m['execution_context'] = {'block_number':m['block_number'], 'timestamp':m['timestamp']}
                for k in ('block_number','timestamp','block_hash'):
                    m[k] = origin['pool']['multi_asset']['managed_v2'][k]
            self.assertEqual(case['expected_pools_after'], expected)
            if case['operation']=='swap':
                self.assertEqual(case['params']['amount'], native['amount_in'])
                self.assertEqual(case['expected_native']['actual_amount_out'], native['expected_native']['swap_event']['amount_out'])

    def test_unqualified_or_changed_source_rejected(self):
        with self.assertRaisesRegex(ValueError,'qualified'):
            self.module.project(dict(self.raw))
        changed=copy.deepcopy(self.raw)
        changed['banks'][0]['pools'][0]['state']['getters'].pop()
        with self.assertRaisesRegex(ValueError,'changed|seal'):
            self.module.project(changed)


class SharedProjectionOutputTests(unittest.TestCase):
    def test_protected_inputs_links_and_repository_targets_are_rejected(self):
        import tempfile
        module=importlib.import_module('managed_v2_scheduled_shared_projection')
        source=ROOT/'integration/test/contracts/managed-v2-scheduled-shared.json'
        with tempfile.TemporaryDirectory(prefix='shared-projection-output-') as folder:
            root=Path(folder)
            target=root/'new.json'
            self.assertEqual(module.output_path(target,source),target.resolve())
            original=root/'prefix-copy.json'
            original.write_text('preserve')
            with self.assertRaises(ValueError):module.output_path(original,source,original)
            for p in (source,ROOT/'integration/tools/reference/balancer-managed-v2-scheduled-shared/README.md',ROOT/'integration/test/contracts/unregistered.json',root/'managed-v2-aum.json'):
                with self.assertRaises(ValueError):module.output_path(p,source)
            target.symlink_to(source)
            with self.assertRaises(ValueError):module.output_path(target,source)
            target.unlink()
            target.hardlink_to(original)
            with self.assertRaises(ValueError):module.output_path(target,source)
            self.assertEqual(original.read_text(),'preserve')


@unittest.skipUnless(__import__('os').environ.get('XONE_SHARED_ACTUAL_REPORT_TEST')=='1',
                     'actual service/VM report required; the service gate executes this path')
class ActualSelectedObservationTests(unittest.TestCase):
    def test_all_actual_large_split_steps_are_native_observation_projections(self):
        import json
        module=importlib.import_module('managed_v2_scheduled_shared_projection')
        bank=module.load(ROOT/'integration/test/contracts/managed-v2-scheduled-shared.json')
        report=json.loads((ROOT/'integration/artifacts/local/managed-v2-scheduled-shared-three-service-smoke.json').read_bytes())['independent_actual_plan_replay']
        original=copy.deepcopy(report)
        projected=module.project_selected_report(bank,report)
        self.assertEqual(len(projected['executions']),len(report['executions']))
        self.assertEqual(projected['selected_transactions'],24)
        self.assertEqual(sum(len(v['steps']) for v in projected['executions']),24)
        self.assertEqual(report,original,'native execution evidence remains unchanged')
        for vector in projected['executions']:
            self.assertEqual(len(vector['source_pools']),2)
            for p in vector['source_pools']:self.assertNotIn('execution_context',p['multi_asset']['managed_v2'])
            for step in vector['steps']:
                self.assertEqual(len(step['expected_pools_after']),2)
                self.assertEqual(step['params']['amount'],step['expected_native']['actual_amount_in'])
                for before,after in zip(vector['source_pools'],step['expected_pools_after']):
                    m=after['multi_asset']['managed_v2']
                    for k in ('block_hash','block_number','timestamp'):self.assertEqual(m[k],before['multi_asset']['managed_v2'][k])
                    self.assertEqual(m['execution_context'],{'block_number':step['params']['execution_block'],'timestamp':step['params']['execution_timestamp']})
        for mutate in (lambda x:x['executions'][0].update(source_restored=False),
                       lambda x:x['executions'][0]['routes'][0].update(actual_amount_out='1'),
                       lambda x:x['executions'][0]['source']['header'].update(hash='0x'+'0'*64)):
            changed=copy.deepcopy(report);mutate(changed)
            with self.assertRaises(ValueError):module.project_selected_report(bank,changed)
