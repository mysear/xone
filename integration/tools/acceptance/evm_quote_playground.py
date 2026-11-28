#!/usr/bin/env python3
"""Serve the retained seven-chain pool graph through normal local binaries."""

import argparse
import copy
import json
from collections import deque
from contextlib import ExitStack
from http.server import BaseHTTPRequestHandler
from pathlib import Path
import secrets
import signal
import subprocess
import tempfile
import time

from configuration import config_for
from r01_public_catalog import ROOT, load_json
from r01_public_catalog_evm_normal import BASE, StageFixedRPC
import r01_public_catalog_evm_process as process
from r01_public_catalog_evm_process import call_key, captured_calls, free_port
from r01_public_catalog_slipstream_normal import ContextRPC


GROUPS = (
    ('base-uniswap-v3-fixed-h.ustwe78e', 'base-aerodrome-v2-fixed-h.80092axw'),
    ('ethereum-26511767.CorGKXrn/ethereum-uniswap-v2',
     'ethereum-26511767.CorGKXrn/ethereum-uniswap-v3', 'ethereum-dai-weth-26511767'),
    ('optimism-uniswap-v3-160113647-indexed', 'optimism-velodrome-v2-160113647'),
    ('arbitrum-uniswap-v3-fixed-h.el56r46q',),
    ('bsc-pancake-v2-finalized.pefduaif',),
    ('polygon-getter-proof.3q923t3l',),
    ('avalanche-lfj-proof.no6DPSRo',),
)
LIFETIME_SECONDS = 86400
FRONTEND_ORIGIN = 'http://127.0.0.1:5173'


class KeepAliveHandler(BaseHTTPRequestHandler):
    protocol_version = 'HTTP/1.1'


def merge(target, additions):
    for key, value in additions.items():
        if key in target and target[key] != value:
            raise ValueError(f'Conflicting retained RPC response for {key}')
        target[key] = value


def add_slipstream(primary):
    case = BASE / 'base-slipstream-rebuilt-replay-63180533'
    capture = load_json(case / 'capture.json')
    source = ROOT / capture['catalog_row']['directory']
    origin = load_json(source / 'fork-execution.json')['sender']
    values = {}
    for path in sorted(case.glob('getters-*.jsonl')):
        for line in path.read_text().splitlines():
            getter = json.loads(line)
            if (getter['block_tag'] != hex(primary.height) or getter['request']['from'] != origin
                    or not getter['all_code_and_storage_MPT_bound'] or not getter['steps']
                    or getter['forbidden'] or getter['error'] is not None):
                raise ValueError('Invalid retained Slipstream getter trace')
            merge(values, {call_key(getter['to'], getter['data']): getter})
    if not values:
        raise ValueError('Missing retained Slipstream getters')
    context = ContextRPC(source, {}, [], origin, values)
    if context.header['hash'] != primary.header['hash'] or context.genesis['hash'] != primary.genesis['hash']:
        raise ValueError('Mismatched Slipstream anchor')
    merge(primary.code, context.code)

    standard_dispatch = primary.dispatch

    def dispatch(method, params):
        if method == 'eth_call':
            key = call_key(params[0]['to'], params[0]['data'])
            if key in values:
                if key in primary.calls and primary.calls[key] != values[key]['value']:
                    raise ValueError('Conflicting archived Slipstream response')
                return context.dispatch(method, params)
        return standard_dispatch(method, params)

    primary.dispatch = dispatch
    return capture['pool_config']


def chain_fixture(names):
    pools = []
    primary = None
    for name in names:
        case = BASE / name
        capture = load_json(case / 'capture.json')
        row = capture['catalog_row']
        probe = load_json(ROOT / capture['getter_execution']['path'])
        fixture = StageFixedRPC(ROOT / row['directory'], captured_calls(capture),
                                capture['public_batches'], case, probe.get('storage_reads', []))
        if primary is None:
            primary = fixture
        else:
            if fixture.header != primary.header or fixture.genesis != primary.genesis:
                raise ValueError(f'Mismatched chain anchor for {name}')
            for field in ('calls', 'code', 'storage', 'failure_calls', 'receipts',
                          'extra_receipts', 'auxiliary_headers'):
                merge(getattr(primary, field), getattr(fixture, field))
        pools.append(capture['pool_config'])
    if row['chain_key'] == 'evm:8453':
        pools.append(add_slipstream(primary))
    primary.trace = deque(maxlen=1024)
    return row['chain_key'], primary, pools


def stop_child(child):
    if child.poll() is None:
        child.terminate()
        try:
            child.wait(timeout=5)
        except subprocess.TimeoutExpired:
            child.kill()
            child.wait()


def serve(port):
    process.BaseHTTPRequestHandler = KeepAliveHandler
    files = Path(tempfile.mkdtemp(prefix='xone-evm-quote-playground-'))
    token = secrets.token_urlsafe(32)
    parser_port = free_port()
    parser_url = f'http://127.0.0.1:{parser_port}'
    chains = []
    timestamps = []
    children = []
    with ExitStack() as cleanup:
        for names in GROUPS:
            key, fixture, pools = chain_fixture(names)
            servers = []
            for _ in range(2):
                server = fixture.serve()
                cleanup.callback(server.server_close)
                cleanup.callback(server.shutdown)
                servers.append(f'http://127.0.0.1:{server.server_port}')
            timestamps.append(int(fixture.header['timestamp'], 16))
            chains.append(dict(key=key, owner_service='chain-parser-go', assignment_version=1,
                               genesis_hash=fixture.genesis['hash'], rpc_urls=servers,
                               pools=pools, confirmation_tag='finalized', auto_reorg_window=128))

        freshness = (int(time.time()) + LIFETIME_SECONDS - min(timestamps) + 120) * 1000
        binaries = (
            ('parser', 'chain-parser-go', 'bin/chain-parser-go', parser_port),
            ('router', 'dex-router', 'target/debug/dex-router', port),
        )
        for name, service, binary, listen_port in binaries:
            config = config_for(service, files / name, token, listen_port)
            config['service']['mode'] = 'rpc'
            config['chains'] = copy.deepcopy(chains)
            config['startup']['max_baseline_age_ms'] = freshness
            config['history']['enabled'] = False
            config['control'].update(backend='disabled', dsn_file='', instance_id=f'playground-{name}')
            config['journal'].update(transport='http-development', source_urls=[parser_url],
                                     poll_interval_ms=250 if name == 'router' else 2000,
                                     request_timeout_ms=10000, max_payload_bytes=16 * 1024 * 1024)
            if name == 'router':
                for chain in config['chains']:
                    chain.update(rpc_urls=[], pools=[])
                config['journal']['source_auth_tokens'] = {parser_url: token}
                config['routing'].update(max_hops=3, max_splits=2, split_parts=4, max_state_age_ms=freshness,
                                         quote_ttl_ms=60000, route_activation_cost='0')
                config['api'].update(auth_token='', cors_allowed_origins=[FRONTEND_ORIGIN],
                                     quote_timeout_ms=10000, quote_concurrency=4)
            config_file = files / f'{name}.json'
            config_file.write_text(json.dumps(config))
            with (files / f'{name}.log').open('wb') as log:
                child = subprocess.Popen([str(ROOT / service / binary), '--config', str(config_file)],
                                         cwd=ROOT / service, stdout=log, stderr=subprocess.STDOUT)
            cleanup.callback(stop_child, child)
            children.append(child)

        print(f'Started fixed-block replay for http://127.0.0.1:{port}. Logs in {files}', flush=True)
        deadline = time.monotonic() + LIFETIME_SECONDS
        while time.monotonic() < deadline:
            if any(child.poll() is not None for child in children):
                raise SystemExit(f'A replay service exited. See logs in {files}')
            time.sleep(1)


def stop(_signal, _frame):
    raise SystemExit(0)


if __name__ == '__main__':
    cli = argparse.ArgumentParser(description=__doc__)
    cli.add_argument('--port', default=18080, type=int)
    args = cli.parse_args()
    if not 1 <= args.port <= 65535:
        cli.error('--port must be in 1..65535')
    signal.signal(signal.SIGTERM, stop)
    signal.signal(signal.SIGINT, stop)
    serve(args.port)
