#!/usr/bin/env python3
"""Bounded historical Curve MetaUSD evidence; never authorizes verified/production use.

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
PREFIX = 'curve-meta'
PRODUCERS = [
    ('smoke.py', 'three-service-smoke.json'),
    ('multi_model_smoke.py', 'multi-model-three-service-smoke.json'),
    ('curve_ng_smoke.py', 'curve-ng-three-service-smoke.json'),
    ('curve_meta_smoke.py', 'curve-meta-three-service-smoke.json'),
]
SCOPES = [entry[1] for entry in PRODUCERS]
REVISION = '5272fd2b1199f185d9cd94822be9262c387dc2c2'
SOURCE_SHA256 = 'edfb3da7c748edb873465e60d471f49b6fd67be2972c97878948e9b9c2c98388'
RUNTIME_SHA256 = 'c5351fd332fdf0f04486a838b2bceba15d7a45965e3df61e9a0ccedc674d450d'
CORPUS_SHA256 = '99b2b6e7d6698f71251098cb8a5346233c85ac56a007ba5572967015451a1d47'
GENERATION_SHA256 = '8a32eecffeb491f99b28d2a30cada101c144dada6040af6e154dd66690237d97'
WIRE_SHA256 = '3de3ea43b3e7555123c13290e8c616b37da4161e4be644bec788e89c525f559e'
SERVICE_NAMES = ['meta-0', 'meta-6', 'meta-8', 'meta-18']
FLAGS = {'verified': False, 'all_requested_features_complete': False, 'production_ready': False}
# Independent minimum, frozen from the worker contract; never imported from it.
REQUIRED_PROCESS_CHECKS = (
    'four-metausd-pools-original-clone-implementation-and-historical-rpc',
    'complete-meta-balances-admin-base-vp-and-dependency-digests-in-live-protobuf',
    '32-original-exchanges-64-router-requests-62-positive-quotes-two-dust-rejections',
    'historical-banks-preserve-original-time-and-complete-source-identity',
    'five-invalid-meta-dependencies-withdraw-whole-evm-scope-and-recover-higher-generation',
    'unrelated-solana-scope-remains-ready',
    'memory-router-restart-restores-meta-bank',
    'verified-and-build-remain-disabled',
    'native-dust-explicitly-rejected',
)
REQUIRED_FAULT_NAMES = ('base-code', 'virtual-price', 'accounting', 'implementation', 'lp-decimals')
REQUIRED_TEST_FRESHNESS_MS = 315360000000


def require(value, message):
    if not value:
        raise ValueError(message)


def write_result(path, value, before_publish=None, after_publish=None):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + '.' + secrets.token_hex(6) + '.tmp')
    published = False
    try:
        temporary.write_text(json.dumps(value, indent=2) + '\n')
        if before_publish is not None:
            before_publish()
        temporary.replace(path)
        published = True
        if after_publish is not None:
            after_publish()
    except BaseException:
        if published:
            path.unlink(missing_ok=True)
        raise
    finally:
        temporary.unlink(missing_ok=True)


def _invalidate(action):
    suffix = {'bind': 'runtime-binding', 'seal': 'scope-execution', 'acceptance': 'acceptance'}[action]
    names = [PREFIX + '-acceptance.json', PREFIX + '-' + suffix + '.json']
    if action == 'bind':
        names.append(PREFIX + '-scope-execution.json')
    for name in dict.fromkeys(names):
        (OUT / name).unlink(missing_ok=True)
        write_result(OUT / name, {'passed': False, 'state': 'RUNNING', **FLAGS})
    return OUT / (PREFIX + '-' + suffix + '.json')


class _EvidenceReadSet:
    """Hash and parse retained bytes; versions detect identical replacements.

    Observations establish sequential stability, never an atomic multi-file read.
    Large binary inputs retain their digest/version, rather than their contents.
    """
    def __init__(self, names=None):
        self.raw = {}
        self.versions = {}
        self.hashes = {}
        self.paths = {}
        names = names if names is not None else ['verification.json', PREFIX + '-runtime-binding.json',
                 PREFIX + '-scope-execution.json', PREFIX + '-review.json'] + SCOPES
        for name in names:
            self.raw[name] = self.read(OUT / name)
            self.hashes[name] = self.hashes[OUT / name]
        self.require_stable()

    @staticmethod
    def _version(path):
        stat = path.stat()
        return stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns

    def read(self, path):
        import hashlib
        path = Path(path)
        before = self._version(path)
        raw = path.read_bytes()
        after = self._version(path)
        require(before == after, 'MetaUSD file changed while capturing ' + str(path))
        digest = hashlib.sha256(raw).hexdigest()
        if path in self.paths:
            require(self.versions[path] == after and self.hashes[path] == digest,
                    'MetaUSD file changed between reads ' + str(path))
        self.paths[path] = path
        self.versions[path] = after
        self.hashes[path] = digest
        return raw

    def json(self, path):
        return json.loads(self.read(path))

    def load(self, name):
        return json.loads(self.raw[name])

    def bind_inputs(self, inputs):
        for key, digest in inputs.items():
            path = Path(key) if Path(key).is_absolute() else ROOT / key
            self.read(path)
            require(self.hashes[path] == digest, 'MetaUSD input changed ' + key)
        self.require_stable()

    def require_stable(self):
        for path in list(self.paths):
            self.read(path)

    def evidence_hashes(self):
        return {name: self.hashes[name] for name in self.raw}


class _PublicationGuard:
    """Bundle and exact read-set barriers before and after result replacement."""
    def __init__(self, common, readset, runtime, inputs):
        import copy
        self.common = common
        self.readset = readset
        self.runtime = copy.deepcopy(runtime)
        self.inputs = copy.deepcopy(inputs)

    def __call__(self):
        self.readset.require_stable()
        runtime, inputs = self.common.validate_bundle(PREFIX, SCOPES)
        require(runtime == self.runtime and inputs == self.inputs,
                'MetaUSD bundle changed during validation/publication')
        require(self.common.snapshot_inputs() == self.inputs,
                'MetaUSD inputs changed during validation/publication')
        require(self.common.runtime_fingerprint() == self.runtime,
                'MetaUSD runtime changed during validation/publication')
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


def _bound(common, inputs, path, readset):
    path = Path(path)
    try:
        key = str(path.relative_to(ROOT))
    except ValueError:
        key = str(path)
    raw = readset.read(path)
    require(inputs.get(key) == readset.hashes[path], 'unbound or changed MetaUSD input ' + key)
    return raw


def _original_verifier():
    import importlib.util
    path = ROOT / 'integration/tools/ci/curve-meta-verify.py'
    spec = importlib.util.spec_from_file_location('curve_meta_original_execution_verifier', path)
    require(spec is not None and spec.loader is not None, 'original MetaUSD verifier unavailable')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    # verify() returns a dictionary; its CLI is the only stdout producer.
    result = module.verify()
    require(isinstance(result, dict), 'original MetaUSD verifier returned no evidence')
    for key, count in [('scenarios', 4), ('steps', 32), ('probes', 8), ('reverted_probes', 6)]:
        require(type(result.get(key)) is int and result[key] == count,
                'original MetaUSD verifier scope changed ' + key)
    require(result.get('corpus_sha256') == CORPUS_SHA256, 'original MetaUSD corpus changed')
    return result


def _validate_oracle(common, inputs, readset):
    import hashlib
    base = ROOT / 'integration/tools/reference/curve-meta'
    def bound(path):
        return _bound(common, inputs, path, readset)
    receipt_raw = bound(base / 'generation.json')
    require(hashlib.sha256(receipt_raw).hexdigest() == GENERATION_SHA256,
            'MetaUSD pinned complete generation receipt changed')
    receipt = json.loads(receipt_raw)
    pin = json.loads(bound(base / 'source-pin.json'))
    expected_pin = {'repo': 'curvefi/curve-factory', 'revision': REVISION,
                    'path': 'contracts/implementations/meta/MetaUSD.vy',
                    'local': 'integration/tools/reference/curve-models/old/MetaUSD.vy',
                    'compiler': '0.3.1', 'sha256': SOURCE_SHA256,
                    'runtime_sha256': RUNTIME_SHA256, 'runtime_bytes': 20060}
    require(pin == receipt.get('source_pin') == expected_pin and
            receipt.get('generation_complete') is True and receipt.get('compiler') == '0.3.1',
            'MetaUSD original source/compiler pin or receipt changed')
    require(hashlib.sha256(bound(ROOT / pin['local'])).hexdigest() == SOURCE_SHA256,
            'MetaUSD original source changed')
    required = [Path(__file__), ROOT / 'integration/tools/acceptance/curve_meta_regression.py',
                ROOT / 'integration/tools/ci/curve-meta-verify.py',
                ROOT / 'integration/tools/ci/curve-meta-golden.cjs']
    required += [path for path in base.iterdir() if path.is_file() and path.suffix != '.log']
    for path in required:
        bound(path)
    for record in receipt['files']:
        require(hashlib.sha256(bound(ROOT / record['path'])).hexdigest() == record['sha256'],
                'MetaUSD generation file changed ' + record['path'])
    for key in ['compiler_executable', 'python_executable', 'node_executable', 'node_lock', 'javascript_compiler']:
        record = receipt[key]
        require(hashlib.sha256(bound(Path(record['path']))).hexdigest() == record['sha256'],
                'MetaUSD generation executable/dependency changed ' + key)
    require(bool(receipt['node_modules']) and bool(receipt['compiler_packages']),
            'MetaUSD generation dependency closure missing')
    for record in receipt['node_modules']:
        path = Path(receipt['node_modules_root']) / record['path']
        require(hashlib.sha256(bound(path)).hexdigest() == record['sha256'],
                'MetaUSD Node closure changed ' + str(path))
    for record in receipt['compiler_packages']:
        require(hashlib.sha256(bound(Path(record['path']))).hexdigest() == record['sha256'],
                'MetaUSD compiler closure changed ' + record['path'])
    raw = bound(ROOT / 'integration/test/contracts/curve-meta-direct.json')
    require(hashlib.sha256(raw).hexdigest() == receipt.get('corpus_sha256') == CORPUS_SHA256,
            'MetaUSD original corpus digest changed')
    require(raw == readset.read(ROOT / 'dex-router/test/contracts/curve-meta-direct.json'),
            'MetaUSD original corpus copies differ')
    corpus = json.loads(raw)
    scenarios = corpus['scenarios']
    steps = [step for scenario in scenarios for step in scenario['steps']]
    require([s['name'] for s in scenarios] == SERVICE_NAMES and
            all(len(s['steps']) == 8 for s in scenarios) and len(steps) == 32 and
            len(corpus['probes']) == 8 and
            sum(p['expected']['reverted'] is True for p in corpus['probes']) == 6,
            'MetaUSD original 4/32/8/6 coverage changed')
    for key, count in [('scenarios', 4), ('steps', 32), ('probes', 8), ('reverted_probes', 6)]:
        require(type(receipt.get(key)) is int and receipt[key] == count,
                'MetaUSD generation count changed ' + key)
    dust = [(s['name'], phase) for s in scenarios for phase, step in enumerate(s['steps'])
            if step['expected'].get('reverted') is False and int(step['expected']['amount_out']) == 0]
    require(dust == [('meta-0', 5)] and all(step['expected']['reverted'] is False for step in steps),
            'MetaUSD native dust/exchange coverage changed')
    reference = corpus['reference']
    require(reference.get('source_pin') == pin and reference.get('allow_unlimited_contract_size') is False and
            reference.get('base_pool_isolated_mock') is True and reference.get('hardfork') == 'shanghai' and
            reference['eip170_enforcement']['status'] == 0,
            'MetaUSD isolated original normal deployment boundary changed')
    artifacts = json.loads(bound(base / 'artifacts.json'))
    runtime = bytes.fromhex(artifacts['original']['runtime'].removeprefix('0x'))
    require(len(runtime) == 20060 and len(runtime) <= 24576 and
            hashlib.sha256(runtime).hexdigest() == RUNTIME_SHA256,
            'MetaUSD original runtime size/hash changed')
    result = _original_verifier()
    readset.require_stable()
    return {**result, 'source_revision': REVISION, 'source_sha256': SOURCE_SHA256,
            'runtime_sha256': RUNTIME_SHA256, 'runtime_bytes': 20060,
            'normal_deployment_size_limit': 24576, 'base_pool_isolated_mock': True,
            'actual_three_pool_deployment_proven': False, 'native_dust_stages': [['meta-0', 5]],
            'generation_sha256': readset.hashes[base / 'generation.json']}


def _validate_service(common, inputs, readset):
    import hashlib
    import curve_meta_fixture as fixture
    require(fixture.NAMES == SERVICE_NAMES, 'MetaUSD service scenario inventory changed')
    base = ROOT / 'integration/test/contracts'
    def bound(path):
        return _bound(common, inputs, path, readset)
    bound(Path(fixture.__file__))
    service = json.loads(bound(base / 'curve-meta-service.json'))
    expected = [fixture.profile(name, step) for name in SERVICE_NAMES for step in range(8)]
    require(len(service['cases']) == 32 and service['cases'] == expected,
            'MetaUSD service 32-case full original projection changed')
    # Complete equality above includes post-state raw/admin accounting, VP,
    # source block/time, and both clone and implementation dependency identities.
    hashes = {}
    for name in ['curve-meta-service.json', 'curve-meta-wire.json']:
        raw = bound(base / name)
        for project in ['chain-parser-go', 'chain-parser-rust', 'dex-router']:
            path = ROOT / project / 'test/contracts' / name
            require(readset.read(path) == raw, 'MetaUSD service/wire copy differs ' + str(path))
            # Copies belong to the runtime source hash, not the integration input map.
        hashes[name] = readset.hashes[base / name]
    wire = json.loads(bound(base / 'curve-meta-wire.json'))
    raw = bytes.fromhex(wire['protobuf_hex'])
    require(bool(raw) and hashlib.sha256(raw).hexdigest() == wire.get('protobuf_sha256') == WIRE_SHA256,
            'MetaUSD independent wire byte digest changed')
    # Validate the independent wire snapshot against its own native generator.
    import importlib.util
    generator_path = ROOT / 'integration/tools/ci/curve-meta-wire-golden.py'
    bound(generator_path)
    spec = importlib.util.spec_from_file_location('curve_meta_wire_contract', generator_path)
    require(spec is not None and spec.loader is not None, 'MetaUSD wire generator unavailable')
    generator = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(generator)
    bound(base / 'multi-asset-state.json')
    snapshot_path = ROOT / 'integration/test/evm/baseline.json'
    snapshot = json.loads(bound(snapshot_path))
    snapshot['pools'] = [generator.pool()]
    snapshot['source'] = 'SYNTHETIC_META_DIRECT_WIRE_CONTRACT'
    reference = snapshot['reference']
    reference['block'].update(number_or_slot=100, hash='0x' + 'a' * 64, timestamp_ms=1767225600000)
    for key, values in [('adapter_set_hash', [snapshot['pools'][0]['adapter_version']]),
                        ('scope_hash', [snapshot['pools'][0]['pool_id']])]:
        reference[key] = hashlib.sha256(json.dumps(values, separators=(',', ':')).encode()).hexdigest()
    require(wire['snapshot'] == snapshot, 'MetaUSD independent wire snapshot changed')
    block = wire['snapshot']['reference']['block']
    state = wire['snapshot']['pools'][0]['multi_asset']['curve_meta']
    require(block['number_or_slot'] == state['block_number'] and block['hash'] == state['block_hash'] and
            block['timestamp_ms'] == state['timestamp'] * 1000,
            'MetaUSD wire source anchor changed')
    for name in ['curve-meta-runtime.json']:
        raw = bound(base / name)
        require(readset.read(ROOT / 'chain-parser-go/test/contracts' / name) == raw,
                'MetaUSD Go runtime fixture differs')
        runtime = json.loads(raw)
        require(list(runtime) == SERVICE_NAMES, 'MetaUSD runtime scenario inventory changed')
        for name in SERVICE_NAMES:
            original = fixture.original_scenario(name)
            expected_codes = {'implementation': fixture.implementation_code(), 'clone': fixture.code_bytes(name),
                              'token0': fixture.dependency_code(name, original['token0']),
                              'token1': fixture.dependency_code(name, original['token1']),
                              'base': fixture.dependency_code(name, original['base_pool'])}
            require(set(runtime[name]) == set(expected_codes) and
                    all(bytes.fromhex(runtime[name][key].removeprefix('0x')) == value
                        for key, value in expected_codes.items()), 'MetaUSD service runtime dependency differs')
    readset.require_stable()
    return {'cases': 32, 'scenarios': 4, 'wire_copies': 3, 'fixture_hashes': hashes,
            'runtime_sha256': RUNTIME_SHA256, 'wire_protobuf_sha256': WIRE_SHA256}


def _validate_process(common, loader=None):
    read = _load if loader is None else loader
    process = read('curve-meta-three-service-smoke.json')
    checks = process.get('checks')
    require(checks == list(REQUIRED_PROCESS_CHECKS),
            'MetaUSD independent process semantic checks missing')
    require(process.get('passed') is True and all(process.get(flag) is False for flag in FLAGS),
            'MetaUSD process failed or scope overstated')
    for name, count in [('pool_count', 4), ('service_steps', 32), ('quote_checks', 64),
                        ('successful_quotes', 62), ('dust_rejections', 2),
                        ('test_freshness_ms', REQUIRED_TEST_FRESHNESS_MS)]:
        require(type(process.get(name)) is int and process[name] == count,
                'MetaUSD process replay bound changed ' + name)
    require(process.get('fault_names') == list(REQUIRED_FAULT_NAMES), 'MetaUSD process fault coverage changed')
    require(process.get('historical_fixture_replay') is True,
            'MetaUSD process must identify historical replay')
    for name in SCOPES[:-1]:
        legacy = read(name)
        require(legacy.get('passed') is True and legacy.get('production_ready') is False and
                legacy.get('verified', False) is False and
                legacy.get('all_requested_features_complete', False) is False,
                'legacy process failed or scope overstated ' + name)
    return {name: process[name] for name in ['checks', 'pool_count', 'service_steps', 'quote_checks',
            'successful_quotes', 'dust_rejections', 'fault_names', 'historical_fixture_replay', 'test_freshness_ms']}


def _evaluate(common):
    import time
    readset = _EvidenceReadSet()
    runtime, inputs = common.validate_bundle(PREFIX, SCOPES)
    readset.require_stable()
    guard = _PublicationGuard(common, readset, runtime, inputs)
    readset.bind_inputs(inputs)
    oracle = _validate_oracle(common, inputs, readset)
    service = _validate_service(common, inputs, readset)
    process = _validate_process(common, readset.load)
    review = readset.load(PREFIX + '-review.json')
    require(review.get('source_tree_sha256') == runtime['source_tree_sha256'] and
            review.get('input_hashes') == inputs, 'MetaUSD review is stale or unbound')
    require(review.get('production_ready') is False and review.get('all_requested_features_complete') is False and
            review.get('verified', False) is False, 'MetaUSD review overstates scope')
    for stage in ['spec_review', 'quality_review']:
        finding = review[stage]
        count = finding.get('open_important_findings')
        require(finding.get('status') == 'CLOSED' and type(count) is int and count == 0,
                'MetaUSD open important review findings ' + stage)
    readset.require_stable()
    result = {'passed': True, **FLAGS, 'scope': 'bounded-original-metausd-fixed-base-direct-historical-replay',
            'source_tree_sha256': runtime['source_tree_sha256'], 'input_hashes': inputs,
            'verification_sha256': readset.hashes['verification.json'],
            'scope_execution_sha256': readset.hashes[PREFIX + '-scope-execution.json'],
            'review_sha256': readset.hashes[PREFIX + '-review.json'],
            'validated_evidence_sha256': readset.evidence_hashes(),
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
        require(result.get('passed') is True, 'shared Curve MetaUSD bundle action failed')
        result = {**result, **FLAGS}
        if guard is None:
            write_result(target, result)
        else:
            write_result(target, result, before_publish=guard, after_publish=guard)
        return result
    except BaseException as error:
        target.unlink(missing_ok=True)
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
        target.unlink(missing_ok=True)
        write_result(target, {'passed': False, **FLAGS, 'error': str(error), 'error_type': type(error).__name__})
        raise


if __name__ == '__main__':
    main()
