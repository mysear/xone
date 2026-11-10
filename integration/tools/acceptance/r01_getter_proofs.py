#!/usr/bin/env python3
"""Acquire bounded Uniswap V3 getter candidates, then rerun original VM/MPT.

ABI observations select candidates only. A passing getter closure still requires
normal service differential execution before any full R01 qualification.
"""
import argparse
import copy
import json
from pathlib import Path
import re
import subprocess
import time

from r01_public_catalog import (ROOT, load_json, public_rpc_endpoint, read_blob,
                                require, sha256)
from r01_public_catalog_capture import fetch_rpc, reference, write_new
from r01_public_catalog_evm_history import CHAINS

FRESH_ROOT = ROOT / 'integration/test/source/deployment-catalog/evm-fresh-v5'
OUTPUT_ROOT = ROOT / 'integration/artifacts/local'
MAX_CALLS, ACQUISITION_SECONDS = 48, 120
INPUT_FILES = (('anchor', 'anchor.json'), ('plan', 'execution-plan.json'),
               ('execution', 'fork-execution.json'), ('rpc_index', 'rpc-index.json'),
               ('local_helper_compilation', 'local-helper-compiler.json'),
               ('prestate', 'prestate-evidence.json'))


def signed_word(word):
    require(isinstance(word, str) and re.fullmatch(r'[0-9a-fA-F]{64}', word),
            'GETTER_ABI_WORD')
    n = int(word, 16)
    return n - (1 << 256) if n & (1 << 255) else n


def slot_plan(tick, spacing, bitmap):
    result = subprocess.run(
        ['node', str(Path(__file__).with_name('r01_getter_slots.cjs'))],
        input=json.dumps(dict(tick=tick, spacing=spacing, bitmap=bitmap)),
        capture_output=True, text=True, timeout=20)
    require(result.returncode == 0, 'GETTER_SLOT_PLAN:' + result.stderr[-1500:])
    plan = json.loads(result.stdout)
    require(0 < len(plan['keys']) <= 1027, 'GETTER_SLOT_BUDGET')
    return plan


def blob_json(ref):
    # Keep duplicate-member rejection as well as the digest/path check.
    read_blob(ROOT, ref)
    return load_json(ROOT / ref['path'])


def fresh_row(previous, directory):
    require(previous.get('schema_version') == 4, 'GETTER_PREVIOUS_VERSION')
    old = previous['row']
    chain = old['chain']
    require(chain in ('polygon', 'arbitrum') and old['protocol'] == 'uniswap-v3'
            and old['id'] == chain + '-uniswap-v3', 'GETTER_LITERAL_SCOPE')
    chain_id = CHAINS[chain][0]
    require(type(old['chain_id']) is int and old['chain_id'] == chain_id and
            old['chain_key'] == f'evm:{chain_id}', 'GETTER_CHAIN_BINDING')
    saved = load_json(directory / 'anchor.json')
    evidence = load_json(directory / 'prestate-evidence.json')
    require(saved.get('schema_version') == evidence.get('schema_version') == 2,
            'GETTER_FRESH_VERSION')
    # The original runner prefers v2 when present. Never silently bind another file.
    require(not (directory / 'prestate-evidence-v2.json').exists(),
            'GETTER_AMBIGUOUS_PRESTATE')
    anchor, block = saved['anchor'], evidence['block']
    require(type(anchor['height']) is int and anchor['height'] > old['anchor']['height']
            and anchor['hash'] != old['anchor']['hash'], 'GETTER_NEW_ANCHOR_REQUIRED')
    require(type(anchor['chain_id']) is int and anchor['chain_id'] == chain_id and
            anchor['chain_key'] == old['chain_key'] and evidence['chain'] == chain and
            evidence['chain_id'] == str(chain_id), 'GETTER_CHAIN_BINDING')
    header_wire = blob_json(saved['header'])
    require(header_wire['method'] == 'eth_getBlockByNumber' and
            header_wire['response']['result'] == block and
            block['hash'] == anchor['hash'] == evidence['expected_block_hash'] and
            block['number'] == hex(anchor['height']) and
            block['stateRoot'] == anchor['state_root'] and
            block['timestamp'] == hex(anchor['timestamp']), 'GETTER_HEADER_BINDING')
    semantics = anchor['native_state_semantics']
    require(semantics['header_height'] == semantics['state_height'] == anchor['height']
            and semantics['state_root'] == block['stateRoot'], 'GETTER_STATE_ANCHOR_BINDING')
    require(not evidence['missing_proofs'], 'GETTER_BASE_PRESTATE_INCOMPLETE')
    genesis_wire = blob_json(saved['genesis'])
    require(genesis_wire['method'] == 'eth_getBlockByNumber' and
            genesis_wire['params'] == ['0x0', False] and
            genesis_wire['response']['result']['number'] == '0x0' and
            genesis_wire['response']['result']['hash'] == old['genesis_hash'] ==
            saved['genesis_hash'], 'GETTER_GENESIS_BINDING')
    deployments = {d['address'].lower(): d for d in evidence['deployments']}
    require(len(deployments) == len(evidence['deployments']), 'GETTER_DUPLICATE_ACCOUNT')
    for address, expected in ((old['pool'], old['pool_runtime_sha256']),
                              (old['factory'], old['factory_runtime_sha256'])):
        require(address.lower() in deployments, 'GETTER_RUNTIME_ACCOUNT_MISSING')
        code = deployments[address.lower()]['code']
        require(isinstance(code, str) and re.fullmatch(r'0x(?:[0-9a-fA-F]{2})+', code)
                and sha256(bytes.fromhex(code[2:])) == expected,
                'GETTER_RUNTIME_VERSION_CHANGED')
    plan = load_json(directory / 'execution-plan.json')
    execution = load_json(directory / 'fork-execution.json')
    require(plan.get('schema_version') == 1 and execution.get('schema_version') == 2,
            'GETTER_EXECUTION_VERSION')
    require(plan['chain'] == execution['chain'] == chain and
            plan['chain_id'] == execution['chain_id'] == chain_id,
            'GETTER_CHAIN_BINDING')
    require(execution['anchor'] == anchor, 'GETTER_EXECUTION_ANCHOR')
    require(all(execution[k] == [] for k in
                ('balance_overrides', 'storage_overrides', 'code_overrides')) and
            execution['public_transactions_sent'] == 0, 'GETTER_ORIGINAL_STATE_REQUIRED')
    cases = [c for c in plan['cases'] if c['catalog_deployment_id'] == old['id']]
    require(len(cases) == 1 and all(cases[0][k] == old[k] for k in
            ('protocol', 'pool', 'factory', 'pool_runtime_sha256', 'factory_runtime_sha256')),
            'GETTER_DEPLOYMENT_BINDING')
    for endpoint in saved['endpoints']:
        public_rpc_endpoint(endpoint)
    row = copy.deepcopy(old)
    row['directory'] = str(directory.relative_to(ROOT))
    row['anchor'] = {k: anchor[k] for k in ('height', 'hash')}
    row['genesis_observation'] = saved['genesis']
    row['inputs'] = {key: reference(directory / name) for key, name in INPUT_FILES}
    return row, saved


def verify_proof_candidate(proof, pool, keys):
    require(isinstance(proof, dict) and proof.get('address', '').lower() == pool.lower(),
            'GETTER_PROOF_ADDRESS')
    storage = proof.get('storageProof')
    require(isinstance(storage, list) and all(isinstance(s, dict) and
            isinstance(s.get('key'), str) and re.fullmatch(r'0x[0-9a-fA-F]{1,64}', s['key'])
            for s in storage), 'GETTER_PROOF_KEY_SHAPE')
    observed = [format(int(item['key'], 16), '064x') for item in storage]
    require(len(observed) == len(set(observed)) == len(keys) and
            set(observed) == {k[2:] for k in keys}, 'GETTER_PROOF_KEYS')
    require(isinstance(proof.get('accountProof'), list) and proof['accountProof'] and
            all(isinstance(s.get('proof'), list) for s in storage), 'GETTER_PROOF_SHAPE')


def verify_closure(result, row, plan, slot0, spacing, bitmap):
    require(result.get('id') == row['id'] and result.get('anchor') == row['anchor'],
            'GETTER_REPLAY_ANCHOR')
    require(result.get('passed') is True and result.get('missing') == [] and
            result.get('error') is None and result.get('normal_admission_rejection') is None,
            'GETTER_PROOF_CLOSURE_INCOMPLETE')
    calls = result['calls']
    require(all(c.get('all_code_and_storage_MPT_bound') is True and not c.get('error')
                and not c.get('misses') for c in calls), 'GETTER_UNBOUND_VM_CALL')
    values = {c['data']: c['value'] for c in calls if c['to'] == row['pool']}
    bitmap_data = '0x5339c296' + format(plan['word'] % (1 << 256), '064x')
    expected_selectors = {'0x0dfe1681', '0xd21220a7', '0xc45a0155', '0xddca3f43',
                          '0xd0c93a7c', '0x3850c7bd', '0x1a686502', bitmap_data}
    expected_selectors.update('0xf30dba93' + format(t % (1 << 256), '064x')
                              for t in plan['ticks'])
    require(len(calls) == len(expected_selectors) and set(values) == expected_selectors,
            'GETTER_REPLAY_CALL_SET')
    require(values['0x3850c7bd'].lower() == slot0.lower() and
            values['0xd0c93a7c'].lower() == spacing.lower() and
            int(values[bitmap_data], 16) == int(bitmap, 16), 'GETTER_ABI_VM_DIFFERENCE')
    sloads = [s for c in calls for s in c['sloads']]
    require(all(s['address'].lower() == row['pool'].lower() for s in sloads) and
            {s['key'].lower() for s in sloads} == set(plan['keys']),
            'GETTER_VM_SLOT_SET_DIFFERENCE')
    require(result.get('public_network_requests') == 0 and result.get('public_writes') == 0
            and result.get('normal_parser_typed_baseline_injected') is False,
            'GETTER_REPLAY_AUTHORITY')



def replay_slot_plan(result, row):
    """Select keys only from a newly executed, complete original-VM closure."""
    calls = result.get('calls', [])
    values = {c['data']: c['value'] for c in calls if c['to'] == row['pool']}
    slot0, spacing = values.get('0x3850c7bd'), values.get('0xd0c93a7c')
    require(isinstance(slot0, str) and re.fullmatch(r'0x[0-9a-fA-F]{448}', slot0) and
            isinstance(spacing, str) and re.fullmatch(r'0x[0-9a-fA-F]{64}', spacing),
            'GETTER_ABI_SHAPE')
    tick, step = signed_word(slot0[66:130]), signed_word(spacing[2:])
    empty = slot_plan(tick, step, '0x0')
    selector = '0x5339c296' + format(empty['word'] % (1 << 256), '064x')
    bitmap = values.get(selector)
    require(isinstance(bitmap, str) and re.fullmatch(r'0x[0-9a-fA-F]{64}', bitmap),
            'GETTER_BITMAP_ABI_SHAPE')
    plan = slot_plan(tick, step, bitmap)
    verify_closure(result, row, plan, slot0, spacing, bitmap)
    return plan


def replay(input_path, output, stem, report):
    execution = output / (stem + '-execution.json')
    probe = subprocess.run(['node', str(Path(__file__).with_name('r01_public_catalog_evm_readonly.cjs')),
                            str(input_path), str(execution)],
                           capture_output=True, text=True, timeout=120)
    log = output / (stem + '-replay.log')
    write_new(log, (probe.stdout + probe.stderr).encode())
    report[stem + '_replay_log'] = reference(log)
    require(probe.returncode == 0, 'GETTER_ORIGINAL_VM_REPLAY_FAILED:' + stem)
    report[stem + '_probe'] = reference(execution)
    return load_json(execution)


def capture(previous_path, directory, output):
    previous_path, directory, output = (p.resolve() for p in (previous_path, directory, output))
    require(directory.is_relative_to(FRESH_ROOT.resolve()) and directory != FRESH_ROOT.resolve(),
            'GETTER_FRESH_DIRECTORY')
    require(output.parent == OUTPUT_ROOT.resolve() and output.name.startswith('r01-getter-')
            and not output.exists(), 'GETTER_NEW_OUTPUT')
    require(previous_path.is_relative_to(ROOT), 'GETTER_PREVIOUS_PATH')
    output.mkdir(parents=True)
    report = dict(schema_version=5, passed=False, full_R01_qualified=False,
                  normal_service_differential_required=True, directory=str(directory.relative_to(ROOT)),
                  previous_input=reference(previous_path),
                  sources=[reference(Path(__file__)),
                           reference(Path(__file__).with_name('r01_getter_slots.cjs'))] +
                          [reference(Path(__file__).with_name(name)) for name in
                           ('r01_public_catalog_evm_readonly.cjs',
                            'r01_public_catalog_evm_commitments.cjs',
                            'r01_public_catalog_proof.cjs', 'r01_public_catalog_evm_trie.cjs')],
                  acquisition_observations=[], supplements=[], rpc_attempts=0)
    try:
        report['capture_inputs'] = {key: reference(directory / name)
                                    for key, name in INPUT_FILES}
        report['missing_base_proofs'] = load_json(directory / 'prestate-evidence.json')['missing_proofs']
        row, saved = fresh_row(load_json(previous_path), directory)
        report.update(id=row['id'], anchor=row['anchor'], inputs=row['inputs'])
        baseline_input = output / 'baseline-input.json'
        write_new(baseline_input, (json.dumps(dict(schema_version=5, row=row,
            previous_input=report['previous_input'], supplements=[]), indent=2) + '\n').encode())
        report['baseline_input'] = reference(baseline_input)
        baseline = replay(baseline_input, output, 'baseline', report)
        report.update(baseline_getter_calls=len(baseline['calls']),
                      baseline_missing=baseline['missing'], baseline_error=baseline.get('error'))
        if baseline.get('passed') is True and baseline.get('missing') == []:
            # A newly executed, already complete MPT closure needs no public
            # reacquisition. Reopening old state can be unavailable even while
            # these immutable proofs remain sufficient for an offline replay.
            plan = replay_slot_plan(baseline, row)
            execution = output / 'getter-execution.json'
            write_new(execution, read_blob(ROOT, report['baseline_probe']))
            report.update(passed=True, plan=plan, proof_slots=len(plan['keys']),
                          getter_calls=len(baseline['calls']), input=reference(baseline_input),
                          getter_probe=reference(execution),
                          getter_probe_copy_of=report['baseline_probe'],
                          getter_replay_log=report['baseline_replay_log'],
                          closure_source='new original-VM execution over retained MPT; no public reads')
            write_new(output / 'report.json', (json.dumps(report, indent=2) + '\n').encode())
            return report
        endpoints = list(dict.fromkeys(saved['endpoints'] + CHAINS[row['chain']][1]))
        started, validated = time.monotonic(), set()
        tag = hex(row['anchor']['height'])

        def fetch(endpoint, method, params):
            require(report['rpc_attempts'] < MAX_CALLS and
                    time.monotonic() - started < ACQUISITION_SECONDS, 'GETTER_ACQUISITION_BUDGET')
            report['rpc_attempts'] += 1
            ref, value = fetch_rpc(endpoint, method, params,
                output / f"{report['rpc_attempts']:03}-{method}.json", budget=16 << 20)
            report['acquisition_observations'].append(ref)
            wire = blob_json(ref)
            response = wire.get('response', {})
            require(time.monotonic() - started < ACQUISITION_SECONDS, 'GETTER_ACQUISITION_DEADLINE')
            if value is not None:
                require(response.get('jsonrpc') == '2.0' and response.get('id') == 1
                        and 'error' not in response, 'GETTER_RPC_ENVELOPE')
            return ref, value

        def call(method, params):
            for endpoint in endpoints:
                if endpoint not in validated:
                    _, chain = fetch(endpoint, 'eth_chainId', [])
                    if chain is None:
                        continue
                    require(chain == hex(row['chain_id']), 'GETTER_RPC_CHAIN')
                    _, header = fetch(endpoint, 'eth_getBlockByNumber', [tag, False])
                    if header is None:
                        continue
                    require(header['hash'] == row['anchor']['hash'] and header['number'] == tag
                            and header['stateRoot'] == saved['anchor']['state_root'] and
                            header['timestamp'] == hex(saved['anchor']['timestamp']),
                            'GETTER_RPC_ANCHOR')
                    validated.add(endpoint)
                ref, value = fetch(endpoint, method, params)
                if value is not None:
                    return ref, value
            raise ValueError('GETTER_PUBLIC_SOURCE_UNAVAILABLE:' + method)

        pool = row['pool']
        _, slot0 = call('eth_call', [{'to': pool, 'data': '0x3850c7bd'}, tag])
        _, spacing_word = call('eth_call', [{'to': pool, 'data': '0xd0c93a7c'}, tag])
        require(isinstance(slot0, str) and re.fullmatch(r'0x[0-9a-fA-F]{448}', slot0) and
                isinstance(spacing_word, str) and re.fullmatch(r'0x[0-9a-fA-F]{64}', spacing_word),
                'GETTER_ABI_SHAPE')
        tick, spacing = signed_word(slot0[66:130]), signed_word(spacing_word[2:])
        empty = slot_plan(tick, spacing, '0x0')
        argument = format(empty['word'] % (1 << 256), '064x')
        _, bitmap = call('eth_call', [{'to': pool, 'data': '0x5339c296' + argument}, tag])
        require(isinstance(bitmap, str) and re.fullmatch(r'0x[0-9a-fA-F]{64}', bitmap),
                'GETTER_BITMAP_ABI_SHAPE')
        plan = slot_plan(tick, spacing, bitmap)
        report.update(plan=plan, proof_slots=len(plan['keys']))
        for offset in range(0, len(plan['keys']), 64):
            keys = plan['keys'][offset:offset + 64]
            ref, proof = call('eth_getProof', [pool, keys, tag])
            verify_proof_candidate(proof, pool, keys)
            report['supplements'].append(ref)
        closing_ref, closing = call('eth_getBlockByNumber', [tag, False])
        require(closing['hash'] == row['anchor']['hash'] and closing['number'] == tag
                and closing['stateRoot'] == saved['anchor']['state_root'] and
                closing['timestamp'] == hex(saved['anchor']['timestamp']), 'GETTER_CLOSING_ANCHOR')
        input_path = output / 'input.json'
        write_new(input_path, (json.dumps(dict(schema_version=5, row=row,
            previous_input=report['previous_input'], supplements=report['supplements'],
            plan=plan, acquisition_observations=report['acquisition_observations'],
            closing_header=closing_ref,
            authority='candidate acquisition; requires original VM/MPT and normal service replay'), indent=2) + '\n').encode())
        report['input'] = reference(input_path)
        result = replay(input_path, output, 'getter', report)
        report.update(getter_calls=len(result['calls']), missing=result['missing'],
                      replay_error=result.get('error'))
        verify_closure(result, row, plan, slot0, spacing_word, bitmap)
        report['passed'] = True
    except (ValueError, KeyError, TypeError, OSError, subprocess.TimeoutExpired) as error:
        report['error'] = str(error)
    write_new(output / 'report.json', (json.dumps(report, indent=2) + '\n').encode())
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--previous-input', type=Path, required=True)
    parser.add_argument('--directory', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    args = parser.parse_args()
    result = capture(args.previous_input, args.directory, args.out)
    print(json.dumps(result, indent=2))
    return 0 if result['passed'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
