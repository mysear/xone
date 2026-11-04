#!/usr/bin/env python3
"""Current build/input binding for the bounded original public ramp service proof."""
import hashlib
import importlib.util
import json
import secrets
import subprocess
import sys
import time
from pathlib import Path
ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / 'integration/artifacts/local'
TARGET = OUT / 'curve-crypto-ramp-acceptance.json'
PROFILE = 'curve-crypto2-ramp/d7d04cd9-ramp-1'
SCOPE = 'd7d04cd9-standard-erc20-public-ramp-stop-online'

def require(ok, message):
    if not ok:
        raise ValueError(message)

def initial():
    return {'passed': False, 'state': 'RUNNING', 'verified': False, 'production_ready': False, 'all_requested_features_complete': False, 'scope': SCOPE}

def write(value):
    OUT.mkdir(parents=True, exist_ok=True)
    temporary = TARGET.with_name(TARGET.name + '.' + secrets.token_hex(6) + '.tmp')
    try:
        temporary.write_text(json.dumps(value, indent=2) + '\n')
        temporary.replace(TARGET)
    finally:
        temporary.unlink(missing_ok=True)

if __name__ == '__main__':
    write(initial())
from weighted_evidence import checked_runtime, runtime_fingerprint, snapshot_inputs, sha
from curve_crypto_ramp_smoke import FAULTS
CHECKS = ['normal-go-rpc-postgres-kafka-and-two-routers', '28-original-exchanges-complete-live-pricing-raw-lp-state', 'independent-live-protobuf-and-canonical-hash-pinning', 'native-positive-only-corpus-no-execution-claim'] + ['scope-withdraw-and-new-generation:' + fault for fault in FAULTS] + ['unrelated-solana-quotes-survive-crypto-withdrawal', 'router-restart-restores-complete-crypto-bank', 'verified-and-build-remain-disabled']

def validate_process(value, runtime, inputs, now_ms):
    require(value.get('passed') is True and value.get('state') == 'PASSED', 'ramp ordinary process failed')
    require(value.get('source_binding') == runtime, 'ramp process binaries/source stale')
    require(bool(inputs) and value.get('input_hashes') == inputs and value.get('inputs_unchanged') is True, 'ramp process reference inputs stale')
    start, end = value.get('started_at_unix_ms'), value.get('completed_at_unix_ms')
    require(type(start) is int and type(end) is int and 0 < start <= end <= now_ms and now_ms - end <= 3_600_000, 'ramp process execution window invalid')
    require(value.get('checks') == CHECKS, 'ramp process fault inventory incomplete')
    require(tuple(value.get(k) for k in ['quote_checks', 'successful_quotes', 'native_revert_rejections', 'phase_count', 'pool_count']) == (56, 56, 0, 14, 2), 'ramp bank/quote count differs')
    require(value.get('receipt_reads', 0) > 0 and all(value.get('read_counts', {}).get(k, 0) > 0 for k in ['ramp-origin', 'eth_getCode', 'eth_getStorageAt', 'eth_call']), 'ramp source dependencies not read')
    require(value.get('profile') == PROFILE and value.get('trust') == 'SYNTHETIC_RPC_NOT_DEPLOYMENT_PROOF', 'ramp profile or trust overstated')
    require(all(value.get(k) is False for k in ['verified', 'production_ready', 'all_requested_features_complete']), 'ramp acceptance scope overstated')

def input_digest(inputs):
    return hashlib.sha256(json.dumps(inputs, sort_keys=True, separators=(',', ':')).encode()).hexdigest()

def validate_review(value, runtime, inputs):
    require(bool(inputs) and value.get('input_sha256') == input_digest(inputs) and value.get('input_files') == len(inputs), 'ramp review integration inputs missing or stale')
    require(value.get('source_tree_sha256') == runtime['source_tree_sha256'] and value.get('scope') == SCOPE, 'ramp reviews stale or wrong scope')
    reviews = value.get('reviews', {})
    require(set(reviews) == {'spec', 'quality'}, 'ramp SPEC and quality reviews required')
    for key, review in reviews.items():
        require(review.get('passed') is True and review.get('agent_id') and review.get('open_p1_p2') == 0, 'ramp unresolved review: ' + key)
    require(reviews['spec']['agent_id'] != reviews['quality']['agent_id'], 'ramp independent reviewers required')

def load_module(name, path):
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

def check_projection():
    native_path = ROOT / 'integration/test/contracts/curve-crypto-ramp-native.json'
    native = json.loads(native_path.read_text())
    service = json.loads((ROOT / 'integration/test/contracts/curve-crypto-ramp-service.json').read_text())
    projection = load_module('ramp_projection', ROOT / 'integration/tools/ci/curve-crypto-ramp-service-golden.py')
    verifier = load_module('ramp_verifier', ROOT / 'integration/tools/ci/curve-crypto-ramp-verify.py')
    template = json.loads((ROOT / 'integration/test/contracts/curve-crypto-runtime-template.json').read_text())
    reference = ROOT / 'integration/tools/reference/curve-crypto-ramp'
    verifier.validate(native, json.loads((reference / 'runtime-metadata.json').read_text()), json.loads((reference / 'generation.json').read_text()))
    require(service['source_corpus_sha256'] == sha(native_path) and service['profile'] == PROFILE, 'ramp projection source identity differs')
    require(len(native['cases']) == len(service['cases']) == 28, 'ramp source inventory changed')
    for source, row in zip(native['cases'], service['cases']):
        require(row['name'] == source['name'] and row['sequence'] == source['sequence'] and row['step'] == source['step'], 'ramp projected member changed')
        require(row['pool'] == projection.project(source, template), 'ramp complete projected state changed')
        require(row['expected'] == source['expected'] and row['input'] == source['input'] and row['index_in'] == source['index_in'] and row['index_out'] == source['index_out'], 'ramp original trade projection changed')

def run():
    verification, runtime = checked_runtime()
    inputs = snapshot_inputs()
    check_projection()
    result = initial()
    result.update(source_tree_sha256=runtime['source_tree_sha256'], source_binding=runtime, commands=[])
    commands = [('original-reference', [sys.executable, ROOT / 'integration/tools/ci/curve-crypto-ramp-verify.py']), ('optimized-reference', [sys.executable, '-OO', ROOT / 'integration/tools/ci/curve-crypto-ramp-verify.py'])]
    for mode, flags in [('normal', []), ('optimized', ['-OO'])]:
        for pattern in ['test_rayclmm_dynamic_evidence.py', 'test_rayclmm_dynamic_resume.py', 'test_rayclmm_seed_evidence.py']:
            commands.append((mode + '-legacy-guards-' + pattern, [sys.executable, *flags, '-m', 'unittest', 'discover', '-s', ROOT / 'integration/tools/acceptance/tests', '-p', pattern]))
    for name, command in commands:
        completed = subprocess.run(list(map(str, command)), capture_output=True, text=True)
        result['commands'].append({'name': name, 'exit_code': completed.returncode, 'output': completed.stdout + completed.stderr})
        require(completed.returncode == 0, 'ramp reference check failed: ' + name)
    process_path = OUT / 'curve-crypto-ramp-three-service-smoke.json'
    review_path = OUT / 'curve-crypto-ramp-review.json'
    validate_process(json.loads(process_path.read_text()), runtime, inputs, int(time.time() * 1000))
    validate_review(json.loads(review_path.read_text()), runtime, inputs)
    require(inputs == snapshot_inputs() and runtime == runtime_fingerprint(), 'ramp acceptance inputs changed')
    result.update(passed=True, state='PASSED', native_cases=28, sequences=6, carried_transitions=22, public_admin_calls=8, service_requests=56, source_faults=len(FAULTS), legacy_verifier_optimization_guards=True, source_unchanged=True, input_files=len(inputs), input_sha256=input_digest(inputs), verification_sha256=sha(OUT / 'verification.json'), process_sha256=sha(process_path), review_sha256=sha(review_path), measured_at_unix_ms=int(time.time() * 1000), remaining=['Other Crypto/Curve models and special tokens', 'Solana advanced combinations and token extensions', 'V4/Infinity hooks and full settlement', 'Real deployment/history, independent transaction simulation and production acceptance'])
    return result

def main():
    write(initial())
    try:
        result = run()
        write(result)
        print(json.dumps({'passed': True, 'scope': SCOPE, 'report': str(TARGET)}))
    except BaseException as error:
        result = initial()
        result.update(state='FAILED', error=str(error), error_type=type(error).__name__)
        write(result)
        raise

if __name__ == '__main__':
    main()
