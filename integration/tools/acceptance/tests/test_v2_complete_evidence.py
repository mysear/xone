"""T33 strict gate regressions; fabricated reports only test the validator."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
try:
    import v2_complete_evidence as m
except ModuleNotFoundError as error:
    if error.name != 'v2_complete_evidence':
        raise
    m = None

ROOT = Path(__file__).resolve().parents[4]
SCOPE = 'original-v2-complete-swap-shanghai-provided-state'
FLAGS = ['verified', 'production_ready', 'all_requested_features_complete']


def digest(data):
    return hashlib.sha256(data).hexdigest()


def word(value):
    return format(int(value), '064x')


class Bindings(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.document = json.loads((ROOT / 'integration/test/contracts/v2-complete.json').read_text())

    def setUp(self):
        self.assertIsNotNone(m, 'T33 strict gate module has not been implemented')
        self.runtime = {'source_tree_sha256': 'a' * 64,
                        'binaries': {p: digest(p.encode()) for p in m.BINARY_PATHS}}
        self.inputs = {'fixture': 'b' * 64, 'helper': 'c' * 64}
        self.now = 10_000_000
        reader = patch.object(m, 'corpus', return_value=self.document)
        reader.start(); self.addCleanup(reader.stop)

    def evidence(self, case):
        pool = case['pool']; after = case['expected']['pool_after']; state = after['v2']['swap']
        specs = {
            'pair_reserves_after': (pool['pool_id'], '0x0902f1ac', [after['reserve0'], after['reserve1'], state['block_timestamp_last']]),
            'pair_balance0_after': (pool['token0'], '0x70a08231' + '0' * 24 + pool['pool_id'][2:], [state['raw_balance0']]),
            'pair_balance1_after': (pool['token1'], '0x70a08231' + '0' * 24 + pool['pool_id'][2:], [state['raw_balance1']]),
            'pair_price0_cumulative_after': (pool['pool_id'], '0x5909c0d5', [state['price0_cumulative_last']]),
            'pair_price1_cumulative_after': (pool['pool_id'], '0x5a3d5493', [state['price1_cumulative_last']]),
            'pair_lock_after': (pool['pool_id'], '', [1]),
        }
        observations = {}
        for phase, (target, calldata, values) in specs.items():
            observations[phase] = dict(phase=phase, target=target, calldata=calldata,
                output='0x' + ''.join(map(word, values)), usage=dict(reads=1, read_bytes=84,
                instructions=1, gas_used=21001, input_bytes=len(calldata[2:]) // 2, executions=1))
        lock = observations['pair_lock_after']
        lock.update(type='STORAGE', slot='0x' + word(12))
        lock['usage'].update(instructions=0, gas_used=0, input_bytes=0, executions=0)
        return dict(case=case['name'], source_block_hash=case['state_anchor']['block_hash'],
                    receipt_hash='d' * 64, observations=observations)

    def report(self):
        history = []
        for index, stage in enumerate(['initial-native-bank', 'authorized-image-reload:pool-code', 'authorized-image-reload:factory-getter']):
            hashes = {p: digest(p.encode()) for p in m.EXECUTION_FILES}
            for actor in ['router1', 'router2']:
                hashes[actor + '/state.json'] = m.bundle_hash(self.document['cases'][index])
            history.append(dict(stage=stage, case=self.document['cases'][index]['name'], hashes=hashes))
        a = self.document['cases'][2]['state_anchor']
        return dict(passed=True, state='PASSED', authority=m.AUTHORITY, profile='uniswap-v2-complete',
            adapter_version='uniswap-v2/core-1.0.1-full-swap-1', native_fixture_sha256=m.FIXTURE_SHA256,
            verified=False, production_ready=False, all_requested_features_complete=False,
            source_binding=copy.deepcopy(self.runtime), input_hashes=copy.deepcopy(self.inputs), inputs_unchanged=True,
            started_at_unix_ms=self.now - 2000, completed_at_unix_ms=self.now - 1000,
            checks=m.CHECKS.copy(), simulation_checks=7, source_faults=2, complete_poststate_checks=7,
            complete_poststate_evidence=[self.evidence(self.document['cases'][i]) for i in [0, 0, 1, 1, 2, 2, 2]],
            execution_file_history=history, execution_file_hashes=copy.deepcopy(history[-1]['hashes']),
            execution_files_unchanged_except_authorized_reloads=True,
            read_counts={p: 1 for p in m.RPC_READS}, test_limits=m.TEST_LIMITS.copy(),
            owner_reference=dict(block=dict(chain_key=a['chain_key'], number_or_slot=a['block_number'],
                hash=a['block_hash'], parent_hash=a['parent_hash'], timestamp_ms=a['timestamp'] * 1000),
                canonical_epoch=1, journal_seq=3, online_generation=3, writer_epoch=3, owner_assignment_version=1,
                schema_version=1, owner_service='chain-parser-go', adapter_set_hash='d' * 64, scope_hash='e' * 64))

    def validate(self, value):
        m.validate_process(value, self.runtime, self.inputs, self.now)

    def review(self):
        return dict(scope=SCOPE, source_tree_sha256=self.runtime['source_tree_sha256'],
            input_sha256=m.input_digest(self.inputs), input_files=len(self.inputs),
            reviews={k: dict(passed=True, agent_id=k + '-independent', open_p1_p2=0) for k in ['spec', 'quality']})

    def test_scope_and_valid_process_remain_bounded(self):
        self.assertEqual(m.SCOPE, SCOPE)
        self.validate(self.report())
        m.validate_review(self.review(), self.runtime, self.inputs)
        for key in FLAGS:
            self.assertIs(m.initial()[key], False)

    def test_process_fields_and_metadata_reject_counterexamples(self):
        edits = [('passed', 1), ('state', 'RUNNING'), ('checks', m.CHECKS[:-1]),
            ('checks', list(reversed(m.CHECKS))), ('simulation_checks', True), ('source_faults', 1),
            ('complete_poststate_checks', 6), ('profile', 'uniswap-v2'), ('adapter_version', 'legacy'),
            ('native_fixture_sha256', 'f' * 64), ('authority', 'CANONICAL'), ('source_binding', {}),
            ('input_hashes', {}), ('inputs_unchanged', 1),
            ('execution_files_unchanged_except_authorized_reloads', False)]
        edits += [(k, True) for k in FLAGS]
        for key, bad in edits:
            report = self.report(); report[key] = bad
            with self.subTest(key=key), self.assertRaises(ValueError): self.validate(report)
        for key in FLAGS:
            report = self.report(); report.pop(key)
            with self.assertRaises(ValueError): self.validate(report)

    def test_runtime_and_fivebins_cannot_be_self_resealed(self):
        for edit in [lambda r: r['binaries'].pop(m.BINARY_PATHS[0]),
                     lambda r: r['binaries'].update(unknown='f' * 64),
                     lambda r: r.update(source_tree_sha256='partial'),
                     lambda r: r.update(unchecked_source='f' * 64),
                     lambda r: r['binaries'].update({m.BINARY_PATHS[0]: False})]:
            runtime = copy.deepcopy(self.runtime); edit(runtime)
            report = self.report(); report['source_binding'] = runtime
            with self.assertRaises(ValueError): m.validate_process(report, runtime, self.inputs, self.now)
        with self.assertRaises(ValueError): m.validate_process(self.report(), self.runtime, {}, self.now)

    def test_window_rejects_missing_boolean_future_reversed_and_stale(self):
        for key, bad in [('started_at_unix_ms', True), ('started_at_unix_ms', self.now),
                ('completed_at_unix_ms', None), ('completed_at_unix_ms', self.now + 1),
                ('completed_at_unix_ms', self.now - 3_600_001), ('completed_at_unix_ms', '9999000')]:
            report = self.report(); report[key] = bad
            with self.subTest(key=key, bad=bad), self.assertRaises(ValueError): self.validate(report)

    def test_all_rpc_reads_and_exact_test_limits_required(self):
        for key in m.RPC_READS:
            for bad in [False, 0, -1, '1', 1.0, None]:
                report = self.report(); report['read_counts'][key] = bad
                with self.subTest(method=key, bad=bad), self.assertRaises(ValueError): self.validate(report)
        self.assertIn('eth_getStorageAt', m.RPC_READS)
        self.assertIn('eth_getTransactionReceipt', m.RPC_READS)
        for key, expected in m.TEST_LIMITS.items():
            for bad in [expected + 1, float(expected), None]:
                report = self.report(); report['test_limits'][key] = bad
                with self.assertRaises(ValueError): self.validate(report)
        report = self.report(); report['test_limits']['extra'] = 1
        with self.assertRaises(ValueError): self.validate(report)

    def test_poststate_independently_checks_values_abi_getters_storage_and_metering(self):
        for phase in self.evidence(self.document['cases'][0])['observations']:
            edits = [lambda r: r.update(target='0x' + '0' * 40), lambda r: r.update(calldata='0x'),
                     lambda r: r.update(phase='other'), lambda r: r.update(output='0x00'),
                     lambda r: r.update(output=r['output'] + '00'), lambda r: r.update(output='0X' + r['output'][2:]),
                     lambda r: r.update(output='0x' + word(1)), lambda r: r['usage'].update(reads=True),
                     lambda r: r['usage'].update(read_bytes=0), lambda r: r.update(extra='forged')]
            if phase == 'pair_lock_after':
                edits += [lambda r: r.update(type=None), lambda r: r.update(slot='0x' + word(13)),
                    lambda r: r['usage'].update(read_bytes=85), lambda r: r['usage'].update(reads=2)]
                edits += [lambda r, k=k: r['usage'].update({k: True}) for k in ['gas_used', 'instructions', 'input_bytes', 'executions']]
            else:
                edits += [lambda r: r.update(type='STORAGE'), lambda r: r.update(slot='0x' + word(12)),
                          lambda r: r['usage'].update(executions=0), lambda r: r['usage'].update(gas_used=0)]
            for mutation, edit in enumerate(edits):
                report = self.report(); row = report['complete_poststate_evidence'][0]['observations'][phase]
                # Even lock's correct word(1) must be changed by this mutation.
                if mutation == 6 and phase == 'pair_lock_after':
                    row['output'] = '0x' + word(0)
                else: edit(row)
                with self.subTest(phase=phase, edit=mutation), self.assertRaises(ValueError): self.validate(report)

    def test_poststate_inventory_timeline_bank_and_receipt_cannot_be_relabelled(self):
        edits = [lambda rows: rows.pop(), lambda rows: rows.append(copy.deepcopy(rows[0])),
            lambda rows: rows.reverse(), lambda rows: rows[0].update(case=rows[2]['case']),
            lambda rows: rows[0].update(source_block_hash='0x' + '0' * 64),
            lambda rows: rows[0].update(receipt_hash='bad'),
            lambda rows: rows[0]['observations'].pop('pair_lock_after'),
            lambda rows: rows[0]['observations'].update(extra=rows[0]['observations']['pair_lock_after'])]
        for edit in edits:
            report = self.report(); edit(report['complete_poststate_evidence'])
            with self.assertRaises(ValueError): self.validate(report)

    def test_image_history_allows_only_two_router_state_reload_paths(self):
        for stage in range(3):
            for path in m.EXECUTION_FILES:
                report = self.report(); report['execution_file_history'][stage]['hashes'][path] = 'f' * 64
                if stage == 2: report['execution_file_hashes'][path] = 'f' * 64
                with self.subTest(stage=stage, path=path), self.assertRaises(ValueError): self.validate(report)
        for edit in [lambda r: r['execution_file_history'].pop(), lambda r: r['execution_file_history'].reverse(),
                lambda r: r['execution_file_history'][1].update(stage='automatic'),
                lambda r: r['execution_file_history'][1].update(case='sync-0-to-1'),
                lambda r: r['execution_file_history'][0]['hashes'].update(extra='a' * 64)]:
            report = self.report(); edit(report)
            with self.assertRaises(ValueError): self.validate(report)

    def test_final_owner_must_be_case2_and_generation_at_least_three(self):
        for key, bad in [('online_generation', 2), ('online_generation', True), ('schema_version', True),
                         ('owner_service', 'chain-parser-rust'), ('scope_hash', None), ('writer_epoch', 0)]:
            report = self.report(); report['owner_reference'][key] = bad
            with self.assertRaises(ValueError): self.validate(report)
        report = self.report(); report['owner_reference']['block']['hash'] = self.document['cases'][0]['state_anchor']['block_hash']
        with self.assertRaises(ValueError): self.validate(report)

    def test_reviews_bind_allinputs_and_distinct_spec_quality_agents(self):
        for edit in [lambda r: r.update(scope='all'), lambda r: r.update(source_tree_sha256='f' * 64),
                lambda r: r.update(input_sha256='f' * 64), lambda r: r.update(input_files=True),
                lambda r: r['reviews'].pop('quality'), lambda r: r['reviews'].update(extra={}),
                lambda r: r['reviews']['quality'].update(agent_id=' spec-independent '),
                lambda r: r['reviews']['quality'].update(agent_id=' '),
                lambda r: r['reviews']['spec'].update(passed=1),
                lambda r: r['reviews']['spec'].update(open_p1_p2=False),
                lambda r: r['reviews']['spec'].update(open_p1_p2=1)]:
            review = self.review(); edit(review)
            with self.assertRaises(ValueError): m.validate_review(review, self.runtime, self.inputs)
        self.assertEqual(m.input_digest({'z': '1', 'a': '2'}), digest(b'{"a":"2","z":"1"}'))

    def test_frozen_native_corpus_counts_are_derived(self):
        self.assertEqual(m.check_corpus(), dict(native_cases=24, native_successes=20, native_reverts=4,
            post_accounts=273, post_storage_slots=1525, sequences=4, sequence_steps=10,
            native_transactions=84, transaction_successes=80, transaction_reverts=4,
            runtime_identities=12, raw_traces=79, scripts=13, package_files=93, loaded_dependencies=284))

    def test_corpus_rejects_coherent_truncation_and_getter_code_metadata_forgery(self):
        edits = [lambda d: (d['cases'].pop(), d['reference'].update(case_count=23, revert_count=3), d['reference']['case_inventory'].pop()),
            lambda d: [c['expected']['post_accounts'].pop() for c in d['cases']],
            lambda d: d['cases'][0]['expected']['post_accounts'][-1]['storage'].pop(),
            lambda d: d['cases'][0]['expected']['post_accounts'][-1].update(code='0x00'),
            lambda d: d['cases'][0]['expected']['pool_after']['v2']['swap'].update(raw_balance0='1'),
            lambda d: d['cases'][0]['evidence']['post_getters'][0].update(calls=[]),
            lambda d: d['reference']['transactions'][0]['trace'].update(gzip_sha256='f' * 64),
            lambda d: d['reference']['runtime_identities'].pop(), lambda d: d['reference']['loaded_dependencies'].pop(),
            lambda d: d['reference']['scripts'].pop(), lambda d: d['source_pins']['packages'][0]['files'].pop(),
            lambda d: d['sequences'][0]['steps'].pop(), lambda d: d.update(authority='canonical')]
        for edit in edits:
            document = copy.deepcopy(self.document); edit(document)
            with self.assertRaises(ValueError): m.check_corpus(document)

    def test_all_three_fixture_bytes_are_pinned(self):
        with tempfile.TemporaryDirectory() as folder, patch.object(m, 'ROOT', Path(folder)):
            for relative in m.FIXTURE_PATHS:
                path = m.ROOT / relative; path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes((ROOT / m.FIXTURE_PATHS[0]).read_bytes())
            m.check_corpus()
            for relative in m.FIXTURE_PATHS:
                path = m.ROOT / relative; original = path.read_bytes(); path.write_bytes(original + b'\n')
                with self.assertRaises(ValueError): m.check_corpus()
                path.write_bytes(original)

    def test_source_closure_rejects_partial_or_resealed_current_bindings(self):
        inputs = m.required_inputs(self.document)
        m.validate_source_inputs(self.document, inputs)
        keys = [next(k for k in inputs if k.endswith('/LICENSE')), self.document['reference']['node']['executable'],
            self.document['reference']['loaded_dependencies'][0]['path'],
            'integration/.runtime/stable-reference/package.json',
            'chain-parser-go/test/contracts/v2-complete.json', 'dex-router/test/contracts/v2-complete.json',
            next(k for k in inputs if k.endswith('.gz')), m.STATIC_CODE_PATHS[0]]
        for key in keys:
            bad = dict(inputs); bad.pop(key)
            with self.subTest(missing=key), self.assertRaises(ValueError): m.validate_source_inputs(self.document, bad)
            bad = dict(inputs); bad[key] = 'f' * 64
            with self.subTest(resealed=key), self.assertRaises(ValueError): m.validate_source_inputs(self.document, bad)
        for name in ['verified.json', 'file-manifest.json']:
            bad = dict(inputs); bad['integration/tools/reference/v2-complete/' + name] = 'a' * 64
            with self.assertRaises(ValueError): m.validate_source_inputs(self.document, bad)

    def test_actual_collector_covers_complete_source_and_native_closure(self):
        # Independently enumerate the actual native manifest, then also exercise
        # the gate validator. A fabricated full map cannot stand in for this.
        inputs = m.snapshot_inputs(); reference = self.document['reference']
        rows = reference['scripts'] + reference['loaded_dependencies']
        rows += [dict(path=reference['node']['executable'], sha256=reference['node']['sha256'])]
        rows += [dict(path='integration/tools/reference/evm-execution/' + row['path'], sha256=row['sha256'])
                 for package in self.document['source_pins']['packages'] for row in package['files']]
        rows += [dict(path='integration/tools/reference/v2-complete/traces/' + tx['trace']['file'], sha256=tx['trace']['gzip_sha256'])
                 for tx in reference['transactions']]
        for row in rows:
            with self.subTest(path=row['path']): self.assertEqual(inputs.get(row['path']), row['sha256'])
        for service in ['integration', 'chain-parser-go', 'dex-router']:
            self.assertEqual(inputs.get(service + '/test/contracts/v2-complete.json'),
                             '63e8260ea53bec9cc9226f7e64e01b463af04a30d7afb773415134e591f494ee')
        for name in ['verified.json', 'file-manifest.json']:
            self.assertNotIn('integration/tools/reference/v2-complete/' + name, inputs)
        m.validate_source_inputs(self.document, inputs)

    def test_image_hashes_match_actual_complete_bank_bundles(self):
        from v2_complete_bank import NativeCompleteBank
        bank = NativeCompleteBank()
        for index in range(3):
            bank.select_case(index)
            self.assertEqual(m.bundle_hash(self.document['cases'][index]),
                             digest(json.dumps(bank.bundle()).encode()))

    def test_native_command_success_records_exact_commands_and_log_hashes(self):
        # These stand-ins only exercise command execution/log binding in a temp
        # root. The final gate invokes the original pinned native tools.
        with tempfile.TemporaryDirectory() as folder, patch.object(m, 'ROOT', Path(folder)), patch.object(m, 'OUT', Path(folder) / 'logs'):
            tools = m.ROOT / 'integration/tools/ci'; tools.mkdir(parents=True)
            for name in ['v2-complete-check.py', 'v2-complete-guards.py']:
                (tools / name).write_text('print("test command ran")\n')
            results = m.run_source_checks()
            self.assertEqual([r['name'] for r in results], ['native-exact-regeneration', 'native-mutation-guards', 'optimized-native-mutation-guards'])
            for row in results:
                self.assertEqual(row['exit_code'], 0)
                log = m.OUT / ('v2-complete-' + row['name'] + '-acceptance.log')
                self.assertEqual(row['log_sha256'], digest(log.read_bytes()))
                self.assertIn('-E', row['command'])
                self.assertTrue(row['command'][-1].endswith('/v2-complete-check.py' if row['name'] == 'native-exact-regeneration' else '/v2-complete-guards.py'))
            self.assertIn('-OO', results[-1]['command'])
            self.assertIn('optimize=0', results[-1]['command'][-2])

    def test_native_commands_and_oo_wrapper_preserve_native_asserts(self):
        with tempfile.TemporaryDirectory() as folder, patch.object(m, 'ROOT', Path(folder)), patch.object(m, 'OUT', Path(folder) / 'logs'):
            tools = m.ROOT / 'integration/tools/ci'; tools.mkdir(parents=True)
            (tools / 'v2-complete-check.py').write_text('print("original regeneration")\n')
            (tools / 'v2-complete-guards.py').write_text('from pathlib import Path\np=Path(__file__).with_suffix(".count")\nn=int(p.read_text())+1 if p.exists() else 1\np.write_text(str(n))\nassert n != 2, "native assertion retained"\n')
            with self.assertRaisesRegex(ValueError, 'optimized-native-mutation-guards'): m.run_source_checks()
            self.assertIn('native assertion retained', (m.OUT / 'v2-complete-optimized-native-mutation-guards-acceptance.log').read_text())

    def test_report_reader_rejects_duplicate_keys_nonfinite_and_nonobject_documents(self):
        self.assertTrue(callable(getattr(m, 'read_document', None)), 'strict report reader missing')
        for data in [b'{"passed":false,"passed":true}', b'{"nested":{"a":1,"a":2}}',
                     b'{"bad":NaN}', b'{"bad":Infinity}', b'[]', b'null']:
            with self.subTest(data=data), self.assertRaises(ValueError): m.read_document(data)
        self.assertEqual(m.read_document(b'{"passed":true}'), {'passed': True})

    def test_full_verification_checks_real_current_source_fivebins_and_verifier(self):
        # Exercise the actual checked_runtime implementation, not a mocked return.
        import weighted_evidence as weighted
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder); verifier = root / 'integration/tools/acceptance/verify.py'
            verifier.parent.mkdir(parents=True); verifier.write_text('current verifier')
            verification = dict(passed=True, source_unchanged_during_verification=True,
                source_tree_sha256=self.runtime['source_tree_sha256'], checked_binary_sha256=self.runtime['binaries'],
                tool_sha256=digest(verifier.read_bytes()), measured_at_unix_ms=self.now - 3000)
            with patch.object(weighted, 'ROOT', root), patch.object(weighted, 'OUT', root), \
                 patch.object(weighted, 'runtime_fingerprint', return_value=self.runtime):
                path = root / 'verification.json'; path.write_text(json.dumps(verification))
                self.assertEqual(m.checked_runtime(), (verification, self.runtime))
                for key, bad in [('passed', False), ('passed', 1), ('source_unchanged_during_verification', 1),
                        ('source_tree_sha256', 'f' * 64), ('checked_binary_sha256', {}), ('tool_sha256', 'f' * 64)]:
                    path.write_text(json.dumps(dict(verification, **{key: bad})))
                    with self.subTest(key=key), self.assertRaises(ValueError): m.checked_runtime()

    def test_main_failure_immediately_invalidates_old_gate(self):
        with tempfile.TemporaryDirectory() as folder, patch.object(m, 'OUT', Path(folder)), patch.object(m, 'TARGET', Path(folder) / 'gate.json'):
            m.TARGET.write_text('{"passed":true}')
            with patch.object(m, 'run', side_effect=ValueError('incomplete')):
                with self.assertRaises(ValueError): m.main()
            self.assertIs(json.loads(m.TARGET.read_text())['passed'], False)
            self.assertEqual(json.loads(m.TARGET.read_text())['state'], 'FAILED')

    def test_entry_import_failure_invalidates_old_success_before_import(self):
        with tempfile.TemporaryDirectory() as folder:
            root = Path(folder); script = root / 'integration/tools/acceptance/v2_complete_evidence.py'
            script.parent.mkdir(parents=True); script.write_bytes(Path(m.__file__).read_bytes())
            (script.parent / 'weighted_evidence.py').write_text('raise RuntimeError("missing dependency")\n')
            target = root / 'integration/artifacts/local/v2-complete-acceptance.json'
            target.parent.mkdir(parents=True); target.write_text('{"passed":true}')
            result = subprocess.run([sys.executable, '-B', '-OO', str(script)], capture_output=True)
            self.assertNotEqual(result.returncode, 0); self.assertIn(b'missing dependency', result.stderr)
            self.assertIs(json.loads(target.read_text())['passed'], False)

    def test_run_binds_bytes_chronology_and_detects_mid_check_changes(self):
        with tempfile.TemporaryDirectory() as folder, patch.object(m, 'OUT', Path(folder)):
            report = self.report(); review = self.review(); verification = dict(measured_at_unix_ms=report['started_at_unix_ms'] - 1)
            paths = [m.OUT / name for name in ['verification.json', 'v2-complete-three-service-smoke.json', 'v2-complete-review.json']]
            def reset():
                for path, data in zip(paths, [verification, report, review]): path.write_text(json.dumps(data))
            reset()
            with patch.object(m, 'checked_runtime', return_value=(verification, self.runtime)) as checked, \
                 patch.object(m, 'snapshot_inputs', return_value=self.inputs), patch.object(m, 'runtime_fingerprint', return_value=self.runtime), \
                 patch.object(m, 'validate_source_inputs'), patch.object(m, 'check_corpus', return_value={'native_cases': 24}), \
                 patch.object(m, 'run_source_checks', return_value=[]), patch.object(m.time, 'time', return_value=self.now / 1000):
                result = m.run(); checked.assert_called_once()
                def replace_before_binding():
                    paths[0].write_bytes(paths[0].read_bytes() + b'\n')
                    return verification, self.runtime
                with patch.object(m, 'checked_runtime', side_effect=replace_before_binding), self.assertRaises(ValueError): m.run()
                reset()
                self.assertIs(result['passed'], True); self.assertEqual(result['process_checks'], 14)
                for key in FLAGS: self.assertIs(result[key], False)
                for key, path in zip(['verification_sha256', 'process_sha256', 'review_sha256'], paths):
                    self.assertEqual(result[key], digest(path.read_bytes()))
                for target in paths:
                    def replace(target=target): target.write_bytes(target.read_bytes() + b'\n'); return []
                    with patch.object(m, 'run_source_checks', side_effect=replace), self.assertRaises(ValueError): m.run()
                    reset()
                with patch.object(m, 'snapshot_inputs', side_effect=[self.inputs, {'drift': 'f' * 64}]), self.assertRaises(ValueError): m.run()
                with patch.object(m, 'runtime_fingerprint', return_value={}), self.assertRaises(ValueError): m.run()
                paths[0].write_text(json.dumps(dict(verification, replaced=True)))
                with self.assertRaises(ValueError): m.run()
                reset(); verification['measured_at_unix_ms'] = report['started_at_unix_ms'] + 1; reset()
                with self.assertRaises(ValueError): m.run()


if __name__ == '__main__':
    unittest.main()
