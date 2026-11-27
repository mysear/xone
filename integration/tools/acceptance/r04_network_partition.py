#!/usr/bin/env python3
"""Exercise cold recovery through live Docker network partitions."""
import argparse
from contextlib import ExitStack, contextmanager
import json
from pathlib import Path
import secrets
import subprocess
import sys
import tempfile
import time

from configuration import config_for, save_config
from infrastructure import Infrastructure, ROOT, command, port
from r01_r04_recovery_process import GENESIS, RETENTION_POOL, TOPIC, RetentionChain, require
from smoke import http, wait_scope
from kafka_smoke import wait
from weighted_evidence import source_hash, write_result


CHAIN = 'evm:31337'
API_PORT = 8080
PEER_RELAY_PORT = 8081
PEER_RELAY_IMAGE = 'postgres:16-alpine'
OWNER_URL = f'http://checkpoint-owner:{API_PORT}'
PEER_URL = f'http://127.0.0.1:{PEER_RELAY_PORT}'


def docker(*args, check=True):
    return command(['docker', *args], check=check, timeout=60)


def service_config(root, alias, service, token, infra, host, rpc_ports):
    owner = service == 'chain-parser-go'
    config = config_for(service, root, token, API_PORT)
    config['api']['listen'] = f'0.0.0.0:{API_PORT}'
    config['state']['path'] = '/tmp/xone-state'
    config['chains'] = [chain for chain in config['chains'] if chain['key'] == CHAIN]
    config['chains'][0].update(genesis_hash=GENESIS, confirmation_tag='latest', auto_reorg_window=4,
                               rpc_urls=[f'http://{host}:{rpc_port}' for rpc_port in rpc_ports] if owner else [],
                               pools=[RETENTION_POOL] if owner else [])
    config['control'].update(backend='postgres' if owner else 'disabled',
                             dsn_file='/acceptance/postgres-dsn' if owner else '',
                             instance_id=alias, lease_ttl_ms=30000, renewal_ms=1000,
                             min_commit_remaining_ms=7000)
    config['journal'].update(transport='kafka', source_urls=[OWNER_URL], poll_interval_ms=100,
                             request_timeout_ms=5000)
    config['journal']['kafka'].update(brokers=['recovery-kafka:9092'], topics={CHAIN: TOPIC},
                                     replica_id=alias, transaction_timeout_ms=4000)
    config['startup'].update(max_baseline_age_ms=180000, max_tail_blocks=128)
    config['history']['enabled'] = False
    if owner:
        dsn = root / 'postgres-dsn'
        dsn.write_text(infra.dsn.replace('127.0.0.1', host))
        dsn.chmod(0o600)
    else:
        config['routing'].update(max_state_age_ms=180000, quote_ttl_ms=60000)
        config['logging']['level'] = 'debug'
    if alias == 'cold':
        config['journal']['checkpoint_peer_urls'] = [PEER_URL]
        config['journal']['source_auth_tokens'] = {url: token for url in (OWNER_URL, PEER_URL)}
    save_config(root, config)


def run(owner_image, quote_image, host):
    sources = source_hash()
    images = {image: docker('image', 'inspect', image, '--format', '{{.Id}}').strip()
              for image in (owner_image, quote_image, PEER_RELAY_IMAGE)}
    output = ROOT / 'integration/artifacts/local'
    output.mkdir(parents=True, exist_ok=True)
    report_path = output / 'r04-network-partition.json'
    write_result(report_path, {'passed': False, 'state': 'RUNNING', 'service_images': images})
    prefix = 'xone-r04-' + secrets.token_hex(4)
    backbone, checkpoints = prefix + '-kafka', prefix + '-checkpoints'
    token = secrets.token_urlsafe(32)
    with ExitStack() as stack:
        docker('network', 'create', backbone)
        stack.callback(docker, 'network', 'rm', backbone, check=False)
        docker('network', 'create', '--internal', checkpoints)
        stack.callback(docker, 'network', 'rm', checkpoints, check=False)
        infra = stack.enter_context(Infrastructure((TOPIC,), kafka_network=backbone))
        directory = Path(stack.enter_context(tempfile.TemporaryDirectory(dir=infra.path)))
        chain = RetentionChain()
        providers = [chain.provider(), chain.provider()]
        for provider in providers:
            stack.callback(provider.server_close)
            stack.callback(provider.shutdown)
        rpc_ports = [provider.server_port for provider in providers]
        names = {alias: f'{prefix}-{alias}' for alias in ('owner', 'peer', 'cold')}
        ports = {alias: port() for alias in names}
        name_to_service = {'owner': 'chain-parser-go', 'peer': 'dex-router', 'cold': 'dex-router'}
        for alias, service in name_to_service.items():
            config_dir = directory / alias
            config_dir.mkdir()
            service_config(config_dir, alias, service, token, infra, host, rpc_ports)

        def base(alias):
            return f'http://127.0.0.1:{ports[alias]}'

        def connect(alias):
            docker('network', 'connect', '--alias', f'checkpoint-{alias}', checkpoints, names[alias])

        def disconnect(alias):
            docker('network', 'disconnect', checkpoints, names[alias])
            running = docker('inspect', '--format', '{{.State.Running}}', names[alias]).strip()
            require(running == 'true', f'{alias} process stopped during the partition')

        @contextmanager
        def service(alias):
            image = owner_image if alias == 'owner' else quote_image
            name = names[alias]
            docker('create', '--name', name, '--network', backbone,
                   '-p', f'127.0.0.1:{ports[alias]}:{API_PORT}',
                   images[image], '--config', '/acceptance/config-test.yaml')
            try:
                with ExitStack() as relays:
                    docker('cp', str(directory / alias), name + ':/acceptance')
                    connect(alias)
                    docker('start', name)
                    if alias == 'cold':
                        # Development HTTP peers require loopback. The relay
                        # shares the cold replica's network namespace.
                        relay = name + '-peer-relay'
                        docker('run', '--rm', '-d', '--name', relay, '--network', f'container:{name}',
                               '--entrypoint', '/bin/busybox', images[PEER_RELAY_IMAGE],
                               'nc', '-lk', '-p', str(PEER_RELAY_PORT), '-s', '127.0.0.1',
                               '-e', '/bin/busybox', 'nc', '-w', '3', 'checkpoint-peer', str(API_PORT))
                        relays.callback(docker, 'rm', '-f', relay, check=False)
                    wait(base(alias) + '/health/live')
                    yield
            except subprocess.CalledProcessError as error:
                print(error.stdout[-1500:], file=sys.stderr)
                raise
            except (AssertionError, OSError, RuntimeError):
                logs = docker('logs', '--tail', '25', name, check=False)
                print(f'{alias} logs\n{logs[-1500:]}', file=sys.stderr)
                raise
            finally:
                docker('rm', '-f', name, check=False)

        def quote(alias, expected=200):
            status, result = http(base(alias) + '/v1/quote', token, {
                'chainKey': CHAIN, 'kind': 'EXACT_IN', 'sellToken': RETENTION_POOL['token0'],
                'buyToken': RETENTION_POOL['token1'], 'sellAmount': '10000', 'slippageBps': 50,
                'caller': RETENTION_POOL['token0'], 'recipient': RETENTION_POOL['token1'],
                'verification': 'FAST',
            })
            require(status == expected, (status, result))
            if expected == 200:
                require(result['grossBuyAmount'] == '9871', result)
                require(result['verification']['level'] == 'INDICATIVE', result)
            return result

        stack.enter_context(service('owner'))
        wait(base('owner') + '/health/ready')
        stack.enter_context(service('peer'))
        wait_scope(base('peer'), token, CHAIN)
        original = quote('peer')
        disconnect('owner')

        started = time.monotonic()
        with service('cold'):
            wait_scope(base('cold'), token, CHAIN)
            peer_scope_ms = int((time.monotonic() - started) * 1000)
            restored = quote('cold')
            peer_ms = int((time.monotonic() - started) * 1000)
            require(restored['snapshotRef'] == original['snapshotRef'], 'peer recovery changed the snapshot')
            require('PEER_CHECKPOINT_AND_KAFKA' in docker('logs', names['cold']),
                    'cold service did not activate the peer checkpoint')

        disconnect('peer')
        stack.enter_context(service('cold'))
        deadline = time.monotonic() + 20
        while 'kafka_restore_source_unavailable' not in docker('logs', names['cold']):
            require(time.monotonic() < deadline, 'cold replica did not attempt the partitioned sources')
            time.sleep(0.1)
        require(http(base('cold') + '/health/ready')[0] == 503, 'cold replica ready during total partition')
        quote('cold', 503)

        started = time.monotonic()
        connect('owner')
        wait_scope(base('cold'), token, CHAIN)
        owner_scope_ms = int((time.monotonic() - started) * 1000)
        recovered = quote('cold')
        owner_ms = int((time.monotonic() - started) * 1000)
        require(recovered['snapshotRef'] == original['snapshotRef'], 'owner recovery changed the snapshot')
        require('OWNER_CHECKPOINT_AND_KAFKA' in docker('logs', names['cold']),
                'cold service did not activate the restored owner checkpoint')
        require(source_hash() == sources, 'sources changed during the run')
        require(all(docker('image', 'inspect', image, '--format', '{{.Id}}').strip() == identity
                    for image, identity in images.items()), 'image tags changed during the run')
        report = {
            'passed': True, 'state': 'LOCAL_NETWORK_PARTITION_SCOPE_VERIFIED',
            'source_tree_sha256': sources, 'service_images': images,
            'first_scope_from_peer_ms': peer_scope_ms, 'first_quote_from_peer_ms': peer_ms,
            'first_scope_after_owner_reconnect_ms': owner_scope_ms,
            'first_quote_after_owner_reconnect_ms': owner_ms,
            'cold_quote_without_checkpoint_links_http_status': 503,
            'snapshot_ref': recovered['snapshotRef'], 'quote_verification': recovered['verification']['level'],
        }
        write_result(report_path, report)
        print(json.dumps(report))


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('owner_image')
    parser.add_argument('quote_image')
    parser.add_argument('--container-host', default='host.docker.internal')
    args = parser.parse_args()
    run(args.owner_image, args.quote_image, args.container_host)
