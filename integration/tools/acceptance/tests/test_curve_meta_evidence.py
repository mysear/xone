"""Finite MetaUSD acceptance faults. No services, builds, or receipt mutations."""
import builtins
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import types
import unittest
from unittest.mock import patch

sys.dont_write_bytecode = True
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import curve_meta_evidence as E
import curve_meta_regression as R

CHECKS = [
    'four-metausd-pools-original-clone-implementation-and-historical-rpc',
    'complete-meta-balances-admin-base-vp-and-dependency-digests-in-live-protobuf',
    '32-original-exchanges-64-router-requests-62-positive-quotes-two-dust-rejections',
    'historical-banks-preserve-original-time-and-complete-source-identity',
    'five-invalid-meta-dependencies-withdraw-whole-evm-scope-and-recover-higher-generation',
    'unrelated-solana-scope-remains-ready',
    'memory-router-restart-restores-meta-bank',
    'verified-and-build-remain-disabled',
    'native-dust-explicitly-rejected',
]


def report():
    return dict(passed=True, checks=CHECKS.copy(), pool_count=4, service_steps=32,
                quote_checks=64, successful_quotes=62, dust_rejections=2,
                fault_names=['base-code', 'virtual-price', 'accounting', 'implementation', 'lp-decimals'],
                historical_fixture_replay=True, test_freshness_ms=315360000000, **E.FLAGS)


class TemporaryReports(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.out = Path(temporary.name)
        for module in (E, R):
            p = patch.object(module, 'OUT', self.out)
            p.start()
            self.addCleanup(p.stop)

    def write(self, name, data):
        (self.out / name).write_text(json.dumps(data))

    def seed(self, names):
        for name in names:
            self.write(name, {'passed': True})

    def failed(self, names):
        for name in names:
            path = self.out / name
            self.assertTrue(not path.exists() or json.loads(path.read_bytes()).get('passed') is False, name)


class Startup(TemporaryReports):
    def test_callable_optional_imports_invalidate_before_loading(self):
        original = builtins.__import__
        def broken(name, *args, **kwargs):
            if name in ('weighted_evidence', 'weighted_regression', 'curve_meta_evidence'):
                raise ImportError('injected optional helper failure')
            return original(name, *args, **kwargs)
        actions = [(E.bind_runtime, ['curve-meta-runtime-binding.json', 'curve-meta-scope-execution.json']),
                   (E.seal_scopes, ['curve-meta-scope-execution.json']),
                   (E.run, []), (E.validate, []),
                   (R.run, ['curve-meta-regression-command.json', 'curve-meta-scope-execution.json'] + E.SCOPES)]
        for action, names in actions:
            names = names + ['curve-meta-acceptance.json']
            with self.subTest(action=action.__name__):
                self.seed(names)
                with patch('builtins.__import__', broken), self.assertRaises(ImportError):
                    action()
                self.failed(names)

    def test_cli_optional_imports_and_arguments_fail_closed_under_oo(self):
        code = '''import builtins,sys
from pathlib import Path
script,out,*args=sys.argv[1:]
original=builtins.__import__
def broken(name,*args,**kwargs):
 if name in ('weighted_evidence','weighted_regression','curve_meta_evidence','argparse'):raise ImportError('optional dependency absent')
 return original(name,*args,**kwargs)
builtins.__import__=broken
sys.argv=[script]+args
source=Path(script).read_text().replace("OUT = ROOT / 'integration/artifacts/local'",'OUT = Path('+repr(out)+')')
exec(compile(source,script,'exec'),{'__file__':script,'__name__':'__main__'})
'''
        for module, args, names in [
            (E, [], []), (E, ['--invalid-option'], []),
            (E, ['--bind-runtime'], ['curve-meta-runtime-binding.json', 'curve-meta-scope-execution.json']),
            (E, ['--seal-scopes'], ['curve-meta-scope-execution.json']),
            (R, [], ['curve-meta-regression-command.json', 'curve-meta-scope-execution.json'] + E.SCOPES),
        ]:
            names = names + ['curve-meta-acceptance.json']
            with self.subTest(module=module.__name__, args=args):
                self.seed(names)
                result = subprocess.run([sys.executable, '-B', '-OO', '-c', code, module.__file__, str(self.out)] + args,
                                        capture_output=True, text=True)
                self.assertNotEqual(result.returncode, 0)
                self.failed(names)

    def test_bind_seal_and_regression_reuse_shared_helpers(self):
        E._invalidate('bind')
        for name in ['curve-meta-acceptance.json', 'curve-meta-runtime-binding.json', 'curve-meta-scope-execution.json']:
            self.assertEqual(json.loads((self.out / name).read_bytes())['state'], 'RUNNING')
        common = types.SimpleNamespace(bind_bundle=lambda prefix: {'passed': True, 'prefix': prefix},
                                       seal_bundle=lambda prefix, scopes: {'passed': True, 'scopes': scopes})
        with patch.dict(sys.modules, weighted_evidence=common):
            self.assertEqual(E.bind_runtime()['prefix'], 'curve-meta')
            self.assertEqual(E.seal_scopes()['scopes'], E.SCOPES)
        calls = []
        def execute(evidence, producers, prefix):
            for name in ['load', 'require', 'sha', 'runtime_fingerprint', 'snapshot_inputs', 'write_result']:
                self.assertTrue(callable(getattr(evidence, name)))
            calls.append((evidence, producers, prefix))
            return ['fresh']
        with patch.dict(sys.modules, weighted_regression=types.SimpleNamespace(run=execute)):
            self.assertEqual(R.run(), ['fresh'])
        self.assertEqual(calls, [(E, E.PRODUCERS, 'curve-meta')])
        self.assertEqual(E.PRODUCERS, [('smoke.py', 'three-service-smoke.json'),
                         ('multi_model_smoke.py', 'multi-model-three-service-smoke.json'),
                         ('curve_ng_smoke.py', 'curve-ng-three-service-smoke.json'),
                         ('curve_meta_smoke.py', 'curve-meta-three-service-smoke.json')])
        self.assertEqual(R.SCOPES, E.SCOPES)

    def test_startup_replacement_failure_removes_prior_authorization(self):
        self.seed(['curve-meta-acceptance.json'])
        with patch.object(Path, 'replace', side_effect=OSError('publication denied')):
            with self.assertRaises(OSError):
                E.run()
        self.failed(['curve-meta-acceptance.json'])


class Semantics(unittest.TestCase):
    def setUp(self):
        self.value = report()
        self.loader = lambda name: self.value if name == E.SCOPES[-1] else {'passed': True, **E.FLAGS}

    def test_independent_nine_semantics_and_finite_counts(self):
        self.assertEqual(list(E.REQUIRED_PROCESS_CHECKS), CHECKS)
        self.assertEqual(E._validate_process(None, self.loader), {k: self.value[k] for k in ['checks', 'pool_count', 'service_steps', 'quote_checks',
                          'successful_quotes', 'dust_rejections', 'fault_names', 'historical_fixture_replay', 'test_freshness_ms']})

    def test_every_semantic_is_required_independently_of_producer(self):
        for check in CHECKS:
            with self.subTest(check=check):
                self.value = report()
                self.value['checks'].remove(check)
                with patch.dict(sys.modules, curve_meta_smoke=types.SimpleNamespace(PROCESS_CHECKS=self.value['checks'])):
                    with self.assertRaises(ValueError):
                        E._validate_process(None, self.loader)
        self.value['checks'] = ['unrelated-' + str(i) for i in range(9)]
        with self.assertRaises(ValueError):
            E._validate_process(None, self.loader)

    def test_counts_flags_faults_and_historical_freshness_fail_closed(self):
        changes = [(k, v) for k, v in [('pool_count', 3), ('service_steps', 31), ('quote_checks', 63),
                   ('successful_quotes', 63), ('dust_rejections', 1), ('test_freshness_ms', 1),
                   ('historical_fixture_replay', False), ('historical_fixture_replay', 1),
                   ('fault_names', list(E.REQUIRED_FAULT_NAMES)[:-1]), ('checks', CHECKS + [CHECKS[0]]),
                   ('checks', list(reversed(CHECKS))), ('checks', CHECKS + ['unreviewed-extra'])]]
        changes += [(flag, True) for flag in E.FLAGS]
        changes += [(key, float(value)) for key, value in report().items() if type(value) is int]
        for key, value in changes:
            with self.subTest(key=key, value=value):
                self.value = report()
                self.value[key] = value
                with self.assertRaises(ValueError):
                    E._validate_process(None, self.loader)
        for key in report():
            self.value = report()
            del self.value[key]
            with self.subTest(missing=key), self.assertRaises(ValueError):
                E._validate_process(None, self.loader)


class Acceptance(TemporaryReports):
    def setUp(self):
        super().setUp()
        self.runtime = {'source_tree_sha256': 'current-source', 'binaries': {'router': 'binary'}}
        self.input = self.out / 'input.json'
        self.input.write_bytes(b'{"input":1}')
        self.inputs = {str(self.input): E.sha(self.input)}
        self.common = types.SimpleNamespace(validate_bundle=lambda prefix, scopes: (copy.deepcopy(self.runtime), copy.deepcopy(self.inputs)),
                     runtime_fingerprint=lambda: copy.deepcopy(self.runtime), snapshot_inputs=lambda: copy.deepcopy(self.inputs))
        self.review = {'source_tree_sha256': self.runtime['source_tree_sha256'], 'input_hashes': self.inputs.copy(),
                       'spec_review': {'status': 'CLOSED', 'open_important_findings': 0},
                       'quality_review': {'status': 'CLOSED', 'open_important_findings': 0}, **E.FLAGS}
        self.write('curve-meta-review.json', self.review)
        for name in ['verification.json', 'curve-meta-runtime-binding.json', 'curve-meta-scope-execution.json']:
            self.write(name, {'passed': True})
        for name in E.SCOPES:
            self.write(name, report() if name == E.SCOPES[-1] else {'passed': True, **E.FLAGS})
        for name in ['_validate_oracle', '_validate_service']:
            p = patch.object(E, name, return_value={'bounded': True})
            p.start()
            self.addCleanup(p.stop)
        p = patch.dict(sys.modules, weighted_evidence=self.common)
        p.start()
        self.addCleanup(p.stop)

    def test_finite_result_binds_exact_readset_and_current_reviews(self):
        result = E.run()
        self.assertTrue(result['passed'])
        self.assertEqual(result['fresh_scope_reports'], E.SCOPES)
        self.assertEqual(result['input_hashes'], self.inputs)
        for flag in E.FLAGS:
            self.assertIs(result[flag], False)
        for name, digest in result['validated_evidence_sha256'].items():
            self.assertEqual(E.sha(self.out / name), digest)
        self.assertEqual(result, json.loads((self.out / 'curve-meta-acceptance.json').read_bytes()))

    def test_missing_stale_open_and_unbound_reviews_fail_closed(self):
        for stage in ['spec_review', 'quality_review']:
            for key, value in [('status', 'OPEN'), ('open_important_findings', 1), ('open_important_findings', False)]:
                changed = copy.deepcopy(self.review)
                changed[stage][key] = value
                self.write('curve-meta-review.json', changed)
                with self.subTest(stage=stage, key=key), self.assertRaises(ValueError):
                    E.run()
                self.failed(['curve-meta-acceptance.json'])
        for key, value in [('source_tree_sha256', 'old'), ('input_hashes', {}), ('production_ready', True), ('verified', True)]:
            changed = copy.deepcopy(self.review)
            changed[key] = value
            self.write('curve-meta-review.json', changed)
            with self.subTest(key=key), self.assertRaises(ValueError):
                E.run()
        (self.out / 'curve-meta-review.json').unlink()
        with self.assertRaises(FileNotFoundError):
            E.run()
        self.failed(['curve-meta-acceptance.json'])

    def test_stale_input_and_scope_report_fail_closed(self):
        self.input.write_bytes(b'{"input":2}')
        with self.assertRaises(ValueError):
            E.run()
        self.input.write_bytes(b'{"input":1}')
        self.write(E.SCOPES[0], {'passed': False, **E.FLAGS})
        with self.assertRaises(ValueError):
            E.run()
        self.failed(['curve-meta-acceptance.json'])

    def test_review_replacement_with_identical_bytes_is_detected(self):
        original = E._validate_process
        def replace(common, loader):
            result = original(common, loader)
            path = self.out / 'curve-meta-review.json'
            other = self.out / 'replacement'
            other.write_bytes(path.read_bytes())
            other.replace(path)
            return result
        with patch.object(E, '_validate_process', side_effect=replace), self.assertRaises(ValueError):
            E.run()
        self.failed(['curve-meta-acceptance.json'])

    def test_pre_and_post_publication_bundle_runtime_input_barriers(self):
        for when in ['before', 'after']:
            for kind in ['bundle', 'runtime', 'inputs', 'review', 'scope', 'input-file']:
                with self.subTest(when=when, kind=kind):
                    self.write('curve-meta-review.json', self.review)
                    self.input.write_bytes(b'{"input":1}')
                    self.write(E.SCOPES[0], {'passed': True, **E.FLAGS})
                    self.common.validate_bundle = lambda prefix, scopes: (copy.deepcopy(self.runtime), copy.deepcopy(self.inputs))
                    self.common.runtime_fingerprint = lambda: copy.deepcopy(self.runtime)
                    self.common.snapshot_inputs = lambda: copy.deepcopy(self.inputs)
                    original = E.write_result
                    def publish(path, value, before_publish=None, after_publish=None):
                        def mutation():
                            if kind == 'bundle':
                                self.common.validate_bundle = lambda prefix, scopes: ({'changed': True}, self.inputs)
                            elif kind == 'runtime':
                                self.common.runtime_fingerprint = lambda: {'changed': True}
                            elif kind == 'inputs':
                                self.common.snapshot_inputs = lambda: {}
                            elif kind == 'input-file':
                                self.input.write_bytes(b'{"input":2}')
                            else:
                                self.write('curve-meta-review.json' if kind == 'review' else E.SCOPES[0], {'changed': True})
                        if value.get('passed') is True:
                            if when == 'before':
                                mutation()
                            else:
                                guard = after_publish
                                after_publish = lambda: (mutation(), guard())
                        return original(path, value, before_publish, after_publish)
                    with patch.object(E, 'write_result', side_effect=publish), self.assertRaises(ValueError):
                        E.run()
                    self.failed(['curve-meta-acceptance.json'])

    def test_publication_failure_under_oo_invalidates_prior_output(self):
        # Compile the real callable at optimization=2; inject only filesystem failure.
        namespace = {'__file__': E.__file__, '__name__': 'optimized_evidence'}
        exec(compile(Path(E.__file__).read_text(), E.__file__, 'exec', optimize=2), namespace)
        namespace['OUT'] = self.out
        namespace['_evaluate'] = lambda common: ({'passed': True}, lambda: None)
        real_replace = Path.replace
        def broken(path, target):
            if json.loads(path.read_bytes()).get('passed') is True:
                raise OSError('injected replacement failure')
            return real_replace(path, target)
        with patch.object(Path, 'replace', broken), self.assertRaises(OSError):
            namespace['run']()
        self.failed(['curve-meta-acceptance.json'])
        with self.assertRaises(ValueError):
            namespace['require'](False, 'explicit condition survives -OO')


class OriginalInputs(unittest.TestCase):
    def test_real_original_reference_and_all_service_wire_copies(self):
        import weighted_evidence as common
        inputs = common.snapshot_inputs()
        readset = E._EvidenceReadSet(names=[])
        oracle = E._validate_oracle(common, inputs, readset)
        self.assertEqual((oracle['scenarios'], oracle['steps'], oracle['probes'], oracle['reverted_probes']), (4, 32, 8, 6))
        self.assertEqual(oracle['runtime_bytes'], 20060)
        self.assertFalse(oracle['actual_three_pool_deployment_proven'])
        self.assertEqual(oracle['native_dust_stages'], [['meta-0', 5]])
        service = E._validate_service(common, inputs, readset)
        self.assertEqual((service['cases'], service['scenarios'], service['wire_copies']), (32, 4, 3))
        self.assertEqual(service['wire_protobuf_sha256'], E.WIRE_SHA256)

    def test_unbound_reference_or_service_is_rejected(self):
        for validate in [E._validate_oracle, E._validate_service]:
            with self.subTest(validate=validate.__name__), self.assertRaises(ValueError):
                validate(None, {}, E._EvidenceReadSet(names=[]))

    def test_bound_keys_inside_and_outside_root_and_parse_hash_same_bytes(self):
        with tempfile.TemporaryDirectory(dir=E.ROOT) as folder:
            path = Path(folder) / 'input.json'
            path.write_bytes(b'{"valid":true}')
            key = str(path.relative_to(E.ROOT))
            readset = E._EvidenceReadSet(names=[])
            raw = E._bound(None, {key: E.sha(path)}, path, readset)
            self.assertEqual(json.loads(raw), {'valid': True})
            self.assertEqual(hashlib.sha256(raw).hexdigest(), readset.hashes[path])
            path.write_bytes(b'{"valid":false}')
            with self.assertRaises(ValueError):
                readset.require_stable()
        with tempfile.TemporaryDirectory() as folder:
            path = Path(folder) / 'external'
            path.write_bytes(b'external')
            self.assertEqual(E._bound(None, {str(path): E.sha(path)}, path, E._EvidenceReadSet(names=[])), b'external')


if __name__ == '__main__':
    unittest.main()
