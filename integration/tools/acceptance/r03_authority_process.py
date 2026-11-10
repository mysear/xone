#!/usr/bin/env python3
"""Actual isolated PG/Kafka + tonic public-style stream + normal Rust owner.

Build before starting infrastructure. Compile/lock time has its own deadline;
the live service scenario keeps a separate finite execution deadline.
"""
from pathlib import Path
import hashlib
import json
import os
import subprocess
import time

from infrastructure import Infrastructure, ROOT
from weighted_evidence import source_hash, write_result

OUT = ROOT / 'integration/artifacts/local'
TARGET = OUT / 'r03-normal-authority-process.json'
TEST = 'actual_postgres_kafka_scope_publish_revoke_restart_and_fence'


def executable_from_artifacts(text):
    paths = []
    for line in text.splitlines():
        try:
            record = json.loads(line)
        except json.JSONDecodeError:
            continue
        if (record.get('reason') == 'compiler-artifact'
                and record.get('target', {}).get('name') == 'normal_yellowstone'
                and record.get('executable')):
            if record.get('target', {}).get('kind') != ['test'] or record.get('profile', {}).get('test') is not True:
                raise RuntimeError('authority artifact is not an integration test executable')
            if 'external-authority-tests' not in record.get('features', []):
                raise RuntimeError('authority feature missing from compiled target')
            paths.append(Path(record['executable']).resolve())
    if len(paths) != 1 or not paths[0].is_relative_to((ROOT / 'chain-parser-rust/target').resolve()):
        raise RuntimeError('actual authority executable identity missing or ambiguous')
    if not paths[0].is_file():
        raise RuntimeError('compiled authority executable unavailable')
    return paths[0]


def run():
    OUT.mkdir(parents=True, exist_ok=True)
    report = {
        'schema_version': 1, 'state': 'RUNNING', 'passed': False,
        'all_requested_features_complete': False, 'production_ready': False,
        'authority': 'controlled tonic source; actual normal Parser/RocksDB/PostgreSQL/Kafka',
        'source_tree_sha256': source_hash(),
        'started_at_unix_ms': int(time.time() * 1000), 'test': TEST,
        'runner_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
    }
    write_result(TARGET, report)
    try:
        env = os.environ.copy()
        env['PATH'] = str(Path.home() / '.cargo/bin') + os.pathsep + env['PATH']
        build_log = OUT / 'r03-normal-authority-build.log'
        report['phase'] = 'build'
        report['build_command'] = [
            str(Path.home() / '.cargo/bin/cargo'), '+1.90.0', 'test', '--locked',
            '-p', 'xone-solana-ingest', '--features', 'external-authority-tests',
            '--test', 'normal_yellowstone', '--no-run', '--message-format=json',
        ]
        write_result(TARGET, report)
        with build_log.open('w') as stream:
            result = subprocess.run(report['build_command'], cwd=ROOT / 'chain-parser-rust',
                                    env=env, stdout=stream, stderr=subprocess.STDOUT,
                                    timeout=1800)
        report['build_exit_code'] = result.returncode
        report['build_log_sha256'] = hashlib.sha256(build_log.read_bytes()).hexdigest()
        if result.returncode:
            raise RuntimeError(f'authority test build failed; see {build_log}')
        binary = executable_from_artifacts(build_log.read_text())
        report['test_binary'] = str(binary.relative_to(ROOT))
        report['test_binary_sha256'] = hashlib.sha256(binary.read_bytes()).hexdigest()
        report['phase'] = 'scenario'
        write_result(TARGET, report)
        with Infrastructure() as infra:
            env['XONE_AUTHORITY_TEST_DSN_FILE'] = str(infra.dsnfile)
            env['XONE_AUTHORITY_TEST_BROKER'] = infra.broker
            log = OUT / 'r03-normal-authority-process.log'
            report['scenario_command'] = [str(binary), TEST, '--exact', '--nocapture']
            with log.open('w') as stream:
                result = subprocess.run(report['scenario_command'],
                                        cwd=ROOT / 'chain-parser-rust/crates/solana-ingest',
                                        env=env, stdout=stream, stderr=subprocess.STDOUT,
                                        timeout=240)
            report['exit_code'] = result.returncode
            report['log'] = str(log.relative_to(ROOT))
            text = log.read_text()
            report['log_sha256'] = hashlib.sha256(log.read_bytes()).hexdigest()
            if result.returncode:
                raise RuntimeError(f'normal authority test failed; see {log}')
            if 'test result: ok. 1 passed; 0 failed; 0 ignored;' not in text:
                raise RuntimeError('targeted authority case did not execute exactly once')
            report['recovery_cases'] = [json.loads(line)['recovery_case']
                                        for line in text.splitlines()
                                        if line.startswith('{"recovery_case":')]
            if [(case['equivalent_offline_hours'], case['empty_owner_kv'])
                    for case in report['recovery_cases']] != [(1, False), (24, False), (720, False), (720, True)]:
                raise RuntimeError('Solana large-gap or empty-owner recovery case missing')
            if hashlib.sha256(binary.read_bytes()).hexdigest() != report['test_binary_sha256']:
                raise RuntimeError('compiled authority executable changed during scenario')
            report['source_tree_sha256_after'] = source_hash()
            if report['source_tree_sha256_after'] != report['source_tree_sha256']:
                raise RuntimeError('source changed during authority test; freeze and rerun')
            report.update(passed=True, state='LOCAL_AUTHORITY_PROCESS_SCOPE_VERIFIED',
                          phase='complete', completed_at_unix_ms=int(time.time() * 1000))
            write_result(TARGET, report)
            print(json.dumps({'passed': True, 'all_requested_features_complete': False,
                              'scope': 'normal stream PG/Kafka owner publish/revoke/restart/fence and bounded HEAD_FIRST gaps'}))
            return report
    except BaseException as error:
        report.update(state='FAILED', error=str(error), error_type=type(error).__name__,
                      completed_at_unix_ms=int(time.time() * 1000))
        write_result(TARGET, report)
        raise


if __name__ == '__main__':
    run()
