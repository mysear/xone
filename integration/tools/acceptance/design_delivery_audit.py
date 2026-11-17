#!/usr/bin/env python3
"""Audit the configured delivery boundary; never promote local tests to production.

Only repository production configuration and bounded local Kafka metadata are
inspected. Credential contents, provider URLs and DSNs are never copied.
"""
import hashlib
import json
import os
from pathlib import Path
import re
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import uuid

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / 'integration/tools'))
sys.path.insert(0, str(ROOT / 'integration/tools/acceptance'))
from progress_document import load_progress, load_coverage
import validate_cache_reclamation as shared

SERVICES = ('chain-parser-go', 'chain-parser-rust', 'dex-router')
BASE = ROOT / 'integration/artifacts/local/design-delivery-audit'


def config_facts(path):
    raw = path.read_bytes()
    config = json.loads(raw)
    kafka = config.get('journal', {}).get('kafka', {})
    control = config.get('control', {})
    chains = config.get('chains', [])
    return dict(
        sha256=hashlib.sha256(raw).hexdigest(),
        chain_count=len(chains),
        missing_rpc_chain_count=sum(not c.get('rpc_urls') for c in chains),
        placeholder_genesis_count=sum(
            not c.get('genesis_hash', '').removeprefix('0x').strip('0') for c in chains),
        kafka_broker_count=len(kafka.get('brokers', [])),
        kafka_security_protocol=kafka.get('security_protocol'),
        control_backend=control.get('backend'),
        control_dsn_declared=bool(control.get('dsn_file')),
        evm_preparation_chain_count=len(config.get('api', {}).get('evm_preparation', {})),
    )


def external_blockers(configs):
    blockers = []
    missing_rpc = [name for name, facts in configs.items()
                   if facts['missing_rpc_chain_count'] or facts['placeholder_genesis_count']]
    if missing_rpc:
        blockers.append(dict(
            id='EXTERNAL_CHAIN_ACCEPTANCE_INPUTS', design_sections=['7', '9', '14.2', '18'],
            affected_services=missing_rpc,
            observed='Production files omit RPC sources or retain placeholder genesis identities',
            required_input='Operator-selected real chain/genesis and independently operated RPC/stream sources with declared authority and historical-state access',
            blocked_scope='Real-source, current-chain and independent-failure-domain acceptance',
        ))
    missing_infra = [name for name, facts in configs.items()
                     if not facts['kafka_broker_count']]
    if missing_infra:
        blockers.append(dict(
            id='EXTERNAL_DEPLOYMENT_ACCEPTANCE_ENVIRONMENT', design_sections=['18', '20', '28.25'],
            affected_services=missing_infra,
            observed='Production files declare no Kafka deployment endpoints',
            required_input='An actual acceptance deployment with declared regions/failure domains, access to its brokers/database/independent archive and an operator-approved load/failover scope',
            blocked_scope='Multi-zone failure, independently stored recovery and production SLO/RPO/RTO acceptance',
        ))
    return blockers


def local_kafka_facts(root):
    paths = list((root / 'integration/.runtime/infrastructure').glob('*/kafka.properties'))
    selected = sorted(paths, key=lambda p: p.stat().st_mtime, reverse=True)[:3]
    result = []
    for path in selected:
        raw = path.read_bytes()
        props = {}
        for line in raw.decode().splitlines():
            if '=' in line and not line.strip().startswith('#'):
                key, value = line.split('=', 1)
                props[key.strip()] = value.strip()
        listeners = props.get('listeners', '').split(',')
        result.append(dict(
            path=str(path.relative_to(root)), sha256=hashlib.sha256(raw).hexdigest(),
            combined_broker_controller=props.get('process.roles') == 'broker,controller',
            controller_voter_count=len([v for v in props.get('controller.quorum.voters', '').split(',') if v]),
            transaction_replication_factor=props.get('transaction.state.log.replication.factor'),
            transaction_min_isr=props.get('transaction.state.log.min.isr'),
            loopback_listeners=bool(props.get('listeners')) and all(
                '127.0.0.1' in v or 'localhost' in v or '[::1]' in v for v in listeners),
            authority='LOCAL_TEST_CONFIGURATION_ONLY',
        ))
    return result


def current_config_probe(binary, config, directory):
    source_hash = shared.sha_file(binary)
    target = directory / binary.name
    shutil.copy2(binary, target)
    if shared.sha_file(target) != source_hash or shared.sha_file(binary) != source_hash:
        raise RuntimeError('Current executable changed while staging')
    completed = subprocess.run([str(target), '--config', str(config), '--check-config'],
                               cwd=ROOT, capture_output=True, timeout=30)
    raw = completed.stdout + completed.stderr
    if len(raw) > 1024 * 1024:
        raise RuntimeError('Configuration probe output exceeded audit limit')
    error_codes = []
    valid = False
    for line in raw.decode(errors='replace').splitlines():
        try:
            value = json.loads(line)
        except ValueError:
            continue
        valid = valid or value.get('valid') is True
        code = value.get('fields', {}).get('error_code')
        if isinstance(code, str) and re.fullmatch('[A-Z0-9_]+', code):
            error_codes.append(code)
    return dict(binary_sha256=source_hash, exit_code=completed.returncode,
                valid=valid, error_codes=error_codes,
                output_sha256=hashlib.sha256(raw).hexdigest())


def main():
    out = BASE / 'runs' / uuid.uuid4().hex
    out.mkdir(parents=True, exist_ok=False)
    report = dict(passed=False, state='RUNNING', verification_scope='DESIGN_DELIVERY_BOUNDARY_AUDIT_ONLY',
                  source_tree_sha256=shared.fingerprint(), run_directory=str(out.relative_to(ROOT)),
                  started_ms=int(time.time()*1000), all_requested_features_complete=False,
                  production_ready=False, publicTransactionsSubmitted=False, checks=[])

    def save():
        text = json.dumps(report, indent=2, ensure_ascii=False)+'\n'
        (out/'report.json').write_text(text)
        (BASE/'report.tmp').write_text(text)
        (BASE/'report.tmp').replace(BASE/'report.json')

    def interrupted(signum, frame):
        raise KeyboardInterrupt('USER_INTERRUPTED')

    signal.signal(signal.SIGTERM, interrupted)
    signal.signal(signal.SIGINT, interrupted)
    env = dict(os.environ, PATH=str(Path.home()/'.cargo/bin')+os.pathsep+os.environ['PATH'])
    env.setdefault('CARGO_BUILD_JOBS', '2')
    env.setdefault('CARGO_TARGET_DIR', str(ROOT/'integration/.runtime/design-review-target'))
    cargo = ['cargo', '+1.90.0']
    commands = [
        ('audit-controls', [sys.executable, '-m', 'unittest', 'discover', '-s', 'integration/tools/acceptance/tests', '-p', 'test_design_delivery_audit.py']),
        ('audit-controls-optimized', [sys.executable, '-OO', '-m', 'unittest', 'discover', '-s', 'integration/tools/acceptance/tests', '-p', 'test_design_delivery_audit.py']),
        ('progress', [sys.executable, '-m', 'unittest', 'discover', '-s', 'integration/tools/ci/tests', '-p', 'test_progress_document.py']),
        ('contracts', [sys.executable, 'integration/tools/ci/contracts.py']),
        ('current-router-build', cargo+['build', '--locked', '--manifest-path', 'dex-router/Cargo.toml', '--bin', 'dex-router']),
    ]
    save()
    try:
        for name, command in commands:
            print('Running '+name, flush=True)
            with (out/(name+'.log')).open('wb') as log:
                child = subprocess.Popen(command, cwd=ROOT, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
                try:
                    code = child.wait()
                finally:
                    if child.poll() is None:
                        os.killpg(child.pid, signal.SIGTERM)
                        try: child.wait(timeout=2)
                        except subprocess.TimeoutExpired:
                            os.killpg(child.pid, signal.SIGKILL)
                            child.wait()
            report['checks'].append(dict(name=name, exit_code=code, log_sha256=shared.sha_file(out/(name+'.log'))))
            save()
            if code:
                raise RuntimeError(name+' failed')
        configs = {service: config_facts(ROOT/service/'config/config-prod.yaml') for service in SERVICES}
        report['production_configurations'] = configs
        report['external_blockers'] = external_blockers(configs)
        report['local_kafka_configurations'] = local_kafka_facts(ROOT)
        target = Path(env['CARGO_TARGET_DIR'])
        if not target.is_absolute(): target = ROOT/target
        with tempfile.TemporaryDirectory(prefix='xone-config-delivery-audit-') as directory:
            path = Path(directory)
            report['current_router_positive_control'] = current_config_probe(
                target/'debug/dex-router', ROOT/'dex-router/config/config-test.yaml', path)
            report['current_router_production_probe'] = current_config_probe(
                target/'debug/dex-router', ROOT/'dex-router/config/config-prod.yaml', path)
        if report['current_router_positive_control']['exit_code'] != 0 or not report['current_router_positive_control']['valid']:
            raise RuntimeError('Current executable failed positive configuration control')
        if report['external_blockers'] and report['current_router_production_probe']['exit_code'] == 0:
            raise RuntimeError('Production configuration accepted despite missing declared acceptance inputs')
        progress = load_progress(ROOT)
        report['design_work_packages'] = [dict(id=p['id'], title=p['title'], status=p['status'],
                full_acceptance_passed=p['full_acceptance_passed'], remaining=p['remaining']) for p in progress['work_packages']]
        report['full_work_package_acceptance_count'] = sum(p['full_acceptance_passed'] for p in progress['work_packages'])
        report['protocol_model_count'] = len(load_coverage(ROOT)['entries'])
        report['local_implementation_gaps'] = [
            dict(id='EVM_COMPLETE_STATE_AND_API', scope='Full RPC-backed execution DB, multi-pool/ExactOut prepared HTTP execution, chain hardfork/L2 costs and wallet lifecycle'),
            dict(id='PROTOCOL_COMPLETENESS', scope='Remaining protocol-specific mathematical/execution templates and token behaviors'),
            dict(id='STATE_SEARCH_AND_RECOVERY', scope='Full pool-state COW, search bounds/shared DAG and immutable remote checkpoint recovery'),
            dict(id='SERVICE_AND_DELIVERY', scope='Production TLS/SASL/ACL, tenant quotas, sustained analytics, CI/security artifacts and advanced solvers'),
        ]
        report['stop_condition'] = 'CONFIRMED_CONFIGURED_EXTERNAL_ACCEPTANCE_BLOCKER' if report['external_blockers'] else 'CONTINUE_LOCAL_IMPLEMENTATION_AND_ACCEPTANCE'
        if shared.fingerprint() != report['source_tree_sha256']:
            raise RuntimeError('Source changed during delivery audit')
        for service in SERVICES:
            if config_facts(ROOT/service/'config/config-prod.yaml') != configs[service]:
                raise RuntimeError('Production configuration changed during delivery audit')
        report.update(passed=True, state='LOCAL_SCOPE_VERIFIED')
    except KeyboardInterrupt:
        report.update(state='CANCELLED', error='USER_INTERRUPTED')
        raise
    except Exception as error:
        report.update(state='FAILED', error=str(error))
        raise
    finally:
        report['finished_ms'] = int(time.time()*1000)
        save()
    print(json.dumps({key: report[key] for key in ['state','stop_condition','full_work_package_acceptance_count','run_directory']}), flush=True)


if __name__ == '__main__':
    main()
