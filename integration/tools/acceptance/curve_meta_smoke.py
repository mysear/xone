#!/usr/bin/env python3
"""Frozen MetaUSD original exchanges through normal Go RPC, PG/Kafka and routers.

The RPC banks contain historical synthetic state. A large test-only freshness
window preserves original January 1 timestamps. Passing is not deployment proof.
"""
from pathlib import Path
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import hashlib
import json
import secrets
import subprocess
import tempfile
import threading
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[3]
OUT = ROOT / 'integration/artifacts/local'
GENESIS = '0x' + '99' * 32
FRESHNESS_MS = 315360000000
BASE_POOL = '0xbebc44782c7db0a1a60cb6fe97d0b483032ff1c7'
BASE_LP = '0x6c3f90f043a72fa612cbac8115ee7e52bde6e490'
TEMPLATE_BYTES = 20060
TEMPLATE_SHA256 = 'c5351fd332fdf0f04486a838b2bceba15d7a45965e3df61e9a0ccedc674d450d'
FAULTS = ['base-code', 'virtual-price', 'accounting', 'implementation', 'lp-decimals']
PROCESS_CHECKS = [
    'four-metausd-pools-original-clone-implementation-and-historical-rpc',
    'complete-meta-balances-admin-base-vp-and-dependency-digests-in-live-protobuf',
    '32-original-exchanges-64-router-requests-62-positive-quotes-two-dust-rejections',
    'historical-banks-preserve-original-time-and-complete-source-identity',
    'five-invalid-meta-dependencies-withdraw-whole-evm-scope-and-recover-higher-generation',
    'unrelated-solana-scope-remains-ready',
    'memory-router-restart-restores-meta-bank',
    'verified-and-build-remain-disabled',
    'native-dust-explicitly-rejected',
]


def initial_report():
    return {'passed': False, 'state': 'RUNNING', 'production_ready': False,
            'verified': False, 'all_requested_features_complete': False, 'checks': [],
            'historical_fixture_replay': True, 'test_freshness_ms': FRESHNESS_MS,
            'pool_count': 4, 'service_steps': 32, 'quote_checks': 0,
            'successful_quotes': 0, 'dust_rejections': 0,
            'fault_names': list(FAULTS), 'read_counts': {}, 'pinned_reads': 0,
            'receipt_reads': 0, 'scope': 'original-metausd-fixed-base-direct-historical-replay',
            'trust': 'SYNTHETIC_RPC_NOT_DEPLOYMENT_PROOF'}


def write_result(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_name(path.name + '.' + secrets.token_hex(6) + '.tmp')
    try:
        temporary.write_text(json.dumps(value, indent=2) + '\n')
        temporary.replace(path)
    finally:
        temporary.unlink(missing_ok=True)


# Invalidate old success before any fallible local or optional dependency import.
if __name__ == '__main__':
    write_result(OUT / 'curve-meta-three-service-smoke.json', initial_report())

from infrastructure import Infrastructure, port
from configuration import config_for, save_config, service_command, sample_for
from receipt_reference import EMPTY_ROOT, SINGLE_EMPTY_ROOT, SINGLE_EMPTY_FIELDS
from kafka_smoke import wait
from reorg_smoke import await_condition
from smoke import http, wait_scope
from curve_ng_smoke import bootstrap_snapshot
from curve_meta_fixture import NAMES, profile, code_bytes, implementation_code, dependency_code


def digest(raw):
    return hashlib.sha256(raw).hexdigest()


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
    require(NAMES == ['meta-0', 'meta-6', 'meta-8', 'meta-18'], 'MetaUSD scenario inventory changed')
    runtime = implementation_code()
    require(len(runtime) == TEMPLATE_BYTES <= 24576 and digest(runtime) == TEMPLATE_SHA256,
            'original MetaUSD implementation runtime mismatch')
    timestamps = []
    common_dependencies = set()
    for phase in range(8):
        items = [profile(name, phase) for name in NAMES]
        states = [item['pool']['multi_asset']['curve_meta'] for item in items]
        times = {state['timestamp'] for state in states}
        prices = {state['base_virtual_price'] for state in states}
        require(len(times) == len(prices) == 1, 'shared MetaUSD phase time/base VP differs')
        timestamps.append(times.pop())
        for item, meta in zip(items, states):
            pool, cfg = item['pool'], item['config']['multi_asset']
            state = pool['multi_asset']; assets = state['assets']; native = item['original']
            require(pool['protocol'] == 'curve-metausd-direct' and
                    pool['adapter_version'] == 'curve-metausd-direct/5272fd2b-fixed-base-1',
                    'unexpected MetaUSD adapter')
            clone = (bytes.fromhex('363d3d373d3d3d363d73') + bytes.fromhex(meta['implementation'][2:]) +
                     bytes.fromhex('5af43d82803e903d91602b57fd5bf3'))
            require(code_bytes(item['name']) == clone and digest(clone) == cfg['expected_code_sha256'],
                    'original MetaUSD clone template/implementation mismatch')
            require(meta['implementation_code_sha256'] == cfg['expected_implementation_code_sha256'] == TEMPLATE_SHA256
                    and cfg['expected_implementation'] == meta['implementation'], 'implementation policy mismatch')
            require(meta['base_pool'] == BASE_POOL and assets[1]['token'] == BASE_LP and
                    assets[1]['scaling'] == str(10**18) and len(assets) == 2 and state['weights'] == [] and
                    cfg['token_policy'] == 'standard-base-lp', 'fixed-base LP policy mismatch')
            require(digest(dependency_code(item['name'], BASE_POOL)) == meta['base_pool_code_sha256'] ==
                    cfg['expected_base_pool_code_sha256'], 'base dependency digest mismatch')
            for index, asset in enumerate(assets):
                require(digest(dependency_code(item['name'], asset['token'])) == asset['code_sha256'] ==
                        cfg['tokens'][index]['expected_code_sha256'], 'token runtime policy mismatch')
                require(int(native['stored_balances'][index]) - int(native['admin_balances'][index]) ==
                        int(asset['balance']), 'original effective accounting mismatch')
            require(not native['expected'].get('reverted'), 'original direct exchange reverted')
            common_dependencies.add((meta['implementation'], meta['implementation_code_sha256'],
                                     meta['base_pool_code_sha256'], assets[1]['code_sha256']))
    require(len(common_dependencies) == 1, 'shared MetaUSD dependency identity differs')
    require(timestamps == list(range(1767225600, 1767225608)), 'frozen historical phase times changed')
    return {'coin_counts': [2] * 4, 'scenario_count': 4, 'step_count': 8, 'timestamps': timestamps}


class Chain:
    def __init__(self):
        check_fixtures()
        self.lock = threading.RLock()
        self.head = 99
        self.step_index = 0
        self.banks = {}
        self.pinned = 0
        self.receipts = 0
        self.read_counts = {}
        self.advance(step_index=0)

    def advance(self, step_index=None, fault=None):
        with self.lock:
            require(step_index is None or type(step_index) is int and 0 <= step_index < 8, 'invalid replay phase')
            require(fault is None or fault in FAULTS, 'unknown injected fault')
            phase = self.step_index if step_index is None else step_index
            number = self.head + 1
            blockhash = '0x' + digest(f'META/process/{number}'.encode())
            items = [profile(name, phase, number, blockhash) for name in NAMES]
            timestamp = items[0]['pool']['multi_asset']['curve_meta']['timestamp']
            self.banks[number] = {'items': items, 'fault': fault, 'timestamp': timestamp, 'hash': blockhash}
            self.head, self.step_index, self.items, self.timestamp = number, phase, items, timestamp

    def block(self, number):
        if number == 0:
            return {'number': '0x0', 'hash': GENESIS, 'parentHash': '0x' + '00' * 32,
                    'timestamp': hex(1767225600), 'receiptsRoot': EMPTY_ROOT, 'transactions': []}
        require(0 < number <= self.head, 'unknown block height')
        bank = self.banks.get(number)
        blockhash = bank['hash'] if bank else '0x' + digest(f'META/process/{number}'.encode())
        parent = GENESIS if number == 1 else '0x' + digest(f'META/process/{number-1}'.encode())
        return {'number': hex(number), 'hash': blockhash, 'parentHash': parent,
                'timestamp': hex(bank['timestamp'] if bank else 1767225600),
                'receiptsRoot': SINGLE_EMPTY_ROOT,
                'transactions': ['0x' + digest(('tx/' + blockhash).encode())]}

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
            self.read_counts[method] = self.read_counts.get(method, 0) + 1
            target = (args[0]['to'] if method == 'eth_call' else args[0]).lower()
            items, fault = bank['items'], bank['fault']
            by_pool = {item['pool']['pool_id']: item for item in items}
            first = items[0]
            common = first['pool']['multi_asset']['curve_meta']
            implementation = common['implementation']
            item = by_pool.get(target)
            if item is None:
                item = next((candidate for candidate in items if target in
                             [a['token'] for a in candidate['pool']['multi_asset']['assets']]), None)
            require(item is not None or target in [BASE_POOL, implementation], 'unknown RPC target')
            item = item or first
            pool, native = item['pool'], item['original']
            state = pool['multi_asset']; meta = state['curve_meta']; assets = state['assets']
            if method == 'eth_getCode':
                raw = code_bytes(item['name']) if target in by_pool else dependency_code(item['name'], target)
                if (fault == 'base-code' and target == BASE_POOL or
                        fault == 'implementation' and target == implementation):
                    raw = raw[:-1] + bytes([raw[-1] ^ 1])
                return '0x' + raw.hex()
            if method == 'eth_getStorageAt':
                require(target in by_pool and int(args[1], 16) == 11, 'unknown MetaUSD storage slot')
                return '0x' + word(assets[0]['scaling'])
            data = args[0]['data'].lower()
            selector = data[:10]
            key = 'eth_call:' + selector
            self.read_counts[key] = self.read_counts.get(key, 0) + 1
            if selector == '0x313ce567':
                require(len(data) == 10 and target in [a['token'] for a in assets], 'invalid decimals target/calldata')
                index = next(i for i, a in enumerate(assets) if a['token'] == target)
                decimals = 18 if index == 1 else 36 - (len(assets[0]['scaling']) - 1)
                if target == BASE_LP and fault == 'lp-decimals':
                    decimals = 17
                return '0x' + word(decimals)
            if selector == '0x70a08231':
                require(len(data) == 74, 'invalid balanceOf calldata')
                owner = '0x' + data[34:]
                require(int(data[10:34], 16) == 0 and owner in by_pool, 'unknown balanceOf owner')
                owned = by_pool[owner]
                owned_assets = owned['pool']['multi_asset']['assets']
                index = next((i for i, a in enumerate(owned_assets) if a['token'] == target), None)
                require(index is not None, 'balanceOf token does not belong to owner')
                raw = int(owned['original']['stored_balances'][index])
                if fault == 'accounting' and owned['name'] == NAMES[-1] and index == 0:
                    raw += 1
                return '0x' + word(raw)
            if selector == '0xbb7b8b80':
                require(target == BASE_POOL and len(data) == 10, 'invalid base VP target/calldata')
                return '0x' + word(0 if fault == 'virtual-price' else common['base_virtual_price'])
            require(target in by_pool, 'getter target mismatch')
            scalars = {'0x76a2f0f0': state['amplification'], '0xddca3f43': pool['fee_numerator'],
                       '0xfee3f7f9': state['admin_fee']}
            if selector in scalars:
                require(len(data) == 10, 'invalid scalar getter calldata')
                return '0x' + word(scalars[selector])
            if selector in ['0xc6610657', '0x4903b0d1', '0xe2e7d264']:
                require(len(data) == 74, 'invalid indexed calldata')
                index = int(data[10:], 16)
                require(index < 2, 'invalid coin index')
                values = {'0xc6610657': [int(a['token'], 16) for a in assets],
                          '0x4903b0d1': [a['balance'] for a in assets], '0xe2e7d264': meta['admin_balances']}
                return '0x' + word(values[selector][index])
            raise ValueError('unexpected MetaUSD selector ' + selector)

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
                except (ValueError, KeyError, IndexError, TypeError, StopIteration) as error:
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


# Local extension: the NG encoder has no base VP or implementation/base digests.
AMOUNTS = {'reserve0', 'reserve1', 'balance', 'scaling', 'weights', 'amplification',
           'stored_balances', 'admin_balances', 'base_virtual_price'}
DIGESTS = {'hash', 'parent_hash', 'adapter_set_hash', 'scope_hash', 'code_sha256',
           'vault_code_sha256', 'block_hash', 'implementation_code_sha256', 'base_pool_code_sha256'}


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


def run():
    chain = Chain()
    servers = [chain.provider(), chain.provider()]
    processes, logs = {}, {}
    report = initial_report()
    try:
        with Infrastructure() as infra, tempfile.TemporaryDirectory(prefix='xone-curve-meta-') as folder:
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
                logs[actor] = (OUT / f'curve-meta-{actor}.log').open('ab')
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
                require(code == 200, ('MetaUSD owner unavailable', code, snapshot))
                return snapshot
            def synced():
                await_condition(lambda: status('go')[1].get('snapshot_ref', {}).get('block', {}).get('number_or_slot') == chain.head, 'MetaUSD owner did not reach source bank')
                snapshot = owner()
                for actor in ['router1', 'router2']:
                    await_condition(lambda: status(actor)[1].get('snapshot_ref') == snapshot['reference'], 'MetaUSD replica source reference mismatch')
                return snapshot
            def complete_bank():
                snapshot = synced()
                actual = {pool['pool_id']: pool for pool in snapshot['pools']}
                expected = {item['pool']['pool_id']: item['pool'] for item in chain.items}
                require(len(snapshot['pools']) == 4 and actual == expected,
                        'complete MetaUSD owner stored/admin/base/implementation bank differs from original')
                block = snapshot['reference']['block']
                require(block['timestamp_ms'] == chain.timestamp * 1000 and block['hash'] == chain.block(chain.head)['hash'], 'MetaUSD original source identity changed')
                request = urllib.request.Request(base('go') + '/internal/chains/evm:31337/bootstrap', headers={'Authorization': 'Bearer ' + token})
                with urllib.request.urlopen(request, timeout=10) as response:
                    require(response.headers['content-type'] == 'application/x-protobuf', 'MetaUSD bootstrap is not protobuf')
                    raw = response.read()
                encoded = subprocess.run(['protoc', '-I', str(ROOT / 'common/protoc/proto'), '--encode=dex.v1.OnlineSnapshot', str(ROOT / 'common/protoc/proto/dex/v1/state.proto')], input=protobuf_text(snapshot).encode(), capture_output=True, check=True).stdout
                require(bootstrap_snapshot(raw) == encoded, 'live MetaUSD protobuf lost/changed full original state')
                return snapshot
            def quote(actor, item):
                pool, native = item['pool'], item['original']
                assets = pool['multi_asset']['assets']
                request = {'chainKey': 'evm:31337', 'kind': 'EXACT_IN', 'sellToken': assets[native['index_in']]['token'], 'buyToken': assets[native['index_out']]['token'], 'sellAmount': native['input'], 'caller': assets[0]['token'], 'recipient': assets[1]['token'], 'slippageBps': 50, 'verification': 'FAST'}
                code, quoted = http(base(actor) + '/v1/quote', token, request)
                if validate_quote_result(item, code, quoted) == 'dust':
                    return quoted, request
                require(quoted['snapshotRef'] == owner()['reference'] and quoted['onlineGeneration'] == quoted['snapshotRef']['online_generation'], 'MetaUSD quote mixed source bank')
                require(quoted['verification']['level'] == 'INDICATIVE', 'MetaUSD verification overstated')
                require(set(quoted['quality']['candidateScope']) == {i['pool']['pool_id'] for i in chain.items} and len(quoted['quality']['candidateScope']) == 4, 'MetaUSD incomplete candidate scope')
                require(bool(quoted['routes']) and all(len(route['path']['edges']) == 1 and route['path']['edges'][0]['pool_id'] == pool['pool_id'] for route in quoted['routes']), 'MetaUSD quote used a different physical pool')
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
                quote_checks = successful_quotes = dust_rejections = 0
                for phase in range(8):
                    if phase:
                        chain.advance(step_index=phase)
                    snapshot = complete_bank()
                    phase_hashes.append(digest(json.dumps(snapshot, sort_keys=True).encode()))
                    for actor in ['router1', 'router2']:
                        for item in chain.items:
                            quote(actor, item)
                            quote_checks += 1
                            if int(item['original']['expected']['amount_out']) == 0:
                                dust_rejections += 1
                            else:
                                successful_quotes += 1
                report['checks'].extend(PROCESS_CHECKS[1:4])
                recoveries = []
                for fault in FAULTS:
                    old = owner()['reference']['online_generation']
                    chain.advance(fault=fault)
                    await_condition(lambda: status('go')[0] == 200 and status('go')[1].get('ready') is False, 'invalid MetaUSD owner scope remained ready')
                    for actor in ['router1', 'router2']:
                        await_condition(lambda: status(actor)[0] == 503, 'invalid MetaUSD scope remained ready')
                        require(status(actor, 'solana:localnet')[0] == 200, 'unrelated Solana scope was revoked')
                        request = quote_request(chain.items[0])
                        code, _ = http(base(actor) + '/v1/quote', token, request)
                        require(code == 503, 'withdrawn MetaUSD scope still quoted')
                    chain.advance()
                    await_condition(lambda: status('go')[1].get('snapshot_ref', {}).get('online_generation', 0) > old, 'MetaUSD recovery reused prior generation')
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
                    require(code == 422 and 'INDEPENDENT_SIMULATION_UNAVAILABLE' in json.dumps(value), 'MetaUSD VERIFIED enabled')
                    code, _ = http(base(actor) + '/v1/quotes/' + quoted['quoteId'] + '/build', token, {})
                    require(code == 422, 'MetaUSD build enabled')
                report['checks'].append(PROCESS_CHECKS[7])
                require(successful_quotes == 62 and dust_rejections == 2, 'incorrect native dust/parity counts')
                report['checks'].append(PROCESS_CHECKS[8])
                require(report['checks'] == PROCESS_CHECKS and quote_checks == 64, 'missing MetaUSD process checks')
                report.update(passed=True, state='PASSED', pool_count=4, service_steps=32, quote_checks=quote_checks,
                              successful_quotes=successful_quotes, dust_rejections=dust_rejections, fault_names=list(FAULTS),
                              recoveries=recoveries, phase_snapshot_sha256=phase_hashes, pinned_reads=chain.pinned,
                              receipt_reads=chain.receipts, read_counts=dict(chain.read_counts), trust='SYNTHETIC_RPC_NOT_DEPLOYMENT_PROOF')
            finally:
                for actor in list(processes):
                    stop(actor)
    finally:
        for server in servers:
            server.shutdown()
            server.server_close()
    return report


def validate_quote_result(item, code, quoted):
    native = item['original']
    require(not native['expected'].get('reverted'), 'unexpected reverted reference exchange')
    if int(native['expected']['amount_out']) == 0:
        require(isinstance(quoted, dict), 'dust response must be an error object')
        require(code == 404 and quoted.get('error') == 'NO_ROUTE' and
                'quoteId' not in quoted and 'quote_id' not in quoted,
                ('native MetaUSD dust must reject without quoteId', item['name'], item['step_index'], code, quoted))
        return 'dust'
    require(code == 200, ('MetaUSD quote failed', item['name'], item['step_index'], code, quoted))
    require(quoted['grossBuyAmount'] == native['expected']['amount_out'] and
            quoted['consumedIn'] == native['input'] and quoted['refund'] == '0',
            'MetaUSD original output/consumption mismatch')
    return 'success'


def quote_request(item):
    native = item['original']
    assets = item['pool']['multi_asset']['assets']
    return {'chainKey': 'evm:31337', 'kind': 'EXACT_IN', 'sellToken': assets[native['index_in']]['token'],
                  'buyToken': assets[native['index_out']]['token'], 'sellAmount': native['input'],
                  'caller': assets[0]['token'], 'recipient': assets[1]['token'], 'slippageBps': 50, 'verification': 'FAST'}


def main():
    target = OUT / 'curve-meta-three-service-smoke.json'
    write_result(target, initial_report())
    try:
        result = run()
        result['measured_at_unix_ms'] = int(time.time() * 1000)
        write_result(target, result)
        print(json.dumps(result, indent=2))
    except BaseException as error:
        write_result(target, {**initial_report(), 'state': 'FAILED', 'error': str(error), 'measured_at_unix_ms': int(time.time() * 1000)})
        raise


if __name__ == '__main__':
    main()
