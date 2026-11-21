import contextlib
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import Mock, patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import replay_beacon_window as replay
import verify_beacon_owner as owner


class RetainedSignedWindow(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name).resolve()
        self.live = ('https://one.example/rpc', 'https://two.example/rpc', 'https://proof.example/rpc')
        self.retained = ('http://127.0.0.1:31001', 'http://127.0.0.1:31002', 'http://127.0.0.1:31003')

    def config(self, path, urls):
        chain = dict(key='evm:1', genesis_hash=owner.GENESIS, rpc_urls=list(urls[:2]),
                     beacon_finality={})
        if len(urls) == 3:
            chain['proof_rpc_url'] = urls[2]
        path.write_text(json.dumps({'chains': [chain]}))
        return path

    def test_retained_requires_explicit_config_and_never_changes_https_defaults(self):
        self.assertEqual(owner.execution_endpoints(None), owner.ENDPOINTS)
        with self.assertRaisesRegex(ValueError, 'RETAINED_SIGNED_EXECUTION_CONFIG_REQUIRED'):
            owner.execution_endpoints(None, retained_signed=True)
        path = self.config(self.root/'config.json', self.live)
        self.assertEqual(owner.execution_endpoints(path), self.live)
        with self.assertRaisesRegex(ValueError, 'URL'):
            owner.execution_endpoints(path, retained_signed=True)
        self.config(path, self.retained)
        self.assertEqual(owner.execution_endpoints(path, retained_signed=True), self.retained)
        with self.assertRaisesRegex(ValueError, 'URL'):
            owner.execution_endpoints(path)

    def test_every_retained_body_and_proof_url_is_path_free_literal_loopback(self):
        path = self.root/'config.json'
        invalid = ['https://one.example/rpc', 'http://localhost:31001', 'http://127.0.0.2:31001',
                   'http://127.0.0.1:0', 'http://127.0.0.1:65536', 'http://127.0.0.1:031001',
                   'http://127.0.0.1:31001/', 'http://127.0.0.1:31001/0',
                   'http://127.0.0.1:31001?source=one', 'http://127.0.0.1:31001#fragment',
                   'http://user:password@127.0.0.1:31001', 'http://[::1]:31001']
        for url in invalid:
            for index in range(3):
                with self.subTest(url=url, source=index):
                    urls = list(self.retained)
                    urls[index] = url
                    self.config(path, urls)
                    with self.assertRaisesRegex(ValueError, 'URL'):
                        owner.execution_endpoints(path, retained_signed=True)
        self.config(path, ('http://127.0.0.1:1', 'http://127.0.0.1:65535'))
        self.assertEqual(owner.execution_endpoints(path, retained_signed=True),
                         ('http://127.0.0.1:1', 'http://127.0.0.1:65535'))

    @staticmethod
    def proof(number, block_hash):
        return {'data': {'finalized_header': {'execution': {
            'block_number': str(number), 'block_hash': block_hash}}}}

    def test_exercise_reports_actual_input_mode_and_keeps_freshness_and_request_budget(self):
        bootstrap = self.root/'bootstrap.json'
        bootstrap.write_text('{}')
        finality = self.root/'finality.json'
        finality.write_text(json.dumps(self.proof(100, 'start')))
        following = self.root/'next.json'
        following.write_text(json.dumps({'bootstrap': {}, 'finality': self.proof(101, 'end')}))
        first = dict(block=dict(chain_key='evm:1', number_or_slot=100, hash='start'), journal_seq=1)
        end = dict(first, journal_seq=2, block=dict(chain_key='evm:1', number_or_slot=101,
                                                  hash='end', parent_hash='start'))
        record = dict(kind='BLOCK_COMMIT', reference=end, record_id='commit')
        for retained in [False, True]:
            urls = self.retained if retained else self.live
            config = self.config(self.root/'execution.json', urls)
            for advancing in [False, True]:
                with self.subTest(retained=retained, advancing=advancing):
                    output = self.root/f'owner-{retained}-{advancing}'
                    replies = [dict(status=200, body={'reference': first})]
                    if not advancing:
                        replies += [dict(status=503, body={}),
                                    dict(status=200, body={'reference': dict(first, journal_seq=2)})]
                    def advance(*args):
                        args[-1](record)
                        return [record]
                    with patch.object(owner, 'free_port', return_value=23000), \
                            patch.object(owner.subprocess, 'Popen', return_value=Mock(poll=lambda: None)), \
                            patch.object(owner, 'http', side_effect=replies),patch.object(owner, 'stop'), \
                            patch.object(owner, 'verify_advancement', side_effect=advance) as verify, \
                            patch.object(owner, 'audit_archive', return_value={'coverage_gaps': []}):
                        result = owner.exercise('owner-binary', output, bootstrap, None, finality, 'root',
                            next_proof=following if advancing else None, execution_config=config,
                            retained_signed=retained)
                    expected = 'RETAINED_RECENT_SIGNED_RPC_BYTES' if retained else 'LIVE_EXPLICIT_SIGNED_RPC'
                    self.assertEqual(result['source_mode'], expected)
                    self.assertFalse(result['consensus_verified']);self.assertFalse(result['production_ready'])
                    self.assertTrue(result['passed'])
                    self.assertEqual(json.loads((output/'report.json').read_bytes())['source_mode'], expected)
                    rendered = json.loads((output/'owner-config.json').read_bytes())
                    self.assertEqual(rendered['startup']['max_baseline_age_ms'], 3600000)
                    self.assertEqual(rendered['journal']['request_timeout_ms'], 60000)
                    self.assertEqual(rendered['chains'][0]['rpc_urls'], list(urls[:2]))
                    self.assertEqual(rendered['chains'][0]['proof_rpc_url'], urls[2])
                    if advancing:
                        self.assertEqual(verify.call_args.args[5], rendered['startup']['max_tail_blocks'])
                        self.assertTrue(result['journal_verified'])
                    else:
                        verify.assert_not_called()

    def test_replay_requires_opt_in_for_recorded_loopback_and_only_rewrites_to_archive(self):
        first = dict(block=dict(chain_key='evm:1', number_or_slot=7, hash='a'),
                     journal_seq=1, online_generation=1)
        end = dict(first, journal_seq=2, block=dict(chain_key='evm:1', number_or_slot=8,
                                                  hash='b', parent_hash='a'))
        report = dict(first=first, commits=[dict(kind='BLOCK_COMMIT', reference=end)],
                      target=dict(block_number='8', block_hash='b'))
        rows = [dict(block=ref['block'], staged_sha256='digest') for ref in [first, end]]
        for retained in [False, True]:
            with self.subTest(retained=retained):
                urls = self.retained if retained else self.live
                directory = self.root/f'replay-{retained}'
                directory.mkdir()
                self.config(directory/'owner-config.json', urls)
                (directory/'report.json').write_text(json.dumps(report))
                sources = {}
                for index, url in enumerate(urls):
                    source = replay.provider_id(url)
                    rpc = Mock(source=source, served=1, trace=[], errors=0)
                    rpc.start.return_value = f'http://127.0.0.1:{32001+index}'
                    sources[source] = rpc
                def decode(args, **kwargs):
                    cfg = json.loads(Path(args[args.index('--config')+1]).read_bytes())
                    self.assertEqual(cfg['chains'][0]['rpc_urls'],
                                     ['http://127.0.0.1:32001', 'http://127.0.0.1:32002'])
                    self.assertEqual(cfg['chains'][0]['proof_rpc_url'], 'http://127.0.0.1:32003')
                    self.assertEqual(kwargs['timeout'], 625)
                    return subprocess.CompletedProcess(args, 0,
                        json.dumps(dict(signed_height=8, signed_hash='b', rows=rows)).encode(), b'')
                with patch.object(replay, 'read_window', return_value=(sources, {(8, 'b'): 'digest'})) as read, \
                        patch.object(replay.subprocess, 'run', side_effect=decode):
                    result = replay.replay('decoder', directory, 'root', retained_signed=retained)
                self.assertEqual(result['scope'], 'historical-published-prefix')
                self.assertEqual(read.call_args.args[1], urls)
                if retained:
                    self.assertEqual(result['source_mode'], 'RETAINED_RECENT_SIGNED_RPC_BYTES')
                    self.assertFalse(result['production_ready']);self.assertFalse(result['consensus_verified'])
                else:
                    self.assertNotIn('source_mode', result)
                for rpc in sources.values():
                    rpc.close.assert_called_once()
                with patch.object(replay, 'read_window') as read, self.assertRaisesRegex(ValueError, 'URL'):
                    replay.replay('decoder', directory, 'root', retained_signed=not retained)
                read.assert_not_called()

    def test_clis_pass_explicit_mode_and_owner_requires_execution_config(self):
        output = self.root/'cli-output'
        output.mkdir()
        common = ['verify_beacon_owner.py', 'owner', str(output), 'bootstrap', 'finality', 'root']
        with patch.object(sys, 'argv', common+['--retained-signed']), \
                patch.object(owner, 'exercise') as exercise,contextlib.redirect_stderr(io.StringIO()), \
                self.assertRaises(SystemExit):
            owner.main()
        exercise.assert_not_called()
        for retained in [False, True]:
            mode = ['--retained-signed'] if retained else []
            with patch.object(sys, 'argv', common+['--execution-config', 'recorded.json']+mode), \
                    patch.object(owner, 'exercise', return_value={}) as exercise:
                owner.main()
                self.assertEqual(exercise.call_args.kwargs['retained_signed'], retained)
            with patch.object(sys, 'argv', ['replay_beacon_window.py', 'decoder', 'recorded', 'root']+mode), \
                    patch.object(replay, 'replay', return_value={}) as replayed, \
                    contextlib.redirect_stdout(io.StringIO()):
                replay.main()
                self.assertEqual(replayed.call_args.kwargs['retained_signed'], retained)


if __name__ == '__main__':
    unittest.main()
