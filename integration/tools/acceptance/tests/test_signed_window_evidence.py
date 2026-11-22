from contextlib import ExitStack, redirect_stdout, redirect_stderr
import copy
import json
import io
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import signed_window_evidence as m


class SignedEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def test_default_cli_reports_real_references_or_failure_then_removes_workspace(self):
        first, last = self.blocks()
        argv = ['--checkpoint-root', '0x'+'33'*32, '--replay-binary', sys.executable]
        for passed in (True, False):
            paths = []
            def run(**args):
                path = args['output']
                paths.append(path)
                path.mkdir()
                if not passed:
                    (path/'report.json').write_text(json.dumps(dict(phase='FAILED', failed_phase='PRIME_A', error='CHECK_FAILED')))
                    raise ValueError('CHECK_FAILED')
                return dict(passed=True, source_mode='RETAINED_RECENT_SIGNED_RPC_BYTES', A=first, B=last,
                            observed_gap=1, verified_commits=1, matched_staged_records=1,
                            frozen_upstream_fetches=0, production_ready=False, consensus_verified=False)
            with patch.object(m, 'run_evidence', run), redirect_stdout(io.StringIO()) as stdout, redirect_stderr(io.StringIO()) as stderr:
                self.assertEqual(m.main(argv), 0 if passed else 1)
            self.assertFalse(paths[0].parent.exists())
            response = json.loads((stdout if passed else stderr).getvalue())
            if passed:
                self.assertEqual((response['A'], response['B']), (first, last))
            else:
                self.assertEqual(response['failed_phase'], 'PRIME_A')
                self.assertNotIn('report', response)

    def test_clock_counts_forward_jumps_without_refunding_on_rollback_or_new_phases(self):
        values = [10., 100.]
        clock = m.CampaignClock(lambda: values[0], lambda: values[1])
        values[:] = [11., 401.]
        self.assertEqual(clock.monotonic(), 311.)
        values[:] = [12., 99.]
        self.assertEqual(clock.monotonic(), 312.)
        values[:] = [14., 101.]
        self.assertEqual(clock.monotonic(), 314.)

    def test_runtime_busy_restoration_and_cleanup_even_if_capture_close_raises(self):
        original = [(module, getattr(module, 'time', None), getattr(module, 'subprocess', None)) for module in m.MODULES]
        global_popen, global_clock = subprocess.Popen, time.monotonic
        capture = Mock(cancel=threading.Event())
        capture.close.side_effect = ValueError('CLOSE_FAILED')
        with self.assertRaisesRegex(ValueError, 'CLOSE_FAILED'):
            with m.Runtime() as runtime:
                runtime.capture = capture
                self.assertIs(subprocess.Popen, global_popen)
                self.assertIs(time.monotonic, global_clock)
                with self.assertRaisesRegex(ValueError, 'SIGNED_EVIDENCE_BUSY'):
                    with m.Runtime():
                        self.fail('parallel campaign entered')
        self.assertTrue(capture.cancel.is_set())
        self.assertFalse(m.RUN_LOCK.locked())
        for module, clock, processes in original:
            self.assertIs(getattr(module, 'time', None), clock)
            self.assertIs(getattr(module, 'subprocess', None), processes)

    def test_runtime_only_stops_registered_children_and_retains_module_process_proxy(self):
        child = Mock(args=['owned'], pid=123)
        child.poll.return_value = None
        child.send_signal.side_effect = lambda _: setattr(child.poll, 'return_value', 0)
        with patch.object(m.subprocess, 'Popen', return_value=child) as spawn:
            with m.Runtime() as runtime:
                result = m.owner.subprocess.Popen(['owned'])
                self.assertIs(result, child)
            spawn.assert_called_once_with(['owned'])
        child.send_signal.assert_called_once_with(m.signal.SIGTERM)
        child.wait.assert_called()

    def test_runtime_reaps_real_owned_process_group_and_proxy_run_preserves_output(self):
        child = None
        started = time.monotonic()
        original_kill = m.os.killpg
        with patch.object(m.os, 'killpg', wraps=original_kill) as killed:
            with self.assertRaisesRegex(ValueError, 'OWNED_TEST_FAILURE'):
                with m.Runtime() as runtime:
                    result = m.owner.subprocess.run([sys.executable, '-c', 'print("original bytes")'],
                        capture_output=True, check=True, timeout=2, start_new_session=True)
                    self.assertEqual(result.stdout, b'original bytes\n')
                    child = runtime.spawn([sys.executable, '-c', 'import time;time.sleep(10)'], start_new_session=True)
                    raise ValueError('OWNED_TEST_FAILURE')
            self.assertTrue(killed.call_args_list)
            self.assertTrue(all(call.args[0] == child.pid for call in killed.call_args_list))
        self.assertIsNotNone(child.poll())
        self.assertLess(time.monotonic()-started, 3)

    def test_watchdog_cancels_capture_and_reaps_only_its_child_on_wall_jump(self):
        capture = Mock(cancel=threading.Event())
        values = [0., 100.]
        with m.Runtime() as runtime:
            runtime.capture = capture
            runtime.clock = m.CampaignClock(lambda: values[0], lambda: values[1])
            runtime.collection = runtime.clock.monotonic()+1800
            child = runtime.spawn([sys.executable, '-c', 'import time;time.sleep(10)'], start_new_session=True)
            values[:] = [1., 1901.]
            self.assertTrue(runtime.abort.wait(2))
            self.assertTrue(capture.cancel.is_set())
            self.assertEqual(runtime.reason, 'SIGNED_EVIDENCE_DEADLINE')
        self.assertIsNotNone(child.poll())

    def test_http_input_never_enters_retained_or_live_source_scope(self):
        for url in ['http://127.0.0.1:9000', 'https://user:secret@example.org', 'https://example.org/#fragment']:
            with self.subTest(url=url), self.assertRaisesRegex(ValueError, 'HTTPS'):
                m.https(url)
        m.https('https://example.org/api?key=secret')
        with self.assertRaisesRegex(ValueError, 'HTTPS'):
            m.https('https://example.org/api?key=secret', beacon=True)

    def test_collection_deadline_cannot_be_reset_and_freeze_keeps_phase_limits(self):
        values = [0., 100.]
        runtime = m.Runtime()
        runtime.clock = m.CampaignClock(lambda: values[0], lambda: values[1])
        runtime.collection = runtime.clock.monotonic()+1800
        values[:] = [1., 1901.]
        with self.assertRaisesRegex(ValueError, 'DEADLINE'):
            runtime.remaining()
        runtime.collection = None
        with self.assertRaisesRegex(ValueError, 'DEADLINE'):
            with runtime.bounded(600):
                values[:] = [2., 2502.]
                runtime.remaining()

    def test_refresh_retries_only_exact_temporary_errors_and_checks_installed_bytes_binding(self):
        first, _ = self.blocks()
        proof = dict(finality={'data': {'finalized_header': {'execution': {
            'block_number': '100', 'block_hash': first['hash'], 'parent_hash': first['parent_hash'],
            'timestamp': str(first['timestamp_ms']//1000)}}}})
        path = self.root/'proof.json';path.write_text(json.dumps(proof))
        original = path.read_bytes()
        runtime = m.Runtime()
        runtime.sleep = Mock()
        retries = []
        codes = ['PROVENANCE_BEACON_REFRESH_NETWORK', 'PROVENANCE_BEACON_UPDATE_PARTICIPATION']
        calls = []
        def cli(*args, **kwargs):
            self.assertEqual(path.read_bytes(), original)
            calls.append(1)
            if len(calls) <= 2:
                return subprocess.CompletedProcess(args, 1, b'', json.dumps(dict(
                    event='service_start_failed', fields=dict(error_code=codes[len(calls)-1]))).encode())
            return subprocess.CompletedProcess(args, 0, json.dumps(dict(
                execution_number=100, execution_hash=first['hash'])).encode(), b'')
        runtime.run = Mock(side_effect=cli)
        self.assertEqual(m.refresh(runtime, 'owner', 'cfg', path, 'https://beacon.example', retries)[2], first)
        self.assertEqual(retries, codes)
        self.assertEqual(runtime.sleep.call_count, 2)
        runtime.run = Mock(return_value=subprocess.CompletedProcess([], 0, json.dumps(dict(
            execution_number=101, execution_hash=first['hash'])).encode(), b''))
        with self.assertRaisesRegex(ValueError, 'REFRESH_BINDING'):
            m.refresh(runtime, 'owner', 'cfg', path, 'https://beacon.example', [])
        runtime.run = Mock(return_value=subprocess.CompletedProcess([], 1, b'', json.dumps(dict(
            event='service_start_failed', fields=dict(error_code='PROVENANCE_BEACON_UPDATE_SIGNATURE'))).encode()))
        with self.assertRaisesRegex(ValueError, 'PROVENANCE_BEACON_UPDATE_SIGNATURE'):
            m.refresh(runtime, 'owner', 'cfg', path, 'https://beacon.example', [])
        self.assertEqual(runtime.run.call_count, 1)

    def test_cold_refresh_rejection_cannot_install_an_unverified_file(self):
        path = self.root/'new-proof.json'
        runtime = m.Runtime()
        def reject(*args, **kwargs):
            path.write_bytes(b'{}')
            error = dict(event='service_start_failed', fields=dict(error_code='PROVENANCE_BEACON_REFRESH_NETWORK'))
            return subprocess.CompletedProcess([], 1, b'', json.dumps(error).encode())
        runtime.run = reject
        with self.assertRaisesRegex(ValueError, 'REJECTED_PROOF_CHANGED'):
            m.refresh(runtime, 'owner', 'cfg', path, 'https://beacon.example', [])

    @staticmethod
    def blocks():
        stamp = int(time.time()*1000)-60000
        first = dict(chain_key='evm:1', number_or_slot=100, hash='0x'+'11'*32,
                     parent_hash='0x'+'00'*32, timestamp_ms=stamp//1000*1000)
        last = dict(first, number_or_slot=101, hash='0x'+'22'*32, parent_hash=first['hash'])
        return first, last

    @staticmethod
    def window_report(first, last):
        ref = dict(block=first, journal_seq=1, online_generation=1, canonical_epoch=1,
                   writer_epoch=1, scope_hash='scope', adapter_set_hash='adapter')
        end = dict(ref, block=last, journal_seq=2)
        return dict(passed=True, journal_verified=True, source_mode='RETAINED_RECENT_SIGNED_RPC_BYTES',
            production_ready=False, consensus_verified=False, first=ref,
            commits=[dict(kind='BLOCK_COMMIT', reference=end, record_id='record')],
            target=dict(block_number='101', block_hash=last['hash']))

    def test_passed_flag_never_substitutes_for_complete_journal_lineage(self):
        first, last = self.blocks()
        report = self.window_report(first, last)
        self.assertEqual(m.window_lineage(report, first, last), 1)
        for field, value in [('writer_epoch', 2), ('scope_hash', 'other'), ('adapter_set_hash', 'other')]:
            changed = copy.deepcopy(report)
            changed['commits'][0]['reference'][field] = value
            with self.subTest(field=field), self.assertRaisesRegex(ValueError, 'LINEAGE'):
                m.window_lineage(changed, first, last)
        report['target']['block_hash'] = first['hash']
        with self.assertRaisesRegex(ValueError, 'LAST_BINDING'):
            m.window_lineage(report, first, last)

    def test_semantic_rejection_is_not_retried_even_when_unrelated_work_was_enqueued(self):
        directory = self.root/'failed';directory.mkdir()
        log = directory/'owner.log'
        before, after = dict(enqueued=1), dict(enqueued=2, pending=1)
        for code, expected in [('RPC_ERROR_-32000', True), ('V2_COMPLETE_STATE_PROOF_VALUE', False),
                               ('PROVENANCE_BEACON_UPDATE_SIGNATURE', False)]:
            log.write_text(json.dumps(dict(event='chain_sync_failed', fields=dict(error_code=code)))+'\n')
            self.assertEqual(m.missing_failure(ValueError('OWNER_JOURNAL_DISCONTINUITY'), directory, before, after), expected)
        self.assertFalse(m.missing_failure(ValueError('SIGNED_EVIDENCE_LINEAGE'), directory, before, after))

    def test_template_copy_keeps_original_bytes_and_provider_identity_without_network(self):
        upstreams = ('https://one.example', 'https://two.example', 'https://old-proof.example')
        source = self.root/'seed-cache'
        raw = b' {"jsonrpc":"2.0","id":1,"result":"0x1"}\n'
        capture = m.capture_module.SignedRPCCapture(source, upstreams, lambda *_: raw).start(serve=False)
        try:
            request = b'{"jsonrpc":"2.0","id":1,"method":"eth_chainId","params":[]}'
            for index in range(3):
                capture.response(index, request)
            self.assertTrue(capture.wait_pending(3))
        finally:
            capture.close()
        result = m.copy_templates(source, self.root/'new-cache', (*upstreams[:2], upstreams[0]))
        self.assertEqual(result, dict(records=2, excluded=1, authority='REQUEST_SHAPE_ONLY'))
        for path in (self.root/'new-cache').iterdir():
            self.assertEqual(path.read_bytes(), (source/path.name).read_bytes())
            self.assertEqual(path.stat().st_mode & 0o777, 0o600)

    def fake_campaign(self, corrupt=None, cold=False):
        first, last = self.blocks()
        proof = lambda block: dict(bootstrap={}, updates=[], finality={'data': {'finalized_header': {'execution': {
            'block_number': str(block['number_or_slot']), 'block_hash': block['hash'],
            'parent_hash': block['parent_hash'], 'timestamp': str(block['timestamp_ms']//1000)}}}})
        seed, config = self.root/'seed.json', self.root/'seed-config.json'
        seed.write_text(json.dumps(proof(first)))
        config.write_text(json.dumps(dict(chains=[dict(key='evm:1', genesis_hash=m.current.GENESIS,
            beacon_finality=dict(checkpoint_root='0x'+'33'*32))], startup={}, journal={})))
        class Capture:
            endpoints = ('http://127.0.0.1:31001', 'http://127.0.0.1:31002', 'http://127.0.0.1:31003')
            def __init__(self, *_):
                self.closed = self.frozen = False
                self.cancel = threading.Event()
            def __enter__(self): return self
            def __exit__(self, *_): self.close()
            def close(self): self.closed = True;self.cancel.set()
            def mapping(self): return []
            def stats(self):
                return dict(closed=self.closed, frozen=self.frozen, storage_failed=False,
                            pending=0, errors=0, enqueued=0, fetches=0)
            def freeze(self): self.frozen = True
        def refresh(runtime, binary, cfg, path, beacon, retries, after=None):
            block = first if after is None else last
            raw = json.dumps(proof(block)).encode()
            path.write_bytes(raw)
            return raw, proof(block), block
        def primed(binary, capture, source, root, output, **kwargs):
            output.mkdir()
            fixed = output/'signed-proof.json';fixed.write_bytes(source.read_bytes())
            return dict(primed=True, code='PRIMED', attempts=[dict(code='PRIMED')], proof_file=str(fixed),
                        binding=dict(execution_number=100, execution_hash=first['hash']))
        def exercise(binary, directory, *_args, **kwargs):
            directory.mkdir()
            result = self.window_report(first, last)
            if corrupt == 'lineage':
                result['commits'][0]['reference']['writer_epoch'] = 2
            (directory/'report.json').write_text(json.dumps(result))
            (directory/'owner-config.json').write_bytes(kwargs['execution_config'].read_bytes())
            if corrupt == 'proof':
                kwargs['next_proof'].write_bytes(kwargs['next_proof'].read_bytes()+b' ')
            return result
        def quotes(output, _owner, _router, *_args, **kwargs):
            pool = dict(m.current.POOL, reserve0=str(10**30), reserve1=str(10**30))
            ref = dict(block=last)
            rows = []
            for _, request in m.current.requests_for(pool):
                amount = int(request.get('sellAmount', 0))
                rin = rout = 10**30
                body = dict(snapshotRef=ref, verification=dict(level='INDICATIVE', success=False),
                            costs=dict(unavailableReason='CHAIN_FEE_MODEL_NOT_IMPLEMENTED'))
                if request['kind'] == 'EXACT_IN':
                    body.update(grossBuyAmount=str(amount*997*rout//(rin*1000+amount*997)),
                                consumedIn=str(amount), refund='0')
                else:
                    target = int(request['buyAmount']);denom = (rout-target)*997
                    body['requiredSellAmount'] = str((rin*target*1000+denom-1)//denom)
                rows.append(dict(request=request, response=dict(status=200, body=body)))
            return dict(baseline=dict(body=dict(reference=ref, pools=[pool])), quotes=rows,
                source_evidence=dict(body=dict(confirmation_tag='finalized', consensus_verified=False,
                                               confirmation_policy='PINNED_BEACON_FINALITY')),
                signed_finality=dict(proof_sha256=m.current.sha(Path(kwargs['signed_finality']['proof_file']).read_bytes())),
                replica_recovery=dict(healthy_peer_quote=rows[0]['response'], cold_restarted_quote=rows[0]['response']))
        decoded = dict(verified_blocks=[dict(block=first), dict(block=last, staged_sha256='staged')],
                       matched_staged_records=1)
        with ExitStack() as stack:
            for target, name, replacement in [(m.capture_module, 'SignedRPCCapture', Capture),
                    (m, 'refresh', refresh), (m.prime, 'prime_current_bank', primed),
                    (m.window, 'prefill_signed_identities', Mock(return_value=4)),
                    (m.window, 'prefill_signed_range', Mock(return_value=dict(planned=1, completed=1, skipped=0))),
                    (m.owner, 'exercise', exercise), (m.current, 'run_processes', quotes),
                    (m.replay, 'replay', Mock(return_value=decoded)),
                    (m.replay, 'read_window', Mock(return_value=({}, {(101, last['hash']): 'staged'})))]:
                stack.enter_context(patch.object(target, name, replacement))
            return m.run_evidence(owner_binary=sys.executable, router_binary=sys.executable,
                replay_binary=sys.executable, output=self.root/'campaign', checkpoint_root='0x'+'33'*32,
                seed_proof=None if cold else seed, seed_config=None if cold else config, body_urls=('https://one.example', 'https://two.example'),
                proof_url='https://one.example', beacon_url='https://beacon.example')

    def test_cold_campaign_can_start_from_versioned_development_config(self):
        report = self.fake_campaign(cold=True)
        self.assertTrue(report['passed'])
        self.assertIsNone(report['seed_inputs']['proof_sha256'])
        self.assertEqual(report['identity_records'], 4)

    def test_complete_composition_keeps_disclosure_and_real_quote_and_raw_digest_checks(self):
        report = self.fake_campaign()
        self.assertTrue(report['passed']);self.assertTrue(report['capture_closed'])
        self.assertFalse(report['production_ready']);self.assertFalse(report['consensus_verified'])
        self.assertEqual(report['proof_shared_with_body_indices'], [0])
        self.assertEqual(report['failure_domain_independence'], 'UNKNOWN')
        self.assertEqual((report['verified_commits'], report['matched_staged_records'], report['frozen_upstream_fetches']), (1, 1, 0))

    def test_composition_rejects_passed_with_wrong_lineage_and_records_failure(self):
        with self.assertRaisesRegex(ValueError, 'WINDOW_REJECTED'):
            self.fake_campaign(corrupt='lineage')
        report = m.read_json(self.root/'campaign/report.json')[1]
        self.assertFalse(report['passed'])
        self.assertEqual(report['attempts'][-1]['code'], 'SIGNED_EVIDENCE_LINEAGE')

    def test_composition_rejects_proof_byte_change_even_when_signed_fields_match(self):
        with self.assertRaisesRegex(ValueError, 'WINDOW_REJECTED'):
            self.fake_campaign(corrupt='proof')
        report = m.read_json(self.root/'campaign/report.json')[1]
        self.assertFalse(report['passed'])
        self.assertEqual(report['attempts'][-1]['code'], 'SIGNED_EVIDENCE_PROOF_CHANGED')


if __name__ == '__main__':
    unittest.main()
