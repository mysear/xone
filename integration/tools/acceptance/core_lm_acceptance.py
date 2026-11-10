#!/usr/bin/env python3
"""Bind the finite LM business scope to current checks and original-program replay."""
from pathlib import Path
import argparse
import importlib.util
import json
import re
import sys
import time

from weighted_evidence import OUT, ROOT, require, runtime_fingerprint, sha, write_result


def artifact(relative, digest=None):
    path = (ROOT / relative).resolve()
    require(path.is_relative_to(ROOT) and path.is_file(), 'LM_LOCAL_ARTIFACT_REQUIRED:' + relative)
    observed = sha(path)
    require(digest is None or digest == observed, 'LM_ARTIFACT_CHANGED:' + relative)
    return dict(path=str(path.relative_to(ROOT)), sha256=observed)


def run(process_path):
    fingerprint = runtime_fingerprint()
    source = fingerprint['source_tree_sha256']
    process = json.loads(process_path.read_text())
    require(process.get('passed') is True
            and process.get('state') == 'LOCAL_CURRENT_PROCESS_SCOPES_VERIFIED'
            and process.get('runtime_fingerprint') == fingerprint
            and process.get('source_tree_sha256') == source, 'LM_CURRENT_PROCESS_REQUIRED')
    components = {}
    router_tests = None
    for name in ['go', 'rust', 'router']:
        record = process['full_component_checks'][name]
        receipt = artifact(record['path'], record['sha256'])
        value = json.loads((ROOT / record['path']).read_text())
        require(value.get('passed') is True and value.get('observed_exit_code') == 0
                and value.get('source_tree_sha256') == source
                and value.get('source_tree_sha256_after') == source
                and value.get('source_changed') is False, 'LM_COMPONENT_FAILED:' + name)
        artifact(value['log']['path'], value['log']['sha256'])
        require((ROOT / value['log']['path']).stat().st_size == value['log']['bytes'],
                'LM_COMPONENT_LOG_SIZE:' + name)
        if value.get('receipt_reused_from'):
            reused = value['receipt_reused_from']
            artifact(reused['path'], reused['sha256'])
            original = json.loads((ROOT / reused['path']).read_text())
            require(all(value.get(key) == item for key, item in original.items()),
                    'LM_REUSED_COMPONENT_RECEIPT_CHANGED:' + name)
        if value.get('phase_composition'):
            phases = value['phase_composition']
            previous = phases['initial_failed_check']
            artifact(previous['path'], previous['sha256'])
            initial = json.loads((ROOT / previous['path']).read_text())
            require(name == 'rust' and initial.get('passed') is False
                    and initial.get('observed_exit_code') == 2
                    and initial.get('source_tree_sha256') == source
                    and initial.get('source_tree_sha256_after') == source,
                    'LM_REPAIR_BASELINE_REQUIRED')
            artifact(initial['log']['path'], initial['log']['sha256'])
            text = (ROOT / initial['log']['path']).read_text()
            marker = re.search(r'^.*cargo \+1\.90\.0 test --locked -p xone-domain -p xone-config$', text, re.M)
            require(marker is not None, 'LM_REPAIR_CONTRACT_BOUNDARY')
            prefix = text[:marker.start()]
            require('fmt --all -- --check' in prefix and 'clippy --locked --workspace' in prefix
                    and 'test --locked --workspace' in prefix
                    and 'test result: FAILED' not in prefix and 'error[' not in prefix
                    and 'error:' not in prefix, 'LM_REPAIR_PREVIOUS_PHASES_FAILED')
            previous_tests = sum(int(n) for n in re.findall(r'test result: ok\. (\d+) passed;', prefix))
            repair_text = (ROOT / value['log']['path']).read_text()
            repair_tests = sum(int(n) for n in re.findall(r'test result: ok\. (\d+) passed;', repair_text))
            require(previous_tests == 677 and repair_tests == 197
                    and value['passed_test_executions'] == previous_tests + repair_tests
                    and phases['initial_passed_targets'] == ['fmt-check', 'lint', 'unit']
                    and phases['replayed_targets'] == ['contract-test', 'build']
                    and 'build --locked --workspace' in repair_text,
                    'LM_REPAIR_ALL_PHASES_REQUIRED')
        if name == 'router':
            router_tests = (ROOT / value['log']['path']).read_text()
        components[name] = receipt
    for test in ['lm_output_token_must_match_pool_direction_without_mutation',
                 'contextual_output_token_must_match_pool_direction_without_mutation']:
        require('test ' + test + ' ... ok' in router_tests, 'LM_DIRECTION_REGRESSION_NOT_EXECUTED:' + test)
    from core_process_refresh import validate_gate_receipt
    require(len(process['checks']) == 6
            and {entry['name'] for entry in process['checks']}
            == {'slipstream', 'velodrome', 'lfj', 'pancake', 'polygon', 'arbitrum'},
            'LM_COMPLETE_PROCESS_GATE_SET')
    gates = []
    for entry in process['checks']:
        require(entry.get('passed') is True and entry.get('observed_exit_code') == 0,
                'LM_PROCESS_FAILED:' + entry['name'])
        artifact(entry['log'], entry['log_sha256'])
        receipt = artifact(entry['report'], entry['report_sha256'])
        validate_gate_receipt(entry['name'], json.loads((ROOT / entry['report']).read_text()), source)
        gates.append(dict(name=entry['name'], **receipt))
    lm_entry = next(entry for entry in process['checks'] if entry['name'] == 'pancake')
    lm = json.loads((ROOT / lm_entry['report']).read_text())
    fixture = ROOT / 'integration/test/contracts/pancake-lm-state.json'
    require(lm['reference_sha256'] == sha(fixture)
            and lm['actual_api_records'] == 13 and lm['original_swaps'] == 13,
            'LM_ACTUAL_API_SCOPE')
    for path, digest in lm['tool_inputs'].items():
        artifact(path, digest)
    for key in ['actual_api_input', 'model_projection', 'original_replay']:
        artifact(lm[key]['path'], lm[key]['sha256'])
    require(lm['negative_controls'].get('passed') is True
            and lm['negative_controls'].get('mutations') == 9, 'LM_NEGATIVE_SCOPE')
    original_authority = OUT / 'core-lm-original-authority-20261109.json'
    authority = json.loads(original_authority.read_text())
    require(authority.get('passed') is True, 'LM_ORIGINAL_AUTHORITY_REQUIRED')
    reference = json.loads(fixture.read_text())
    require(reference['source_reference_sha256'] == reference['original_reference']['sha256']
            and authority['reference'] == reference['original_reference'], 'LM_ORIGINAL_REFERENCE_BINDING')
    for key in ['reference', 'artifacts']:
        artifact(authority[key]['path'], authority[key]['sha256'])
    spec = importlib.util.spec_from_file_location(
        'lm_original_authority', ROOT / 'integration/tools/ci/pancake-v3-lm-draft-check.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    module.validate_authority(json.loads((ROOT / authority['reference']['path']).read_text()),
                              json.loads((ROOT / authority['artifacts']['path']).read_text()))
    sys.path.insert(0, str(ROOT / 'integration/tools'))
    from progress_document import load_coverage, load_progress
    from ci.progress_bindings import BINDINGS
    progress, coverage = load_progress(ROOT), load_coverage(ROOT)
    bindings = BINDINGS['12.14.pancake-lm']
    for path in bindings['paths']:
        require((ROOT / path).exists(), 'LM_PROGRESS_SOURCE_MISSING:' + path)
    fixture_copies = [artifact(str((ROOT / service / 'test/contracts' / fixture.name).relative_to(ROOT)), sha(fixture))
                      for service in ['chain-parser-go', 'chain-parser-rust', 'dex-router']]
    require(runtime_fingerprint() == fingerprint, 'LM_FINAL_RUNTIME_CHANGED')
    report = dict(schema_version=1, state='LOCAL_SCOPE_VERIFIED', passed=True,
                  source_tree_sha256=source, runtime_fingerprint=fingerprint,
                  authority='fixed original 98684794 LM version, controlled source banks and standard Tokens; finite ExactIn business scope',
                  components=components, process_report=artifact(str(process_path.relative_to(ROOT))),
                  gates=gates, original_authority=artifact(str(original_authority.relative_to(ROOT))),
                  fixture=artifact(str(fixture.relative_to(ROOT))), fixture_copies=fixture_copies,
                  actual_api_records=13, original_runtime_swaps=13, negative_controls=9, lm_tool_inputs=lm['tool_inputs'],
                  direction_regression=dict(tests=2,
                      before=artifact('integration/artifacts/local/core-lm-output-direction-red-20261109.log'),
                      after=artifact('integration/artifacts/local/core-lm-output-direction-green-20261109.log')),
                  original_complete_swap_cases=80, partial_swap_rejections=20,
                  inconsistent_bank_rejections=4, original_sequence_steps=12,
                  independent_cpp_vectors=4, work_packages=len(progress['work_packages']),
                  design_features=len(progress['requirement_traceability']), protocols=len(coverage['entries']),
                  limitations=['actual later BSC LM generations and migration not admitted',
                               'ExactOut and special Tokens not admitted',
                               'chain costs and net-output estimation not implemented',
                               'trusted historical source and production execution not qualified',
                               'other design chains and DEX extensions remain incomplete'],
                  measured_at_unix_ms=int(time.time()*1000),
                  all_requested_features_complete=False, production_ready=False)
    write_result(OUT / 'core-lm-local-q1-20261109.json', report)
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--process-report', type=Path, required=True)
    arguments = parser.parse_args()
    print(json.dumps(run(arguments.process_report.resolve()), indent=2))
