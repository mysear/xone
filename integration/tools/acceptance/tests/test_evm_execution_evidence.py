"""T32 bindings; synthetic process attestations are never execution evidence."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import evm_execution_evidence as m

CASE_NAMES = [
    'token0-to-token1-distinct-recipient', 'token1-to-token0-distinct-recipient',
    'token0-to-token1-same-recipient', 'token1-to-token0-same-recipient',
    'deadline-revert', 'min-out-revert', 'allowance-revert', 'balance-revert',
    'fee-token-output-counterexample', 'input-donation-before-swap',
]
CHECKS = [
    'normal-original-v2-bank-go-rpc-pg-kafka-two-routers',
    'isolated-current-executable-revm-real-token-balances-and-gas',
    'original-recipient-delta-exact-with-quote-and-bound-transaction',
    'cached-execution-evidence-retains-quote-and-all-commitments',
    'local-execution-never-claims-canonical-verified-or-complete-chain-fees',
    'simulation-auth-and-request-context-overrides-rejected',
    'source-revocation-new-generation-and-local-worker-recovery:pool-code',
    'stale-image-refused-until-explicit-reload:pool-code',
    'source-revocation-new-generation-and-local-worker-recovery:factory-getter',
    'stale-image-refused-until-explicit-reload:factory-getter',
    'router-restart-restores-source-and-isolated-execution',
    'verified-and-public-build-remain-closed',
]


def digest(value):
    return hashlib.sha256(value).hexdigest()


def image_hash(case):
    bundle = dict(schema_version=1,
                  fixture={key: case[key] for key in ['schema_version', 'state_anchor',
                           'execution_environment', 'accounts', 'block_hashes']},
                  executor=case['executor'], transaction_context=case['transaction_context'])
    return digest(json.dumps(bundle).encode())


class Bindings(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.document = json.loads((m.ROOT / 'integration/test/contracts/evm-execution.json').read_text())

    def setUp(self):
        self.runtime = {'source_tree_sha256': 'a' * 64,
                        'binaries': {path: digest(path.encode()) for path in m.BINARY_PATHS}}
        self.inputs = {'fixture': 'b' * 64, 'integration/tools/acceptance/helper.py': 'c' * 64}
        self.now = 10_000_000

    def report(self):
        history = []
        for index, stage in enumerate(['initial-native-bank', 'authorized-image-reload:pool-code',
                                       'authorized-image-reload:factory-getter']):
            hashes = {path: digest(path.encode()) for path in m.EXECUTION_FILES}
            for actor in ['router1', 'router2']:
                hashes[actor + '/state.json'] = image_hash(self.document['cases'][index])
            history.append({'stage': stage, 'case': CASE_NAMES[index], 'hashes': hashes})
        anchor = self.document['cases'][2]['state_anchor']
        return dict(passed=True, state='PASSED', authority=m.AUTHORITY,
                    verified=False, production_ready=False, all_requested_features_complete=False,
                    source_binding=copy.deepcopy(self.runtime), input_hashes=copy.deepcopy(self.inputs),
                    inputs_unchanged=True, started_at_unix_ms=self.now - 2000,
                    completed_at_unix_ms=self.now - 1000, checks=CHECKS.copy(),
                    simulation_checks=7, source_faults=2,
                    execution_file_hashes=copy.deepcopy(history[-1]['hashes']),
                    execution_file_history=history,
                    execution_files_unchanged_except_authorized_reloads=True,
                    read_counts={key: 1 for key in ['eth_chainId', 'eth_getBlockByNumber',
                                                  'eth_getTransactionReceipt', 'eth_getCode', 'eth_call']},
                    owner_reference={'block': {'chain_key': anchor['chain_key'],
                        'number_or_slot': anchor['block_number'], 'hash': anchor['block_hash'],
                        'parent_hash': anchor['parent_hash'], 'timestamp_ms': anchor['timestamp'] * 1000},
                        'canonical_epoch': 1, 'journal_seq': 3, 'schema_version': 1,
                        'adapter_set_hash': 'd' * 64, 'online_generation': 3,
                        'scope_hash': 'e' * 64, 'writer_epoch': 3,
                        'owner_service': 'chain-parser-go', 'owner_assignment_version': 1},
                    test_limits={'simulation_timeout_ms': 30000, 'max_gas': 15000000,
                        'max_state_reads': 8192, 'max_instructions': 2000000,
                        'max_memory_bytes': 67108864, 'concurrency': 2})

    def validate(self, value):
        m.validate_process(value, self.runtime, self.inputs, self.now)

    def review(self):
        return {'scope': m.SCOPE, 'source_tree_sha256': self.runtime['source_tree_sha256'],
                'input_sha256': m.input_digest(self.inputs), 'input_files': len(self.inputs),
                'reviews': {key: {'passed': True, 'agent_id': key + '-independent',
                                 'open_p1_p2': 0} for key in ['spec', 'quality']}}

    def test_exact_current_process_contract(self):
        self.assertEqual(m.CHECKS, CHECKS)
        self.assertEqual(m.SCOPE, 'provided-local-state-original-v2-shanghai-independent-execution')
        self.validate(self.report())

    def test_process_rejects_counts_check_reordering_flags_and_stale_bindings(self):
        for key, bad in [('passed', False), ('state', 'RUNNING'), ('simulation_checks', 20),
                ('simulation_checks', True), ('source_faults', 1), ('source_faults', 2.0),
                ('checks', CHECKS[:-1]), ('checks', list(reversed(CHECKS))),
                ('authority', 'LIVE_CANONICAL'), ('authority', None), ('inputs_unchanged', 1),
                ('input_hashes', {}), ('source_binding', {}), ('verified', True),
                ('production_ready', True), ('all_requested_features_complete', True),
                ('execution_files_unchanged_except_authorized_reloads', False)]:
            with self.subTest(key=key, bad=bad):
                value = self.report(); value[key] = bad
                with self.assertRaises(ValueError): self.validate(value)
        for key in ['verified', 'production_ready', 'all_requested_features_complete']:
            value = self.report(); del value[key]
            with self.assertRaises(ValueError): self.validate(value)

    def test_runtime_requires_source_and_exact_five_binary_hashes(self):
        for edit in [lambda r: r['binaries'].pop(next(iter(r['binaries']))),
                     lambda r: r['binaries'].update({'unknown': 'f' * 64}),
                     lambda r: r.update(source_tree_sha256='old'),
                     lambda r: r['binaries'].update({m.BINARY_PATHS[-1]: 'bad'})]:
            runtime = copy.deepcopy(self.runtime); edit(runtime)
            value = self.report(); value['source_binding'] = runtime
            with self.assertRaises(ValueError): m.validate_process(value, runtime, self.inputs, self.now)
        with self.assertRaises(ValueError): m.validate_process(self.report(), self.runtime, {}, self.now)

    def test_execution_window_rejects_stale_future_reversed_missing_and_boolean_times(self):
        for key, bad in [('started_at_unix_ms', 0), ('started_at_unix_ms', True),
                ('started_at_unix_ms', self.now), ('completed_at_unix_ms', self.now + 1),
                ('completed_at_unix_ms', '9999000'), ('completed_at_unix_ms', None),
                ('completed_at_unix_ms', self.now - 3_600_001)]:
            value = self.report(); value[key] = bad
            with self.subTest(key=key, bad=bad), self.assertRaises(ValueError): self.validate(value)
        value = self.report(); value['started_at_unix_ms'] = self.now - 3_601_000
        value['completed_at_unix_ms'] = self.now - 3_600_001
        with self.assertRaises(ValueError): self.validate(value)

    def test_each_actual_rpc_read_and_every_exact_limit_required(self):
        for method in self.report()['read_counts']:
            for bad in [0, -1, True, '1', 1.0, None]:
                value = self.report(); value['read_counts'][method] = bad
                with self.subTest(method=method, bad=bad), self.assertRaises(ValueError): self.validate(value)
        for limit, expected in self.report()['test_limits'].items():
            for bad in [expected + 1, float(expected), None]:
                value = self.report(); value['test_limits'][limit] = bad
                with self.subTest(limit=limit, bad=bad), self.assertRaises(ValueError): self.validate(value)
        value = self.report(); value['test_limits']['unknown'] = 1
        with self.assertRaises(ValueError): self.validate(value)

    def test_only_three_source_derived_state_images_may_change(self):
        for stage in range(3):
            for key in m.EXECUTION_FILES:
                value = self.report(); value['execution_file_history'][stage]['hashes'][key] = 'f' * 64
                if stage == 2: value['execution_file_hashes'][key] = 'f' * 64
                with self.subTest(stage=stage, path=key), self.assertRaises(ValueError): self.validate(value)
        for edit in [lambda v: v['execution_file_history'].pop(),
                lambda v: v['execution_file_history'].append(copy.deepcopy(v['execution_file_history'][-1])),
                lambda v: v['execution_file_history'].reverse(),
                lambda v: v['execution_file_history'][1].update(stage='automatic-reload'),
                lambda v: v['execution_file_history'][1].update(case=CASE_NAMES[0]),
                lambda v: v['execution_file_history'][0]['hashes'].pop('infrastructure-postgres-dsn'),
                lambda v: v['execution_file_history'][0]['hashes'].update({'auth_token': 'secret'}),
                lambda v: v['execution_file_history'][0]['hashes'].update({'go/config/config-test.yaml': 'secret'}),
                lambda v: v['execution_file_hashes'].update({'router1/state.json': 'f' * 64}),
                lambda v: [row['hashes'].update({'router1/state.json': image_hash(self.document['cases'][0])})
                           for row in v['execution_file_history']]]:
            value = self.report(); edit(value)
            with self.assertRaises(ValueError): self.validate(value)

    def test_owner_snapshot_must_be_final_actual_h_with_new_generation(self):
        for key, bad in [('online_generation', 2), ('online_generation', True),
                ('owner_service', 'chain-parser-rust'), ('schema_version', 2),
                ('scope_hash', 'invalid'), ('adapter_set_hash', None), ('writer_epoch', 0)]:
            value = self.report(); value['owner_reference'][key] = bad
            with self.subTest(key=key), self.assertRaises(ValueError): self.validate(value)
        for key in self.report()['owner_reference']['block']:
            value = self.report(); value['owner_reference']['block'][key] = 'stale'
            with self.subTest(key=key), self.assertRaises(ValueError): self.validate(value)

    def test_review_requires_current_all_inputs_and_independent_spec_quality(self):
        m.validate_review(self.review(), self.runtime, self.inputs)
        for edit in [lambda v: v.pop('input_sha256'), lambda v: v.update(input_files=1),
                lambda v: v.update(input_sha256=m.input_digest({'fixture': 'b' * 64})),
                lambda v: v.update(source_tree_sha256='f' * 64), lambda v: v.update(scope='all'),
                lambda v: v['reviews'].pop('quality'),
                lambda v: v['reviews'].update({'extra': v['reviews']['spec']}),
                lambda v: v['reviews']['quality'].update(agent_id='spec-independent'),
                lambda v: v['reviews']['quality'].update(agent_id='  '),
                lambda v: v['reviews']['spec'].update(passed=1),
                lambda v: v['reviews']['spec'].update(open_p1_p2=False),
                lambda v: v['reviews']['spec'].update(open_p1_p2=1)]:
            value = self.review(); edit(value)
            with self.assertRaises(ValueError): m.validate_review(value, self.runtime, self.inputs)
        changed = dict(self.inputs, **{'integration/tools/acceptance/helper.py': 'f' * 64})
        with self.assertRaises(ValueError): m.validate_review(self.review(), self.runtime, changed)

    def test_canonical_input_digest_independent_of_mapping_order(self):
        self.assertEqual(m.input_digest({'z': '1', 'a': '2'}),
                         digest(b'{"a":"2","z":"1"}'))

    def test_actual_corpus_inventory_and_complete_declared_post_state(self):
        counts = m.check_corpus()
        self.assertEqual(counts['native_cases'], 10)
        self.assertEqual(counts['native_successes'], 6)
        self.assertEqual(counts['native_reverts'], 4)
        self.assertEqual(counts['post_accounts'], 110)
        self.assertEqual(counts['post_storage_slots'], 494)
        self.assertEqual(counts['package_files'], 93)
        self.assertEqual(counts['loaded_dependencies'], 284)

    def test_coherent_inventory_truncation_cannot_reseal_counts(self):
        for edit in [lambda d: (d['cases'].pop(), d['reference'].update(case_count=9, success_count=5),
                               d['reference']['case_inventory'].pop()),
                lambda d: [c['expected']['post_accounts'].pop() for c in d['cases']],
                lambda d: [(c['accounts'].pop(), c['expected']['post_accounts'].pop()) for c in d['cases']],
                lambda d: d['cases'][0]['expected']['post_accounts'][1]['storage'].pop(),
                lambda d: d['cases'][0]['expected'].pop('pool_after'),
                lambda d: d['reference']['loaded_dependencies'].pop(),
                lambda d: d['source_pins']['packages'][0]['files'].pop(),
                lambda d: d.update(authority='LIVE_CANONICAL'),
                lambda d: d['cases'][0]['execution_environment'].update(block_number=18),
                lambda d: d['cases'][0]['evidence']['preblock_header'].update(stateRoot='0x00'),
                lambda d: d['cases'][0]['evidence']['execution_block_header'].update(parentHash='0x' + 'f' * 64),
                lambda d: d['cases'][0]['evidence'].update(execution_scope='canonical'),
                lambda d: d['cases'][8]['expected'].update(amount_out=d['cases'][8]['expected']['router_amount_out']),
                lambda d: d['cases'][9]['evidence']['input_donation'].update(amount='0')]:
            document = copy.deepcopy(self.document); edit(document)
            with self.assertRaises(ValueError): m.check_corpus(document)

    def test_fixture_copies_must_be_byte_identical(self):
        with tempfile.TemporaryDirectory() as folder, patch.object(m, 'ROOT', Path(folder)):
            for relative in ['integration/test/contracts/evm-execution.json', 'dex-router/test/contracts/evm-execution.json']:
                target = m.ROOT / relative; target.parent.mkdir(parents=True, exist_ok=True)
                target.write_text(json.dumps(self.document))
            m.check_corpus()
            (m.ROOT / 'dex-router/test/contracts/evm-execution.json').write_text(json.dumps(self.document) + '\n')
            with self.assertRaises(ValueError): m.check_corpus()

    def test_pinned_sources_include_raw_license_scripts_and_exact_node_closure(self):
        inputs = {}
        for row in self.document['reference']['scripts'] + self.document['reference']['loaded_dependencies']:
            inputs[row['path']] = row['sha256']
        inputs[self.document['reference']['node']['executable']] = self.document['reference']['node']['sha256']
        for package in self.document['source_pins']['packages']:
            for row in package['files']:
                inputs['integration/tools/reference/evm-execution/' + row['path']] = row['sha256']
        for relative in m.STATIC_CODE_PATHS + m.FIXTURE_PATHS:
            inputs[relative] = digest((m.ROOT / relative).read_bytes())
        m.validate_source_inputs(self.document, inputs)
        for key in [next(k for k in inputs if k.endswith('/LICENSE')),
                    self.document['reference']['node']['executable'],
                    self.document['reference']['loaded_dependencies'][0]['path'],
                    'integration/tools/acceptance/evm_execution_smoke.py']:
            value = dict(inputs); value.pop(key)
            with self.subTest(key=key), self.assertRaises(ValueError): m.validate_source_inputs(self.document, value)
        value = dict(inputs); value[m.STATIC_CODE_PATHS[0]] = 'f' * 64
        with self.assertRaises(ValueError): m.validate_source_inputs(self.document, value)

    def test_actual_input_snapshot_covers_every_pinned_reference_input(self):
        # Use the real collector, not a manually complete synthetic hash map.
        # The runtime package manifest is outside ordinary source directories.
        m.validate_source_inputs(self.document, m.snapshot_inputs())

    def test_main_invalidates_old_success_on_validation_failure(self):
        with tempfile.TemporaryDirectory() as folder, patch.object(m, 'OUT', Path(folder)), \
                patch.object(m, 'TARGET', Path(folder) / 'acceptance.json'), \
                patch.object(m, 'run', side_effect=ValueError('source drift')):
            m.TARGET.write_text('{"passed": true}')
            with self.assertRaises(ValueError): m.main()
            result = json.loads(m.TARGET.read_text())
            self.assertIs(result['passed'], False); self.assertEqual(result['state'], 'FAILED')
            self.assertIs(result['verified'], False)
            self.assertEqual(list(Path(folder).glob('*.tmp')), [])

    def test_entrypoint_invalidates_success_before_dependency_import(self):
        with tempfile.TemporaryDirectory() as folder:
            entry = Path(folder) / 'integration/tools/acceptance/evm_execution_evidence.py'
            entry.parent.mkdir(parents=True); entry.write_bytes(Path(m.__file__).read_bytes())
            target = Path(folder) / 'integration/artifacts/local/evm-execution-acceptance.json'
            target.parent.mkdir(parents=True); target.write_text('{"passed": true}')
            code = ('import runpy,sys\n'
                    'class Block:\n'
                    ' def find_spec(self,fullname,*args):\n'
                    '  if fullname=="weighted_evidence": raise ImportError("missing dependency")\n'
                    'sys.meta_path.insert(0,Block())\n'
                    'runpy.run_path(sys.argv[1],run_name="__main__")\n')
            result = subprocess.run([sys.executable, '-E', '-c', code, str(entry)], capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(b'missing dependency', result.stderr)
            self.assertIs(json.loads(target.read_text())['passed'], False)

    def test_optimized_guard_wrapper_preserves_native_assertions(self):
        with tempfile.TemporaryDirectory() as folder, patch.object(m, 'ROOT', Path(folder)), \
                patch.object(m, 'OUT', Path(folder) / 'logs'):
            tools = m.ROOT / 'integration/tools/ci'; tools.mkdir(parents=True)
            (tools / 'evm-execution-check.py').write_text('print("native checker executed")\n')
            # The normal run succeeds; only the second (-OO) run trips the
            # assertion. Its nonzero exit must still fail the entire gate.
            (tools / 'evm-execution-guards.py').write_text(
                'from pathlib import Path\n'
                'p=Path(__file__).with_suffix(".count")\n'
                'n=int(p.read_text())+1 if p.exists() else 1\n'
                'p.write_text(str(n))\n'
                'assert n != 2, "optimized native assertion retained"\n')
            with self.assertRaisesRegex(ValueError, 'optimized-native-mutation-guards'):
                m.run_source_checks()
            log = m.OUT / 'evm-execution-optimized-native-mutation-guards-acceptance.log'
            self.assertIn('optimized native assertion retained', log.read_text())
            self.assertEqual((tools / 'evm-execution-guards.count').read_text(), '2')

    def test_run_binds_current_verification_process_reviews_and_rechecks_inputs(self):
        with tempfile.TemporaryDirectory() as folder, patch.object(m, 'OUT', Path(folder)):
            process = self.report(); review = self.review()
            verification = {'measured_at_unix_ms': process['started_at_unix_ms'] - 1}
            for name, value in [('verification.json', verification),
                    ('evm-execution-three-service-smoke.json', process), ('evm-execution-review.json', review)]:
                (m.OUT / name).write_text(json.dumps(value))
            events = []
            def snapshot(): events.append('inputs'); return copy.deepcopy(self.inputs)
            def source_checks(): events.append('checks'); return [{'name': 'native-check', 'exit_code': 0}]
            with patch.object(m, 'checked_runtime', return_value=(verification, self.runtime)) as checked, \
                    patch.object(m, 'snapshot_inputs', side_effect=snapshot), \
                    patch.object(m, 'runtime_fingerprint', return_value=self.runtime), \
                    patch.object(m, 'validate_source_inputs'), patch.object(m, 'run_source_checks', side_effect=source_checks), \
                    patch.object(m.time, 'time', return_value=self.now / 1000):
                result = m.run(); checked.assert_called_once()
                self.assertLess(events.index('inputs'), events.index('checks'))
                self.assertEqual(events[-1], 'inputs')
                self.assertIs(result['passed'], True)
                for key in ['verified', 'production_ready', 'all_requested_features_complete']:
                    self.assertIs(result[key], False)
                self.assertEqual(result['process_sha256'], digest((m.OUT / 'evm-execution-three-service-smoke.json').read_bytes()))
                self.assertEqual(result['review_sha256'], digest((m.OUT / 'evm-execution-review.json').read_bytes()))
                self.assertEqual(result['verification_sha256'], digest((m.OUT / 'verification.json').read_bytes()))
                self.assertEqual(result['simulation_checks'], 7)
                self.assertEqual(result['input_sha256'], m.input_digest(self.inputs))
                with patch.object(m, 'snapshot_inputs', side_effect=[self.inputs, {'drift': 'f' * 64}]):
                    with self.assertRaises(ValueError): m.run()
                with patch.object(m, 'runtime_fingerprint', return_value={}):
                    with self.assertRaises(ValueError): m.run()
                def replace_review():
                    (m.OUT / 'evm-execution-review.json').write_text(json.dumps(dict(review, changed=True)))
                    return []
                with patch.object(m, 'run_source_checks', side_effect=replace_review):
                    with self.assertRaises(ValueError): m.run()
                (m.OUT / 'evm-execution-review.json').write_text(json.dumps(review))
                # The loaded verification and the artifact whose digest is sealed
                # must be the very same report, even if both describe this build.
                (m.OUT / 'verification.json').write_text(json.dumps(dict(verification, attempt='replacement')))
                with self.assertRaises(ValueError): m.run()
                (m.OUT / 'verification.json').write_text(json.dumps(verification))
                verification['measured_at_unix_ms'] = process['started_at_unix_ms'] + 1
                with self.assertRaises(ValueError): m.run()


if __name__ == '__main__':
    unittest.main()
