#!/usr/bin/env python3
"""Verify the actual candidate-scope disclosure and original-program regression."""
import argparse
import json
import time
from weighted_evidence import ROOT, OUT, require, sha, runtime_fingerprint, write_result


def artifact(path, expected=None):
    path = (ROOT / path).resolve()
    require(path.is_relative_to(ROOT) and path.is_file(), 'CANDIDATE_LOCAL_ARTIFACT_REQUIRED')
    value = sha(path)
    require(expected is None or expected == value, 'CANDIDATE_ARTIFACT_CHANGED')
    return dict(path=str(path.relative_to(ROOT)), sha256=value)


def run():
    fingerprint = runtime_fingerprint()
    validation_path = OUT / 'core-candidate-scope-validation-20261109.json'
    validation = json.loads(validation_path.read_text())
    require(validation.get('passed') is True and validation['state'] == 'PASSED'
            and validation['source_tree_sha256'] == validation['source_tree_sha256_after']
            == fingerprint['source_tree_sha256'] and validation['runtime_fingerprint'] == fingerprint,
            'CANDIDATE_CURRENT_VALIDATION_REQUIRED')
    checks = {item['name']: item for item in validation['checks']}
    require(len(checks) == len(validation['checks'])
            and set(checks) == {'router-regression', 'parser-domain', 'lint', 'build', 'process'}, 'CANDIDATE_CHECK_SET')
    for check in checks.values():
        require(check['observed_exit_code'] == 0, 'CANDIDATE_CHECK_FAILED')
        artifact(check['log']['path'], check['log']['sha256'])
    expected_tests = {
        'router-regression': [
            'unsupported_math_and_token_behavior_are_excluded_from_public_candidate_scope',
            'every_unsearchable_source_row_has_an_explicit_candidate_exclusion',
            'token_capability_exclusion_is_visible_and_restores_without_deleting_source',
            'live_http_admission_refresh_revokes_cached_quotes_and_restores_both_directions',
            'changing_admission_cannot_bypass_an_explicit_scope_suspension'],
        'parser-domain': ['duplicate_registration_conflicts_reject_in_either_order_without_counting_identical_twice'],
    }
    for group, tests in expected_tests.items():
        text = (ROOT / checks[group]['log']['path']).read_text()
        for name in tests:
            require('test ' + name + ' ... ok' in text, 'CANDIDATE_REQUIRED_TEST_NOT_EXECUTED:' + name)
    red = OUT / 'core-candidate-scope-red-20261109.log'
    text = red.read_text()
    require('test result: FAILED' in text and 'unsupported-math-pool' in text and 'unsupported-token-pool' in text,
            'CANDIDATE_ORIGINAL_COUNTEREXAMPLE_REQUIRED')
    process_path = OUT / 'core-pancake-bsc4-normal-process.json'
    process = json.loads(process_path.read_text())
    require(process.get('passed') is True and process['runtime_fingerprint'] == fingerprint
            and process['source_tree_sha256'] == fingerprint['source_tree_sha256']
            and process['actual_api_records'] == process['original_swaps'] == 13
            and process['actual_tick_migrations'] == 6
            and process['negative_controls'].get('passed') is True
            and process['negative_controls']['mutations'] == 13, 'CANDIDATE_NORMAL_PROCESS_REGRESSION')
    for path, digest in process['tool_inputs'].items():
        artifact(path, digest)
    for key in ['actual_api_input', 'model_projection', 'original_replay']:
        artifact(process[key]['path'], process[key]['sha256'])
    reference = ROOT / 'integration/test/contracts/pancake-bsc4-state.json'
    require(sha(reference) == process['reference_sha256'], 'CANDIDATE_PROCESS_REFERENCE_CHANGED')
    # This is a state-replica/API change. Bind the preserved original-model
    # authority instead of claiming that older global checks used this source.
    prior_path = OUT / 'core-bsc4-delivery-20261109.json'
    prior = json.loads(prior_path.read_text())
    require(prior.get('passed') is True, 'CANDIDATE_PRIOR_PROTOCOL_VALIDATION_REQUIRED')
    authority_path = ROOT / prior['acceptance']['path']
    artifact(authority_path, prior['acceptance']['sha256'])
    authority = json.loads(authority_path.read_text())
    original_process = artifact('integration/artifacts/local/core-bsc4-frozen-phase41-normal-process-20261109.json',
                                authority['process_report']['sha256'])
    require(authority['fixture']['sha256'] == sha(reference), 'CANDIDATE_PROTOCOL_REFERENCE_CHANGED')
    for role in ['chain-parser-go/bin/chain-parser-go', 'chain-parser-go/bin/fixture-parser-go']:
        require(fingerprint['binaries'][role] == authority['runtime_fingerprint']['binaries'][role],
                'CANDIDATE_UNCHANGED_PARSER_BINARY_REQUIRED:' + role)
    require(runtime_fingerprint() == fingerprint, 'CANDIDATE_FINAL_RUNTIME_CHANGED')
    frozen_process = OUT / ('core-candidate-scope-process-' + fingerprint['source_tree_sha256'][:12] + '.json')
    process_bytes = process_path.read_bytes()
    if frozen_process.exists():
        require(frozen_process.read_bytes() == process_bytes, 'CANDIDATE_FROZEN_RECEIPT_CHANGED')
    else:
        with frozen_process.open('xb') as stream:
            stream.write(process_bytes)
    report = dict(schema_version=1, passed=True, state='LOCAL_SCOPE_VERIFIED',
                  source_tree_sha256=fingerprint['source_tree_sha256'], runtime_fingerprint=fingerprint,
                  authority='current full-bank capability exclusions, actual graph edges and real HTTP handlers; current BSC4 normal-service original-program replay retained separately',
                  original_failure=artifact(red), validation=artifact(validation_path),
                  expected_executed_tests=expected_tests, process_regression=artifact(frozen_process),
                  previous_protocol_delivery=artifact(prior_path), previous_protocol_process_backup=original_process,
                  router_regression_test_executions=checks['router-regression']['passed_test_executions'],
                  parser_domain_test_executions=checks['parser-domain']['passed_test_executions'],
                  actual_original_program_swaps=13, actual_tick_migrations=6,
                  limitations=['capability exclusion is not implementation of unknown protocols or unsupported Token behavior',
                               'this scope fixes candidate disclosure, not extra protocol support or global solver guarantees',
                               'net execution cost, ExactOut and remaining protocol/chain models still incomplete'],
                  measured_at_unix_ms=int(time.time()*1000), all_requested_features_complete=False,
                  production_ready=False)
    write_result(OUT / 'core-candidate-scope-local-20261109.json', report)
    return report


if __name__ == '__main__':
    argparse.ArgumentParser(description=__doc__).parse_args()
    print(json.dumps(run(), indent=2))
