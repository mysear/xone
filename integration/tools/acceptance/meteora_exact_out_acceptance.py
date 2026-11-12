#!/usr/bin/env python3
"""Bind final code, tools, raw original captures and finite ExactOut checks."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

from weighted_evidence import ROOT, OUT, source_hash, sha, write_result
from validate_meteora_exact_out import TARGET as VALIDATION
from meteora_exact_out_smoke import TARGET as PROCESS

TARGET = OUT / 'core-meteora-exact-out-local-q1-20261111.json'


def require(condition, message):
    if not condition:
        raise ValueError('METEORA_EXACT_OUT_ACCEPTANCE:' + message)


def artifact(item):
    path = (ROOT / item['path']).resolve()
    require(path.is_relative_to(ROOT), 'artifact-path')
    require(sha(path) == item['sha256'], 'artifact-digest:' + item['path'])
    return path


def main():
    report = json.loads(VALIDATION.read_bytes())
    require(report.get('passed') is True and report['state'] == 'LOCAL_SCOPE_VERIFIED', 'incomplete-validation')
    require(len(report['checks']) == 19 and all(c['exit_code'] == 0 for c in report['checks']), 'finite-check-coverage')
    source = source_hash()
    require(source == report['source_tree_sha256'] == report['source_tree_sha256_after'], 'code-changed')
    require(sha(Path(__file__).with_name('validate_meteora_exact_out.py')) == report['tool_sha256'], 'validation-tool-changed')
    for check in report['checks']:
        artifact(check['log'])
    receipt = artifact(report['process_receipt'])
    process = json.loads(receipt.read_bytes())
    require(process.get('passed') is True and source == process['source_tree_sha256'] == process['source_tree_sha256_after'], 'process-code-changed')
    require(process['api_records'] == process['original_sbf_swaps'] == process['input_limit_controls'] == 43 and process['source_faults'] == 9, 'normal-process-coverage')
    for path, expected in (process['inputs'] | process['helpers'] | process['binaries']).items():
        require(sha(ROOT / path) == expected, 'normal-process-dependency-changed:' + path)
    captures = {name: json.loads(artifact(item).read_bytes()) for name, item in process['artifacts'].items()}
    for name, capture in captures.items():
        require(len(capture['records']) == 43, 'capture-coverage:' + name)
    public, model, natives = (captures[name]['records'] for name in ['captured_plans', 'model_projection', 'original_capture'])
    controls = 0
    for request, projection, native in zip(public, model, natives):
        require(request['case_index'] == projection['case_index'] == native['case_index'], 'capture-case-alignment')
        require(request['execution_plan']['plan_hash'] == projection['plan_hash'] == native['plan_hash'], 'capture-plan-alignment')
        require(len(projection['steps']) == 1, 'original-template')
        step, raw, negative = projection['steps'][0], native['native'], native['input_minus_one']
        require(raw['passed'] is True and raw['kind'] == 'EXACT_OUT' and raw['payer_input_delta'] == step['amount_in'] and raw['recipient_output_delta'] == step['amount_out'] == request['request']['buyAmount'], 'actual-original-funds')
        require(negative['passed'] is False and 'err' in negative['native'] and negative['before'] == raw['before'] == negative['after'] and negative['payer_input_delta'] == negative['recipient_output_delta'] == '0', 'original-input-minus-one')
        controls += 1
    # The verifier was hardened during the run. Reexecute the final tool version
    # and its ordinary/optimized tampering controls before recording acceptance.
    supplemental = []
    commands = [('final-reference', [sys.executable, 'integration/tools/acceptance/meteora_exact_out_reference.py', '--reexecute'])]
    test = ['-m', 'unittest', 'discover', '-s', 'integration/tools/acceptance/tests', '-p', 'test_meteora_exact_out_reference.py']
    commands.extend([('final-reference-controls', [sys.executable] + test), ('final-reference-controls-optimized', [sys.executable, '-OO'] + test)])
    tag = __import__('secrets').token_hex(6)
    for name, command in commands:
        path = OUT / f'meteora-xo-acceptance-{tag}-{name}.log'
        with path.open('w') as output:
            result = subprocess.run(command, cwd=ROOT, stdout=output, stderr=subprocess.STDOUT)
        require(result.returncode == 0, 'supplemental-failed:' + str(path))
        supplemental.append(dict(name=name, exit_code=result.returncode, log=dict(path=str(path.relative_to(ROOT)), sha256=sha(path))))
    require(source_hash() == source, 'source-changed-during-final-check')
    paths = [Path(__file__), Path(__file__).with_name('meteora_exact_out_reference.py'), Path(__file__).parent / 'tests/test_meteora_exact_out_reference.py', Path(__file__).with_name('meteora_exact_out_smoke.py'), ROOT / 'integration/tools/ci/meteora-exact-out-reference.py']
    result = dict(schema_version=1, passed=True, state='LOCAL_SCOPE_VERIFIED', source_tree_sha256=source,
                  validation=dict(path=str(VALIDATION.relative_to(ROOT)), sha256=sha(VALIDATION)),
                  process_receipt=report['process_receipt'], final_tools={str(p.relative_to(ROOT)): sha(p) for p in paths},
                  supplemental_checks=supplemental, finite_validation_checks=19, rust_test_executions=report['rust_test_executions'],
                  original_targets=164, original_successful_initial_targets=142, original_rejected_targets=22,
                  original_sequence_followups=20, original_vault_dust_steps=25, api_plans=43, actual_plan_sbf_swaps=43,
                  actual_plan_hard_input_controls=controls, source_faults=9, all_requested_features_complete=False, production_ready=False,
                  authority='controlled fixed MM-only local SBF banks and finite indicative same-source routing; no canonical public deployment, public execution, chain-cost or global optimality claim')
    write_result(TARGET, result)
    print(json.dumps({k: result[k] for k in ['passed', 'state', 'source_tree_sha256', 'rust_test_executions', 'api_plans']}))


if __name__ == '__main__':
    main()
