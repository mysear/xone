#!/usr/bin/env python3
"""Bounded Managed fee mathematics/state evidence. Never enables online support."""
from pathlib import Path
import hashlib
import json
import secrets
import sys
import time

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / 'integration/artifacts/local'
PREFIX = 'managed-reference'
PRODUCERS = [('smoke.py', 'three-service-smoke.json'), ('multi_model_smoke.py', 'multi-model-three-service-smoke.json'), ('curve_ng_smoke.py', 'curve-ng-three-service-smoke.json')]
SCOPES = [entry[1] for entry in PRODUCERS]
FLAGS = {'service_admission': False, 'verified': False, 'all_requested_features_complete': False, 'production_ready': False}


def require(value, message):
    if not value:
        raise ValueError(message)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def write_result(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + '.' + secrets.token_hex(6) + '.tmp')
    try:
        temporary.write_text(json.dumps(value, indent=2) + '\n')
        temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)


def load(name):
    return json.loads((OUT / name).read_bytes())


def runtime_fingerprint():
    import weighted_evidence as common
    return common.runtime_fingerprint()


def snapshot_inputs():
    import weighted_evidence as common
    return common.snapshot_inputs()


class ReadSet:
    """Exact parsed bytes with sequential identity/content stability checks."""
    @staticmethod
    def version(path):
        stat = path.stat()
        return stat.st_dev, stat.st_ino, stat.st_size, stat.st_mtime_ns, stat.st_ctime_ns

    def __init__(self, paths):
        self.rows = {}
        for path in paths:
            path = Path(path)
            before = self.version(path)
            raw = path.read_bytes()
            require(before == self.version(path), 'evidence changed during read: ' + str(path))
            self.rows[path] = (before, raw)
        self.check()

    def load(self, path):
        return json.loads(self.rows[Path(path)][1])

    def check(self):
        for path, (version, raw) in self.rows.items():
            require(self.version(path) == version and path.read_bytes() == raw and self.version(path) == version,
                    'evidence changed during validation/publication: ' + str(path))

    def hashes(self):
        return {str(path): hashlib.sha256(raw).hexdigest() for path, (_, raw) in self.rows.items()}



# These service files participate in the checked source-tree commitment,
# while integration/reference files participate in the separate input map.
SERVICE_FROZEN_SOURCES = {
    'dex-router/crates/protocol-math/src/balancer_managed_fees.rs',
    'dex-router/crates/protocol-math/src/balancer_log_exp.rs',
    'dex-router/crates/protocol-math/tests/balancer_managed_fees.rs',
    'dex-router/test/contracts/balancer-managed-fees.json',
}


def validate_frozen_files(common, frozen, inputs, runtime):
    require(common.source_hash() == runtime['source_tree_sha256'], 'frozen service source tree changed')
    for relative, digest in frozen['sha256'].items():
        require(sha(ROOT / relative) == digest, 'frozen reference bytes changed: ' + relative)
        if relative not in SERVICE_FROZEN_SOURCES:
            require(inputs.get(relative) == digest, 'frozen reference absent from pre-execution inputs: ' + relative)


def evaluate():
    import importlib.util
    import weighted_evidence as common
    ref = ROOT / 'integration/tools/reference/balancer-managed-fees'
    names = ['verification.json', PREFIX + '-runtime-binding.json', PREFIX + '-scope-execution.json', PREFIX + '-review.json'] + SCOPES
    readset = ReadSet([OUT / name for name in names] + [ref / name for name in ['generation.json', 'runtime-metadata.json', 'frozen-manifest.json', 'standard-evm-deployment.json', 'compiler-probe-0.7.1.json', 'compiler-probe-0.7.6.json']])
    runtime, inputs = common.validate_bundle(PREFIX, SCOPES)
    readset.check()
    path = ROOT / 'integration/tools/ci/balancer-managed-fees-verify.py'
    spec = importlib.util.spec_from_file_location('managed_original_verifier', path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    reference = module.verify()
    require((reference['cases'], reference['sequences'], reference['steps']) == (357, 30, 120), 'Managed original reference scope changed')
    frozen = readset.load(ref / 'frozen-manifest.json')
    require(frozen['status'] == 'FROZEN_REFERENCE_ONLY' and frozen['service_admission'] is False, 'reference scope overstated')
    validate_frozen_files(common, frozen, inputs, runtime)
    receipt = readset.load(ref / 'generation.json')
    for relative, digest in receipt['source_and_dependency_sha256'].items():
        require(inputs.get(relative) == digest, 'original dependency absent from pre-execution binding: ' + relative)
    meta = readset.load(ref / 'runtime-metadata.json')
    normal = readset.load(ref / 'standard-evm-deployment.json')
    require(meta['runtime_bytes'] == 27545 and meta['runtime_bytes'] > 24576 and meta['eip170_deployable'] is False,
            'compiler admission boundary changed')
    require(normal['deployed'] is False and normal['code_size_limit_enforced'] is True and normal['status'] == 0, 'normal deployment failure missing')
    for version in ['0.7.1', '0.7.6']:
        probe = readset.load(ref / ('compiler-probe-' + version + '.json'))
        require(len(probe['results']) == 4 and all(row['runtime_bytes'] > 24576 for row in probe['results']), 'compiler size probes changed')
    wire = ROOT / 'integration/test/contracts/managed-fees-wire.json'
    for service in ['chain-parser-go', 'chain-parser-rust', 'dex-router']:
        require((ROOT / service / 'test/contracts/managed-fees-wire.json').read_bytes() == wire.read_bytes(), 'Managed wire copies differ')
    review = readset.load(OUT / (PREFIX + '-review.json'))
    require(review['source_tree_sha256'] == runtime['source_tree_sha256'] and review['input_hashes'] == inputs, 'review stale')
    require(all(review.get(key) is False for key in FLAGS), 'review overstates implementation')
    for stage in ['spec_review', 'quality_review']:
        require(review[stage]['status'] == 'CLOSED' and type(review[stage]['open_important_findings']) is int and review[stage]['open_important_findings'] == 0, 'open review: ' + stage)

    def guard():
        readset.check()
        current_runtime, current_inputs = common.validate_bundle(PREFIX, SCOPES)
        require(current_runtime == runtime and current_inputs == inputs and common.runtime_fingerprint() == runtime and common.snapshot_inputs() == inputs,
                'reference inputs/runtime changed during publication')
        readset.check()
    guard()
    return {'passed': True, **FLAGS, 'scope': 'original-managed-swap-bpt-reference-and-unadmitted-state-contract',
            'source_tree_sha256': runtime['source_tree_sha256'], 'input_hashes': inputs,
            'reference': reference, 'wire_contract_bytes': len(bytes.fromhex(json.loads(wire.read_text())['protobuf_hex'])),
            'runtime_bytes': meta['runtime_bytes'], 'eip170_limit': 24576,
            'online_profile_status': 'MANAGED_FEE_RUNTIME_UNAVAILABLE', 'fresh_existing_scope_reports': SCOPES,
            'validated_evidence_sha256': readset.hashes(), 'measured_at_unix_ms': int(time.time() * 1000)}, guard


def run():
    target = OUT / (PREFIX + '-acceptance.json')
    write_result(target, {'passed': False, 'state': 'RUNNING', **FLAGS})
    try:
        result, guard = evaluate()
        guard()
        write_result(target, result)
        guard()
        return result
    except BaseException as error:
        write_result(target, {'passed': False, **FLAGS, 'error': str(error), 'error_type': type(error).__name__})
        raise


def main():
    # Invalidate before argument parsing, optional imports or binding attempts.
    target = OUT / (PREFIX + '-acceptance.json')
    write_result(target, {'passed': False, 'state': 'RUNNING', **FLAGS})
    try:
        import argparse
        parser = argparse.ArgumentParser(description=__doc__)
        group = parser.add_mutually_exclusive_group()
        group.add_argument('--bind-runtime', action='store_true')
        group.add_argument('--seal-scopes', action='store_true')
        args = parser.parse_args()
        import weighted_evidence as common
        result = common.bind_bundle(PREFIX) if args.bind_runtime else common.seal_bundle(PREFIX, SCOPES) if args.seal_scopes else run()
        print(json.dumps({'passed': result['passed'], **FLAGS}))
    except BaseException as error:
        write_result(target, {'passed': False, **FLAGS, 'error': str(error), 'error_type': type(error).__name__})
        raise


if __name__ == '__main__':
    main()
