"""Reject stale freezes, incomplete reviews and residual successful audits."""
from pathlib import Path
import copy
import hashlib
import json
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT / 'integration/tools/acceptance'))
import managed_v2_breaker_acceptance as gate


class BreakerClosureTests(unittest.TestCase):
    def setUp(self):
        self.runtime = {'source_tree_sha256': 'current', 'binaries': {'parser': 'binary'}}
        self.inputs = {'input': 'current-input'}
        self.files = {}
        self.serial = dict(schema_version=2, passed=True, state='PASSED', inputs_unchanged=True,
                           runtime=self.runtime, input_hashes=self.inputs, freeze_sha256='freeze', progress_structure_sha256='a' * 64,
                           started_at_unix_ms=100, completed_at_unix_ms=900, commands=[])
        for i, (stage, command) in enumerate(gate.COMMANDS):
            name = f'managed-v2-breaker-final-serial-{stage}.log'
            self.files[name] = stage.encode()
            evidence = {}
            for report in gate.STAGE_REPORTS.get(stage, ()):
                self.files[report] = b'{}'
                evidence[report] = hashlib.sha256(b'{}').hexdigest()
            self.serial['commands'].append(dict(stage=stage, command=list(command), exit_code=0,
                started_at_unix_ms=110 + i * 100, completed_at_unix_ms=190 + i * 100,
                log=name, log_sha256=hashlib.sha256(stage.encode()).hexdigest(), evidence_sha256=evidence))
        self.review = dict(runtime=self.runtime, input_hashes=self.inputs,
            spec_reviews=[dict(scope=scope, reviewer=f'spec-{i}', status='CLOSED')
                          for i, scope in enumerate(gate.REQUIRED_REVIEW_SCOPES)],
            quality_review=dict(reviewer='quality', status='CLOSED'),
            source_files={p: hashlib.sha256(b'code').hexdigest() for p in gate.REQUIRED_REVIEW_FILES})

    def serial_check(self, serial):
        gate.validate_serial(serial, self.runtime, self.inputs, 'freeze', self.files.__getitem__, 'a' * 64)

    def test_serial_binds_the_exact_current_freeze_commands_and_outputs(self):
        self.serial_check(self.serial)
        for key, value in [('runtime', {}), ('input_hashes', {}), ('freeze_sha256', 'old'),
                           ('state', 'RUNNING'), ('passed', False), ('inputs_unchanged', False)]:
            with self.subTest(field=key):
                bad = copy.deepcopy(self.serial); bad[key] = value
                with self.assertRaises(ValueError): self.serial_check(bad)
        bad = copy.deepcopy(self.serial); bad.pop('input_hashes')
        with self.assertRaises(ValueError): self.serial_check(bad)

    def test_wrong_missing_reordered_failed_or_unbound_stage_is_rejected(self):
        for field, value in [('command', ['true']), ('exit_code', 1), ('exit_code', False),
                             ('log_sha256', 'old'), ('log', '../old.log'),
                             ('started_at_unix_ms', 0), ('completed_at_unix_ms', 999),
                             ('evidence_sha256', {})]:
            with self.subTest(field=field, value=value):
                bad = copy.deepcopy(self.serial); bad['commands'][0][field] = value
                with self.assertRaises(ValueError): self.serial_check(bad)
        for commands in [self.serial['commands'][:-1], list(reversed(self.serial['commands']))]:
            bad = copy.deepcopy(self.serial); bad['commands'] = commands
            with self.assertRaises(ValueError): self.serial_check(bad)

    def test_current_output_bytes_must_match_the_retained_stage_commitment(self):
        self.files['verification.json'] = b'{"passed":true,"old":true}'
        with self.assertRaises(ValueError): self.serial_check(self.serial)

    def test_stage_report_must_have_been_produced_inside_this_command(self):
        gate.validate_stage_report(self.serial, 'verification', {'measured_at_unix_ms': 150})
        gate.validate_stage_report(self.serial, 'worker', {'started_at_unix_ms': 420, 'completed_at_unix_ms': 480})
        for stage, report in [('verification', {'measured_at_unix_ms': 99}),
                              ('verification', {'measured_at_unix_ms': True}),
                              ('verification', {}),
                              ('worker', {'started_at_unix_ms': 399, 'completed_at_unix_ms': 480}),
                              ('worker', {'started_at_unix_ms': 420, 'completed_at_unix_ms': 901}),
                              ('worker', {'started_at_unix_ms': 480, 'completed_at_unix_ms': 420})]:
            with self.subTest(stage=stage, report=report):
                with self.assertRaises(ValueError): gate.validate_stage_report(self.serial, stage, report)

    def test_review_requires_every_scope_file_and_independent_reviewer(self):
        gate.validate_review(self.review, self.runtime, self.inputs, lambda path: b'code')
        for field, value in [('spec_reviews', []), ('source_files', {}), ('runtime', {}),
                             ('input_hashes', {}), ('quality_review', {})]:
            with self.subTest(field=field):
                bad = copy.deepcopy(self.review); bad[field] = value
                with self.assertRaises(ValueError):
                    gate.validate_review(bad, self.runtime, self.inputs, lambda path: b'code')
        for mutation in ['missing-scope', 'duplicate-scope', 'missing-file', 'self-review', 'open', 'no-reviewer', 'changed-file']:
            with self.subTest(mutation=mutation):
                bad = copy.deepcopy(self.review)
                if mutation == 'missing-scope': bad['spec_reviews'].pop()
                if mutation == 'duplicate-scope': bad['spec_reviews'][-1] = bad['spec_reviews'][0]
                if mutation == 'missing-file': bad['source_files'].pop(next(iter(bad['source_files'])))
                if mutation == 'self-review': bad['spec_reviews'][0]['reviewer'] = 'quality'
                if mutation == 'open': bad['spec_reviews'][0]['status'] = 'OPEN'
                if mutation == 'no-reviewer': bad['spec_reviews'][0]['reviewer'] = ''
                if mutation == 'changed-file': bad['source_files'][next(iter(bad['source_files']))] = 'old'
                with self.assertRaises(ValueError):
                    gate.validate_review(bad, self.runtime, self.inputs, lambda path: b'code')

    def test_failed_or_interrupted_audit_never_leaves_an_old_success(self):
        with tempfile.TemporaryDirectory() as directory:
            target = Path(directory) / 'acceptance.json'
            for error in [ValueError('not complete'), KeyboardInterrupt()]:
                target.write_text(json.dumps({'passed': True, 'state': 'LOCAL_SCOPE_VERIFIED'}))
                def fail():
                    self.assertEqual(json.loads(target.read_text())['state'], 'RUNNING')
                    raise error
                with self.assertRaises(type(error)): gate.run_audit(fail, target)
                value = json.loads(target.read_text())
                self.assertIs(value['passed'], False); self.assertEqual(value['state'], 'FAILED')
            gate.run_audit(lambda: {'passed': True, 'state': 'LOCAL_SCOPE_VERIFIED'}, target)
            self.assertIs(json.loads(target.read_text())['passed'], True)


    def test_serial_rejects_missing_changed_or_invalid_progress_commitment(self):
        for value in [None, '', 'old', 'b' * 64, True]:
            with self.subTest(value=value):
                bad = copy.deepcopy(self.serial)
                if value is None: bad.pop('progress_structure_sha256')
                else: bad['progress_structure_sha256'] = value
                with self.assertRaises(ValueError): self.serial_check(bad)

    def test_audit_rejects_missing_or_changed_freeze_progress_before_reading_serial(self):
        from unittest import mock
        for value in [None, '', 'b' * 64]:
            frozen = dict(runtime=self.runtime, input_hashes=self.inputs)
            if value is not None: frozen['progress_structure_sha256'] = value
            with self.subTest(value=value), mock.patch.object(gate, 'load', return_value=frozen) as load, \
                 mock.patch.object(gate, 'runtime_fingerprint', return_value=self.runtime), \
                 mock.patch.object(gate, 'snapshot_inputs', return_value=self.inputs), \
                 mock.patch.object(gate, 'progress_structure_sha256', return_value='a' * 64):
                with self.assertRaisesRegex(ValueError, 'progress'): gate.audit()
                self.assertEqual(load.call_count, 1)


    def test_audit_rechecks_live_structure_and_records_its_digest(self):
        from unittest import mock
        fault = dict(hits=1, catchup_to_poison_ms=0, poison_started_monotonic=1,
                     withdrawal_events={actor: dict(latency_ms=0, observed_monotonic=1) for actor in ['router1', 'router2']},
                     withdrawals={actor: dict(evm_status=503, fast_quote_status=503, solana_status=200) for actor in ['router1', 'router2']})
        namespaces = []
        for name, frames, quotes, rejects in zip(['cached', 'dynamic', 'boundaries', 'shared'],
                                               [28, 163, 97, 12], [50, 302, 132, 32], [12, 0, 18, 0]):
            namespaces.append(dict(namespace=name, passed=True, restarted=True,
                source_frames=[dict(independent_cpp_identical=True)] * frames, quotes=[{}] * quotes,
                rejections=[dict(status=400, response={}, case='case')] * rejects,
                faults=[dict(fault, fault=str(i)) for i in range(6)]))
        reports = {
            'managed-v2-breaker-freeze.json': dict(runtime=self.runtime, input_hashes=self.inputs, progress_structure_sha256='a' * 64),
            'managed-v2-breaker-final-serial-run.json': self.serial,
            'verification.json': dict(measured_at_unix_ms=150),
            'managed-v2-breaker-three-service-smoke.json': dict(passed=True, state='PASSED', inputs_unchanged=True,
                source_binding_started=self.runtime, input_hashes_started=self.inputs, started_at_unix_ms=520,
                completed_at_unix_ms=580, namespaces=namespaces,
                independent_actual_plan_replay=dict(passed=True, executions=[dict(quote=dict(routes=[{}, {}]))] +
                    [dict(quote=dict(routes=[{}]))] * 229, quote_links=[{}] * 516, selected_transactions_executed=233),
                actual_selected_model_differential=dict(passed=True, executions=230, selected_transactions=233,
                    native_report_sha256='freeze', vectors_sha256='freeze', log_sha256='freeze')),
            'evm-execution-three-service-smoke.json': dict(passed=True, inputs_unchanged=True, simulation_checks=7,
                source_faults=2, source_binding=self.runtime, input_hashes=self.inputs,
                started_at_unix_ms=420, completed_at_unix_ms=480),
            'managed-v2-breaker-review.json': self.review,
        }
        # Keep this test focused on closure chronology; protocol validators have separate suites.
        go_log = 'Ran 7 tests in 0s\nEVM race inventory: 95 cases in 8 complete shards\n'
        go_log += ''.join(f"EVM race shard {i + 1}/8: " + ', '.join(f'Test{j}' for j in range(i * 12, min((i + 1) * 12, 95))) + '\n' for i in range(8))
        helper_log = ('Ran 44 tests in 0s\nOK\n' * 2) + ('Ran 17 tests in 0s\nOK\n' * 2)
        for changed in [False, True]:
            with self.subTest(changed=changed), \
                 mock.patch.object(gate, 'load', side_effect=reports.__getitem__), \
                 mock.patch.object(gate, 'sha', return_value='freeze'), \
                 mock.patch.object(gate, 'runtime_fingerprint', return_value=self.runtime), \
                 mock.patch.object(gate, 'snapshot_inputs', return_value=self.inputs), \
                 mock.patch.object(gate, 'progress_structure_sha256', side_effect=['a' * 64, ('b' if changed else 'a') * 64]) as digest, \
                 mock.patch.object(gate, 'validate_serial'), \
                 mock.patch.object(gate, 'validate_verification'), \
                 mock.patch.object(gate, 'validate_review'), \
                 mock.patch.object(gate, 'validate_rejected_case'), \
                 mock.patch.object(Path, 'read_text', side_effect=[helper_log, go_log]), \
                 mock.patch.object(Path, 'read_bytes', return_value=b'{"cases":[{"id":"case"}]}'):
                if changed:
                    with self.assertRaisesRegex(ValueError, 'progress'): gate.audit()
                else:
                    self.assertEqual(gate.audit()['progress_structure_sha256'], 'a' * 64)
                self.assertEqual(digest.call_count, 2)
                gate.validate_serial.assert_called_once_with(self.serial, self.runtime, self.inputs,
                                                             'freeze', mock.ANY, 'a' * 64)


class BreakerProgressStructureTests(unittest.TestCase):
    def setUp(self):
        sys.path.insert(0, str(ROOT / 'integration/tools'))
        from progress_document import load_progress
        self.document = load_progress(ROOT)
        # This fixture validates the breaker plan, even after later work batches
        # become current. Production audit still reads and binds its live plan.
        own = [batch for batch in self.document['batch_history']
               if batch.get('plan_anchor') == gate.PROGRESS_PLAN_ANCHOR]
        self.assertEqual(len(own), 1)
        self.document['current_batch'] = copy.deepcopy(own[0])

    def test_exact_current_plan_and_nonempty_unique_cb_inventory_are_required(self):
        for mutation in ['anchor', 'delete-cb5', 'empty', 'duplicate', 'extra']:
            bad = copy.deepcopy(self.document); batch = bad['current_batch']
            tasks = batch['plan_task_progress']
            if mutation == 'anchor': batch['plan_anchor'] = 'plan-other'
            if mutation == 'delete-cb5': tasks[:] = [r for r in tasks if r['task_id'] != 'CB5']
            if mutation == 'empty': tasks.clear()
            if mutation == 'duplicate': tasks.append(copy.deepcopy(tasks[0]))
            if mutation == 'extra': tasks.append(dict(task_id='CB6', work_packages=['WP25']))
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                gate.validate_progress_document(bad)

    def test_every_breaker_design_mapping_is_required_exactly_once(self):
        for mutation in ['delete-all', 'delete-one', 'duplicate', 'extra']:
            bad = copy.deepcopy(self.document); rows = bad['requirement_traceability']
            breaker = [r for r in rows if '.managed-v2-breaker-' in r['design_clause']]
            if mutation == 'delete-all': rows[:] = [r for r in rows if r not in breaker]
            if mutation == 'delete-one': rows.remove(breaker[0])
            if mutation == 'duplicate': rows.append(copy.deepcopy(breaker[0]))
            if mutation == 'extra': rows.append(dict(design_clause='23.managed-v2-breaker-extra', work_packages=['WP25']))
            with self.subTest(mutation=mutation), self.assertRaises(ValueError):
                gate.validate_progress_document(bad)

    def test_task_and_design_work_packages_cannot_be_missing_wrong_or_duplicate(self):
        for kind in ['task', 'design']:
            original = (self.document['current_batch']['plan_task_progress'] if kind == 'task' else
                        [r for r in self.document['requirement_traceability'] if '.managed-v2-breaker-' in r['design_clause']])
            for row in original:
                for packages in [[], ['WP26'], row['work_packages'] + row['work_packages'][:1]]:
                    bad = copy.deepcopy(self.document)
                    rows = (bad['current_batch']['plan_task_progress'] if kind == 'task' else
                            [r for r in bad['requirement_traceability'] if '.managed-v2-breaker-' in r['design_clause']])
                    rows[original.index(row)]['work_packages'] = packages
                    with self.subTest(kind=kind, row=row, packages=packages), self.assertRaises(ValueError):
                        gate.validate_progress_document(bad)

    def test_structure_is_canonical_and_ignores_normal_progress_updates(self):
        expected = gate.validate_progress_document(self.document)
        bad = copy.deepcopy(self.document)
        bad['current_batch']['plan_task_progress'].reverse()
        bad['requirement_traceability'].reverse()
        for row in bad['current_batch']['plan_task_progress'] + bad['requirement_traceability']:
            row.update(status='LOCAL_SCOPE_VERIFIED', title='updated', remaining='updated', evidence_refs=['new.json'])
            row['work_packages'].reverse()
            for key in ['paths', 'tests']:
                if key in row: row[key].reverse()
        self.assertEqual(gate.validate_progress_document(bad), expected)

    def test_structure_binds_paths_and_tests_and_rejects_empty_bindings(self):
        expected = gate.validate_progress_document(self.document)
        for field in ['paths', 'tests']:
            bad = copy.deepcopy(self.document)
            row = next(r for r in bad['requirement_traceability'] if '.managed-v2-breaker-' in r['design_clause'])
            row[field].append('changed-binding')
            self.assertNotEqual(gate.validate_progress_document(bad), expected)
            row[field] = []
            with self.assertRaises(ValueError): gate.validate_progress_document(bad)

    def test_digest_reads_live_document_and_hashes_canonical_structure(self):
        from unittest import mock
        import progress_document
        structure = gate.validate_progress_document(self.document)
        expected = hashlib.sha256(json.dumps(structure, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()).hexdigest()
        with mock.patch.object(progress_document, 'load_progress', return_value=self.document) as load:
            self.assertEqual(gate.progress_structure_sha256(ROOT), expected)
            load.assert_called_once_with(ROOT)
        self.document['current_batch']['plan_task_progress'].pop()
        with mock.patch.object(progress_document, 'load_progress', return_value=self.document):
            with self.assertRaises(ValueError): gate.progress_structure_sha256(ROOT)


class BreakerSerialRunnerTests(unittest.TestCase):
    def prepare(self):
        import managed_v2_breaker_final as runner
        from unittest import mock
        temporary = tempfile.TemporaryDirectory(); self.addCleanup(temporary.cleanup)
        root = Path(temporary.name); out = root / 'integration/artifacts/local'; out.mkdir(parents=True)
        runtime = {'source_tree_sha256': 'current', 'binaries': {}}
        inputs = {'input': 'current'}
        (out / 'managed-v2-breaker-freeze.json').write_text(json.dumps({'runtime': runtime, 'input_hashes': inputs, 'progress_structure_sha256': 'a' * 64}))
        for name, value in [('ROOT', root), ('OUT', out), ('TARGET', out / 'managed-v2-breaker-final-serial-run.json')]:
            patch = mock.patch.object(runner, name, value); patch.start(); self.addCleanup(patch.stop)
        for name, value in [('runtime_fingerprint', runtime), ('snapshot_inputs', inputs)]:
            patch = mock.patch.object(runner, name, return_value=value); patch.start(); self.addCleanup(patch.stop)
        patch = mock.patch.object(gate, 'progress_structure_sha256', return_value='a' * 64)
        patch.start(); self.addCleanup(patch.stop)
        return runner, out, runtime, inputs

    def test_serial_runner_commits_freeze_inputs_commands_and_each_output(self):
        import subprocess
        from unittest import mock
        runner, out, runtime, inputs = self.prepare()
        acceptance = out / 'managed-v2-breaker-local-acceptance.json'
        acceptance.write_text(json.dumps({'passed': True, 'state': 'LOCAL_SCOPE_VERIFIED'}))
        def execute(command, **kwargs):
            self.assertIs(json.loads(acceptance.read_text())['passed'], False)
            stage = next(name for name, args in gate.COMMANDS if list(args) == command)
            kwargs['stdout'].write(stage)
            for name in gate.STAGE_REPORTS.get(stage, ()): (out / name).write_text('{}')
            return subprocess.CompletedProcess(command, 0)
        with mock.patch.object(runner.subprocess, 'run', side_effect=execute): result = runner.run_serial()
        self.assertEqual(result['input_hashes'], inputs)
        self.assertEqual(result['progress_structure_sha256'], 'a' * 64)
        self.assertEqual(result['freeze_sha256'], hashlib.sha256((out / 'managed-v2-breaker-freeze.json').read_bytes()).hexdigest())
        gate.validate_serial(result, runtime, inputs, result['freeze_sha256'], lambda name: (out / name).read_bytes(), 'a' * 64)
        self.assertEqual(len(result['commands']), 5)
        self.assertIs(json.loads(acceptance.read_text())['passed'], False)

    def test_serial_failure_or_missing_freeze_invalidates_old_acceptance(self):
        import subprocess
        from unittest import mock
        runner, out, _, _ = self.prepare(); acceptance = out / 'managed-v2-breaker-local-acceptance.json'
        for missing in [False, True]:
            with self.subTest(missing_freeze=missing):
                acceptance.write_text(json.dumps({'passed': True, 'state': 'LOCAL_SCOPE_VERIFIED'}))
                if missing: (out / 'managed-v2-breaker-freeze.json').unlink()
                with mock.patch.object(runner.subprocess, 'run', return_value=subprocess.CompletedProcess([], 9)):
                    with self.assertRaises((ValueError, OSError)): runner.run_serial()
                self.assertEqual(json.loads(acceptance.read_text())['state'], 'FAILED')
                self.assertIs(json.loads(acceptance.read_text())['passed'], False)

    def test_source_input_or_freeze_change_stops_before_the_next_command(self):
        import subprocess
        from unittest import mock
        for field in ['runtime', 'inputs', 'freeze']:
            with self.subTest(field=field):
                runner, out, runtime, inputs = self.prepare()
                def execute(command, **kwargs):
                    for name in gate.STAGE_REPORTS.get('verification', ()): (out / name).write_text('{}')
                    if field == 'runtime': runtime['source_tree_sha256'] = 'changed'
                    if field == 'inputs': inputs['input'] = 'changed'
                    if field == 'freeze':
                        p = out / 'managed-v2-breaker-freeze.json'; p.write_text(p.read_text() + ' ')
                    return subprocess.CompletedProcess(command, 0)
                with mock.patch.object(runner.subprocess, 'run', side_effect=execute) as called:
                    with self.assertRaises(ValueError): runner.run_serial()
                self.assertEqual(called.call_count, 1)
                self.assertIs(json.loads((out / 'managed-v2-breaker-final-serial-run.json').read_text())['passed'], False)


    def test_runner_rejects_missing_or_changed_freeze_structure_before_any_command(self):
        from unittest import mock
        for value in [None, '', 'b' * 64]:
            runner, out, _, _ = self.prepare()
            path = out / 'managed-v2-breaker-freeze.json'; frozen = json.loads(path.read_text())
            if value is None: frozen.pop('progress_structure_sha256')
            else: frozen['progress_structure_sha256'] = value
            path.write_text(json.dumps(frozen))
            with self.subTest(value=value), mock.patch.object(runner.subprocess, 'run') as called:
                with self.assertRaisesRegex(ValueError, 'progress'): runner.run_serial()
                called.assert_not_called()
            self.assertIs(json.loads((out / 'managed-v2-breaker-local-acceptance.json').read_text())['passed'], False)

    def test_progress_change_after_any_stage_stops_and_invalidates_acceptance(self):
        import subprocess
        from unittest import mock
        for change_at in range(1, len(gate.COMMANDS) + 1):
            runner, out, _, _ = self.prepare(); calls = 0
            def execute(command, **kwargs):
                nonlocal calls
                calls += 1
                stage = next(name for name, args in gate.COMMANDS if list(args) == command)
                for name in gate.STAGE_REPORTS.get(stage, ()): (out / name).write_text('{}')
                if calls == change_at: gate.progress_structure_sha256.return_value = 'b' * 64
                return subprocess.CompletedProcess(command, 0)
            with self.subTest(stage=change_at), mock.patch.object(runner.subprocess, 'run', side_effect=execute):
                with self.assertRaisesRegex(ValueError, 'progress'): runner.run_serial()
            self.assertEqual(calls, change_at)
            self.assertEqual(json.loads((out / 'managed-v2-breaker-final-serial-run.json').read_text())['state'], 'FAILED')
            self.assertIs(json.loads((out / 'managed-v2-breaker-local-acceptance.json').read_text())['passed'], False)


if __name__ == '__main__': unittest.main()
