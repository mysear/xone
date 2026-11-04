#!/usr/bin/env python3
"""Bounded historical Curve NG evidence; never authorizes verified/production use.

Optional helpers are imported only after the callable/CLI result is invalidated.
The original oracle verifier remains the authority for its frozen execution
closure. The shared weighted bundle owns build/input/execution chronology.
"""
from pathlib import Path
import json
import secrets
import sys

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / 'integration/artifacts/local'
PREFIX = 'curve-ng'
PRODUCERS = [
    ('smoke.py', 'three-service-smoke.json'),
    ('multi_model_smoke.py', 'multi-model-three-service-smoke.json'),
    ('curve_ng_smoke.py', 'curve-ng-three-service-smoke.json'),
]
SCOPES = [entry[1] for entry in PRODUCERS]
REVISION = '3332cd44656ec64b3c048885e1dd71955254b262'
SOURCE_SHA256 = 'dfe8f8a4dda288c41c630ea4be9bb0e5d61244c50dfdeaf1981a098b55ee12ab'
TEMPLATE_SHA256 = '390b83f661547f817358df2b503210646c1749400f7eb4cad3a9547f3c462810'
CORPUS_SHA256 = '3496c41e5fbf8a0c24c2c57f301b94c6231202c64134b7c65c00568591189603'
SERVICE_NAMES = ['two-sequential', 'three-mixed-decimals', 'four', 'five', 'six', 'seven', 'eight']
FLAGS = {'verified': False, 'all_requested_features_complete': False, 'production_ready': False}
# Independent acceptance contract. Do not derive this minimum from a producer
# declaration: shrinking the producer and its report together must still fail.
REQUIRED_PROCESS_CHECKS = (
    'seven-ng-pools-fixed-original-runtime-and-historical-rpc',
    'complete-ng-balances-admin-and-oracles-in-live-protobuf',
    'all-56-original-quotes-match-both-router-replicas',
    'historical-banks-preserve-original-time-and-complete-source-identity',
    'invalid-ng-dependencies-revoke-whole-evm-scope-and-recover-higher-generation',
    'unrelated-solana-scope-remains-ready',
    'memory-router-restart-restores-ng-bank',
    'verified-and-build-remain-disabled',
)
REQUIRED_FAULT_NAMES = ('accounting', 'oracle', 'immutable', 'template')
REQUIRED_TEST_FRESHNESS_MS = 315360000000


def require(value, message):
    if not value:
        raise ValueError(message)


def write_result(path, value, before_publish=None, after_publish=None):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + '.' + secrets.token_hex(6) + '.tmp')
    try:
        temporary.write_text(json.dumps(value, indent=2) + '\n')
        if before_publish is not None:
            before_publish()
        temporary.replace(path)
        if after_publish is not None:
            after_publish()
    finally:
        temporary.unlink(missing_ok=True)


def _invalidate(action):
    suffix = {'bind': 'runtime-binding', 'seal': 'scope-execution', 'acceptance': 'acceptance'}[action]
    names = {PREFIX + '-' + suffix + '.json', PREFIX + '-acceptance.json'}
    if action == 'bind':
        names.add(PREFIX + '-scope-execution.json')
    for name in names:
        write_result(OUT / name, {'passed': False, 'state': 'RUNNING', **FLAGS})
    return OUT / (PREFIX + '-' + suffix + '.json')


class _EvidenceReadSet:
    """Observed stable files, not an atomic snapshot of multiple files.

    Parse and hash the same retained bytes. File versions also detect an atomic
    replacement with identical bytes, or replacement followed by restoration.
    """
    def __init__(self):
        import hashlib
        self.raw = {}
        self.versions = {}
        self.hashes = {}
        names = ['verification.json', PREFIX + '-runtime-binding.json',
                 PREFIX + '-scope-execution.json', PREFIX + '-review.json'] + SCOPES
        for name in names:
            path = OUT / name
            before = self._version(path)
            raw = path.read_bytes()
            after = self._version(path)
            require(before == after, 'NG evidence changed while capturing ' + name)
            self.raw[name] = raw
            self.versions[name] = after
            self.hashes[name] = hashlib.sha256(raw).hexdigest()
        self.require_stable()

    @staticmethod
    def _version(path):
        stat = path.stat()
        return stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns

    def load(self, name):
        return json.loads(self.raw[name])

    def require_stable(self):
        for name, raw in self.raw.items():
            path = OUT / name
            require(self._version(path) == self.versions[name] and path.read_bytes() == raw and
                    self._version(path) == self.versions[name],
                    'NG evidence changed during validation/publication ' + name)


class _PublicationGuard:
    """Double-validation barrier around the acceptance-file publication.

    These are sequential stability observations. They do not lock external
    writers, promise a multi-file transaction, or guarantee future freshness.
    """
    def __init__(self, common, readset, runtime, inputs):
        self.common = common
        self.readset = readset
        self.runtime = json.loads(json.dumps(runtime))
        self.inputs = json.loads(json.dumps(inputs))

    def __call__(self):
        self.readset.require_stable()
        runtime, inputs = self.common.validate_bundle(PREFIX, SCOPES)
        require(runtime == self.runtime and inputs == self.inputs,
                'NG bundle changed during validation/publication')
        require(self.common.snapshot_inputs() == self.inputs,
                'NG inputs changed during validation/publication')
        require(self.common.runtime_fingerprint() == self.runtime,
                'NG runtime changed during validation/publication')
        self.readset.require_stable()


def _load(name):
    return json.loads((OUT / name).read_text())


# The reusable weighted regression runner consumes this read/hash interface.
# Authorization-changing entry points invalidate outputs before calling it.
def load(name):
    return _load(name)


def sha(path):
    import hashlib
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def runtime_fingerprint():
    import weighted_evidence as common
    return common.runtime_fingerprint()


def snapshot_inputs():
    import weighted_evidence as common
    return common.snapshot_inputs()


def _bound(common, inputs, path):
    path = Path(path)
    try:
        key = str(path.relative_to(ROOT))
    except ValueError:
        key = str(path)
    require(inputs.get(key) == common.sha(path), 'unbound or changed Curve NG input ' + key)


def _original_verifier():
    import importlib.util
    import contextlib
    import io
    path = ROOT / 'integration/tools/ci/curve-ng-state-verify.py'
    spec = importlib.util.spec_from_file_location('curve_ng_original_execution_verifier', path)
    require(spec is not None and spec.loader is not None, 'original NG verifier unavailable')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    stream = io.StringIO()
    with contextlib.redirect_stdout(stream):
        module.verify()
    result = json.loads(stream.getvalue())
    require(result.get('source_verified') is True and result.get('deployments_verified') == 21 and
            result.get('steps') == 147 and result.get('reverts') == 3 and
            result.get('runtime_template_bytes') == 21491 and
            result.get('immutable_offset') == 21491 and result.get('immutable_bytes') == 2144 and
            result.get('fixture_sha256') == CORPUS_SHA256,
            'original NG verifier scope changed')
    return result


def _validate_oracle(common, inputs):
    result = _original_verifier()
    base = ROOT / 'integration/tools/reference/curve-ng-state'
    closure = json.loads((base / 'execution-closure.json').read_text())
    import shutil
    node_path = shutil.which('node')
    require(node_path is not None, 'original NG Node executable unavailable')
    node = Path(node_path)
    require(common.sha(node) == closure.get('node_executable_sha256'),
            'original NG Node executable changed')
    _bound(common, inputs, node)
    pin = closure['source_pin']
    require(closure.get('generation_complete') is True and closure.get('status') == 'COMPLETE',
            'original NG generation incomplete')
    require(pin.get('repo') == 'curvefi/stableswap-ng' and pin.get('revision') == REVISION and
            pin.get('path') == 'contracts/main/CurveStableSwapNG.vy' and
            pin.get('sha256') == SOURCE_SHA256 and closure.get('compiler') == '0.3.10+commit.9136169',
            'original NG source/compiler pin changed')
    require(closure.get('runtime_template_sha256') == TEMPLATE_SHA256 and
            closure.get('runtime_template_bytes') == 21491 and
            closure.get('fixture_sha256') == CORPUS_SHA256,
            'original NG runtime/template/corpus changed')
    corpus = ROOT / 'integration/test/contracts/curve-ng-state.json'
    data = json.loads(corpus.read_text())
    scenarios = data['scenarios']
    steps = [row for scenario in scenarios for row in scenario['steps']]
    successful = sum(row['expected'].get('reverted') is not True for row in steps)
    require(len(scenarios) == 21 and len(steps) == 147 and successful == 144 and
            len(steps) - successful == 3, 'original NG 21/147/144/3 math coverage changed')
    require(corpus.read_bytes() == (ROOT / 'dex-router/test/contracts/curve-ng-state.json').read_bytes(),
            'original NG corpus copies differ')
    required = [ROOT / 'integration/tools/ci/curve-ng-state-verify.py',
                ROOT / 'integration/tools/ci/curve-ng-state-golden.cjs', corpus,
                ROOT / 'integration/tools/reference/curve-models/ng/CurveStableSwapNG.vy',
                Path(__file__), ROOT / 'integration/tools/acceptance/curve_ng_regression.py',
                ROOT / 'integration/tools/acceptance/curve_ng_evidence_test.py']
    required += [p for p in base.iterdir() if p.is_file() and p.suffix != '.log']
    required += [ROOT / 'integration/.runtime/stable-reference/package-lock.json']
    required += [ROOT / 'integration/.runtime/stable-reference/node_modules' / item['path']
                 for item in closure['node_modules']]
    site = ROOT / 'integration/tools/reference/curve-models/.runtime/vyper-0.3.10-py313/lib/python3.13/site-packages'
    required += [site / item['path'] for item in closure['compiler_packages']]
    required += [ROOT / 'integration/tools/reference/curve-models/.runtime/vyper-0.3.10-py313/bin/vyper']
    for path in required:
        _bound(common, inputs, path)
    return {**result, 'successful_steps': successful, 'source_revision': REVISION,
            'source_sha256': SOURCE_SHA256, 'runtime_template_sha256': TEMPLATE_SHA256,
            'execution_closure_sha256': common.sha(base / 'execution-closure.json')}


def _validate_service(common, inputs):
    import hashlib
    import curve_ng_fixture as fixture
    require(fixture.NAMES == SERVICE_NAMES, 'NG service projection scenarios changed')
    base = ROOT / 'integration/test/contracts'
    path = base / 'curve-ng-service-state.json'
    service = json.loads(path.read_text())
    expected = [fixture.profile(name, step) for name in SERVICE_NAMES for step in range(8)]
    require(len(service['cases']) == 56 and service['cases'] == expected,
            'NG service 56-case original projection changed')
    hashes = {}
    for name in ['curve-ng-service-state.json', 'curve-ng-service-wire.json']:
        original = base / name
        _bound(common, inputs, original)
        for project in ['chain-parser-go', 'chain-parser-rust', 'dex-router']:
            copy = ROOT / project / 'test/contracts' / name
            require(copy.read_bytes() == original.read_bytes(), 'NG service/wire copy differs ' + str(copy))
            # Project JSON copies are also committed by the shared source-tree fingerprint.
        hashes[name] = common.sha(original)
    wire = json.loads((base / 'curve-ng-service-wire.json').read_text())
    raw = bytes.fromhex(wire['protobuf_hex'])
    require(bool(raw) and hashlib.sha256(raw).hexdigest() == wire['protobuf_sha256'], 'NG wire byte digest changed')
    require(wire['snapshot']['pools'] == [expected[0]['pool']], 'NG wire snapshot differs from original service fixture')
    state = expected[0]['pool']['multi_asset']['curve_ng']
    block = wire['snapshot']['reference']['block']
    require(block['number_or_slot'] == state['block_number'] and block['hash'] == state['block_hash'] and
            block['timestamp_ms'] == state['timestamp'] * 1000, 'NG wire outer anchor differs')
    template_path = base / 'curve-ng-runtime-template.json'
    template = json.loads(template_path.read_text())
    compiled = bytes.fromhex(template['runtime_template_hex'])
    require(template['revision'] == REVISION and template['compiler'] == '0.3.10+commit.9136169' and
            template['runtime_template_sha256'] == TEMPLATE_SHA256 and len(compiled) == 21491 and
            hashlib.sha256(compiled).hexdigest() == TEMPLATE_SHA256, 'NG service runtime template changed')
    go_template = ROOT / 'chain-parser-go/test/contracts/curve-ng-runtime-template.json'
    require(go_template.read_bytes() == template_path.read_bytes(), 'NG Go runtime template copy changed')
    for path in [template_path, Path(fixture.__file__)]:
        _bound(common, inputs, path)
    return {'cases': 56, 'scenarios': 7, 'wire_copies': 3, 'fixture_hashes': hashes,
            'runtime_template_sha256': TEMPLATE_SHA256}


def _validate_process(common, loader=None):
    read = _load if loader is None else loader
    import curve_ng_smoke as producer
    required = list(REQUIRED_PROCESS_CHECKS)
    declared = getattr(producer, 'PROCESS_CHECKS', None)
    require(isinstance(declared, list) and declared == required,
            'NG producer must declare the independently fixed process semantics')
    process = read('curve-ng-three-service-smoke.json')
    require(process.get('checks') == required, 'NG process semantic checks missing or changed')
    require(process.get('passed') is True and process.get('production_ready') is False,
            'NG process failed or readiness overstated')
    require(process.get('verified', False) is False and
            process.get('all_requested_features_complete', False) is False,
            'NG process verification/completeness overstated')
    for name, count in [('pool_count', 7), ('service_steps', 56), ('quote_checks', 112),
                        ('test_freshness_ms', REQUIRED_TEST_FRESHNESS_MS)]:
        require(type(process.get(name)) is int and process[name] == count,
                'NG process replay bound changed ' + name)
    require(process.get('fault_names') == list(REQUIRED_FAULT_NAMES),
            'NG process fault coverage changed')
    require(process.get('historical_fixture_replay') is True,
            'NG process must explicitly identify historical fixture replay')
    # Fresh execution of these legacy producers is required by validate_bundle.
    for name in SCOPES[:-1]:
        legacy = read(name)
        require(legacy.get('passed') is True and legacy.get('production_ready') is False,
                'legacy process scope failed or readiness overstated ' + name)
    return {name: process[name] for name in ['checks', 'pool_count', 'service_steps', 'quote_checks',
                                            'fault_names', 'historical_fixture_replay', 'test_freshness_ms']}


def _evaluate(common):
    import time
    readset = _EvidenceReadSet()
    runtime, inputs = common.validate_bundle(PREFIX, SCOPES)
    readset.require_stable()
    guard = _PublicationGuard(common, readset, runtime, inputs)
    oracle = _validate_oracle(common, inputs)
    service = _validate_service(common, inputs)
    process = _validate_process(common, readset.load)
    review = readset.load(PREFIX + '-review.json')
    require(review.get('source_tree_sha256') == runtime['source_tree_sha256'] and
            review.get('input_hashes') == inputs, 'NG review is stale or unbound')
    require(review.get('production_ready') is False and review.get('all_requested_features_complete') is False and
            review.get('verified', False) is False, 'NG review overstates scope')
    for stage in ['spec_review', 'quality_review']:
        finding = review[stage]
        count = finding.get('open_important_findings')
        require(finding.get('status') == 'CLOSED' and type(count) is int and count == 0,
                'NG open important review findings ' + stage)
    readset.require_stable()
    result = {'passed': True, **FLAGS, 'scope': 'bounded-standard-type0-historical-original-ng-exchange-replay',
            'source_tree_sha256': runtime['source_tree_sha256'], 'input_hashes': inputs,
            'verification_sha256': readset.hashes['verification.json'],
            'scope_execution_sha256': readset.hashes[PREFIX + '-scope-execution.json'],
            'review_sha256': readset.hashes[PREFIX + '-review.json'],
            'validated_evidence_sha256': dict(readset.hashes),
            'validation_mode': 'stable-readset-with-pre-post-publication-revalidation',
            'fresh_scope_reports': SCOPES, 'math': oracle, 'service': service,
            'process_checks': process['checks'], 'process_replay': process, 'measured_at_unix_ms': int(time.time() * 1000)}
    return result, guard


def _action(action):
    target = _invalidate(action)
    try:
        import weighted_evidence as common
        guard = None
        if action == 'bind':
            result = common.bind_bundle(PREFIX)
        elif action == 'seal':
            result = common.seal_bundle(PREFIX, SCOPES)
        else:
            result, guard = _evaluate(common)
        require(result.get('passed') is True, 'shared Curve NG bundle action failed')
        result = {**result, **FLAGS}
        if guard is None:
            write_result(target, result)
        else:
            write_result(target, result, before_publish=guard, after_publish=guard)
        return result
    except BaseException as error:
        write_result(target, {'passed': False, **FLAGS, 'error': str(error), 'error_type': type(error).__name__})
        raise


def bind_runtime():
    return _action('bind')


def seal_scopes():
    return _action('seal')


def run():
    return _action('acceptance')


def validate():
    return run()


def main(argv=None):
    args = sys.argv[1:] if argv is None else argv
    action = 'bind' if '--bind-runtime' in args else 'seal' if '--seal-scopes' in args else 'acceptance'
    target = _invalidate(action)
    try:
        import argparse
        parser = argparse.ArgumentParser(description=__doc__)
        mode = parser.add_mutually_exclusive_group()
        mode.add_argument('--bind-runtime', action='store_true')
        mode.add_argument('--seal-scopes', action='store_true')
        parser.parse_args(args)
        result = _action(action)
        print(json.dumps({'passed': result['passed'], **FLAGS}))
        return result
    except BaseException as error:
        write_result(target, {'passed': False, **FLAGS, 'error': str(error), 'error_type': type(error).__name__})
        raise


if __name__ == '__main__':
    main()
