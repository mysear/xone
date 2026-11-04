#!/usr/bin/env python3
"""Frozen original NG replay through normal RPC/PG/Kafka and two routers.

This is synthetic historical RPC evidence. The large freshness window belongs
only to this runner; original exchange/oracle timestamps are never rewritten.
"""
from pathlib import Path
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import secrets
import subprocess
import tempfile
import threading
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / 'integration/artifacts/local'


def write_result(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + '.' + secrets.token_hex(6) + '.tmp')
    try:
        temporary.write_text(json.dumps(value, indent=2) + '\n')
        temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)


# Invalidate direct CLI attempts before loading any optional helper dependency.
if __name__ == '__main__':
    write_result(OUT / 'curve-ng-three-service-smoke.json', {'passed': False, 'state': 'RUNNING'})

from infrastructure import Infrastructure, port
from configuration import config_for, save_config, service_command, sample_for
from receipt_reference import EMPTY_ROOT, SINGLE_EMPTY_ROOT, SINGLE_EMPTY_FIELDS
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import http, wait_scope
from curve_ng_fixture import NAMES, profile, code_bytes, token_code, digest

GENESIS = '0x' + '99' * 32
FRESHNESS_MS = 315360000000
TEMPLATE_BYTES = 21491
TEMPLATE_SHA256 = '390b83f661547f817358df2b503210646c1749400f7eb4cad3a9547f3c462810'
FAULTS = ['accounting', 'oracle', 'immutable', 'template']
PROCESS_CHECKS = [
    'seven-ng-pools-fixed-original-runtime-and-historical-rpc',
    'complete-ng-balances-admin-and-oracles-in-live-protobuf',
    'all-56-original-quotes-match-both-router-replicas',
    'historical-banks-preserve-original-time-and-complete-source-identity',
    'invalid-ng-dependencies-revoke-whole-evm-scope-and-recover-higher-generation',
    'unrelated-solana-scope-remains-ready',
    'memory-router-restart-restores-ng-bank',
    'verified-and-build-remain-disabled',
]


def require(condition, message):
    if not condition:
        raise ValueError(message)


def word(value):
    value = int(value)
    require(0 <= value < 1 << 256, 'invalid uint256 fixture')
    return f'{value:064x}'


def configure_freshness(config):
    config['startup']['max_baseline_age_ms'] = FRESHNESS_MS
    if 'routing' in config:
        config['routing']['max_state_age_ms'] = FRESHNESS_MS


def check_fixtures():
    timestamps = []
    for step in range(8):
        items = [profile(name, step) for name in NAMES]
        values = {item['pool']['multi_asset']['curve_ng']['timestamp'] for item in items}
        require(len(values) == 1, 'original scenarios do not share a phase time')
        timestamps.append(values.pop())
        for item in items:
            raw = code_bytes(item['name'])
            require(len(raw) == TEMPLATE_BYTES + 2144 and digest(raw[:TEMPLATE_BYTES]) == TEMPLATE_SHA256,
                    'original runtime/template mismatch')
    require(len(NAMES) == 7 and timestamps[0] == 1767225600, 'historical reference changed')
    return {'coin_counts': [len(profile(n)['pool']['multi_asset']['assets']) for n in NAMES],
            'scenario_count': 7, 'step_count': 8, 'timestamps': timestamps}


class Chain:
    def __init__(self):
        check_fixtures()
        self.lock = threading.RLock()
        self.head = 99
        self.step_index = 0
        self.banks = {}
        self.pinned = 0
        self.receipts = 0
        self.advance(step_index=0)

    def advance(self, step_index=None, fault=None):
        with self.lock:
            if step_index is not None:
                require(0 <= step_index < 8, 'invalid replay phase')
                self.step_index = step_index
            require(fault is None or fault in FAULTS, 'unknown injected fault')
            self.head += 1
            blockhash = '0x' + digest(f'NG/process/{self.head}'.encode())
            self.items = [profile(name, self.step_index, self.head, blockhash) for name in NAMES]
            self.timestamp = self.items[0]['pool']['multi_asset']['curve_ng']['timestamp']
            self.banks[self.head] = {'items': self.items, 'fault': fault, 'timestamp': self.timestamp, 'hash': blockhash}

    def block(self, number):
        if number == 0:
            return {'number': '0x0', 'hash': GENESIS, 'parentHash': '0x' + '00' * 32,
                    'timestamp': hex(1767225600), 'receiptsRoot': EMPTY_ROOT, 'transactions': []}
        bank = self.banks.get(number)
        blockhash = bank['hash'] if bank else '0x' + digest(f'NG/process/{number}'.encode())
        parent = GENESIS if number == 1 else '0x' + digest(f'NG/process/{number-1}'.encode())
        timestamp = bank['timestamp'] if bank else 1767225600
        return {'number': hex(number), 'hash': blockhash, 'parentHash': parent, 'timestamp': hex(timestamp),
                'receiptsRoot': SINGLE_EMPTY_ROOT, 'transactions': ['0x' + digest(('tx/' + blockhash).encode())]}

    def rpc(self, method, args):
        with self.lock:
            if method == 'eth_chainId':
                return '0x7a69'
            if method == 'eth_getBlockByNumber':
                return self.block(self.head if args[0] in ['latest', 'safe', 'finalized'] else int(args[0], 16))
            if method == 'eth_getTransactionReceipt':
                for number in range(1, self.head + 1):
                    block = self.block(number)
                    if args[0] in block['transactions']:
                        self.receipts += 1
                        return {'blockHash': block['hash'], 'transactionHash': args[0],
                                'transactionIndex': '0x0', 'logs': [], **SINGLE_EMPTY_FIELDS}
                raise ValueError('unknown transaction')
            require(method in ['eth_call', 'eth_getCode', 'eth_getStorageAt'], 'unexpected RPC method')
            anchor = args[2] if method == 'eth_getStorageAt' else args[1]
            require(isinstance(anchor, dict) and anchor.get('requireCanonical') is True,
                    'RPC dependency is not canonical-hash pinned')
            bank = next((b for b in self.banks.values() if b['hash'] == anchor.get('blockHash')), None)
            require(bank is not None, 'unknown dependency hash')
            self.pinned += 1
            target = args[0]['to'] if method == 'eth_call' else args[0]
            item = next((item for item in bank['items'] if target == item['pool']['pool_id'] or
                         target in [a['token'] for a in item['pool']['multi_asset']['assets']]), None)
            require(item is not None, 'unknown RPC target')
            pool = item['pool']
            state = pool['multi_asset']
            ng = state['curve_ng']
            assets = state['assets']
            n = len(assets)
            fault = bank['fault'] if item['name'] == NAMES[-1] else None
            if method == 'eth_getCode':
                raw = code_bytes(item['name']) if target == pool['pool_id'] else token_code(
                    next(i for i, asset in enumerate(assets) if asset['token'] == target))
                if target == pool['pool_id'] and fault == 'template':
                    raw = bytes([raw[0] ^ 1]) + raw[1:]
                return '0x' + raw.hex()
            if method == 'eth_getStorageAt':
                require(target == pool['pool_id'], 'unexpected Token storage access')
                slots = {1: n, 16: n, 25: n - 1, 34: ng['last_d_packed'], 37: ng['ma_last_time']}
                slots.update({2 + i: value for i, value in enumerate(ng['stored_balances'])})
                slots.update({17 + i: value for i, value in enumerate(ng['admin_balances'])})
                slots.update({26 + i: value for i, value in enumerate(ng['last_prices_packed'])})
                if fault == 'accounting':
                    slots[2 + n - 1] = int(slots[2 + n - 1]) + 1
                if fault == 'oracle':
                    slots[37] = (int(slots[37]) >> 128 << 128) | (bank['timestamp'] + 1)
                slot = int(args[1], 16)
                require(slot in slots, 'unknown original storage slot')
                return '0x' + word(slots[slot])
            data = args[0]['data']
            selector = data[:10]
            if selector == '0x313ce567':
                asset = next((a for a in assets if a['token'] == target), None)
                require(asset is not None, 'decimals target mismatch')
                return '0x' + word(36 - (len(asset['scaling']) - 1))
            require(target == pool['pool_id'], 'getter target mismatch')
            scalar = {'0x29357750': n, '0x76a2f0f0': state['amplification'],
                      '0xddca3f43': pool['fee_numerator'], '0xfee3f7f9': state['admin_fee'],
                      '0x8edfdd5f': ng['offpeg_fee_multiplier'], '0x1be913a5': ng['ma_exp_time'],
                      '0x9c4258c4': ng['d_ma_time']}
            if selector in scalar:
                require(len(data) == 10, 'invalid scalar calldata')
                return '0x' + word(scalar[selector])
            if selector in ['0xfd0684b1', '0x14f05979']:
                values = [a['scaling'] if selector == '0xfd0684b1' else a['balance'] for a in assets]
                if fault == 'immutable' and selector == '0xfd0684b1':
                    values[-1] = str(int(values[-1]) + 1)
                return '0x' + word(32) + word(n) + ''.join(word(v) for v in values)
            if selector in ['0xc6610657', '0xe2e7d264']:
                require(len(data) == 74, 'invalid indexed calldata')
                index = int(data[10:], 16)
                require(index < n, 'invalid coin index')
                return '0x' + (assets[index]['token'][2:].rjust(64, '0') if selector == '0xc6610657'
                               else word(ng['admin_balances'][index]))
            raise ValueError('unexpected NG selector ' + selector)

    def provider(self):
        chain = self
        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass
            def do_POST(self):
                query = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                response = {'jsonrpc': '2.0', 'id': query['id']}
                try:
                    response['result'] = chain.rpc(query['method'], query['params'])
                except (ValueError, KeyError, StopIteration) as error:
                    response['error'] = {'code': -32000, 'message': str(error)}
                raw = json.dumps(response).encode()
                self.send_response(200)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(raw)))
                self.end_headers()
                try:
                    self.wfile.write(raw)
                except (BrokenPipeError, ConnectionResetError):
                    pass
        server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        return server


AMOUNTS = {'reserve0', 'reserve1', 'balance', 'scaling', 'weights', 'amplification', 'stored_balances',
           'admin_balances', 'last_prices_packed', 'last_d_packed', 'ma_last_time', 'offpeg_fee_multiplier'}
DIGESTS = {'hash', 'parent_hash', 'adapter_set_hash', 'scope_hash', 'code_sha256', 'vault_code_sha256', 'block_hash'}


def protobuf_text(obj, level=''):
    def blob(raw):
        return '"' + ''.join('\\x%02x' % value for value in raw) + '"'
    text = ''
    for key, value in obj.items():
        if value is None:
            continue
        for item in value if isinstance(value, list) else [value]:
            if isinstance(item, dict):
                text += key + ' { ' + protobuf_text(item, key) + ' } '
            elif key in AMOUNTS:
                text += key + ' { uint256_be: ' + blob(int(item).to_bytes(32, 'big')) + ' } '
            elif key in DIGESTS or key == 'pool_id' and level == 'multi_asset':
                text += key + ': ' + blob(bytes.fromhex(item.removeprefix('0x'))) + ' '
            else:
                text += key + ': ' + (json.dumps(item) if isinstance(item, (str, bool)) else str(item)) + ' '
    return text


def bootstrap_snapshot(raw):
    def varint(offset):
        value = 0
        for shift in range(0, 70, 7):
            require(offset < len(raw), 'truncated bootstrap varint')
            byte = raw[offset]
            offset += 1
            value |= (byte & 127) << shift
            if byte < 128:
                return value, offset
        raise ValueError('invalid bootstrap varint')
    offset = 0
    snapshot = None
    while offset < len(raw):
        tag, offset = varint(offset)
        require(tag >> 3 != 0 and tag & 7 == 2, 'invalid bootstrap envelope field')
        length, offset = varint(offset)
        require(offset + length <= len(raw), 'truncated bootstrap field')
        if tag >> 3 == 1:
            require(snapshot is None and length > 0, 'missing/duplicate bootstrap snapshot')
            snapshot = raw[offset:offset + length]
        offset += length
    require(snapshot is not None, 'bootstrap snapshot missing')
    return snapshot


def run():
    chain = Chain()
    servers = [chain.provider(), chain.provider()]
    processes, logs = {}, {}
    report = {'passed': False, 'verified': False, 'all_requested_features_complete': False,
              'production_ready': False, 'checks': [], 'historical_fixture_replay': True,
              'test_freshness_ms': FRESHNESS_MS, 'scope': 'original-standard-type0-ng-historical-replay'}
    try:
        with Infrastructure() as infra, tempfile.TemporaryDirectory(prefix='xone-curve-ng-') as folder:
            root = Path(folder)
            token = secrets.token_urlsafe(32)
            names = {'go': 'chain-parser-go', 'rust': 'chain-parser-rust', 'router1': 'dex-router', 'router2': 'dex-router'}
            ports = {name: port() for name in names}
            configs = {}
            sources = [f'http://127.0.0.1:{ports[a]}' for a in ['go', 'rust']]
            for actor, name in names.items():
                config = config_for(name, root / actor, token, ports[actor])
                config['journal'].update(transport='kafka', source_urls=sources, poll_interval_ms=100, request_timeout_ms=15000)
                config['journal']['kafka'].update(brokers=[infra.broker], topics={'evm:31337': 'canonical.evm-31337.v1', 'solana:localnet': 'canonical.solana-localnet.v1'}, replica_id=actor, transaction_timeout_ms=4000)
                config['control'].update(backend='disabled' if name == 'dex-router' else 'postgres', dsn_file='' if name == 'dex-router' else str(infra.dsnfile), instance_id=actor, lease_ttl_ms=30000, renewal_ms=1000, min_commit_remaining_ms=7000)
                configure_freshness(config)
                if actor == 'go':
                    config['chains'][0].update(genesis_hash=GENESIS, confirmation_tag='latest', auto_reorg_window=4,
                                              rpc_urls=[f'http://127.0.0.1:{s.server_port}' for s in servers], pools=[i['config'] for i in chain.items])
                elif actor == 'rust':
                    sample_for('solana:localnet', root / actor / 'fixtures', int(time.time() * 1000))
                else:
                    config['routing']['quote_ttl_ms'] = 60000
                configs[actor] = save_config(root / actor / 'config', config)

            def base(actor):
                return f'http://127.0.0.1:{ports[actor]}'
            def start(actor):
                logs[actor] = (OUT / f'curve-ng-{actor}.log').open('ab')
                processes[actor] = subprocess.Popen(service_command(names[actor], configs[actor], root / actor / 'fixtures' if actor == 'rust' else None), cwd=ROOT / names[actor], stdout=logs[actor], stderr=subprocess.STDOUT)
            def stop(actor):
                process = processes.pop(actor, None)
                if process:
                    process.terminate()
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
                if actor in logs:
                    logs.pop(actor).close()
            def status(actor, key='evm:31337'):
                return http(base(actor) + '/internal/chains/' + key + '/status', token)
            def owner():
                code, snapshot = http(base('go') + '/internal/chains/evm:31337/baseline', token)
                require(code == 200, ('NG owner unavailable', code, snapshot))
                return snapshot
            def synced():
                await_condition(lambda: status('go')[1].get('snapshot_ref', {}).get('block', {}).get('number_or_slot') == chain.head, 'NG owner did not reach source bank')
                snapshot = owner()
                for actor in ['router1', 'router2']:
                    await_condition(lambda: status(actor)[1].get('snapshot_ref') == snapshot['reference'], 'NG replica source reference mismatch')
                return snapshot
            def complete_bank():
                snapshot = synced()
                actual = {pool['pool_id']: pool for pool in snapshot['pools']}
                expected = {item['pool']['pool_id']: item['pool'] for item in chain.items}
                require(actual == expected, 'complete NG owner balance/admin/oracle bank differs from original')
                block = snapshot['reference']['block']
                require(block['timestamp_ms'] == chain.timestamp * 1000 and block['hash'] == chain.block(chain.head)['hash'], 'NG original source identity changed')
                request = urllib.request.Request(base('go') + '/internal/chains/evm:31337/bootstrap', headers={'Authorization': 'Bearer ' + token})
                with urllib.request.urlopen(request, timeout=10) as response:
                    require(response.headers['content-type'] == 'application/x-protobuf', 'NG bootstrap is not protobuf')
                    raw = response.read()
                encoded = subprocess.run(['protoc', '-I', str(ROOT / 'common/protoc/proto'), '--encode=dex.v1.OnlineSnapshot', str(ROOT / 'common/protoc/proto/dex/v1/state.proto')], input=protobuf_text(snapshot).encode(), capture_output=True, check=True).stdout
                require(bootstrap_snapshot(raw) == encoded, 'live NG protobuf lost/changed full original state')
                return snapshot
            def quote(actor, item):
                pool, native = item['pool'], item['original']
                assets = pool['multi_asset']['assets']
                request = {'chainKey': 'evm:31337', 'kind': 'EXACT_IN', 'sellToken': assets[native['index_in']]['token'], 'buyToken': assets[native['index_out']]['token'], 'sellAmount': native['input'], 'caller': assets[0]['token'], 'recipient': assets[1]['token'], 'slippageBps': 50, 'verification': 'FAST'}
                code, quoted = http(base(actor) + '/v1/quote', token, request)
                require(code == 200, ('NG quote failed', item['name'], item['step_index'], code, quoted))
                require(quoted['grossBuyAmount'] == native['expected']['amount_out'] and quoted['consumedIn'] == native['input'] and quoted['refund'] == '0', 'NG original output/consumption mismatch')
                require(quoted['snapshotRef'] == owner()['reference'] and quoted['onlineGeneration'] == quoted['snapshotRef']['online_generation'], 'NG quote mixed source bank')
                require(quoted['verification']['level'] == 'INDICATIVE', 'NG verification overstated')
                require(set(quoted['quality']['candidateScope']) == {i['pool']['pool_id'] for i in chain.items} and len(quoted['quality']['candidateScope']) == 7, 'NG incomplete candidate scope')
                require(bool(quoted['routes']) and all(len(route['path']['edges']) == 1 and route['path']['edges'][0]['pool_id'] == pool['pool_id'] for route in quoted['routes']), 'NG quote used a different physical pool')
                return quoted, request
            try:
                for actor in ['go', 'rust']:
                    start(actor)
                    wait(base(actor) + '/health/ready')
                for actor in ['router1', 'router2']:
                    start(actor)
                    wait(base(actor) + '/health/live')
                    wait_scope(base(actor), token, 'evm:31337')
                    wait_scope(base(actor), token, 'solana:localnet')
                report['checks'].append(PROCESS_CHECKS[0])
                phase_hashes = []
                quote_checks = 0
                for phase in range(8):
                    if phase:
                        chain.advance(step_index=phase)
                    snapshot = complete_bank()
                    phase_hashes.append(digest(json.dumps(snapshot, sort_keys=True).encode()))
                    for actor in ['router1', 'router2']:
                        for item in chain.items:
                            quote(actor, item)
                            quote_checks += 1
                report['checks'].extend(PROCESS_CHECKS[1:4])
                recoveries = []
                for fault in FAULTS:
                    old = owner()['reference']['online_generation']
                    chain.advance(fault=fault)
                    for actor in ['router1', 'router2']:
                        await_condition(lambda: status(actor)[0] == 503, 'invalid NG scope remained ready')
                        require(status(actor, 'solana:localnet')[0] == 200, 'unrelated Solana scope was revoked')
                        request = quote_request(chain.items[0])
                        code, _ = http(base(actor) + '/v1/quote', token, request)
                        require(code == 503, 'withdrawn NG scope still quoted')
                    chain.advance()
                    await_condition(lambda: status('go')[1].get('snapshot_ref', {}).get('online_generation', 0) > old, 'NG recovery reused prior generation')
                    snapshot = complete_bank()
                    recoveries.append({'fault': fault, 'old_generation': old, 'recovered_generation': snapshot['reference']['online_generation']})
                    for actor in ['router1', 'router2']:
                        quote(actor, chain.items[-1])
                report['checks'].extend(PROCESS_CHECKS[4:6])
                stop('router2')
                start('router2')
                wait(base('router2') + '/health/live')
                wait_scope(base('router2'), token, 'evm:31337')
                complete_bank()
                quote('router2', chain.items[-1])
                report['checks'].append(PROCESS_CHECKS[6])
                for actor in ['router1', 'router2']:
                    quoted, request = quote(actor, chain.items[0])
                    request['verification'] = 'VERIFIED'
                    code, value = http(base(actor) + '/v1/quote', token, request)
                    require(code == 422 and 'INDEPENDENT_SIMULATION_UNAVAILABLE' in json.dumps(value), 'NG VERIFIED enabled')
                    code, _ = http(base(actor) + '/v1/quotes/' + quoted['quoteId'] + '/build', token, {})
                    require(code == 422, 'NG build enabled')
                report['checks'].append(PROCESS_CHECKS[7])
                require(report['checks'] == PROCESS_CHECKS and quote_checks == 112, 'missing NG process checks')
                report.update(passed=True, pool_count=7, service_steps=56, quote_checks=quote_checks, fault_names=FAULTS,
                              recoveries=recoveries, phase_snapshot_sha256=phase_hashes, pinned_reads=chain.pinned,
                              receipt_reads=chain.receipts, trust='SYNTHETIC_RPC_NOT_DEPLOYMENT_PROOF')
            finally:
                for actor in list(processes):
                    stop(actor)
    finally:
        for server in servers:
            server.shutdown()
            server.server_close()
    return report


def quote_request(item):
    native = item['original']
    assets = item['pool']['multi_asset']['assets']
    return {'chainKey': 'evm:31337', 'kind': 'EXACT_IN', 'sellToken': assets[native['index_in']]['token'],
                  'buyToken': assets[native['index_out']]['token'], 'sellAmount': native['input'],
                  'caller': assets[0]['token'], 'recipient': assets[1]['token'], 'slippageBps': 50, 'verification': 'FAST'}


def main():
    target = OUT / 'curve-ng-three-service-smoke.json'
    write_result(target, {'passed': False, 'state': 'RUNNING'})
    try:
        result = run()
        result['measured_at_unix_ms'] = int(time.time() * 1000)
        write_result(target, result)
        print(json.dumps(result, indent=2))
    except BaseException as error:
        write_result(target, {'passed': False, 'error': str(error), 'measured_at_unix_ms': int(time.time() * 1000)})
        raise


if __name__ == '__main__':
    main()
