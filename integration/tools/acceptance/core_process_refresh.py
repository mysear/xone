#!/usr/bin/env python3
"""Freeze final component receipts and repeat current calculation process gates."""
from pathlib import Path
import argparse
import json
import re
import shutil
import subprocess
import time

from weighted_evidence import OUT, ROOT, require, runtime_fingerprint, sha, source_hash, write_result



def validate_gate_receipt(name, value, source):
    if name in ['slipstream', 'velodrome', 'lfj', 'pancake']:
        require(value.get('passed') is True, 'PROCESS_GATE_RECEIPT_FAILED:' + name)
        require(value.get('source_tree_sha256') == source, 'PROCESS_GATE_SOURCE_MISMATCH:' + name)
    else:
        require(name in ['polygon', 'arbitrum'], 'UNKNOWN_PROCESS_GATE')
        require(value.get('status') == 'QUALIFIED_FINITE_NORMAL_REFERENCE_DIFFERENTIAL',
                'PUBLIC_DIFFERENTIAL_STATUS:' + name)
        require(value.get('gross_matches_original_VM_token_delta') is True
                and value.get('normal_quote_observed') is True
                and value.get('normal_parser_typed_baseline_injected') is False
                and value.get('normal_replay_public_network_requests') == 0,
                'PUBLIC_DIFFERENTIAL_AUTHORITY:' + name)
        quantities = value.get('quantities', {})
        require(isinstance(quantities.get('router_pool_gross'), str)
                and quantities['router_pool_gross'] == quantities.get('reference_recipient_token_delta'),
                'PUBLIC_DIFFERENTIAL_QUANTITY:' + name)


def public_gate_destination(stem, name):
    require(re.fullmatch(r'core-[a-z0-9-]{1,100}', stem), 'OWNED_REFRESH_STEM_REQUIRED')
    require(name in ['polygon', 'arbitrum'], 'UNKNOWN_PUBLIC_GATE')
    return OUT / ('r01-public-' + stem + '-' + name)


def reusable_gate(previous, name, fingerprint):
    require(previous.get('source_tree_sha256') == fingerprint['source_tree_sha256']
            and previous.get('runtime_fingerprint') == fingerprint, 'REUSED_RUNTIME_CHANGED')
    entries = [entry for entry in previous.get('checks', []) if entry.get('name') == name]
    if not entries or entries[0].get('passed') is not True:
        return None
    require(len(entries) == 1, 'DUPLICATE_REUSED_GATE:' + name)
    entry = entries[0]
    require(entry.get('observed_exit_code') == 0, 'REUSED_GATE_EXIT:' + name)
    for path_key, digest_key in [('log', 'log_sha256'), ('report', 'report_sha256')]:
        path = (ROOT / entry[path_key]).resolve()
        require(path.is_relative_to(OUT.resolve()) and sha(path) == entry[digest_key],
                'REUSED_GATE_ARTIFACT_CHANGED:' + name)
    validate_gate_receipt(name, json.loads((ROOT / entry['report']).read_text()),
                          fingerprint['source_tree_sha256'])
    return dict(entry)


def run(stem, wait_seconds, reuse_completed_from=None, component_stem='core-context-final', include_pancake_lm=False):
    require(re.fullmatch(r'core-[a-z0-9-]{1,100}', stem), 'OWNED_REFRESH_STEM_REQUIRED')
    target = OUT / (stem + '.json')
    require(not target.exists(), 'NEW_REFRESH_OUTPUT_REQUIRED')
    before = source_hash()
    report = dict(schema_version=1, state='WAITING_COMPONENT_CHECKS', passed=False,
                  source_tree_sha256=before, started_at_unix_ms=int(time.time()*1000),
                  all_requested_features_complete=False, production_ready=False,
                  full_component_checks={}, checks=[])
    write_result(target, report)
    deadline = time.monotonic() + wait_seconds
    try:
        while True:
            pending = []
            for service in ['go', 'rust', 'router']:
                receipt = OUT / (component_stem + '-' + service + '.json')
                value = json.loads(receipt.read_text())
                require(value['source_tree_sha256'] == before, 'COMPONENT_SOURCE_MISMATCH:' + service)
                if value['state'] == 'RUNNING':
                    pending.append(service)
                    continue
                require(value.get('passed') is True and value.get('observed_exit_code') == 0,
                        'COMPONENT_CHECK_FAILED:' + service)
                require(value.get('source_tree_sha256_after') == before,
                        'COMPONENT_CHANGED_SOURCE:' + service)
                log = ROOT / value['log']['path']
                require(log.stat().st_size == value['log']['bytes'] and sha(log) == value['log']['sha256'],
                        'COMPONENT_LOG_CHANGED:' + service)
                report['full_component_checks'][service] = dict(path=str(receipt.relative_to(ROOT)), sha256=sha(receipt))
            write_result(target, report)
            if not pending:
                break
            require(time.monotonic() < deadline, 'COMPONENT_CHECK_DEADLINE')
            time.sleep(5)
        require(source_hash() == before, 'SOURCE_CHANGED_BEFORE_BUILD')
        report['state'] = 'BUILDING_FINAL_TOOLS'
        write_result(target, report)
        build_log = OUT / (stem + '-build.log')
        with build_log.open('x') as stream:
            build_targets = ['build', 'debug-build', 'lfj-v21-model-projector',
                             'slipstream-context-model-projector']
            if include_pancake_lm:
                build_targets.append('pancake-v3-lm-model-projector')
            build = subprocess.run(['make', '-C', 'integration'] + build_targets,
                                   cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT)
        report['build'] = dict(observed_exit_code=build.returncode, log=str(build_log.relative_to(ROOT)), sha256=sha(build_log))
        require(build.returncode == 0, 'FINAL_TOOLS_BUILD_FAILED')
        fingerprint = runtime_fingerprint()
        require(fingerprint['source_tree_sha256'] == before, 'SOURCE_CHANGED_DURING_BUILD')
        previous = json.loads(reuse_completed_from.read_text()) if reuse_completed_from else None
        report['runtime_fingerprint'] = fingerprint
        report['state'] = 'REFRESHING_PROCESS_GATES'
        write_result(target, report)
        specs = [(name, ['python3', 'integration/tools/acceptance/' + script], OUT / report_name)
                 for name, script, report_name in [
                     ('slipstream', 'slipstream_context_smoke.py', 'core-slipstream-context-normal-process.json'),
                     ('velodrome', 'velodrome_smoke.py', 'core-velodrome-process.json'),
                     ('lfj', 'lfj_smoke.py', 'core-lfj-normal-process.json')]]
        if include_pancake_lm:
            specs.append(('pancake', ['python3', 'integration/tools/acceptance/pancake_lm_smoke.py'],
                          OUT / 'core-pancake-lm-normal-process.json'))
        for name, case, probe in [
                ('polygon', 'polygon-uniswap-v3-refresh-04', 'r01-getter-polygon-refresh-04-closure'),
                ('arbitrum', 'arbitrum-uniswap-v3-refresh-07', 'r01-getter-arbitrum-refresh-07-closure')]:
            destination = public_gate_destination(stem, name)
            specs.append((name, ['python3', 'integration/tools/acceptance/r01_public_catalog_evm_normal.py',
                                '--case-directory', 'integration/test/source/deployment-catalog/evm-normal-v4/' + case,
                                '--probe', str((OUT / probe / 'getter-execution.json').relative_to(ROOT)),
                                '--out', str(destination), '--timeout-seconds', '90'],
                          destination / 'bound-report.json'))
        for name, command, receipt in specs:
            reused = reusable_gate(previous, name, fingerprint) if previous else None
            if reused is not None:
                reused['reused_from'] = str(reuse_completed_from)
                report['checks'].append(reused)
                write_result(target, report)
                continue
            require(runtime_fingerprint() == fingerprint, 'RUNTIME_CHANGED_BEFORE_GATE:' + name)
            started = time.monotonic()
            log = OUT / (stem + '-' + name + '.log')
            with log.open('x') as stream:
                process = subprocess.run(command, cwd=ROOT, stdout=stream, stderr=subprocess.STDOUT)
            entry = dict(name=name, command=command, observed_exit_code=process.returncode,
                         elapsed_ms=int((time.monotonic()-started)*1000), log=str(log.relative_to(ROOT)),
                         log_sha256=sha(log), passed=False)
            report['checks'].append(entry)
            write_result(target, report)
            require(process.returncode == 0, 'PROCESS_GATE_FAILED:' + name)
            value = json.loads(receipt.read_text())
            validate_gate_receipt(name, value, before)
            require(runtime_fingerprint() == fingerprint, 'RUNTIME_CHANGED_DURING_GATE:' + name)
            if name in ['slipstream', 'velodrome', 'lfj', 'pancake']:
                require(value.get('source_tree_sha256') == before, 'PROCESS_GATE_SOURCE_MISMATCH:' + name)
                frozen = OUT / (stem + '-' + name + '-receipt.json')
                require(not frozen.exists(), 'NEW_FROZEN_RECEIPT_REQUIRED')
                shutil.copyfile(receipt, frozen)
                receipt = frozen
            entry.update(passed=True, report=str(receipt.relative_to(ROOT)), report_sha256=sha(receipt))
            write_result(target, report)
        report.update(state='LOCAL_CURRENT_PROCESS_SCOPES_VERIFIED', passed=True,
                      completed_at_unix_ms=int(time.time()*1000))
    except BaseException as error:
        report.update(state='FAILED', error=str(error), error_type=type(error).__name__,
                      completed_at_unix_ms=int(time.time()*1000))
        write_result(target, report)
        raise
    write_result(target, report)
    return report


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--stem', required=True)
    parser.add_argument('--wait-seconds', type=int, default=10800)
    parser.add_argument('--reuse-completed-from', type=Path)
    parser.add_argument('--component-stem', default='core-context-final')
    parser.add_argument('--include-pancake-lm', action='store_true')
    args = parser.parse_args()
    print(json.dumps(run(args.stem, args.wait_seconds, args.reuse_completed_from, args.component_stem, args.include_pancake_lm), indent=2))
