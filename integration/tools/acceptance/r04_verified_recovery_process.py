#!/usr/bin/env python3
"""Measure a cold quote process recovering a fixed Cancun scope from Go and Kafka."""
import argparse
import hashlib
import json
from pathlib import Path
import secrets
import subprocess
import tempfile
import time
from contextlib import ExitStack

from configuration import config_for, save_config, service_command
from infrastructure import Infrastructure, ROOT, port
from r01_r04_recovery_process import binary_fingerprint, owner_cursor, require
from smoke import http, wait, wait_scope
from weighted_evidence import source_hash, write_result


TOPIC = 'canonical.evm-31337.v1'


def stop(process):
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait()


def wait_manifest(directory, process):
    manifest = directory / 'manifest.json'
    deadline = time.monotonic() + 20
    while time.monotonic() < deadline:
        if manifest.is_file():
            return json.loads(manifest.read_text())
        if process.poll() is not None:
            raise RuntimeError(f'Geth fixture exited with {process.returncode}')
        time.sleep(0.05)
    raise RuntimeError('Geth fixture did not publish a manifest')


def pool_config(pool):
    advanced = pool['singleton_advanced']
    state = advanced['snapshot']
    return {
        'id': pool['pool_id'], 'protocol': pool['protocol'],
        'token0': pool['token0'], 'token1': pool['token1'],
        'fee_numerator': advanced['key_fee'], 'fee_denominator': 1_000_000,
        'program_id': '', 'accounts': [],
        'singleton_advanced': {
            'protocol': advanced['protocol'], 'manager': state['manager'],
            'expected_manager_code_sha256': state['manager_code_sha256'],
            'pool_id': state['pool_id'], 'currency0': advanced['currency0'],
            'currency1': advanced['currency1'], 'key_fee': advanced['key_fee'],
            'tick_spacing': state['core']['tick_spacing'], 'hooks': state['hooks'],
            'parameters': state['parameters'], 'hook_model': advanced['hook_model'],
            'tick_word_radius': 1, 'max_initialized_ticks': 256, 'max_steps': 256,
        },
    }


def make_config(name, directory, token, listen, owner, infra, fixture):
    parser = name == 'chain-parser-go'
    config = config_for(name, directory, token, listen)
    chain = fixture['quote']['chainKey']
    config['chains'] = [configured for configured in config['chains'] if configured['key'] == chain]
    config['chains'][0].update(
        genesis_hash=fixture['genesis'], rpc_urls=[fixture['rpc']], confirmation_tag='safe',
        pools=[pool_config(fixture['pool'])] if parser else [],
    )
    config['journal'].update(transport='kafka', source_urls=[owner], poll_interval_ms=100)
    config['journal']['kafka'].update(
        brokers=[infra.broker], topics={chain: TOPIC}, replica_id=name,
        transaction_timeout_ms=4000,
    )
    config['control'].update(
        backend='postgres' if parser else 'disabled', dsn_file=str(infra.dsnfile) if parser else '',
        instance_id=name, lease_ttl_ms=30000, renewal_ms=1000, min_commit_remaining_ms=7000,
    )
    config['history']['enabled'] = False
    config['startup'].update(max_baseline_age_ms=180000, max_tail_blocks=128)
    if not parser:
        config['routing'].update(max_state_age_ms=180000, quote_ttl_ms=60000)
        config['api'].update(quote_timeout_ms=15000, v4_execution={chain: fixture['executor']})
        config['simulation'].update(
            backend='revm-fixture', evm_cancun_verified_chain=chain, rpc_url='',
            state_file=fixture['bundle'], max_state_bytes=16777216, max_instructions=50000000,
            max_memory_bytes=67108864, max_gas=10000000, max_state_reads=200000,
            timeout_ms=15000, concurrency=1, allowed_targets=[fixture['executor']['executor']],
        )
    return save_config(directory / 'config', config)


def run(replay, geth, case):
    output = ROOT / 'integration/artifacts/local'
    output.mkdir(parents=True, exist_ok=True)
    report_path = output / 'r04-verified-recovery-normal-process.json'
    sources, binaries = source_hash(), binary_fingerprint()
    with ExitStack() as stack:
        infra = stack.enter_context(Infrastructure(topics=(TOPIC,)))
        directory = Path(stack.enter_context(tempfile.TemporaryDirectory(prefix='xone-r04-verified-')))
        fixture_log = stack.enter_context((output / 'r04-verified-geth.log').open('wb'))
        server = subprocess.Popen(
            [str(replay), '--serve-recovery-fixture', str(geth), str(case), str(directory)],
            stdin=subprocess.PIPE, stdout=fixture_log, stderr=subprocess.STDOUT,
        )

        def stop_fixture():
            server.stdin.close()
            try:
                server.wait(timeout=5)
            except subprocess.TimeoutExpired:
                stop(server)

        stack.callback(stop_fixture)
        fixture = wait_manifest(directory, server)
        token = secrets.token_urlsafe(32)
        owner_port, router_port = port(), port()
        owner = f'http://127.0.0.1:{owner_port}'
        router = f'http://127.0.0.1:{router_port}'
        chain = fixture['quote']['chainKey']

        def start(name, config, label):
            log = stack.enter_context((output / f'r04-verified-{label}.log').open('wb'))
            process = subprocess.Popen(service_command(name, config), cwd=ROOT / name,
                                       stdout=log, stderr=subprocess.STDOUT)
            stack.callback(stop, process)
            return process

        owner_config = make_config('chain-parser-go', directory / 'owner', token,
                                   owner_port, owner, infra, fixture)
        router_config = make_config('dex-router', directory / 'router', token,
                                    router_port, owner, infra, fixture)
        start('chain-parser-go', owner_config, 'owner')
        wait(owner + '/health/ready')
        checkpoint = owner_cursor(owner, token)
        fast_request = dict(fixture['quote'], verification='FAST')
        quote_url = router + '/v1/quote'
        first = start('dex-router', router_config, 'initial-router')
        wait(router + '/health/live')
        wait_scope(router, token, chain)
        status, initial = http(quote_url, token, fast_request)
        require(status == 200 and initial['grossBuyAmount'] == fixture['expectedOutput'], (status, initial))
        stop(first)
        started = time.monotonic()
        start('dex-router', router_config, 'cold-router')
        wait(router + '/health/live')
        wait_scope(router, token, chain)
        scope_ms = int((time.monotonic() - started) * 1000)
        status, fast = http(quote_url, token, fast_request)
        fast_ms = int((time.monotonic() - started) * 1000)
        require(status == 200 and fast['verification']['level'] == 'INDICATIVE', (status, fast))
        status, verified = http(quote_url, token, fixture['quote'], timeout=18)
        verified_ms = int((time.monotonic() - started) * 1000)
        require(status == 200 and verified['verification']['level'] == 'VERIFIED'
                and verified['verification']['success'], (status, verified))
        require(verified['snapshotRef'] == fast['snapshotRef'] == initial['snapshotRef']
                and verified['snapshotRef']['block'] == fixture['reference']['block'], 'snapshot changed')
        require(verified['grossBuyAmount'] == fast['grossBuyAmount'] == fixture['expectedOutput'],
                'independent quote output mismatch')
        status, observed = http(router + '/internal/chains/' + chain + '/status', token)
        require(status == 200, observed)
        timing = observed['recovery']
        require(timing['first_verified_quote_ms'] >= timing['first_successful_quote_ms']
                and timing['first_verified_quote_generation'] == verified['onlineGeneration'], timing)
        status, collected = http(router + '/internal/quotes/' + verified['quoteId'] + '/evidence', token)
        require(status == 200, collected)
        evidence = collected['quote_verification_evidence']
        native = evidence['nodeSimulation']
        require(native['sourceStateRoot'] == fixture['sourceStateRoot']
                and native['executionContextBlockHash'] == fixture['executionContextBlockHash']
                and native['settlement']['actualBuyAmount'] == fixture['expectedOutput'], native)
        require(source_hash() == sources and binary_fingerprint() == binaries, 'inputs changed during run')
        report = {
            'passed': True, 'state': 'LOCAL_VERIFIED_RECOVERY_PROCESS_SCOPE_VERIFIED',
            'authority': 'controlled Cancun Geth with PostgreSQL, Kafka and normal service binaries',
            'source_tree_sha256': sources, 'binary_sha256': binaries,
            'fixture_sha256': hashlib.sha256(case.read_bytes()).hexdigest(),
            'checkpoint_cursor': checkpoint, 'snapshot_ref': verified['snapshotRef'],
            'first_qualified_scope_ms': scope_ms, 'first_successful_quote_ms': fast_ms,
            'first_verified_quote_ms': verified_ms, 'router_startup_timing': timing,
            'verified_amount': verified['grossBuyAmount'], 'native_evidence': native,
        }
        write_result(report_path, report)
        print(json.dumps(report))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('replay', 'geth', 'case'):
        parser.add_argument(name, type=lambda path: Path(path).resolve(strict=True))
    args = parser.parse_args()
    run(args.replay, args.geth, args.case)
