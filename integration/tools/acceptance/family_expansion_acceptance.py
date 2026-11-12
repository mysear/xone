#!/usr/bin/env python3
"""Bind finite OnlyY/Pump validation and normal-process receipts to current inputs."""
from pathlib import Path
from collections import Counter
import base64
import hashlib
import json
import secrets
import subprocess
import sys
from family_reference import load_module, verify_only_y, verify_pump
from weighted_evidence import ROOT, OUT, sha, source_hash, write_result

VALIDATION = OUT / 'core-family-expansion-validation-20261111.json'
TARGET = OUT / 'core-family-expansion-acceptance-20261111.json'
EXPECTED_CHECKS = {
    'router-format', 'parser-format', 'protocol', 'contracts', 'reference',
    'reference-tests', 'reference-tests-optimized', 'progress', 'progress-optimized',
    'parser-tests', 'math-new-and-existing', 'api-new-and-existing',
    'combination-routing', 'parser-all-targets', 'router-all-targets',
    'parser-lint', 'router-lint', 'new-tests-lint', 'go-domain',
    'go-fixture-build', 'parser-build', 'router-build', 'pump-normal-process',
}


def require(value, message):
    if not value:
        raise ValueError(message)


def artifact(item):
    path = (ROOT / item['path']).resolve()
    require(path.is_relative_to(ROOT) and path.is_file(), 'artifact-path')
    require(sha(path) == item['sha256'], 'artifact-changed:' + item['path'])
    return path


def freeze(path, source):
    frozen = OUT / ('family-frozen-' + path.stem[:40] + '-' + source[:12] + '-' + sha(path) + '.json')
    if not frozen.exists():
        frozen.write_bytes(path.read_bytes())
    return dict(path=str(frozen.relative_to(ROOT)), sha256=sha(frozen))



def pubkey_bytes(value):
    alphabet = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'
    number = 0
    for char in value:
        number = number * 58 + alphabet.index(char)
    decoded = b'\0' * (len(value) - len(value.lstrip('1'))) + number.to_bytes((number.bit_length() + 7) // 8, 'big')
    require(len(decoded) == 32, 'public-key-width')
    return decoded


def token_delta(native, address):
    before = base64.b64decode(native['before'][address]['data_base64'], validate=True)
    after = base64.b64decode(native['after'][address]['data_base64'], validate=True)
    require(len(before) == len(after) == 165, 'classic-token-account-width')
    return int.from_bytes(after[64:72], 'little') - int.from_bytes(before[64:72], 'little')


def user_token(native, mint, owner):
    matches = []
    for address, account in native['before'].items():
        raw = base64.b64decode(account['data_base64'], validate=True)
        if len(raw) == 165 and raw[:32] == pubkey_bytes(mint) and raw[32:64] == pubkey_bytes(owner):
            matches.append(address)
    require(len(matches) == 1, 'user-token-account-identity')
    return matches[0]


def process_receipt(path, source, family, count, faults):
    receipt = json.loads(path.read_bytes())
    require(receipt.get('passed') is True, family + ':process-failed')
    require(source == receipt['source_tree_sha256'] == receipt['source_tree_sha256_after'], family + ':process-source')
    require(receipt['api_records'] == receipt['original_sbf_swaps'] == len(receipt['original_replay']) == count, family + ':coverage')
    require(receipt['negative_controls'] == faults, family + ':fault-coverage')
    require(receipt['atomic_account_batches'] > 0 and receipt['block_reads'] > 0, family + ':rpc-coverage')
    tool = ROOT / ('integration/tools/acceptance/' + ('pump_bonding_smoke.py' if family == 'pump' else 'meteora_only_y_smoke.py'))
    require(sha(tool) == receipt['tool_sha256'], family + ':tool-changed')
    for relative, expected in (receipt['binaries'] | receipt['helper_sha256']).items():
        require(sha(ROOT / relative) == expected, family + ':dependency-changed:' + relative)
    reference = ROOT / ('integration/test/contracts/' + ('pump-bonding-decoder.json' if family == 'pump' else 'meteora-only-y.json'))
    original = OUT / ('core-pump-bonding-original-20261111.json' if family == 'pump' else 'core-meteora-only-y-original-20261111.json')
    require(sha(reference) == receipt['reference_sha256'] and sha(original) == receipt['original_reference_sha256'], family + ':reference-changed')
    pinned_records = json.loads(original.read_bytes())['records']
    projection = load_module('family_acceptance_projection_' + family.replace('-', '_'), 'integration/tools/ci/' + ('pump-bonding-reference.py' if family == 'pump' else 'meteora-only-y-reference.py'))
    captures = {key: json.loads(artifact(receipt[key]).read_bytes()) for key in ['captured_plans', 'model_projection', 'original_capture']}
    require(all(len(c['records']) == count for c in captures.values()), family + ':capture-coverage')
    public, model, native = (captures[key]['records'] for key in ['captured_plans', 'model_projection', 'original_capture'])
    expected_cases = Counter({26: 2, 46: 3}) if family == 'pump' else Counter({index: 3 if index == 14 else 2 for index in [14, 34, 54, 74, 94, 114, 134, 154, 174, 194, 202, 218, 228, 230, 232, 234]})
    require(Counter(row['case_index'] for row in public) == expected_cases, family + ':distinct-bank-coverage')
    require(len([c for c in receipt['checks'] if c.startswith('fault-withdraw-and-recovery:')]) == faults, family + ':fault-recovery-coverage')
    require('router-checkpoint-tail-restart' in receipt['checks'], family + ':restart-coverage')
    if family == 'pump':
        require('completed-curve-source-retained-graph-withdrawn-and-qualified-scope-restored' in receipt['checks'], 'pump:completed-curve-coverage')
        require(captures['model_projection'].get('passed') is True and captures['model_projection'].get('original_program_execution_claimed') is False, 'pump:model-authority')
        require(captures['model_projection']['input_sha256'] == receipt['captured_plans']['sha256'], 'pump:model-input-binding')
    for request, projected, original in zip(public, model, native):
        require(request['case_index'] == projected['case_index'] == original['case_index'], family + ':case-alignment')
        plan = request['execution_plan']
        plan_hash = plan['plan_hash']
        payload = {key: plan[key] for key in ['context', 'snapshot', 'allocation', 'min_buy_amount', 'economic_policy_hash']}
        require(hashlib.sha256(json.dumps(payload, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()).hexdigest() == plan_hash, family + ':plan-hash')
        require(plan_hash == original['plan_hash'], family + ':plan-alignment')
        require(plan['snapshot'] == request['snapshot']['reference'] == request['quote']['snapshotRef'], family + ':snapshot-binding')
        for internal, public_name in [('chain_key', 'chainKey'), ('caller', 'caller'), ('recipient', 'recipient'), ('sell_token', 'sellToken'), ('buy_token', 'buyToken')]:
            require(plan['context'][internal] == request['request'][public_name], family + ':context-binding')
        require(len(plan['allocation']['routes']) == 1, family + ':single-route')
        route = plan['allocation']['routes'][0]
        require(route['amount_in'] == request['request']['sellAmount'] and route['amount_out'] == request['quote']['grossBuyAmount'], family + ':route-amounts')
        raw = original['native']
        require(raw['passed'] is True, family + ':original-failed')
        pinned = pinned_records[request['case_index']]
        pinned = pinned if family == 'pump' else pinned['native']
        require(raw['before'] == pinned['before'] and raw['after'] == pinned['after'], family + ':original-complete-bank')
        if family == 'pump':
            require(projected['plan_hash'] == plan_hash and len(projected['steps']) == 1, 'pump:model-alignment')
            step = projected['steps'][0]
            sell = raw['kind'] == 'sell'
            require(raw['ids']['user'] == request['request']['caller'] == request['request']['recipient'], 'pump:user-binding')
            sol_delta = raw['after'][raw['ids']['user']]['lamports'] - raw['before'][raw['ids']['user']]['lamports']
            tokens = token_delta(raw, raw['ids']['user_ata'])
            require(sol_delta == raw['user_sol_delta'] and tokens == raw['user_token_delta'], 'pump:reported-funds')
            amount_in = -tokens if sell else -sol_delta
            amount_out = sol_delta if sell else tokens
            require(str(amount_in) == step['amount_in'] == request['request']['sellAmount'], 'pump:actual-input')
            require(str(amount_out) == step['amount_out'] == request['quote']['grossBuyAmount'], 'pump:actual-output')
            for state in ['before', 'after']:
                require(projection.pool(raw[state], raw['ids'])['pump']['accounts'] == step[state]['pump']['accounts'], 'pump:complete-model-ledger')
        else:
            require(projected['execution_plan']['plan_hash'] == plan_hash, 'only-y:captured-projection')
            require(raw['payer'] == request['request']['caller'] and raw['recipient'] == request['request']['recipient'], 'only-y:user-binding')
            paid = -token_delta(raw, user_token(raw, request['request']['sellToken'], raw['payer']))
            received = token_delta(raw, user_token(raw, request['request']['buyToken'], raw['recipient']))
            require(str(paid) == raw['payer_input_delta'] == request['request']['sellAmount'] and str(received) == raw['recipient_output_delta'] == request['quote']['grossBuyAmount'], 'only-y:actual-funds')
            before = projection.pool(raw['before'], raw['clock'], raw['program_sha256'])['meteora']
            observed = request['snapshot']['pools'][0]['meteora']
            require(all(before[key] == observed[key] for key in ['pair_data', 'oracle_data', 'bin_arrays', 'raw_vault0', 'raw_vault1']), 'only-y:source-bank-binding')
    if family == 'only-y':
        require(receipt.get('normal_exact_out_exercised') is False, 'only-y:exact-out-authority')
    return freeze(path, source)


def main():
    report = json.loads(VALIDATION.read_bytes())
    require(report.get('passed') is True and report['state'] == 'LOCAL_SCOPE_VERIFIED', 'validation-failed')
    source = source_hash()
    require(source == report['source_tree_sha256'] == report['source_tree_sha256_after'], 'validation-source-changed')
    require(sha(Path(__file__).with_name('validate_family_expansion.py')) == report['tool_sha256'], 'validation-tool-changed')
    names = [c['name'] for c in report['checks']]
    require(len(names) == len(EXPECTED_CHECKS) and set(names) == EXPECTED_CHECKS, 'validation-check-coverage')
    for check in report['checks']:
        require(check['exit_code'] == 0, 'failed-check:' + check['name'])
        artifact(check['log'])
    require(sum(c['rust_test_executions'] for c in report['checks']) == report['rust_test_executions'], 'test-count')
    pump = process_receipt(artifact(report['process_receipt']), source, 'pump', 5, 7)
    only_y = process_receipt(OUT / 'core-meteora-only-y-normal-process.json', source, 'only-y', 33, 9)
    references = dict(pump=verify_pump(False), only_y=verify_only_y(False))
    supplemental = []
    tag = secrets.token_hex(6)
    for optimized in [False, True]:
        name = 'tamper-controls' + ('-optimized' if optimized else '')
        command = [sys.executable] + (['-OO'] if optimized else []) + ['-m', 'unittest', 'discover', '-s', 'integration/tools/acceptance/tests', '-p', 'test_family_expansion_acceptance.py']
        log = OUT / ('family-acceptance-' + tag + '-' + name + '.log')
        with log.open('w') as output:
            checked = subprocess.run(command, cwd=ROOT, stdout=output, stderr=subprocess.STDOUT)
        text = log.read_text()
        require(checked.returncode == 0 and 'Ran 4 tests' in text and 'skipped' not in text, name + ':failed-or-skipped')
        supplemental.append(dict(name=name, exit_code=0, log=dict(path=str(log.relative_to(ROOT)), sha256=sha(log))))
    require(source_hash() == source, 'source-changed-during-acceptance')
    tools = [Path(__file__), Path(__file__).with_name('family_reference.py'), Path(__file__).with_name('validate_family_expansion.py'), Path(__file__).parent / 'tests/test_family_expansion_acceptance.py']
    result = dict(
        schema_version=1, passed=True, state='LOCAL_SCOPE_VERIFIED', source_tree_sha256=source,
        validation=freeze(VALIDATION, source), pump_process=pump, only_y_process=only_y,
        final_tools={str(p.relative_to(ROOT)): sha(p) for p in tools}, reference_results=references,
        finite_validation_checks=len(names), rust_test_executions=report['rust_test_executions'],
        supplemental_checks=supplemental, python_tampering_controls=8,
        pump_api_plans=5, only_y_api_plans=33, actual_plan_sbf_swaps=38,
        pump_source_faults=7, only_y_source_faults=9, only_y_normal_exact_out_exercised=False,
        only_y_model_authority='same-source Rust full-state differential; normal process uses captured-plan direct original replay',
        chain_combination_authority='synthetic first-wave chain-key reuse and isolation; no new deployment or second-wave adapter qualification',
        all_requested_features_complete=False, production_ready=False,
        authority='finite controlled original SBF banks, local finalized RPC, actual PG/Kafka, dual Router; no public atomic execution, trusted canonical bank, net chain cost or global optimality claim',
    )
    write_result(TARGET, result)
    print(json.dumps({key: result[key] for key in ['passed', 'state', 'rust_test_executions', 'actual_plan_sbf_swaps']}))


if __name__ == '__main__':
    main()
