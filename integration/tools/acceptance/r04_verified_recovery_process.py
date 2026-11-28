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
from r01_r04_recovery_process import binary_fingerprint, expire_owner_cursor, owner_cursor, require
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
        genesis_hash=fixture['genesis'],
        rpc_urls=fixture.get('ownerRpcUrls', [fixture['rpc']]) if parser else [fixture['rpc']],
        confirmation_tag='safe',
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
        if fixture.get('rpcState'):
            config['simulation']['evm_cancun_verified_rpc_state'] = True
    return save_config(directory / 'config', config)


def previous_template(fixture, directory):
    bundle = json.loads(Path(fixture['bundle']).read_text())
    state = bundle['fixture']
    anchor, environment = state['state_anchor'], state['execution_environment']
    status, response = http(fixture['rpc'], body={
        'jsonrpc': '2.0', 'id': 1, 'method': 'eth_getBlockByHash',
        'params': [anchor['parent_hash'], False],
    })
    require(status == 200 and response['result']['hash'] == anchor['parent_hash'], response)
    previous = response['result']
    environment.update(block_number=anchor['block_number'], block_hash=anchor['block_hash'],
                       parent_hash=anchor['parent_hash'])
    anchor.update(block_number=int(previous['number'], 16), block_hash=previous['hash'],
                  parent_hash=previous['parentHash'], timestamp=int(previous['timestamp'], 16),
                  base_fee_per_gas=str(int(previous['baseFeePerGas'], 16)))
    bundle['transaction_context']['nonce'] += 1
    for account in state['accounts']:
        if account['address'] == fixture['quote']['caller']:
            account['balance'] = '0'
            account['nonce'] += 1
        for entry in account['storage']:
            entry['value'] = '0x' + '00' * 32
    path = directory / 'previous-template.json'
    path.write_text(json.dumps(bundle))
    manager = fixture['pool']['singleton_advanced']['snapshot']['manager']
    next(account for account in state['accounts'] if account['address'] == manager)['storage'] = []
    incomplete = directory / 'incomplete-template.json'
    incomplete.write_text(json.dumps(bundle))
    return anchor, str(path), str(incomplete)


def run(replay, geth, case, check_timeout, rpc_state, retention):
    output = ROOT / 'integration/artifacts/local'
    output.mkdir(parents=True, exist_ok=True)
    scenario = ('retention' if retention else 'rpc-state' if rpc_state
                else 'timeout' if check_timeout else 'normal-process')
    report_path = output / f'r04-verified-recovery-{scenario}.json'
    sources, binaries = source_hash(), binary_fingerprint()
    with ExitStack() as stack:
        infra = stack.enter_context(Infrastructure(topics=(TOPIC,)))
        directory = Path(stack.enter_context(tempfile.TemporaryDirectory(prefix='xone-r04-verified-')))
        fixture_log = stack.enter_context((output / 'r04-verified-geth.log').open('wb'))
        command = [str(replay), '--serve-recovery-fixture', str(geth), str(case), str(directory)]
        if check_timeout:
            command.append('--stall-first-simulation')
        server = subprocess.Popen(
            command,
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
        if retention:
            require(fixture['rpc'].startswith('http://127.0.0.1:'), 'expected a loopback fixture')
            fixture['ownerRpcUrls'] = [fixture['rpc'], fixture['rpc'].replace('127.0.0.1', 'localhost', 1)]
        if rpc_state:
            template_anchor, complete, incomplete = previous_template(fixture, directory)
            fixture['rpcState'] = True
            fixture['bundle'] = complete
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
        owner_process = start('chain-parser-go', owner_config, 'owner')
        wait(owner + '/health/ready')
        checkpoint = owner_cursor(owner, token)
        fast_request = dict(fixture['quote'], verification='FAST')
        quote_url = router + '/v1/quote'
        initial_config = router_config
        if rpc_state:
            initial_config = make_config('dex-router', directory / 'incomplete-router', token,
                                         router_port, owner, infra, dict(fixture, bundle=incomplete))
        first = start('dex-router', initial_config, 'initial-router')
        wait(router + '/health/live')
        wait_scope(router, token, chain)
        status, initial = http(quote_url, token, fast_request)
        require(status == 200 and initial['grossBuyAmount'] == fixture['expectedOutput'], (status, initial))
        if rpc_state:
            status, refused = http(quote_url, token, fixture['quote'], timeout=18)
            require(status == 422 and refused['error'] == 'SIMULATION_FAILED', (status, refused))
        stop(first)
        retention_report = None
        if retention:
            stop(owner_process)
            low_before, low_after = expire_owner_cursor(infra, directory, checkpoint)
        started = time.monotonic()
        start('dex-router', router_config, 'cold-router')
        wait(router + '/health/live')
        if retention:
            status, unavailable = http(quote_url, token, fast_request)
            require(status == 503, (status, unavailable))
            started = time.monotonic()
            start('chain-parser-go', owner_config, 'recovered-owner')
            wait(owner + '/health/ready')
            restored_cursor = owner_cursor(owner, token)
            require(restored_cursor > low_after, (restored_cursor, low_after))
            retention_report = {
                'initial_cursor': checkpoint, 'low_watermark_before': low_before,
                'low_watermark_after': low_after, 'restored_cursor': restored_cursor,
                'cold_quote_without_owner_status': status,
                'rpc_topology': 'two loopback URLs for one controlled Geth node',
            }
        wait_scope(router, token, chain)
        scope_ms = int((time.monotonic() - started) * 1000)
        status, fast = http(quote_url, token, fast_request)
        fast_ms = int((time.monotonic() - started) * 1000)
        require(status == 200 and fast['verification']['level'] == 'INDICATIVE', (status, fast))
        chain_status_url = router + '/internal/chains/' + chain + '/status'
        timeout_probe = None
        if check_timeout:
            attempt = time.monotonic()
            status, refused = http(quote_url, token, fixture['quote'], timeout=18)
            elapsed_ms = int((time.monotonic() - attempt) * 1000)
            require(status == 504 and refused['error'] == 'BUDGET_EXHAUSTED', (status, refused))
            require((directory / 'stalled-simulation').is_file(), 'native simulation did not start')
            status, observed = http(chain_status_url, token)
            require(status == 200, observed)
            timing = observed['recovery']
            require(timing['first_verified_quote_ms'] is None
                    and timing['first_verified_quote_generation'] is None, timing)
            timeout_probe = {'elapsed_ms': elapsed_ms, 'error': refused['error'], 'recovery': timing}
        status, verified = http(quote_url, token, fixture['quote'], timeout=18)
        verified_ms = int((time.monotonic() - started) * 1000)
        require(status == 200 and verified['verification']['level'] == 'VERIFIED'
                and verified['verification']['success'], (status, verified))
        require(verified['snapshotRef'] == fast['snapshotRef'], 'recovered quote snapshots differ')
        require(verified['snapshotRef']['block'] == initial['snapshotRef']['block'] == fixture['reference']['block'],
                'fixed block changed')
        require(verified['grossBuyAmount'] == fast['grossBuyAmount'] == fixture['expectedOutput'],
                'independent quote output mismatch')
        if retention:
            require(verified['onlineGeneration'] == fast['onlineGeneration'] > initial['onlineGeneration'],
                    'recovered quote did not bind a new owner generation')
            gaps = initial['quality']['historyGaps']
            require(gaps and verified['quality']['historyGaps'] == fast['quality']['historyGaps'] == gaps,
                    'recovered quote dropped existing history gaps')
            retention_report['history_gaps'] = gaps
        else:
            require(verified['snapshotRef'] == initial['snapshotRef'], 'snapshot changed')
        status, observed = http(chain_status_url, token)
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
            'passed': True,
            'state': ('LOCAL_VERIFIED_RECOVERY_TIMEOUT_SCOPE_VERIFIED' if check_timeout
                      else 'LOCAL_VERIFIED_RECOVERY_PROCESS_SCOPE_VERIFIED'),
            'authority': 'controlled Cancun Geth with PostgreSQL, Kafka and normal service binaries',
            'source_tree_sha256': sources, 'binary_sha256': binaries,
            'fixture_sha256': hashlib.sha256(case.read_bytes()).hexdigest(),
            'checkpoint_cursor': checkpoint, 'snapshot_ref': verified['snapshotRef'],
            'first_qualified_scope_ms': scope_ms, 'first_successful_quote_ms': fast_ms,
            'first_verified_quote_ms': verified_ms, 'router_startup_timing': timing,
            'verified_amount': verified['grossBuyAmount'], 'native_evidence': native,
        }
        if timeout_probe is not None:
            report['timeout_probe'] = timeout_probe
        if rpc_state:
            report['template_anchor'] = template_anchor
            report['incomplete_template_error'] = 'SIMULATION_FAILED'
            report['state'] = 'LOCAL_VERIFIED_RECOVERY_RPC_STATE_SCOPE_VERIFIED'
        if retention_report is not None:
            report['retention'] = retention_report
            report['state'] = 'LOCAL_VERIFIED_RECOVERY_RETENTION_SCOPE_VERIFIED'
        write_result(report_path, report)
        print(json.dumps(report))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('replay', 'geth', 'case'):
        parser.add_argument(name, type=lambda path: Path(path).resolve(strict=True))
    scenario = parser.add_mutually_exclusive_group()
    scenario.add_argument('--check-timeout', action='store_true',
                          help='stall the first native simulation and verify recovery on retry')
    scenario.add_argument('--rpc-state', action='store_true',
                          help='verify a newer block with RPC state and an older bundle template')
    scenario.add_argument('--retention', action='store_true',
                          help='verify a cold quote after the owner Kafka cursor expires')
    args = parser.parse_args()
    run(args.replay, args.geth, args.case, args.check_timeout, args.rpc_state, args.retention)
