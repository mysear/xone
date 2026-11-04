#!/usr/bin/env python3
"""Bind the bounded T32 original local bank proof to current inputs and builds.

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
TARGET = OUT / 'evm-execution-acceptance.json'
SCOPE = 'provided-local-state-original-v2-shanghai-independent-execution'
AUTHORITY = 'ORIGINAL_LOCAL_BANK_PROVIDED_STATE_REVM_NOT_CANONICAL'
BINARY_PATHS = [
    'chain-parser-go/bin/chain-parser-go', 'chain-parser-go/bin/fixture-parser-go',
    'chain-parser-rust/target/debug/chain-parser-rust',
    'chain-parser-rust/target/debug/fixture-parser-rust', 'dex-router/target/debug/dex-router',
]
FIXTURE_PATHS = ['integration/test/contracts/evm-execution.json',
                 'dex-router/test/contracts/evm-execution.json']
STATIC_CODE_PATHS = [
    'integration/tools/acceptance/evm_execution_evidence.py',
    'integration/tools/acceptance/tests/test_evm_execution_evidence.py',
    'integration/tools/acceptance/evm_execution_smoke.py',
    'integration/tools/acceptance/weighted_evidence.py', 'integration/tools/acceptance/verify.py',
    'integration/tools/acceptance/configuration.py', 'integration/tools/acceptance/infrastructure.py',
    'integration/tools/ci/evm-execution-check.py', 'integration/tools/ci/evm-execution-guards.py',
    'integration/tools/ci/evm-execution-golden.cjs',
    'integration/tools/reference/evm-execution/generate.cjs',
    'integration/tools/reference/evm-execution/validate.cjs',
    'integration/tools/reference/evm-execution/source-pins.json',
]
CASE_NAMES = [
    'token0-to-token1-distinct-recipient', 'token1-to-token0-distinct-recipient',
    'token0-to-token1-same-recipient', 'token1-to-token0-same-recipient',
    'deadline-revert', 'min-out-revert', 'allowance-revert', 'balance-revert',
    'fee-token-output-counterexample', 'input-donation-before-swap',
]
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
             'eth_getBlockByNumber', 'eth_getTransactionReceipt']


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
    require(type(runtime) is dict and is_sha(runtime.get('source_tree_sha256')),
            'T32 current source hash missing')
    binaries = runtime.get('binaries')
    require(hash_map(binaries) and set(binaries) == set(BINARY_PATHS),
            'T32 requires the exact five checked binaries')


def corpus():
    return json.loads((ROOT / FIXTURE_PATHS[0]).read_text())


def bundle_hash(case):
    # These bytes deliberately match NativeBank.bundle() and its json.dumps write.
    bundle = dict(schema_version=1,
                  fixture={key: case[key] for key in ['schema_version', 'state_anchor',
                           'execution_environment', 'accounts', 'block_hashes']},
                  executor=case['executor'], transaction_context=case['transaction_context'])
    return hashlib.sha256(json.dumps(bundle).encode()).hexdigest()


def validate_execution_history(value, document):
    history = value.get('execution_file_history')
    require(type(history) is list and len(history) == 3, 'T32 requires three execution image stages')
    stages = ['initial-native-bank'] + ['authorized-image-reload:' + fault for fault in FAULTS]
    previous = None
    for index, (row, stage) in enumerate(zip(history, stages)):
        require(type(row) is dict and set(row) == {'stage', 'case', 'hashes'},
                'T32 execution history fields differ')
        require(row.get('stage') == stage and row.get('case') == CASE_NAMES[index],
                'T32 execution stage or native case differs')
        hashes = row.get('hashes')
        require(hash_map(hashes) and set(hashes) == set(EXECUTION_FILES),
                'T32 generated config/fixture/state/DSN hash inventory differs')
        expected = bundle_hash(document['cases'][index])
        for actor in ['router1', 'router2']:
            require(hashes[actor + '/state.json'] == expected,
                    'T32 state image differs from actual NativeBank bundle')
        if previous is not None:
            changed = {key for key in hashes if hashes[key] != previous[key]}
            require(changed == {'router1/state.json', 'router2/state.json'},
                    'T32 unauthorized config/fixture/DSN change or missing image transition')
        previous = hashes
    require(value.get('execution_file_hashes') == previous,
            'T32 final execution files differ from last authorized reload')
    require(value.get('execution_files_unchanged_except_authorized_reloads') is True,
            'T32 execution input stability missing')


def validate_owner_reference(reference, case):
    require(type(reference) is dict, 'T32 owner snapshot missing')
    anchor = case['state_anchor']
    expected = dict(chain_key=anchor['chain_key'], number_or_slot=anchor['block_number'],
                    hash=anchor['block_hash'], parent_hash=anchor['parent_hash'],
                    timestamp_ms=anchor['timestamp'] * 1000)
    require(reference.get('block') == expected, 'T32 final owner snapshot is not the last native H bank')
    require(reference.get('owner_service') == 'chain-parser-go'
            and type(reference.get('schema_version')) is int and reference['schema_version'] == 1,
            'T32 owner service/schema differs')
    for key in ['canonical_epoch', 'journal_seq', 'online_generation', 'writer_epoch', 'owner_assignment_version']:
        require(positive_integer(reference.get(key)), 'T32 owner generation/epoch missing: ' + key)
    require(reference['online_generation'] >= 3, 'T32 two source recoveries must advance generations')
    require(is_sha(reference.get('adapter_set_hash')) and is_sha(reference.get('scope_hash')),
            'T32 snapshot adapter/scope commitments missing')


def validate_process(value, runtime, inputs, now_ms):
    validate_runtime(runtime)
    require(hash_map(inputs), 'T32 pre-execution inputs missing')
    require(type(value) is dict and value.get('passed') is True and value.get('state') == 'PASSED',
            'T32 ordinary execution process failed')
    require(value.get('source_binding') == runtime, 'T32 process source/five binaries changed')
    require(value.get('input_hashes') == inputs and value.get('inputs_unchanged') is True,
            'T32 pre-execution source/reference inputs changed')
    start, end = value.get('started_at_unix_ms'), value.get('completed_at_unix_ms')
    require(positive_integer(now_ms) and positive_integer(start) and positive_integer(end)
            and start <= end <= now_ms and now_ms - end <= 3_600_000,
            'T32 process execution window invalid or stale')
    require(value.get('checks') == CHECKS, 'T32 exact ordered twelve process checks required')
    for key, count in [('simulation_checks', 7), ('source_faults', 2)]:
        require(type(value.get(key)) is int and value[key] == count, 'T32 execution count differs: ' + key)
    require(value.get('authority') == AUTHORITY, 'T32 local source authority overstated')
    require(all(value.get(key) is False for key in ['verified', 'production_ready', 'all_requested_features_complete']),
            'T32 bounded execution must retain all three false completion flags')
    reads = value.get('read_counts')
    require(type(reads) is dict and all(positive_integer(reads.get(key)) for key in RPC_READS),
            'T32 actual source code/call/header/receipt reads required')
    limits = value.get('test_limits')
    require(type(limits) is dict and set(limits) == set(TEST_LIMITS)
            and all(type(limits[key]) is int and limits[key] == expected for key, expected in TEST_LIMITS.items()),
            'T32 exact execution test limits differ')
    document = corpus()
    validate_execution_history(value, document)
    validate_owner_reference(value.get('owner_reference'), document['cases'][2])


def validate_review(value, runtime, inputs):
    validate_runtime(runtime)
    require(hash_map(inputs) and type(value) is dict, 'T32 review inputs missing')
    require(value.get('source_tree_sha256') == runtime['source_tree_sha256'] and value.get('scope') == SCOPE,
            'T32 review source or bounded scope stale')
    require(value.get('input_sha256') == input_digest(inputs)
            and type(value.get('input_files')) is int and value['input_files'] == len(inputs),
            'T32 review must bind all current integration inputs')
    reviews = value.get('reviews')
    require(type(reviews) is dict and set(reviews) == {'spec', 'quality'},
            'T32 independent SPEC and quality reviews required')
    for key, review in reviews.items():
        require(type(review) is dict and review.get('passed') is True
                and isinstance(review.get('agent_id'), str) and bool(review['agent_id'].strip())
                and type(review.get('open_p1_p2')) is int and review['open_p1_p2'] == 0,
                'T32 unresolved review: ' + key)
    require(reviews['spec']['agent_id'].strip() != reviews['quality']['agent_id'].strip(),
            'T32 independent reviewer identities required')


def _check_corpus(document):
    require(document['schema_version'] == 1 and document['authority'] == 'original-local-node-artifact-replay-not-live',
            'T32 original local node authority differs')
    cases, reference = document['cases'], document['reference']
    require([case['name'] for case in cases] == CASE_NAMES
            and reference['case_inventory'] == CASE_NAMES, 'T32 exact native case inventory differs')
    require(all(type(reference[key]) is int and reference[key] == count for key, count in
                [('case_count', 10), ('success_count', 6), ('revert_count', 4)]),
            'T32 native case metadata differs')
    require(reference['ethers_version'] == 'ethers/5.8.0' and reference['ganache_version'] == '7.9.2'
            and reference['chain_options']['chain']['hardfork'] == 'shanghai'
            and reference['chain_options']['chain']['allowUnlimitedContractSize'] is False,
            'T32 native node environment differs')
    packages = document['source_pins']['packages']
    require(document['source_pins']['authority'] == 'published-official-contract-package-not-live-deployment'
            and sorted((p['name'], p['version']) for p in packages) ==
            [('@uniswap/lib', '1.1.1'), ('@uniswap/v2-core', '1.0.1'), ('@uniswap/v2-periphery', '1.1.0-beta.0')],
            'T32 official package identity differs')
    package_paths = [row['path'] for package in packages for row in package['files']]
    require(len(package_paths) == len(set(package_paths)) == 93
            and sum(path.endswith('/LICENSE') for path in package_paths) == 3,
            'T32 full official source/license inventory differs')
    dependencies = reference['loaded_dependencies']
    require(len(dependencies) == len({row['path'] for row in dependencies}) == 284
            and len(reference['runtime_identities']) == 8 and len(reference['scripts']) == 11,
            'T32 native source/runtime closure truncated')
    require(reference['node']['version'] == 'v25.3.0' and is_sha(reference['node']['sha256']),
            'T32 original node executable identity missing')
    post_accounts = post_slots = successes = 0
    for index, case in enumerate(cases):
        anchor, env, evidence, expected = (case[key] for key in
                                          ['state_anchor', 'execution_environment', 'evidence', 'expected'])
        before_header, after_header = evidence['preblock_header'], evidence['execution_block_header']
        require(anchor['chain_key'] == 'evm:31337' and env['chain_id'] == 31337 and env['fork'] == 'shanghai'
                and positive_integer(anchor['block_number']) and env['block_number'] == anchor['block_number'] + 1,
                'T32 native H/H+1 environment differs')
        require(before_header['number'] == hex(anchor['block_number'])
                and before_header['hash'] == anchor['block_hash']
                and before_header['parentHash'] == anchor['parent_hash']
                and int(before_header['timestamp'], 16) == anchor['timestamp']
                and anchor['genesis_hash'] == reference['genesis_header']['hash'], 'T32 actual H header differs')
        require(after_header['number'] == hex(env['block_number'])
                and after_header['hash'] == env['block_hash']
                and after_header['parentHash'] == env['parent_hash'] == anchor['block_hash']
                and int(after_header['timestamp'], 16) == env['timestamp']
                and after_header['miner'] == env['coinbase']
                and int(after_header['gasLimit'], 16) == env['gas_limit']
                and int(after_header['baseFeePerGas'], 16) == int(env['base_fee'])
                and after_header['mixHash'] == env['prevrandao'], 'T32 actual Node H+1 execution header differs')
        require(all(isinstance(h.get('stateRoot'), str) and re.fullmatch('0x[0-9a-f]{64}', h['stateRoot'])
                    for h in [before_header, after_header]), 'T32 actual Node state roots missing')
        require(evidence['state_key'] == 'evm:31337/' + anchor['block_hash']
                and evidence['execution_scope'] == 'observed-local-next-block-environment-not-live-canonical',
                'T32 native execution scope differs')
        require(type(expected['receiptstatus']) is int and expected['receiptstatus'] == (0 if 4 <= index <= 7 else 1),
                'T32 native success/revert member differs')
        receipt = evidence['receipt']
        require(receipt['blockHash'] == env['block_hash'] and receipt['blockNumber'] == hex(env['block_number'])
                and int(receipt['status'], 16) == expected['receiptstatus']
                and int(receipt['gasUsed'], 16) == int(expected['gas_used'])
                and receipt['transactionHash'] == expected['tx_hash'], 'T32 native execution receipt differs')
        before, after = case['accounts'], expected['post_accounts']
        require(len(before) == len(after) == 11 and len({a['address'] for a in before}) == 11
                and [a['address'] for a in before] == [a['address'] for a in after],
                'T32 all declared post accounts required')
        slots = 0
        for pre, post in zip(before, after):
            require(set(post) == {'address', 'exists', 'balance', 'nonce', 'code', 'storage'}
                    and post['exists'] is True and type(post['nonce']) is int and post['nonce'] >= 0
                    and isinstance(post['balance'], str) and re.fullmatch('(0|[1-9][0-9]*)', post['balance'])
                    and post['code'] == pre['code'], 'T32 complete post account fields required')
            require([row['slot'] for row in pre['storage']] == [row['slot'] for row in post['storage']]
                    and len({row['slot'] for row in post['storage']}) == len(post['storage']),
                    'T32 every declared post storage slot required')
            require(all(set(row) == {'slot', 'value'} and all(isinstance(row[key], str)
                        and re.fullmatch('0x[0-9a-f]{64}', row[key]) for key in ['slot', 'value'])
                        for row in post['storage']), 'T32 explicit post storage words required')
            slots += len(post['storage'])
        require(slots == [50, 50, 47, 47, 50, 50, 50, 50, 50, 50][index], 'T32 post storage inventory truncated')
        pool = next(a for a in after if a['address'] == case['pool']['pool_id'])
        packed = int(next(row['value'] for row in pool['storage'] if int(row['slot'], 16) == 8), 16)
        require(expected['pool_after'] == {'reserve0': str(packed & ((1 << 112) - 1)),
                'reserve1': str((packed >> 112) & ((1 << 112) - 1)), 'block_timestamp_last': packed >> 224},
                'T32 actual post pool getters differ from declared slots')
        post_accounts += len(after); post_slots += slots; successes += expected['receiptstatus']
    fee = cases[8]
    require(int(fee['expected']['amount_out']) < int(fee['quote_context']['quoted_buy_amount']) * 9950 // 10000
            <= int(fee['expected']['router_amount_out']), 'T32 successful fee-token recipient-minimum boundary missing')
    donation = cases[9]['evidence']['input_donation']
    require(donation['amount'] == '50000000000000000000'
            and int(donation['pair_input_balance_at_anchor']) == int(cases[9]['pool']['reserve0']) + int(donation['amount'])
            and donation['reserves_after_swap'] == cases[9]['expected']['pool_after'],
            'T32 real unsynced input donation boundary missing')
    require(post_accounts == 110 and post_slots == 494 and successes == 6, 'T32 complete native inventory differs')
    return dict(native_cases=10, native_successes=6, native_reverts=4, post_accounts=post_accounts,
                post_storage_slots=post_slots, package_files=93, loaded_dependencies=284)


def check_corpus(document=None):
    """Check declared boundaries only; exact regeneration remains the native checker."""
    if document is None:
        require((ROOT / FIXTURE_PATHS[0]).read_bytes() == (ROOT / FIXTURE_PATHS[1]).read_bytes(),
                'T32 source fixture copies differ')
        document = corpus()
    try:
        return _check_corpus(document)
    except (KeyError, TypeError, IndexError, StopIteration) as error:
        raise ValueError('T32 malformed native corpus') from error


def validate_source_inputs(document, inputs):
    require(hash_map(inputs), 'T32 source input commitment missing')
    reference = document['reference']
    rows = reference['scripts'] + reference['loaded_dependencies']
    rows += [{'path': reference['node']['executable'], 'sha256': reference['node']['sha256']}]
    rows += [{'path': 'integration/tools/reference/evm-execution/' + row['path'], 'sha256': row['sha256']}
             for package in document['source_pins']['packages'] for row in package['files']]
    for row in rows:
        path = Path(row['path'])
        require(path.is_absolute() or '..' not in path.parts, 'T32 invalid pinned source path')
        require(inputs.get(row['path']) == row['sha256'] and is_sha(row['sha256'])
                and sha(path if path.is_absolute() else ROOT / path) == row['sha256'],
                'T32 original source/dependency input missing or changed')
    for relative in STATIC_CODE_PATHS + FIXTURE_PATHS:
        require(inputs.get(relative) == sha(ROOT / relative), 'T32 execution producer/fixture input changed')
    require(reference['source_pins_sha256'] == sha(ROOT / 'integration/tools/reference/evm-execution/source-pins.json'),
            'T32 official package pin manifest changed')


def run_source_checks():
    # Preserve the existing guard's assertions even when this helper runs -OO.
    # Its checker subprocess also runs without an inherited PYTHONOPTIMIZE.
    harness = ("import pathlib,sys; p=pathlib.Path(sys.argv[1]); "
               "exec(compile(p.read_bytes(),str(p),'exec',optimize=0),"
               "{'__name__':'__main__','__file__':str(p)})")
    commands = [
        ('native-exact-regeneration', [sys.executable, '-E', ROOT / 'integration/tools/ci/evm-execution-check.py']),
        ('native-mutation-guards', [sys.executable, '-E', ROOT / 'integration/tools/ci/evm-execution-guards.py']),
        ('optimized-native-mutation-guards', [sys.executable, '-E', '-OO', '-c', harness,
                                              ROOT / 'integration/tools/ci/evm-execution-guards.py']),
    ]
    import os
    environment = dict(os.environ)
    environment.pop('PYTHONOPTIMIZE', None)
    environment.pop('PYTHONPATH', None)
    OUT.mkdir(parents=True, exist_ok=True)
    results = []
    for name, command in commands:
        log = OUT / ('evm-execution-' + name + '-acceptance.log')
        with log.open('w') as stream:
            completed = subprocess.run(list(map(str, command)), cwd=ROOT, env=environment,
                                       stdout=stream, stderr=subprocess.STDOUT)
        require(completed.returncode == 0, 'T32 original native check failed: ' + name)
        results.append(dict(name=name, command=list(map(str, command)), exit_code=completed.returncode,
                            log_sha256=sha(log)))
    return results


def run():
    verification, runtime = checked_runtime()
    validate_runtime(runtime)
    inputs = snapshot_inputs()
    counts = check_corpus()
    validate_source_inputs(corpus(), inputs)
    paths = [OUT / name for name in ['verification.json', 'evm-execution-three-service-smoke.json',
                                     'evm-execution-review.json']]
    documents = [path.read_bytes() for path in paths]
    hashes = [hashlib.sha256(data).hexdigest() for data in documents]
    require(json.loads(documents[0]) == verification, 'T32 checked verification report replaced before binding')
    process = json.loads(documents[1])
    review = json.loads(documents[2])
    validate_process(process, runtime, inputs, int(time.time() * 1000))
    validate_review(review, runtime, inputs)
    verified_at = verification.get('measured_at_unix_ms')
    require(positive_integer(verified_at) and verified_at <= process['started_at_unix_ms'],
            'T32 process predates current full build verification')
    commands = run_source_checks()
    require(runtime_fingerprint() == runtime, 'T32 source or five binaries changed during acceptance')
    require(snapshot_inputs() == inputs, 'T32 source/reference inputs changed during acceptance')
    require([sha(path) for path in paths] == hashes, 'T32 verification/process/review replaced during acceptance')
    validate_process(process, runtime, inputs, int(time.time() * 1000))
    result = initial()
    result.update(counts)
    result.update(passed=True, state='PASSED', source_tree_sha256=runtime['source_tree_sha256'],
                  source_binding=runtime, commands=commands, input_sha256=input_digest(inputs),
                  input_files=len(inputs), source_unchanged=True, verification_sha256=hashes[0],
                  process_sha256=hashes[1], review_sha256=hashes[2], simulation_checks=7, source_faults=2,
                  process_checks=12, execution_file_stages=3, execution_files=8,
                  execution_files_unchanged_except_authorized_reloads=True,
                  execution_file_hashes=process['execution_file_hashes'],
                  execution_file_history=process['execution_file_history'],
                  owner_reference=process['owner_reference'], read_counts=process['read_counts'],
                  test_limits=dict(TEST_LIMITS), measured_at_unix_ms=int(time.time() * 1000),
                  remaining=['Canonical chain state and deployment/history authority',
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
