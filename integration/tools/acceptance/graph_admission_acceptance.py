#!/usr/bin/env python3
"""Verify the current route-admission refresh scope and original-program regression."""
import argparse
import json
import time
from weighted_evidence import ROOT, OUT, require, sha, runtime_fingerprint, write_result


def artifact(path, expected=None):
    path = (ROOT / path).resolve()
    require(path.is_relative_to(ROOT) and path.is_file(), 'GRAPH_LOCAL_ARTIFACT_REQUIRED')
    value = sha(path)
    require(expected is None or expected == value, 'GRAPH_ARTIFACT_CHANGED')
    return dict(path=str(path.relative_to(ROOT)), sha256=value)


def run():
    fingerprint = runtime_fingerprint()
    validation_path = OUT / 'core-graph-admission-validation-20261109.json'
    validation = json.loads(validation_path.read_text())
    require(validation.get('passed') is True and validation['state'] == 'PASSED'
            and validation['source_tree_sha256'] == validation['source_tree_sha256_after']
            == fingerprint['source_tree_sha256'] and validation['runtime_fingerprint'] == fingerprint,
            'GRAPH_CURRENT_VALIDATION_REQUIRED')
    checks = {item['name']: item for item in validation['checks']}
    require(len(checks) == len(validation['checks'])
            and set(checks) == {'replica', 'api', 'lint', 'build', 'process'}, 'GRAPH_CHECK_SET')
    for check in checks.values():
        require(check['observed_exit_code'] == 0, 'GRAPH_CHECK_FAILED')
        artifact(check['log']['path'], check['log']['sha256'])
    expected_tests = {
        'replica': ['block_commit_refreshes_admission_even_when_tokens_and_protocols_are_unchanged',
                    'admitted_bank_without_quote_edges_advances_watermark_revokes_then_recovers',
                    'changing_admission_cannot_bypass_an_explicit_scope_suspension',
                    'reorg_barrier_revokes_all_handles_and_requires_exact_commit'],
        'api': ['live_http_admission_refresh_revokes_cached_quotes_and_restores_both_directions',
                'empty_admission_scope_http_is_unready_then_recovers_without_losing_cursor',
                'unsupported_vault_does_not_change_another_chains_quotes'],
    }
    for group, tests in expected_tests.items():
        text = (ROOT / checks[group]['log']['path']).read_text()
        for name in tests:
            require('test ' + name + ' ... ok' in text, 'GRAPH_REQUIRED_TEST_NOT_EXECUTED:' + name)
    red = OUT / 'core-graph-admission-red-20261109.log'
    text = red.read_text()
    require('test result: FAILED' in text and 'left: 0' in text and 'right: 3' in text,
            'GRAPH_ORIGINAL_COUNTEREXAMPLE_REQUIRED')
    process_path = OUT / 'core-pancake-bsc4-normal-process.json'
    process = json.loads(process_path.read_text())
    require(process.get('passed') is True and process['runtime_fingerprint'] == fingerprint
            and process['source_tree_sha256'] == fingerprint['source_tree_sha256']
            and process['actual_api_records'] == process['original_swaps'] == 13
            and process['actual_tick_migrations'] == 6
            and process['negative_controls'].get('passed') is True
            and process['negative_controls']['mutations'] == 13, 'GRAPH_NORMAL_PROCESS_REGRESSION')
    for path, digest in process['tool_inputs'].items():
        artifact(path, digest)
    for key in ['actual_api_input', 'model_projection', 'original_replay']:
        artifact(process[key]['path'], process[key]['sha256'])
    reference = ROOT / 'integration/test/contracts/pancake-bsc4-state.json'
    require(sha(reference) == process['reference_sha256'], 'GRAPH_PROCESS_REFERENCE_CHANGED')
    # This is a state-replica/API change. Bind the preserved original-model
    # authority instead of claiming that older global checks used this source.
    prior_path = OUT / 'core-bsc4-delivery-20261109.json'
    prior = json.loads(prior_path.read_text())
    require(prior.get('passed') is True, 'GRAPH_PRIOR_PROTOCOL_VALIDATION_REQUIRED')
    authority_path = ROOT / prior['acceptance']['path']
    artifact(authority_path, prior['acceptance']['sha256'])
    authority = json.loads(authority_path.read_text())
    original_process = artifact('integration/artifacts/local/core-bsc4-frozen-phase41-normal-process-20261109.json',
                                authority['process_report']['sha256'])
    require(authority['fixture']['sha256'] == sha(reference), 'GRAPH_PROTOCOL_REFERENCE_CHANGED')
    for role in ['chain-parser-go/bin/chain-parser-go', 'chain-parser-go/bin/fixture-parser-go',
                 'chain-parser-rust/target/debug/chain-parser-rust',
                 'chain-parser-rust/target/debug/fixture-parser-rust']:
        require(fingerprint['binaries'][role] == authority['runtime_fingerprint']['binaries'][role],
                'GRAPH_UNCHANGED_PARSER_BINARY_REQUIRED:' + role)
    require(runtime_fingerprint() == fingerprint, 'GRAPH_FINAL_RUNTIME_CHANGED')
    frozen_process = OUT / ('core-graph-admission-process-' + fingerprint['source_tree_sha256'][:12] + '.json')
    process_bytes = process_path.read_bytes()
    if frozen_process.exists():
        require(frozen_process.read_bytes() == process_bytes, 'GRAPH_FROZEN_RECEIPT_CHANGED')
    else:
        with frozen_process.open('xb') as stream:
            stream.write(process_bytes)
    report = dict(schema_version=1, passed=True, state='LOCAL_SCOPE_VERIFIED',
                  source_tree_sha256=fingerprint['source_tree_sha256'], runtime_fingerprint=fingerprint,
                  authority='current complete source banks, Replica and real in-process HTTP handlers; original BSC4 runtime normal-service regression retained separately',
                  original_failure=artifact(red), validation=artifact(validation_path),
                  expected_executed_tests=expected_tests, process_regression=artifact(frozen_process),
                  previous_protocol_delivery=artifact(prior_path), previous_protocol_process_backup=original_process,
                  replica_test_executions=checks['replica']['passed_test_executions'],
                  api_test_executions=checks['api']['passed_test_executions'],
                  actual_original_program_swaps=13, actual_tick_migrations=6,
                  limitations=['admission transitions exercised with explicit valid source banks, no new public deployment qualification',
                               'this scope repairs graph admission refresh, not a general incremental or COW solver',
                               'net execution cost, ExactOut and remaining protocol/chain models still incomplete'],
                  measured_at_unix_ms=int(time.time()*1000), all_requested_features_complete=False,
                  production_ready=False)
    write_result(OUT / 'core-graph-admission-local-20261109.json', report)
    return report


if __name__ == '__main__':
    argparse.ArgumentParser(description=__doc__).parse_args()
    print(json.dumps(run(), indent=2))
