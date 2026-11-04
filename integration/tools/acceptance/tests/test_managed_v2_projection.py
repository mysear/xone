"""Frozen original-bank -> public Pool projection contract (also run with -OO)."""
import copy
import hashlib
import gzip
import subprocess
import tempfile
import importlib
import json
from pathlib import Path
import sys
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[4]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / 'integration/tools/acceptance'))
BANK = ROOT / 'integration/test/contracts/managed-v2-aum.json'

class ProjectionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.p = importlib.import_module('managed_v2_projection')
        cls.bank = cls.p.load(BANK)

    def test_service_golden_cli_exists_and_checks_one_local_copy(self):
        script = ROOT / 'integration/tools/ci/managed-v2-aum-service-golden.py'
        self.assertTrue(script.is_file(), 'missing thin service golden generator')
        with tempfile.TemporaryDirectory() as temp:
            local = Path(temp) / 'managed-v2-aum.json'
            output = Path(temp) / 'managed-v2-aum-pools.json'
            local.write_bytes(BANK.read_bytes())
            for flags in ([], ['--check']):
                result = subprocess.run([sys.executable, str(script), '--input', str(local), '--output', str(output), *flags], capture_output=True)
                self.assertEqual(result.returncode, 0, result.stderr.decode())
            thin = json.loads(output.read_bytes())
            self.assertEqual(len(thin['cases']), 27)
            self.assertEqual(thin['original_bank']['reference'], self.bank['reference'])
            self.assertNotIn('objects', thin)
            output.write_bytes(b'{}')
            result = subprocess.run([sys.executable, str(script), '--input', str(local), '--output', str(output), '--check'], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual(output.read_bytes(), b'{}')

    def test_declared_template_spans_exact_original_bytes(self):
        builds = {}
        for row in self.bank._identities:
            build = row['artifact']['build']
            if build not in builds:
                path = self.p.REFERENCE / 'build-info' / (build + '.json.gz')
                builds[build] = json.loads(gzip.decompress(path.read_bytes()))
            b = builds[build]
            artifact = b['output']['contracts'][row['artifact']['source']][row['artifact']['name']]['evm']['deployedBytecode']
            refs = artifact.get('immutableReferences', {})
            expected = {(x['start'], x['length'], 'immutable') for xs in refs.values() for x in xs}
            expected |= {(x['start'], x['length'], 'link') for libs in artifact.get('linkReferences', {}).values() for xs in libs.values() for x in xs}
            actual = {(s['start'], s['length'], s['kind']) for s in row['normalization_spans']}
            self.assertEqual(actual - {(1, 20, 'library-self-address')}, expected)
            identity = self.p.resolve(self.bank, row['identity_ref'], 'identity')
            raw = bytearray.fromhex(self.p.resolve(self.bank, identity['code'], 'code')['hex'][2:])
            template = list(artifact['object'])
            for s in row['normalization_spans']:
                start, length = s['start'], s['length']
                if s['kind'] == 'library-self-address':
                    self.assertEqual(raw[1:21].hex(), row['address'][2:])
                raw[start:start + length] = bytes(length)
                template[2*start:2*(start+length)] = '0' * (2*length)
            self.assertEqual(raw.hex(), ''.join(template))
            self.assertEqual(hashlib.sha256(raw).hexdigest(), row['template_sha256'])

    def test_all_original_cases_and_two_authorities(self):
        for case in self.bank['cases']:
            with self.subTest(case=case['name']):
                v = self.p.project_case(self.bank, case)
                before = self.p.resolve(self.bank, case['execution']['before'], 'snapshot')
                after = self.p.resolve(self.bank, case['execution']['after'], 'snapshot')
                h = self.p.resolve(self.bank, before['header'], 'header')
                ah = self.p.resolve(self.bank, after['header'], 'header')
                sm = v['source_pool']['multi_asset']['managed_v2']
                dm = v['expected_pool_after']['multi_asset']['managed_v2']
                om = v['observed_pool_after']['multi_asset']['managed_v2']
                self.assertNotIn('execution_context', sm)
                self.assertNotIn('execution_context', om)
                tx = self.p.resolve(self.bank, case['execution']['transaction'], 'transaction')
                eh = self.p.resolve(self.bank, tx['execution_header'], 'header')
                self.assertEqual(dm['execution_context'], {'block_number': int(eh['number'], 16),
                                                           'timestamp': int(eh['timestamp'], 16)})
                self.assertEqual(sm['block_hash'], h['hash'])
                self.assertEqual(dm['block_hash'], h['hash'])
                self.assertEqual(om['block_hash'], ah['hash'])
                self.assertEqual(om['actual_supply'], after['state']['actual_supply'])
                self.assertEqual(v['authority']['expected_pool_after'], 'derived-after-state-at-source-header')
                self.assertEqual(v['authority']['observed_pool_after'], 'original-local-node-observation')
                self.assertEqual(v['source_refs']['before'], case['execution']['before'])
                self.assertEqual(v['expected_native']['expectation'], case['expectation'])
                self.assertEqual(v['params']['execution_timestamp'], int(ah['timestamp'], 16))
                self.assertEqual(v['config']['multi_asset']['tokens'][0]['token'], v['source_pool']['token0'])

    def test_native_quantity_comes_from_original_case_or_observed_supply_delta(self):
        for case in self.bank['cases']:
            v = self.p.project_case(self.bank, case)
            before = self.p.resolve(self.bank, case['execution']['before'], 'snapshot')['state']
            after = self.p.resolve(self.bank, case['execution']['after'], 'snapshot')['state']
            if case['operation'] == 'collect':
                expected = str(int(after['total_supply']) - int(before['total_supply']))
            elif case['operation'] == 'swap':
                expected = case['expectation']['raw_out']
            else:
                expected = case['expectation']['amount']
            self.assertEqual(v['expected_native']['quantity'], expected)

    def test_original_pool_and_vault_pauses_and_windows_remain_distinct(self):
        original_resolve = self.p.resolve
        case = self.bank['cases'][0]
        for vault_paused, pool_paused in ((False, False), (True, False), (False, True), (True, True)):
            def observed(bank, ref, kind=None):
                value = original_resolve(bank, ref, kind)
                if kind == 'snapshot':
                    value['state']['vault_paused'] = vault_paused
                    value['state']['paused'] = pool_paused
                return value
            with patch.object(self.p, 'resolve', side_effect=observed):
                row = self.p.project_case(self.bank, case)
            for field, side in [('source_pool','before'), ('observed_pool_after','after'), ('expected_pool_after','after')]:
                raw = original_resolve(self.bank, case['execution'][side], 'snapshot')['state']
                m = row[field]['multi_asset']['managed_v2']
                self.assertEqual(m['paused'], pool_paused)
                self.assertEqual(m['vault_paused'], vault_paused)
                self.assertEqual(m['pause_window_end'], int(raw['pause_window_end']))
                self.assertEqual(m['buffer_period_end'], int(raw['buffer_period_end']))
                self.assertEqual(m['vault_pause_window_end'], int(raw['vault_pause_window_end']))
                self.assertEqual(m['vault_buffer_period_end'], int(raw['vault_buffer_period_end']))

    def test_underlying_cash_managed_scaling_and_actual_code_sha(self):
        for case in self.bank['cases']:
            v = self.p.project_case(self.bank, case)
            for side, key in [('before', 'source_pool'), ('after', 'observed_pool_after'), ('after', 'expected_pool_after')]:
                state = self.p.resolve(self.bank, case['execution'][side], 'snapshot')['state']
                pool = v[key]; multi = pool['multi_asset']
                self.assertEqual(multi['managed_v2']['vault_bpt_actual_balance'], state['vault_bpt']['actual_balance'])
                self.assertEqual(len(multi['assets']), len(state['underlying']))
                self.assertNotIn(pool['pool_id'], [a['token'] for a in multi['assets']])
                for ref, a, t in zip(state['underlying'], multi['assets'], multi['managed_v2']['token_info']):
                    raw = self.p.resolve(self.bank, ref, 'token_state')
                    self.assertEqual(a['balance'], str(int(raw['cash']) + int(raw['managed'])))
                    self.assertEqual(a['scaling'], str(10 ** (36 - raw['decimals'])))
                    self.assertEqual(t['cash'], raw['cash'])
                    self.assertEqual(t['managed'], raw['managed'])
                    self.assertEqual(t['vault_actual_balance'], raw['actual_balances'][state['vault']])
                    identity = next(self.p.resolve(self.bank, r, 'identity') for r in self.bank['reference']['identities'] if self.p.resolve(self.bank, r, 'identity')['address'] == a['token'])
                    code = self.p.resolve(self.bank, identity['code'], 'code')['hex']
                    self.assertEqual(a['code_sha256'], hashlib.sha256(bytes.fromhex(code[2:])).hexdigest())

    def test_synthetic_global_wallet_observation_exceeds_registered_cash(self):
        # Synthetic observations exercise projection only; the frozen bank and
        # authenticated reference records are never rewritten or requalified.
        original_resolve = self.p.resolve

        def observed(bank, ref, kind=None):
            value = original_resolve(bank, ref, kind)
            if kind == 'token_state':
                value['actual_balances'] = {owner: str(int(balance) + 17)
                                            for owner, balance in value['actual_balances'].items()}
            if kind == 'snapshot':
                state = value['state']
                actual = str(int(state['vault_bpt']['actual_balance']) + 23)
                state['vault_bpt']['actual_balance'] = actual
                state['bpt_balances'][state['vault']] = actual
            return value

        with patch.object(self.p, 'resolve', side_effect=observed):
            row = self.p.project_case(self.bank, self.bank['cases'][0])
        for key, side in [('source_pool', 'before'), ('observed_pool_after', 'after'), ('expected_pool_after', 'after')]:
            state = original_resolve(self.bank, self.bank['cases'][0]['execution'][side], 'snapshot')['state']
            m = row[key]['multi_asset']['managed_v2']
            self.assertEqual(m['vault_bpt_cash'], state['vault_bpt']['cash'])
            self.assertEqual(m['vault_bpt_actual_balance'], str(int(state['vault_bpt']['actual_balance']) + 23))
            self.assertGreater(int(m['vault_bpt_actual_balance']), int(m['vault_bpt_cash']))
            for info, ref in zip(m['token_info'], state['underlying']):
                token = original_resolve(self.bank, ref, 'token_state')
                self.assertEqual(info['cash'], token['cash'])
                self.assertEqual(info['managed'], token['managed'])
                self.assertEqual(info['vault_actual_balance'], str(int(token['actual_balances'][state['vault']]) + 17))
                self.assertGreater(int(info['vault_actual_balance']), int(info['cash']))
        self.assertNotIn('execution_context', row['source_pool']['multi_asset']['managed_v2'])
        self.assertNotIn('execution_context', row['observed_pool_after']['multi_asset']['managed_v2'])
        self.assertIn('execution_context', row['expected_pool_after']['multi_asset']['managed_v2'])

    def test_projection_rejects_missing_or_invalid_actual_wallet_observations(self):
        original_resolve = self.p.resolve
        case = self.bank['cases'][0]
        vault = original_resolve(self.bank, case['execution']['before'], 'snapshot')['state']['vault']
        for target in ('token_state', 'snapshot'):
            for mutation in ('missing', 'unknown', 'null', 'negative', 'overflow', 'below-cash'):
                with self.subTest(target=target, mutation=mutation):
                    def observed(bank, ref, kind=None):
                        value = original_resolve(bank, ref, kind)
                        if kind == target:
                            container, key = ((value['actual_balances'], vault) if target == 'token_state'
                                              else (value['state']['vault_bpt'], 'actual_balance'))
                            if mutation == 'missing':
                                if target == 'token_state': del value['actual_balances']
                                else: del container[key]
                            elif mutation == 'unknown':
                                if target == 'token_state': del container[key]
                                else: container[key] = 'unknown'
                            else:
                                container[key] = {'null': None, 'negative': '-1', 'overflow': str(2**256), 'below-cash': '0'}[mutation]
                        return value
                    with patch.object(self.p, 'resolve', side_effect=observed), self.assertRaises(ValueError):
                        self.p.project_case(self.bank, case)

    def test_fifty_alias_provider_mapping_revert_and_service_clock(self):
        rows = {c['name']: self.p.project_case(self.bank, c) for c in self.bank['cases']}
        self.assertEqual(max(len(v['source_pool']['multi_asset']['assets']) for v in rows.values()), 50)
        for name, v in rows.items():
            m = v['source_pool']['multi_asset']['managed_v2']
            state = self.p.resolve(self.bank, v['source_refs']['before'], 'snapshot')['state']
            self.assertEqual(m['live_provider_aum_percentage'], state['provider_percentages'][state['provider_fee_id']])
            if name.startswith('alias:'):
                self.assertEqual(m['manager'], m['protocol_collector'])
                self.assertEqual(m['manager_bpt_balance'], m['protocol_collector_bpt_balance'])
        r = rows['u32:collect-overflow']
        self.assertEqual(r['expected_native']['status'], 0)
        self.assertEqual(r['expected_native']['rejection'], 'original-transaction-reverted')
        reverted = copy.deepcopy(r['expected_pool_after'])
        context = reverted['multi_asset']['managed_v2'].pop('execution_context')
        self.assertEqual(context, {'block_number': r['params']['execution_block'],
                                   'timestamp': r['params']['execution_timestamp']})
        self.assertEqual(r['source_pool'], reverted)
        self.assertFalse(self.p.service_eligible(r['source_pool'], now=1793664000, max_future_seconds=30))
        self.assertFalse(self.p.service_eligible(rows['two:collect-long']['source_pool'], now=1793664000, max_future_seconds=30))
        self.assertTrue(self.p.service_eligible(rows['two:swap-forward']['source_pool'], now=1793664000, max_future_seconds=30))

    def test_resolve_rejects_unknown_record_kind_and_wrong_nested_ref_type(self):
        b = copy.deepcopy(self.bank)
        unknown = {'kind': 'unqualified', 'value': {}}
        key = hashlib.sha256(self.p.canonical(unknown)).hexdigest()
        b['objects'][key] = unknown
        with self.assertRaises(ValueError): self.p.resolve(b, {'ref': key})
        snap = self.p.resolve(self.bank, self.bank['cases'][0]['execution']['before'], 'snapshot')
        snap['header'] = snap['state']['underlying'][0]
        rec = {'kind': 'snapshot', 'value': snap}
        key = hashlib.sha256(self.p.canonical(rec)).hexdigest()
        b['objects'][key] = rec
        with self.assertRaises(ValueError): self.p.resolve(b, {'ref': key}, 'snapshot')

    def test_resolve_rejects_bad_sha_kind_missing_and_cycles(self):
        ref = self.bank['cases'][0]['execution']['before']
        for mutation in ('sha', 'missing', 'kind', 'cycle'):
            b = copy.deepcopy(self.bank)
            if mutation == 'sha': b['objects'][ref['ref']]['value']['authority'] = 'forged'
            if mutation == 'missing': del b['objects'][ref['ref']]
            if mutation == 'kind':
                with self.assertRaises(ValueError): self.p.resolve(b, ref, 'identity')
                continue
            if mutation == 'cycle': b['objects'][ref['ref']]['value']['header'] = ref
            with self.assertRaises(ValueError): self.p.resolve(b, ref, 'snapshot')

    def test_derived_origin_rejects_cross_pool_or_future_header(self):
        c = self.bank['cases'][0]
        other = next(c for c in self.bank['cases'] if c['scenario'] == 'three')
        for ref, origin in [(c['execution']['before'], c['execution']['after']),
                            (c['execution']['after'], other['execution']['before'])]:
            with self.assertRaises(ValueError):
                self.p.project_snapshot(self.bank, ref, origin=origin)

    def test_load_rejects_rehashed_getter_unknown_template_and_input_changes(self):
        for mutation in ('getter', 'runtime', 'closure'):
            b = json.loads(BANK.read_bytes())
            if mutation == 'getter':
                snap = self.p.resolve(b, b['cases'][0]['execution']['before'], 'snapshot')
                b['objects'][snap['getters'][0]['ref']]['value']['result'] = '0x'
            elif mutation == 'runtime':
                identity = self.p.resolve(b, b['reference']['identities'][0], 'identity')
                b['objects'][identity['code']['ref']]['value']['hex'] = '0x00'
            else:
                b['reference']['compiler_jobs'][0]['compiler'] = 'unqualified'
            with tempfile.TemporaryDirectory() as temp:
                path = Path(temp) / 'managed-v2-aum.json'
                path.write_bytes(self.p.canonical(b))
                with self.assertRaises(ValueError): self.p.load(path)

    def test_projection_rejects_rehashed_mutations_and_unqualified_input(self):
        for field, value in [('last_collection', str(2**32)), ('provider_fee_id', '99'), ('pool', '0x123'), ('swap_fee', None), ('annual_aum_fee', str(2**256))]:
            b = copy.deepcopy(self.bank); c = b['cases'][0]; old = c['execution']['before']['ref']
            rec = b['objects'][old]; rec['value']['state'][field] = value
            new = hashlib.sha256(self.p.canonical(rec)).hexdigest(); b['objects'][new] = rec; c['execution']['before'] = {'ref': new}
            with self.assertRaises(ValueError): self.p.project_case(b, c)
        with self.assertRaises(ValueError): self.p.project_snapshot(json.loads(BANK.read_bytes()), self.bank['cases'][0]['execution']['before'])

    def test_origin_is_explicit_snapshot_and_copies_are_deterministic(self):
        case = next(c for c in self.bank['cases'] if c['name'] == 'two:collect-year')
        source = self.p.project_snapshot(self.bank, case['execution']['before'])
        observed = self.p.project_snapshot(self.bank, case['execution']['after'])
        derived = self.p.project_snapshot(self.bank, case['execution']['after'], origin=case['execution']['before'])
        self.assertEqual(source['authority'], 'original-local-node-observation')
        self.assertEqual(derived['authority'], 'derived-after-state-at-source-header')
        self.assertNotEqual(observed['pool']['multi_asset']['managed_v2']['block_hash'], derived['pool']['multi_asset']['managed_v2']['block_hash'])
        for project in ('chain-parser-go', 'dex-router'):
            path = ROOT / project / 'test/contracts/managed-v2-aum.json'
            self.assertEqual(path.read_bytes(), BANK.read_bytes())
            self.assertEqual(self.p.project_case(self.p.load(path), case), self.p.project_case(self.bank, case))

if __name__ == '__main__': unittest.main()
