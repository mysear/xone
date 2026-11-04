#!/usr/bin/env python3
"""Bind the bounded T33 complete V2 swap proof to current inputs and builds.

A passing binding retains PROVIDED_LOCAL_STATE authority. It cannot certify
canonical state, production readiness, or completion of the requested design.
"""
import hashlib
import json
from pathlib import Path
import re
import secrets
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / 'integration/artifacts/local'
TARGET = OUT / 'v2-complete-acceptance.json'
SCOPE = 'original-v2-complete-swap-shanghai-provided-state'
AUTHORITY = 'ORIGINAL_LOCAL_V2_COMPLETE_BANK_PROVIDED_STATE_REVM_NOT_CANONICAL'
BINARY_PATHS = [
    'chain-parser-go/bin/chain-parser-go', 'chain-parser-go/bin/fixture-parser-go',
    'chain-parser-rust/target/debug/chain-parser-rust',
    'chain-parser-rust/target/debug/fixture-parser-rust', 'dex-router/target/debug/dex-router',
]
FIXTURE_SHA256 = '63e8260ea53bec9cc9226f7e64e01b463af04a30d7afb773415134e591f494ee'
# A fixed logical commitment also protects callers validating an already parsed
# fixture; a forged coherent inventory must not define its own acceptance set.
DOCUMENT_SHA256 = 'f5cae6402c3e1595a9c3967f9f6c09edf03ab30901a20514872fb173fd69aa82'
PROFILE = 'uniswap-v2-complete'
ADAPTER_VERSION = 'uniswap-v2/core-1.0.1-full-swap-1'
FIXTURE_PATHS = [service + '/test/contracts/v2-complete.json'
                 for service in ['integration', 'chain-parser-go', 'dex-router']]
STATIC_CODE_PATHS = [
    'integration/tools/acceptance/v2_complete_evidence.py',
    'integration/tools/acceptance/tests/test_v2_complete_evidence.py',
    'integration/tools/acceptance/v2_complete_smoke.py',
    'integration/tools/acceptance/v2_complete_bank.py',
    'integration/tools/acceptance/evm_execution_smoke.py',
    'integration/tools/acceptance/weighted_evidence.py',
    'integration/tools/acceptance/verify.py',
    'integration/tools/acceptance/configuration.py',
    'integration/tools/acceptance/infrastructure.py',
]
CASE_NAMES = [
    'sync-0-to-1', 'sync-1-to-0', 'donate0-0-to-1', 'donate0-1-to-0',
    'donate1-0-to-1', 'donate1-1-to-0', 'donate-both-0-to-1', 'donate-both-1-to-0',
    'donation-sequence-forward-1', 'donation-sequence-forward-2',
    'donation-sequence-reverse-1', 'donation-sequence-reverse-2',
    'after-sync-0-to-1', 'after-sync-1-to-0', 'after-skim-0-to-1', 'after-skim-1-to-0',
    'uint32-before-wrap', 'uint32-after-wrap', 'cumulative0-wrap', 'cumulative1-wrap',
    'reserve-overflow-0', 'reserve-overflow-1', 'pair-k-revert-0', 'pair-k-revert-1',
]
SIMULATION_CASES = [0, 0, 1, 1, 2, 2, 2]
FAULTS = ['pool-code', 'factory-getter']
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
    'complete-profile-wire-retained-through-normal-parser-and-two-replicas',
    'actual-complete-pair-poststate-and-metered-slot12-match-original-node',
]
TEST_LIMITS = {'simulation_timeout_ms': 30000, 'max_gas': 15000000,
               'max_state_reads': 8192, 'max_instructions': 2000000,
               'max_memory_bytes': 67108864, 'concurrency': 2}
EXECUTION_FILES = [
    'go/config/config-test.yaml', 'rust/config/config-test.yaml',
    'router1/config/config-test.yaml', 'router2/config/config-test.yaml',
    'rust/fixtures/solana-localnet.json', 'router1/state.json', 'router2/state.json',
    'infrastructure-postgres-dsn',
]
RPC_READS = ['eth_chainId', 'eth_getCode', 'eth_call',
             'eth_getBlockByNumber', 'eth_getTransactionReceipt', 'eth_getStorageAt']


def require(ok, message):
    if not ok:
        raise ValueError(message)


def initial():
    return dict(passed=False, state='RUNNING', scope=SCOPE, authority=AUTHORITY,
                verified=False, production_ready=False, all_requested_features_complete=False)


def write(value):
    OUT.mkdir(parents=True, exist_ok=True)
    temporary = TARGET.with_name(TARGET.name + '.' + secrets.token_hex(6) + '.tmp')
    try:
        temporary.write_text(json.dumps(value, indent=2) + '\n')
        temporary.replace(TARGET)
    finally:
        temporary.unlink(missing_ok=True)


# Invalidate previous success even when importing an execution dependency fails.
if __name__ == '__main__':
    write(initial())
from weighted_evidence import checked_runtime, runtime_fingerprint, snapshot_inputs, sha


def is_sha(value):
    return isinstance(value, str) and re.fullmatch('[0-9a-f]{64}', value) is not None


def positive_integer(value):
    return type(value) is int and value > 0


def hash_map(value):
    return (type(value) is dict and bool(value)
            and all(isinstance(key, str) and key and is_sha(item) for key, item in value.items()))


def input_digest(inputs):
    return hashlib.sha256(json.dumps(inputs, sort_keys=True, separators=(',', ':')).encode()).hexdigest()


def validate_runtime(runtime):
    require(type(runtime) is dict and set(runtime) == {'source_tree_sha256', 'binaries'}
            and is_sha(runtime.get('source_tree_sha256')),
            'T33 current source hash missing')
    binaries = runtime.get('binaries')
    require(hash_map(binaries) and set(binaries) == set(BINARY_PATHS),
            'T33 requires the exact five checked binaries')


def corpus():
    return json.loads((ROOT / FIXTURE_PATHS[0]).read_text())


def bundle_hash(case, document=None):
    """Reconstruct the actual NativeCompleteBank image without importing its producer."""
    document = corpus() if document is None else document
    routers = [row for row in document['reference']['runtime_identities']
               if row['artifact'].endswith('/UniswapV2Router02.json')]
    require(len(routers) == 1, 'T33 original router identity missing')
    router, pool, anchor = routers[0], case['pool'], case['state_anchor']
    known = [dict(number=anchor['block_number'], hash=anchor['block_hash'])]
    if anchor['block_number']:
        known.append(dict(number=anchor['block_number'] - 1, hash=anchor['parent_hash']))
    fixture = {key: case[key] for key in ['schema_version', 'state_anchor', 'execution_environment', 'accounts']}
    fixture['block_hashes'] = known
    executor = dict(target=router['address'], code_sha256=router['runtime_code_sha256'],
                    factory=pool['v2']['factory'], factory_code_sha256=pool['v2']['swap']['factory_code_sha256'],
                    adapter_version='uniswap-v2-router02/1.1.0-beta.0')
    bundle = dict(schema_version=1, fixture=fixture, executor=executor,
                  transaction_context=case['transaction_context'])
    return hashlib.sha256(json.dumps(bundle).encode()).hexdigest()


def validate_execution_history(value, document):
    history = value.get('execution_file_history')
    require(type(history) is list and len(history) == 3, 'T33 requires three execution image stages')
    stages = ['initial-native-bank'] + ['authorized-image-reload:' + fault for fault in FAULTS]
    previous = None
    for index, (row, stage) in enumerate(zip(history, stages)):
        require(type(row) is dict and set(row) == {'stage', 'case', 'hashes'},
                'T33 execution history fields differ')
        require(row.get('stage') == stage and row.get('case') == CASE_NAMES[index],
                'T33 execution stage or native case differs')
        hashes = row.get('hashes')
        require(hash_map(hashes) and set(hashes) == set(EXECUTION_FILES),
                'T33 generated config/fixture/state/DSN hash inventory differs')
        expected = bundle_hash(document['cases'][index], document)
        for actor in ['router1', 'router2']:
            require(hashes[actor + '/state.json'] == expected,
                    'T33 state image differs from actual NativeBank bundle')
        if previous is not None:
            changed = {key for key in hashes if hashes[key] != previous[key]}
            require(changed == {'router1/state.json', 'router2/state.json'},
                    'T33 unauthorized config/fixture/DSN change or missing image transition')
        previous = hashes
    require(value.get('execution_file_hashes') == previous,
            'T33 final execution files differ from last authorized reload')
    require(value.get('execution_files_unchanged_except_authorized_reloads') is True,
            'T33 execution input stability missing')


def validate_owner_reference(reference, case):
    require(type(reference) is dict, 'T33 owner snapshot missing')
    anchor = case['state_anchor']
    expected = dict(chain_key=anchor['chain_key'], number_or_slot=anchor['block_number'],
                    hash=anchor['block_hash'], parent_hash=anchor['parent_hash'],
                    timestamp_ms=anchor['timestamp'] * 1000)
    require(reference.get('block') == expected, 'T33 final owner snapshot is not the last native H bank')
    require(reference.get('owner_service') == 'chain-parser-go'
            and type(reference.get('schema_version')) is int and reference['schema_version'] == 1,
            'T33 owner service/schema differs')
    for key in ['canonical_epoch', 'journal_seq', 'online_generation', 'writer_epoch', 'owner_assignment_version']:
        require(positive_integer(reference.get(key)), 'T33 owner generation/epoch missing: ' + key)
    require(reference['online_generation'] >= 3, 'T33 two source recoveries must advance generations')
    require(is_sha(reference.get('adapter_set_hash')) and is_sha(reference.get('scope_hash')),
            'T33 snapshot adapter/scope commitments missing')


def validate_process(value, runtime, inputs, now_ms):
    validate_runtime(runtime)
    require(hash_map(inputs), 'T33 pre-execution inputs missing')
    require(type(value) is dict and value.get('passed') is True and value.get('state') == 'PASSED',
            'T33 ordinary execution process failed')
    require(value.get('source_binding') == runtime, 'T33 process source/five binaries changed')
    require(value.get('input_hashes') == inputs and value.get('inputs_unchanged') is True,
            'T33 pre-execution source/reference inputs changed')
    start, end = value.get('started_at_unix_ms'), value.get('completed_at_unix_ms')
    require(positive_integer(now_ms) and positive_integer(start) and positive_integer(end)
            and start <= end <= now_ms and now_ms - end <= 3_600_000,
            'T33 process execution window invalid or stale')
    require(value.get('checks') == CHECKS, 'T33 exact ordered fourteen process checks required')
    for key, count in [('simulation_checks', 7), ('source_faults', 2)]:
        require(type(value.get(key)) is int and value[key] == count, 'T33 execution count differs: ' + key)
    require(value.get('authority') == AUTHORITY, 'T33 local source authority overstated')
    require(all(value.get(key) is False for key in ['verified', 'production_ready', 'all_requested_features_complete']),
            'T33 bounded execution must retain all three false completion flags')
    reads = value.get('read_counts')
    require(type(reads) is dict and all(positive_integer(reads.get(key)) for key in RPC_READS),
            'T33 actual source code/call/header/receipt reads required')
    limits = value.get('test_limits')
    require(type(limits) is dict and set(limits) == set(TEST_LIMITS)
            and all(type(limits[key]) is int and limits[key] == expected for key, expected in TEST_LIMITS.items()),
            'T33 exact execution test limits differ')
    require(value.get('profile') == PROFILE and value.get('adapter_version') == ADAPTER_VERSION
            and value.get('native_fixture_sha256') == FIXTURE_SHA256, 'T33 complete profile/fixture metadata differs')
    document = corpus()
    validate_poststate(value, document)
    validate_execution_history(value, document)
    validate_owner_reference(value.get('owner_reference'), document['cases'][2])


def validate_review(value, runtime, inputs):
    validate_runtime(runtime)
    require(hash_map(inputs) and type(value) is dict, 'T33 review inputs missing')
    require(value.get('source_tree_sha256') == runtime['source_tree_sha256'] and value.get('scope') == SCOPE,
            'T33 review source or bounded scope stale')
    require(value.get('input_sha256') == input_digest(inputs)
            and type(value.get('input_files')) is int and value['input_files'] == len(inputs),
            'T33 review must bind all current integration inputs')
    reviews = value.get('reviews')
    require(type(reviews) is dict and set(reviews) == {'spec', 'quality'},
            'T33 independent SPEC and quality reviews required')
    for key, review in reviews.items():
        require(type(review) is dict and review.get('passed') is True
                and isinstance(review.get('agent_id'), str) and bool(review['agent_id'].strip())
                and type(review.get('open_p1_p2')) is int and review['open_p1_p2'] == 0,
                'T33 unresolved review: ' + key)
    require(reviews['spec']['agent_id'].strip() != reviews['quality']['agent_id'].strip(),
            'T33 independent reviewer identities required')


def validate_poststate(value, document):
    """Compare actual observation bytes to original per-case Pool poststate."""
    require(type(value.get('complete_poststate_checks')) is int and value['complete_poststate_checks'] == 7,
            'T33 complete poststate count differs')
    evidence = value.get('complete_poststate_evidence')
    require(type(evidence) is list and len(evidence) == 7, 'T33 seven actual complete observations required')
    for record, index in zip(evidence, SIMULATION_CASES):
        case = document['cases'][index]
        require(type(record) is dict and set(record) == {'case', 'source_block_hash', 'receipt_hash', 'observations'},
                'T33 complete evidence fields differ')
        require(record['case'] == CASE_NAMES[index]
                and record['source_block_hash'] == case['state_anchor']['block_hash']
                and is_sha(record['receipt_hash']), 'T33 complete case timeline/bank/receipt differs')
        pool, after = case['pool'], case['expected']['pool_after']
        state = after['v2']['swap']
        anchor = case['state_anchor']
        require(state['bank_block_hash'] == anchor['block_hash']
                and state['bank_block_number'] == anchor['block_number']
                and state['bank_unix_timestamp'] == anchor['timestamp'] and state['unlocked'] is True,
                'T33 derived poststate source bank/lock differs')
        specs = {
            'pair_reserves_after': (pool['pool_id'], '0x0902f1ac',
                                   [int(after['reserve0']), int(after['reserve1']), state['block_timestamp_last']]),
            'pair_balance0_after': (pool['token0'], '0x70a08231' + '0' * 24 + pool['pool_id'][2:], [int(state['raw_balance0'])]),
            'pair_balance1_after': (pool['token1'], '0x70a08231' + '0' * 24 + pool['pool_id'][2:], [int(state['raw_balance1'])]),
            'pair_price0_cumulative_after': (pool['pool_id'], '0x5909c0d5', [int(state['price0_cumulative_last'])]),
            'pair_price1_cumulative_after': (pool['pool_id'], '0x5a3d5493', [int(state['price1_cumulative_last'])]),
            'pair_lock_after': (pool['pool_id'], '', [1]),
        }
        rows = record['observations']
        require(type(rows) is dict and set(rows) == set(specs), 'T33 exact six poststate phases required')
        for phase, (target, calldata, expected) in specs.items():
            row = rows[phase]
            require(type(row) is dict and {'phase', 'target', 'calldata', 'output', 'usage'} <= set(row)
                    and set(row) <= {'phase', 'target', 'calldata', 'output', 'usage', 'type', 'slot'},
                    'T33 poststate observation fields differ: ' + phase)
            require(row['phase'] == phase and row['target'] == target and row['calldata'] == calldata,
                    'T33 poststate target/getter/phase differs: ' + phase)
            output = row['output']
            require(isinstance(output, str) and re.fullmatch('0x[0-9a-f]{' + str(64 * len(expected)) + '}', output),
                    'T33 poststate exact ABI width/hex differs: ' + phase)
            actual = [int(output[2 + 64 * i:2 + 64 * (i + 1)], 16) for i in range(len(expected))]
            require(actual == expected, 'T33 poststate differs from original native Pool: ' + phase)
            usage = row['usage']
            keys = {'reads', 'read_bytes', 'instructions', 'input_bytes', 'gas_used', 'executions'}
            require(type(usage) is dict and set(usage) == keys
                    and all(type(usage[key]) is int and usage[key] >= 0 for key in keys)
                    and positive_integer(usage['reads']) and positive_integer(usage['read_bytes']),
                    'T33 actual observation read metering missing: ' + phase)
            if phase == 'pair_lock_after':
                require(row.get('type') == 'STORAGE' and row.get('slot') == '0x' + format(12, '064x')
                        and usage == dict(reads=1, read_bytes=84, instructions=0, input_bytes=0, gas_used=0, executions=0),
                        'T33 lock must be exact metered STORAGE slot12')
            else:
                require(row.get('type') is None and row.get('slot') is None
                        and all(positive_integer(usage[key]) for key in ['instructions', 'gas_used', 'executions']),
                        'T33 actual getter execution metering missing: ' + phase)


def _check_corpus(document):
    require(type(document) is dict and input_digest(document) == DOCUMENT_SHA256,
            'T33 frozen original native corpus changed or truncated')
    reference, cases = document['reference'], document['cases']
    require([case['name'] for case in cases] == CASE_NAMES
            and reference['case_inventory'] == CASE_NAMES, 'T33 exact original case inventory differs')
    successes = sum(case['expected']['receiptstatus'] == 1 for case in cases)
    transactions = reference['transactions']
    transaction_successes = sum(int(tx['receipt']['status'], 16) == 1 for tx in transactions)
    return dict(native_cases=len(cases), native_successes=successes, native_reverts=len(cases) - successes,
                post_accounts=sum(len(case['expected']['post_accounts']) for case in cases),
                post_storage_slots=sum(len(account['storage']) for case in cases for account in case['expected']['post_accounts']),
                sequences=len(document['sequences']), sequence_steps=sum(len(s['steps']) for s in document['sequences']),
                native_transactions=len(transactions), transaction_successes=transaction_successes,
                transaction_reverts=len(transactions) - transaction_successes,
                runtime_identities=len(reference['runtime_identities']),
                raw_traces=len({tx['trace']['file'] for tx in transactions}), scripts=len(reference['scripts']),
                package_files=sum(len(p['files']) for p in document['source_pins']['packages']),
                loaded_dependencies=len(reference['loaded_dependencies']))


def check_corpus(document=None):
    # This input is intentionally frozen. Native regeneration, including all
    # getters, transactions and full account/storage projections, still runs.
    if document is None:
        for relative in FIXTURE_PATHS:
            require(sha(ROOT / relative) == FIXTURE_SHA256, 'T33 frozen fixture bytes differ: ' + relative)
        document = corpus()
    try:
        return _check_corpus(document)
    except (KeyError, TypeError, IndexError, StopIteration) as error:
        raise ValueError('T33 malformed native corpus') from error


def required_inputs(document):
    """Minimum native closure, derived from the frozen actual manifest shape."""
    reference = document['reference']
    rows = reference['scripts'] + reference['loaded_dependencies']
    rows += [dict(path=reference['node']['executable'], sha256=reference['node']['sha256'])]
    rows += [dict(path='integration/tools/reference/evm-execution/' + row['path'], sha256=row['sha256'])
             for package in document['source_pins']['packages'] for row in package['files']]
    rows += [dict(path='integration/tools/reference/v2-complete/traces/' + tx['trace']['file'],
                  sha256=tx['trace']['gzip_sha256']) for tx in reference['transactions']]
    expected = {}
    for row in rows:
        name, digest = row['path'], row['sha256']
        path = Path(name)
        require(isinstance(name, str) and (path.is_absolute() or '..' not in path.parts) and is_sha(digest),
                'T33 invalid native source identity')
        require(name not in expected or expected[name] == digest, 'T33 conflicting native source hashes')
        expected[name] = digest
    for relative in STATIC_CODE_PATHS:
        expected[relative] = sha(ROOT / relative)
    for relative in FIXTURE_PATHS:
        expected[relative] = FIXTURE_SHA256
    require(reference['source_pins_sha256'] == expected['integration/tools/reference/evm-execution/source-pins.json'],
            'T33 original published package pin differs')
    return expected


def validate_source_inputs(document, inputs):
    require(hash_map(inputs), 'T33 source input commitment missing')
    for name in ['verified.json', 'file-manifest.json']:
        require('integration/tools/reference/v2-complete/' + name not in inputs,
                'T33 mutable native audit output must not be an input')
    for name, digest in required_inputs(document).items():
        path = Path(name)
        require(inputs.get(name) == digest and sha(path if path.is_absolute() else ROOT / path) == digest,
                'T33 original source/dependency input missing or changed: ' + name)


def run_source_checks():
    # Preserve the existing guard's assertions even when this helper runs -OO.
    # Its checker subprocess also runs without an inherited PYTHONOPTIMIZE.
    harness = ("import pathlib,sys; p=pathlib.Path(sys.argv[1]); "
               "exec(compile(p.read_bytes(),str(p),'exec',optimize=0),"
               "{'__name__':'__main__','__file__':str(p)})")
    commands = [
        ('native-exact-regeneration', [sys.executable, '-E', ROOT / 'integration/tools/ci/v2-complete-check.py']),
        ('native-mutation-guards', [sys.executable, '-E', ROOT / 'integration/tools/ci/v2-complete-guards.py']),
        ('optimized-native-mutation-guards', [sys.executable, '-E', '-OO', '-c', harness,
                                              ROOT / 'integration/tools/ci/v2-complete-guards.py']),
    ]
    import os
    environment = dict(os.environ)
    environment.pop('PYTHONOPTIMIZE', None)
    environment.pop('PYTHONPATH', None)
    OUT.mkdir(parents=True, exist_ok=True)
    results = []
    for name, command in commands:
        log = OUT / ('v2-complete-' + name + '-acceptance.log')
        with log.open('w') as stream:
            completed = subprocess.run(list(map(str, command)), cwd=ROOT, env=environment,
                                       stdout=stream, stderr=subprocess.STDOUT)
        require(completed.returncode == 0, 'T33 original native check failed: ' + name)
        results.append(dict(name=name, command=list(map(str, command)), exit_code=completed.returncode,
                            log_sha256=sha(log)))
    return results


def read_document(data):
    def unique(pairs):
        result = {}
        for key, value in pairs:
            require(key not in result, 'T33 duplicate JSON report key: ' + key)
            result[key] = value
        return result
    def invalid(value):
        raise ValueError('T33 nonfinite JSON report number: ' + value)
    value = json.loads(data, object_pairs_hook=unique, parse_constant=invalid)
    require(type(value) is dict, 'T33 report must be a JSON object')
    return value


def run():
    # Retain exact bytes before invoking any verifier or input collector. A
    # semantically identical replacement is still a different evidence artifact.
    paths = [OUT / name for name in ['verification.json', 'v2-complete-three-service-smoke.json',
                                     'v2-complete-review.json']]
    documents = [path.read_bytes() for path in paths]
    hashes = [hashlib.sha256(data).hexdigest() for data in documents]
    captured_verification, process, review = map(read_document, documents)
    verification, runtime = checked_runtime()
    validate_runtime(runtime)
    require(captured_verification == verification, 'T33 checked verification report replaced before binding')
    require([sha(path) for path in paths] == hashes, 'T33 verification/process/review replaced before binding')
    inputs = snapshot_inputs()
    counts = check_corpus()
    validate_source_inputs(corpus(), inputs)
    validate_process(process, runtime, inputs, int(time.time() * 1000))
    validate_review(review, runtime, inputs)
    verified_at = verification.get('measured_at_unix_ms')
    require(positive_integer(verified_at) and verified_at <= process['started_at_unix_ms'],
            'T33 process predates current full build verification')
    commands = run_source_checks()
    require(runtime_fingerprint() == runtime, 'T33 source or five binaries changed during acceptance')
    require(snapshot_inputs() == inputs, 'T33 source/reference inputs changed during acceptance')
    require([sha(path) for path in paths] == hashes, 'T33 verification/process/review replaced during acceptance')
    validate_process(process, runtime, inputs, int(time.time() * 1000))
    result = initial()
    result.update(counts)
    result.update(passed=True, state='PASSED', source_tree_sha256=runtime['source_tree_sha256'],
                  source_binding=runtime, commands=commands, input_sha256=input_digest(inputs),
                  input_files=len(inputs), source_unchanged=True, verification_sha256=hashes[0],
                  process_sha256=hashes[1], review_sha256=hashes[2], simulation_checks=7, source_faults=2,
                  process_checks=14, complete_poststate_checks=7, native_mutation_guards=49, execution_file_stages=3, execution_files=8,
                  execution_files_unchanged_except_authorized_reloads=True,
                  execution_file_hashes=process['execution_file_hashes'],
                  execution_file_history=process['execution_file_history'],
                  complete_poststate_evidence=process['complete_poststate_evidence'],
                  owner_reference=process['owner_reference'], read_counts=process['read_counts'],
                  test_limits=dict(TEST_LIMITS), measured_at_unix_ms=int(time.time() * 1000),
                  remaining=['Canonical chain state and deployment/history authority',
                             'Mint/Burn/LP supply, tax/rebase tokens and future-block execution guarantees',
                             'Complete engine coverage and all requested design functions',
                             'Production VERIFIED and public transaction build acceptance'])
    return result


def main():
    write(initial())
    try:
        result = run()
        write(result)
        print(json.dumps({'passed': True, 'scope': SCOPE, 'verified': False,
                          'production_ready': False, 'all_requested_features_complete': False}))
    except BaseException as error:
        result = initial()
        result.update(state='FAILED', error=str(error), error_type=type(error).__name__)
        write(result)
        raise


if __name__ == '__main__':
    main()
