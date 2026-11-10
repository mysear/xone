#!/usr/bin/env python3
"""Audit the fixed BSC4 swap scope without upgrading broader design completion."""
import argparse
import json
import time
from pathlib import Path
from weighted_evidence import ROOT, OUT, require, runtime_fingerprint, sha, write_result


def bound(path, digest=None):
    path = (ROOT / path).resolve()
    require(path.is_relative_to(ROOT) and path.is_file(), 'BSC4_LOCAL_INPUT_REQUIRED')
    actual = sha(path)
    require(digest is None or actual == digest, 'BSC4_INPUT_CHANGED:' + str(path))
    return dict(path=str(path.relative_to(ROOT)), sha256=actual)


def runtime_match(code, artifact):
    require(code.startswith('0x'), 'BSC4_RUNTIME_HEX')
    raw = bytearray.fromhex(code[2:])
    declared = artifact['evm']['deployedBytecode']
    expected = bytes.fromhex(declared['object'])
    require(len(raw) == len(expected), 'BSC4_RUNTIME_LENGTH')
    for positions in declared['immutableReferences'].values():
        for position in positions:
            start, size = position['start'], position['length']
            require(size == 32 and 0 <= start <= len(raw) - size, 'BSC4_IMMUTABLE_RANGE')
            raw[start:start+size] = bytes(size)
    require(bytes(raw) == expected, 'BSC4_ORIGINAL_LOCAL_RUNTIME_CHANGED')


def run():
    fingerprint = runtime_fingerprint()
    process_path = OUT / 'core-pancake-bsc4-normal-process.json'
    process = json.loads(process_path.read_text())
    require(process.get('passed') is True and process['runtime_fingerprint'] == fingerprint
            and process['source_tree_sha256'] == fingerprint['source_tree_sha256'], 'BSC4_CURRENT_PROCESS_REQUIRED')
    require(process['actual_api_records'] == process['original_swaps'] == 13
            and process['actual_tick_migrations'] > 0
            and process['negative_controls'].get('passed') is True
            and process['negative_controls']['mutations'] == 13, 'BSC4_ACTUAL_PROCESS_SCOPE')
    for name, digest in process['tool_inputs'].items():
        bound(name, digest)
    for key in ['actual_api_input', 'model_projection', 'original_replay']:
        bound(process[key]['path'], process[key]['sha256'])
    fixture_path = ROOT / 'integration/test/contracts/pancake-bsc4-state.json'
    require(sha(fixture_path) == process['reference_sha256'], 'BSC4_PROCESS_REFERENCE_CHANGED')
    fixture = json.loads(fixture_path.read_text())
    original_ref = fixture['original_reference']
    bound(original_ref['path'], original_ref['sha256'])
    require(original_ref['sha256'] == fixture['source_reference_sha256'], 'BSC4_ORIGINAL_SOURCE_BINDING')
    original = json.loads((ROOT / original_ref['path']).read_text())
    require(original.get('passed') is True and original['generator_sha256']
            == sha(ROOT / 'integration/tools/ci/pancake-bsc4-reference.cjs'), 'BSC4_ORIGINAL_GENERATOR_CHANGED')
    require(len(original['cases']) == 64 and len(original['invalid_bank_cases']) == 4
            and len(original['sequences']) == 5
            and sum(len(s['steps']) for s in original['sequences']) == 15,
            'BSC4_ORIGINAL_CASE_SCOPE')
    complete = sum(not c['expected']['reverted'] and c['expected']['consumed_in'] == c['amount_in'] for c in original['cases'])
    partial = sum(not c['expected']['reverted'] and c['expected']['consumed_in'] != c['amount_in'] for c in original['cases'])
    require(complete == 48 and partial == 16
            and all(c['expected']['reverted'] for c in original['invalid_bank_cases']),
            'BSC4_ORIGINAL_EXECUTION_COUNTS')
    authorities = []
    for key in ['exact_source_reference', 'predecessor_source_reference']:
        ref = original[key]
        authorities.append(bound(ref['path'], ref['sha256']))
    exact = json.loads((ROOT / original['exact_source_reference']['path']).read_text())
    prior = json.loads((ROOT / original['predecessor_source_reference']['path']).read_text())
    require(exact['passed'] is True and prior['passed'] is True, 'BSC4_SOURCE_REBUILD_REQUIRED')
    artifacts = dict(original['artifacts'])
    for name in ['first', 'second', 'third']:
        artifacts[name] = prior['contracts'][name]['artifact']
    for role, artifact in artifacts.items():
        runtime_match(original['deployment'][role]['code'], artifact)
    # Match the controlled code back to exact retained original compilation jobs.
    for role, build_role in [('pool', 'pool'), ('lm', 'lm'), ('masterChef', 'masterchef')]:
        require(artifacts[role] == exact['contracts'][build_role]['artifact'], 'BSC4_ARTIFACT_IDENTITY:' + role)
    for role in ['first', 'second', 'third', 'factory']:
        require(artifacts[role] == prior['contracts'][role]['artifact'], 'BSC4_PREDECESSOR_ARTIFACT:' + role)
        entry = prior['contracts'][role]
        bound(entry['source_path'], entry['source_sha256'])
    acquisition = json.loads((ROOT / 'integration/tools/reference/pancake-v3-lm-bsc/acquisition.json').read_text())
    for entry in acquisition['records']:
        bound('integration/tools/reference/pancake-v3-lm-bsc/' + entry['role'] + '-source.json', entry['sha256'])
    checks = []
    for filename in ['core-bsc4-post-clone-revalidation-20261109.json', 'core-bsc4-final-lint-20261109.json']:
        path = OUT / filename
        result = json.loads(path.read_text())
        require(result.get('passed') is True and result['source_tree_sha256']
                == result['source_tree_sha256_after'] == fingerprint['source_tree_sha256'],
                'BSC4_CURRENT_CHECK_REQUIRED:' + filename)
        for entry in result['checks']:
            require(entry['observed_exit_code'] == 0, 'BSC4_CHECK_EXIT')
            bound(entry['log']['path'], entry['log']['sha256'])
        checks.append(bound(path))
    go_path = OUT / 'core-bsc4-go-fixed-check-20261109.json'
    go = json.loads(go_path.read_text())
    require(go.get('passed') is True and go['observed_exit_code'] == 0
            and go['source_tree_sha256'] == go['source_tree_sha256_after']
            == fingerprint['source_tree_sha256'], 'BSC4_GO_CHECK_REQUIRED')
    bound(go['log']['path'], go['log']['sha256'])
    checks.append(bound(go_path))
    require(runtime_fingerprint() == fingerprint, 'BSC4_FINAL_RUNTIME_CHANGED')
    report = dict(schema_version=1, state='LOCAL_SCOPE_VERIFIED', passed=True,
                  source_tree_sha256=fingerprint['source_tree_sha256'], runtime_fingerprint=fingerprint,
                  authority='fixed original BSC4 swap and actual predecessor runtimes, controlled banks and standard Tokens',
                  process_report=bound(process_path), fixture=bound(fixture_path),
                  original_reference=bound(original_ref['path']), original_authorities=authorities,
                  checks=checks, actual_api_records=13, original_runtime_swaps=13,
                  actual_tick_migrations=process['actual_tick_migrations'], negative_controls=13,
                  full_original_swaps=48, partial_swaps_rejected=16, invalid_banks_rejected=4,
                  original_sequence_steps=15, cpp_wire_vectors=4,
                  metadata_match_by_role={k: v['full_runtime_match'] for k, v in prior['contracts'].items()},
                  limitations=['controlled bank and synthetic fixed-clock header, no independent historical bank qualification',
                               'first predecessor metadata differs, executable body matches original source',
                               'standard ERC20 and finite complete ExactIn swap only',
                               'position modification and claiming not implemented by this swap profile',
                               'net chain cost, other chains and remaining DEX models still incomplete',
                               'production public transaction execution not qualified'],
                  measured_at_unix_ms=int(time.time()*1000), all_requested_features_complete=False,
                  production_ready=False)
    write_result(OUT / 'core-bsc4-local-q1-20261109.json', report)
    return report


if __name__ == '__main__':
    argparse.ArgumentParser(description=__doc__).parse_args()
    print(json.dumps(run(), indent=2))
