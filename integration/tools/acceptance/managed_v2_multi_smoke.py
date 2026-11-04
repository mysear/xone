#!/usr/bin/env python3
"""T34.8/T34.9 native multi-pool RPC banks through PG/Kafka/two replicas.

The two RPC endpoints intentionally replay the same chain; they test withdrawal,
not independent trust. Each scope owns fresh infrastructure and source copies.
Only the CLI process run can emit PASSED. Helper tests never write a pass report.
"""
import argparse
import copy
from contextlib import contextmanager, ExitStack
import hashlib
import json
import os
from pathlib import Path
import secrets
import shutil
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request

sys.dont_write_bytecode = True
import managed_v2_catalog as catalog
import managed_v2_smoke as single

base = single.base
projection = single.projection
wire = single.wire
ROOT = single.ROOT
OUT = single.OUT
SCOPES = ('catalog', 'shared')
REFERENCE = {
    'catalog': ROOT / 'integration/test/contracts/managed-v2-aum-catalog.json',
    'shared': ROOT / 'integration/test/contracts/managed-v2-shared-vault.json',
}
SHARED_SHA256 = '1f917c9951269028d24f927e1d415229f200019881bd05991e8a84f0b49c79e2'
SHARED_VALIDATOR = ROOT / 'integration/tools/ci/managed-v2-shared-vault-golden.cjs'
PLAN_REPLAY = ROOT / 'integration/tools/ci/managed-v2-shared-plan-replay.cjs'
REPORTS = {scope: OUT / f'managed-v2-{scope}-three-service-smoke.json' for scope in SCOPES}
require = projection.require


def sha(value):
    return hashlib.sha256(value).hexdigest()


def node():
    return os.environ.get('XONE_NODE') or shutil.which('node25.3.0') or '/usr/local/bin/node'


def faults_for(scope):
    require(scope in SCOPES, 'unknown scope')
    return ('missing-getter', 'wallet-mismatch',
            'vault-pause-source-poison' if scope == 'shared' else 'factory-code-source-poison', 'future-head')


def native_request(case):
    """Use native Swap arguments and key4 funds, without generating prices."""
    swap, funds, *_ = case['execution']['args']
    event = case['expected_native']['swap_event']
    require((swap['assetIn'].lower(), swap['assetOut'].lower(), swap['amount']) ==
            (event['token_in'], event['token_out'], event['amount_in']), 'native request/event differs')
    caller = case['execution']['transaction']['from'].lower()
    require(caller == funds['sender'].lower(), 'native sender differs')
    return dict(chainKey='evm:31337', kind='EXACT_IN', sellToken=event['token_in'],
                buyToken=event['token_out'], sellAmount=event['amount_in'], caller=caller,
                recipient=funds['recipient'].lower(), slippageBps=50, verification='FAST', constraints={'maxHops': 1})


def source_id(scope, case):
    return case['source_bank4pools' if scope == 'catalog' else 'source_bank2pools']


def load_chain(scope):
    """Authenticate the repository copy using its complete original validator."""
    prefix = projection.load(ROOT / 'integration/test/contracts/managed-v2-aum.json')
    if scope == 'catalog':
        authenticated = catalog.load(REFERENCE[scope])
        projected = catalog.project(authenticated)
        raw = authenticated.raw
    else:
        data = REFERENCE[scope].read_bytes()
        require(sha(data) == SHARED_SHA256, 'shared reference SHA256 differs')
        checked = subprocess.run([node(), str(SHARED_VALIDATOR), '--validate', str(REFERENCE[scope])],
                                 capture_output=True, text=True, timeout=180)
        require(checked.returncode == 0, 'shared original validation failed: ' + checked.stderr[-4000:])
        require(json.loads(checked.stdout)['passed'], 'shared original validation did not pass')
        require(REFERENCE[scope].read_bytes() == data, 'shared raw changed during validation')
        raw, projected = json.loads(data, object_pairs_hook=projection._pairs), None
    chain = ReplayChain(scope, raw, prefix)
    if projected is not None:
        first = chain.frames[0]['id']
        chain.configs = copy.deepcopy([p['config'] for p in projected['banks'][first]['pools']])
    return chain


class ReplayChain:
    provider = base.Chain.provider

    def __init__(self, scope, raw, prefix):
        require(scope in SCOPES, 'unknown scope')
        self.scope, self.raw = scope, copy.deepcopy(raw)
        self.lock = threading.RLock()
        self.reads, self.getter_reads, self.code_reads = {}, {}, {}
        self.receipt_reads, self.fault_hits = 0, 0
        self.headers, self.receipts, self.codes = {}, {}, {}
        self.reference_sha256 = sha(REFERENCE[scope].read_bytes())
        self.cases = copy.deepcopy(raw['cases'])
        require(len(raw['banks']) == (10 if scope == 'catalog' else 7), 'source bank inventory')
        require(len(self.cases) == (8 if scope == 'catalog' else 5), 'native case inventory')
        require(sum(c['operation'] == 'swap' for c in self.cases) == 4, 'four native swaps required')

        def add_header(h):
            n = int(h['number'], 16)
            require(n not in self.headers or self.headers[n] == h, 'conflicting original header')
            self.headers[n] = copy.deepcopy(h)

        def add_receipt(r):
            tx = r['transactionHash']
            require(tx not in self.receipts or self.receipts[tx] == r, 'conflicting original receipt')
            self.receipts[tx] = copy.deepcopy(r)

        for row in prefix['objects'].values():
            value = row['value']
            if row['kind'] == 'header' and int(value['number'], 16) <= 28:
                add_header(value)
            elif row['kind'] == 'transaction' and int(value['receipt']['blockNumber'], 16) <= 28:
                add_receipt(value['receipt'])
        for h in raw['headers']:
            add_header(h)
        for transaction in raw['transactions']:
            add_receipt(transaction['receipt'])
        final = max(self.headers)
        require(sorted(self.headers) == list(range(final + 1)), 'missing original genesis/parent header')
        for n in range(1, final + 1):
            require(self.headers[n]['parentHash'] == self.headers[n-1]['hash'], 'original parent continuity differs')
        for r in self.receipts.values():
            require(r['blockHash'] == self.headers[int(r['blockNumber'], 16)]['hash'], 'receipt/header anchor differs')
        for h in self.headers.values():
            for tx in h['transactions']:
                require(type(tx) is str and tx in self.receipts, 'original block transaction receipt absent')
        for identity in raw['identities'] + raw['factory_creation_code_storage']:
            address, code = identity['address'].lower(), identity['code_hex']
            require(address not in self.codes or self.codes[address] == code, 'conflicting original runtime')
            self.codes[address] = code

        frames = []
        for ident, bank in raw['banks'].items():
            h = bank['header']
            require(bank['id'] == ident == 'H'+str(int(h['number'], 16)), 'bank/header identity differs')
            require(h == bank['header_anchor_before'] == bank['header_anchor_after'] ==
                    self.headers[int(h['number'], 16)], 'bank fixed original header differs')
            pools, getters, methods = [], {}, {}
            for row in bank['pools']:
                require(row['header'] == row['header_anchor_before'] == row['header_anchor_after'] == h,
                        'pool fixed original header differs')
                pool = copy.deepcopy(row['pool'])
                state = pool['multi_asset']['managed_v2']
                require(set(state) == set(catalog.MANAGED_FIELDS), 'complete 39-field observed state required')
                require((state['block_number'], state['block_hash'], state['timestamp']) ==
                        (int(h['number'], 16), h['hash'], int(h['timestamp'], 16)), 'pool fixed anchor differs')
                require(len(row['getters']) == row['getter_count'], 'getter inventory differs')
                for getter in row['getters']:
                    require(getter['block_hash'] == h['hash'] and getter['block_tag'] == h['number'],
                            'getter fixed anchor differs')
                    key = (getter['to'].lower(), getter['data'].lower())
                    require(key not in getters or (getters[key] == getter['result'] and methods[key] == getter['method']),
                            'conflicting shared getter at common source H')
                    getters[key], methods[key] = getter['result'], getter['method']
                pools.append(pool)
            require(len(pools) == (4 if scope == 'catalog' else 2), 'complete physical registrations required')
            require(bank['getter_count'] == sum(p['getter_count'] for p in bank['pools']), 'raw getter count differs')
            frames.append(dict(id=ident, header=copy.deepcopy(h), pools=pools, getters=getters, methods=methods))
        self.frames = tuple(sorted(frames, key=lambda f: int(f['header']['number'], 16)))
        self.by_hash = {f['header']['hash']: f for f in self.frames}
        require(len(self.by_hash) == len(self.frames), 'duplicate native bank hash')
        ids = [p['pool_id'] for p in self.frames[0]['pools']]
        for frame in self.frames:
            require([p['pool_id'] for p in frame['pools']] == ids, 'registration order changed')
        self.configs = [copy.deepcopy(projection._config(p)) for p in self.frames[0]['pools']]
        self.select(0)

    def anchor(self):
        return {'blockHash': self.frame['header']['hash'], 'requireCanonical': True}

    def select(self, index, fault=None):
        """Fault a private serving copy; never alter the immutable native frames."""
        with self.lock:
            require(fault is None or fault in faults_for(self.scope), 'unknown fault')
            self.frame = copy.deepcopy(self.frames[index])
            self.head = int(self.frame['header']['number'], 16)
            self.timestamp = int(self.frame['header']['timestamp'], 16)
            self.fault, self.fault_evidence, self.fault_hits = fault, None, 0
            if fault is None:
                return
            frame = self.frame
            method, body, before, after = 'eth_call', None, None, None
            if fault == 'future-head':
                method, body = 'eth_getBlockByNumber', 'latest'
                before, after = frame['header']['timestamp'], hex(int(time.time()) + 120)
                frame['header']['timestamp'] = after
            elif fault == 'factory-code-source-poison':
                method, body = 'eth_getCode', frame['pools'][0]['multi_asset']['managed_v2']['factory']
                before = self.codes[body]
                after = before[:-2] + ('00' if before[-2:] != '00' else '01')
            else:
                vault = frame['pools'][0]['multi_asset']['vault']
                if fault == 'missing-getter':
                    key = next(k for k, m in frame['methods'].items() if m == 'getActualSupply')
                elif fault == 'vault-pause-source-poison':
                    key = next(k for k, m in frame['methods'].items() if k[0] == vault and m == 'getPausedState')
                else:
                    # Shared registrations observe exactly one ERC20 wallet. Set
                    # it below the sum of registered cash, consistently for both
                    # pools, rather than forking identical RPC call bodies.
                    token = frame['pools'][0]['multi_asset']['assets'][0]['token']
                    key = next(k for k, m in frame['methods'].items() if k[0] == token and m == 'balanceOf'
                               and k[1].endswith(vault[2:]))
                    cash = [int(p['multi_asset']['managed_v2']['token_info'][i]['cash'])
                            for p in frame['pools'] for i, a in enumerate(p['multi_asset']['assets']) if a['token'] == token]
                    after = '0x' + base.word(sum(cash)-1 if self.scope == 'shared' else 0)
                    require(int(after, 16) != int(frame['getters'][key], 16), 'wallet fault has no effect')
                body, before = dict(to=key[0], data=key[1]), frame['getters'][key]
                if after is None:
                    del frame['getters'][key]
                else:
                    frame['getters'][key] = after
            require(before != after, 'fault has no actual effect')
            self.fault_evidence = dict(fault=fault, source_hash=self.frames[index]['header']['hash'],
                source_bank=self.frames[index]['id'], method=method, body=body, before=before, after=after,
                semantics='source poison; no per-pool fork of shared RPC keys; original native frame retained')

    def rpc(self, method, args):
        with self.lock:
            self.reads[method] = self.reads.get(method, 0) + 1
            require(type(args) is list, 'RPC params must be an array')
            if method == 'eth_chainId':
                require(not args, 'unexpected chainId body')
                return hex(self.raw['chain_options']['chain']['chainId'])
            if method == 'eth_getBlockByNumber':
                require(len(args) == 2 and args[1] is False, 'exact header request required')
                n = self.head if args[0] in ('latest', 'safe', 'finalized') else int(args[0], 16)
                require(n <= self.head and n in self.headers, 'unknown or unpublished original header')
                if self.fault == 'future-head' and n == self.head:
                    self.fault_hits += 1
                    return copy.deepcopy(self.frame['header'])
                return copy.deepcopy(self.headers[n])
            if method == 'eth_getTransactionReceipt':
                require(len(args) == 1 and args[0] in self.receipts, 'unknown original receipt')
                receipt = self.receipts[args[0]]
                require(int(receipt['blockNumber'], 16) <= self.head, 'unpublished original receipt')
                self.receipt_reads += 1
                return copy.deepcopy(receipt)
            require(method in ('eth_call', 'eth_getCode'), 'unexpected RPC ' + method)
            require(len(args) == 2 and type(args[1]) is dict and set(args[1]) == {'blockHash', 'requireCanonical'}
                    and args[1]['requireCanonical'] is True, 'exact canonical EIP1898 anchor required')
            original = self.by_hash.get(args[1]['blockHash'])
            require(original is not None and int(original['header']['number'], 16) <= self.head,
                    'unknown or unpublished source bank')
            current = original['header']['hash'] == self.frame['header']['hash']
            frame = self.frame if current else original
            if method == 'eth_getCode':
                require(type(args[0]) is str, 'invalid code body')
                address = args[0].lower()
                require(address in self.codes, 'unknown original code')
                key = original['id'] + ':' + address
                self.code_reads[key] = self.code_reads.get(key, 0) + 1
                if current and self.fault == 'factory-code-source-poison' and address == self.fault_evidence['body']:
                    self.fault_hits += 1
                    return self.fault_evidence['after']
                return self.codes[address]
            require(type(args[0]) is dict and set(args[0]) == {'to', 'data'}
                    and all(type(v) is str for v in args[0].values()), 'exact original getter body required')
            key = (args[0]['to'].lower(), args[0]['data'].lower())
            read = original['id'] + ':' + ':'.join(key)
            self.getter_reads[read] = self.getter_reads.get(read, 0) + 1
            if current and self.fault_evidence and method == self.fault_evidence['method'] and args[0] == self.fault_evidence['body']:
                self.fault_hits += 1
            require(key in frame['getters'], 'getter absent from original source or explicitly poisoned')
            return frame['getters'][key]


class Processes:
    """Own every log/process from the first startup attempt through teardown."""
    def __init__(self):
        self.processes, self.logs = {}, {}

    def __enter__(self):
        return self

    def start(self, actor, command, cwd, log_path):
        require(actor not in self.processes and actor not in self.logs, 'duplicate process startup')
        self.logs[actor] = log_path.open('ab')
        self.processes[actor] = subprocess.Popen(command, cwd=cwd, stdout=self.logs[actor], stderr=subprocess.STDOUT)

    def stop(self, actor):
        process = self.processes.pop(actor, None)
        try:
            if process is not None:
                process.terminate()
                try:
                    process.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    process.kill()
                    process.wait(timeout=10)
        finally:
            log = self.logs.pop(actor, None)
            if log is not None:
                log.close()

    def __exit__(self, *_):
        with ExitStack() as stack:
            for actor in list(dict.fromkeys([*self.logs, *self.processes])):
                stack.callback(self.stop, actor)


@contextmanager
def resources(chain):
    """Register cleanup before the next provider or infrastructure can fail."""
    with ExitStack() as stack:
        servers = []
        for _ in range(2):
            server = chain.provider()
            stack.callback(server.server_close)
            stack.callback(server.shutdown)
            servers.append(server)
        infra = base.Infrastructure()
        stack.callback(infra.close)
        infra.start()
        yield servers, infra


def initial(scope):
    return dict(scope=scope, passed=False, state='RUNNING', verified=False, production_ready=False,
        all_requested_features_complete=False, started_at_unix_ms=int(time.time()*1000), checks=[], source_frames=[], positive_plans=[], faults=[], reverts=[],
        cpp_wire=[], trust='FROZEN_LOCAL_ORIGINAL_RPC_REPLAY_NOT_TRUSTED_CHAIN_OR_EXECUTION',
        providers='two endpoints, same chain and fault model; no independent provider trust',
        independent_gas_reference=False, full_service_run=False)


def write(scope, report):
    # Do not mutate the old smoke/base module's global output target.
    target = REPORTS[scope]
    target.parent.mkdir(parents=True, exist_ok=True)
    temporary = target.with_name(target.name+'.'+secrets.token_hex(6)+'.tmp')
    try:
        temporary.write_text(json.dumps(report, indent=2)+'\n')
        temporary.replace(target)
    finally:
        temporary.unlink(missing_ok=True)


def replay_plan(chain, request, quote):
    value = dict(schema_version=1, original_bank_sha256=projection.BANK_SHA256,
                 shared_reference_sha256=chain.reference_sha256, request=request, quote=quote)
    executed = subprocess.run([node(), str(PLAN_REPLAY)], input=json.dumps(value).encode(),
                              capture_output=True, timeout=180)
    require(executed.stdout, 'independent selected-plan VM produced no receipt: '+executed.stderr.decode()[-2000:])
    actual = json.loads(executed.stdout)
    require(executed.returncode == 0 and actual.get('passed') is True and actual.get('vm_started') is True,
            'independent selected-plan VM failed: '+str(actual.get('error')))
    require(actual['actual_amount_in'] == request['sellAmount'] and
            actual['actual_amount_out'] == quote['grossBuyAmount'], 'API gross differs from actual native wallet credit')
    require(all(actual.get(k) is False for k in ('verified', 'production_ready', 'all_requested_features_complete')),
            'independent replay overstated admission')
    require(actual['shared_reference_sha256'] == chain.reference_sha256, 'independent raw hash differs')
    return actual


def run_scope(scope, report):
    report['started_at_unix_ms'] = int(time.time()*1000)
    write(scope, report)
    chain = load_chain(scope)
    fingerprint, inputs = base.runtime_fingerprint(), base.snapshot_inputs()
    report.update(source_binding=fingerprint, input_hashes=inputs, source_binding_started=fingerprint,
                  input_hashes_started=inputs, exact_reference_sha256=chain.reference_sha256,
                  original_bank_sha256=projection.BANK_SHA256, native_case_inventory=[
                      dict(name=c['name'], operation=c['operation'], source_bank=source_id(scope, c),
                           after_bank=c['expected_after_bank4pools' if scope == 'catalog' else 'expected_after_bank2pools'])
                      for c in chain.cases],
                  runtime_codes=[dict(address=a, sha256=sha(bytes.fromhex(code[2:]))) for a, code in chain.codes.items()],
                  native_header_count=len(chain.headers), native_receipt_count=len(chain.receipts),
                  original_receipts=copy.deepcopy(list(chain.receipts.values())))
    write(scope, report)
    try:
        with resources(chain) as (servers, infra), tempfile.TemporaryDirectory(prefix=f'xone-managed-v2-{scope}-') as folder:
            root, token = Path(folder), secrets.token_urlsafe(32)
            names = {'go': 'chain-parser-go', 'rust': 'chain-parser-rust', 'router1': 'dex-router', 'router2': 'dex-router'}
            ports = {actor: base.port() for actor in names}
            urls = {actor: f'http://127.0.0.1:{ports[actor]}' for actor in names}
            configs = {}
            max_splits = 2
            for actor, name in names.items():
                config = base.config_for(name, root/actor, token, ports[actor])
                config['journal'].update(transport='kafka', source_urls=[urls[a] for a in ('go', 'rust')],
                                         poll_interval_ms=100, request_timeout_ms=30000)
                config['journal']['kafka'].update(brokers=[infra.broker], topics={
                    'evm:31337': 'canonical.evm-31337.v1', 'solana:localnet': 'canonical.solana-localnet.v1'},
                    replica_id=actor, transaction_timeout_ms=4000)
                config['control'].update(backend='disabled' if name == 'dex-router' else 'postgres',
                    dsn_file='' if name == 'dex-router' else str(infra.dsnfile), instance_id=actor,
                    lease_ttl_ms=30000, renewal_ms=1000, min_commit_remaining_ms=7000)
                config['startup']['max_baseline_age_ms'] = base.FRESHNESS
                if actor == 'go':
                    config['chains'][0].update(genesis_hash=chain.headers[0]['hash'], confirmation_tag='latest',
                        auto_reorg_window=4, rpc_urls=[f'http://127.0.0.1:{s.server_port}' for s in servers], pools=chain.configs)
                elif actor == 'rust':
                    base.sample_for('solana:localnet', root/actor/'fixtures', chain.timestamp*1000)
                else:
                    config['routing'].update(max_hops=1, max_splits=max_splits, max_state_age_ms=base.FRESHNESS, quote_ttl_ms=60000)
                    config['api']['quote_timeout_ms'] = 15000
                configs[actor] = base.save_config(root/actor/'config', config)

            def http(actor, path, body=None):
                return base.http(urls[actor]+path, token, body, timeout=15)

            def status(actor, key='evm:31337'):
                return http(actor, '/internal/chains/'+key+'/status')

            def complete_bank():
                expected = chain.by_hash[chain.frame['header']['hash']]
                base.await_condition(lambda: status('go')[0] == 200 and status('go')[1].get('snapshot_ref', {}).get('block', {}).get('hash')
                                     == expected['header']['hash'], 'owner did not reach native multi-pool source bank', timeout=60)
                code, snapshot = http('go', '/internal/chains/evm:31337/baseline')
                require(code == 200 and snapshot['pools'] == expected['pools'], 'complete owner pool bank differs from original native source')
                request = urllib.request.Request(urls['go']+'/internal/chains/evm:31337/bootstrap',
                                                 headers={'Authorization': 'Bearer '+token})
                with urllib.request.urlopen(request, timeout=15) as response:
                    require(response.headers['content-type'] == 'application/x-protobuf', 'bootstrap is not protobuf')
                    raw = response.read()
                encoded = subprocess.run(['protoc', '-I', str(ROOT/'common/protoc/proto'), '--encode=dex.v1.OnlineSnapshot',
                    str(ROOT/'common/protoc/proto/dex/v1/state.proto')], input=wire.encode(snapshot).encode(),
                    capture_output=True, check=True, timeout=30).stdout
                payload = base.bootstrap_snapshot(raw)
                require(payload == encoded, 'complete live protobuf differs from C++ protoc bytes')
                replicas = {}
                for actor in ('router1', 'router2'):
                    def replica_complete():
                        code, state = status(actor)
                        return (code == 200 and state.get('ready') is True and
                                state.get('snapshot_ref') == snapshot['reference'] and
                                state.get('scope_pool_count') == len(expected['pools']))
                    base.await_condition(replica_complete, 'replica did not restore complete native bank: '+actor, timeout=60)
                    replicas[actor] = status(actor)[1]
                report['cpp_wire'].append(dict(source_bank=expected['id'], reference=snapshot['reference'],
                    pool_ids=[p['pool_id'] for p in snapshot['pools']], managed_fields=39, protobuf_sha256=sha(payload),
                    allcomplete_bootstrap=True,
                    bytes=len(payload), replicas=replicas))
                return snapshot

            def quote(actor, case, snapshot, phase, amount=None):
                request = native_request(case)
                if amount is not None:
                    request['sellAmount']=amount
                code, q = http(actor, '/v1/quote', request)
                require(code == 200, ('multi-pool API quote failed', actor, case['name'], code, q))
                require(q['consumedIn'] == request['sellAmount'] and q['refund'] == '0' and int(q['grossBuyAmount']) > 0,
                        'API exact-in conservation differs')
                require(q['verification']['level'] == 'INDICATIVE' and q['verification'].get('success') is False and
                        q['verification'].get('verified', False) is False, 'API overstated verification')
                require(q['snapshotRef'] == snapshot['reference'] and q['scopeHash'] == snapshot['reference']['scope_hash'],
                        'API lost complete source H/scope binding')
                candidates = [p['pool_id'] for p in snapshot['pools']]
                require(q['quality']['candidateScope'] == candidates, 'API executable candidate inventory differs')
                require(0 < len(q['routes']) <= max_splits, 'API split limit differs')
                for route in q['routes']:
                    require(len(route['path']['edges']) == 1 and route['path']['edges'][0]['pool_id'] in candidates,
                            'API route is not a registered single-hop physical pool')
                if phase == 'capacity-split':
                    require(chain.scope == 'shared' and len(q['routes']) == 2
                            and len({route['path']['edges'][0]['pool_id'] for route in q['routes']}) == 2,
                            'capacity-limited API order must select both real physical pools')
                row = dict(actor=actor, case=case['name'], phase=phase, source_bank='H'+str(chain.head),
                           source_hash=chain.frame['header']['hash'], request=request, quote=q,
                           route_count=len(q['routes']), independent_gas_reference=False)
                # Persist the actual API plan before starting an expensive VM, so
                # a failure retains the request and the real optimizer response.
                report['positive_plans'].append(row)
                write(scope, report)
                if scope == 'catalog':
                    require(source_id(scope, case) == row['source_bank'], 'catalog quote must use its native source bank')
                    require(q['grossBuyAmount'] == case['expected_native']['amount_out'], 'catalog native wallet output differs')
                    selected = next(p['pool_id'] for p in snapshot['pools'] if p['multi_asset']['pool_id'] ==
                                    case['expected_native']['swap_event']['pool_id'])
                    require(all(r['path']['edges'][0]['pool_id'] == selected for r in q['routes']), 'catalog foreign physical route')
                    row['native_expected'] = copy.deepcopy(case['expected_native'])
                else:
                    row['independent_actual'] = replay_plan(chain, request, q)
                row['passed'] = True
                write(scope, report)
                return q, request

            def solana_still_quotes(actor):
                require(status(actor, 'solana:localnet')[0] == 200, 'unrelated Solana readiness withdrawn')
                code, q = http(actor, '/v1/quote', dict(chainKey='solana:localnet', kind='EXACT_IN', sellToken='A', buyToken='B',
                    sellAmount='10003', caller='A', recipient='B', slippageBps=50, verification='FAST', constraints={'maxHops': 1}))
                require(code == 200 and q['verification']['level'] == 'INDICATIVE' and int(q['grossBuyAmount']) > 0,
                        'unrelated Solana actual API quote failed')
                return dict(actor=actor, request_chain='solana:localnet', quote=q)

            with Processes() as processes:
                def start(actor):
                    processes.start(actor, base.service_command(names[actor], configs[actor],
                        root/actor/'fixtures' if actor == 'rust' else None), ROOT/names[actor],
                        OUT/f'managed-v2-{scope}-{actor}.log')
                for actor in ('go', 'rust', 'router1', 'router2'):
                    start(actor)
                    base.wait(urls[actor]+('/health/ready' if actor in ('go', 'rust') else '/health/live'))
                for actor in ('router1', 'router2'):
                    base.wait_scope(urls[actor], token, 'solana:localnet')
                report['checks'].append('online-go-rpc-rust-debug-solana-and-two-router-processes')
                last_quotes = {}
                for index, frame in enumerate(chain.frames):
                    chain.select(index)
                    snapshot = complete_bank()
                    report['source_frames'].append(dict(source_bank=frame['id'], header=frame['header'],
                        pools=copy.deepcopy(frame['pools']), reference=snapshot['reference'],
                        raw_getter_count=chain.raw['banks'][frame['id']]['getter_count'], unique_getter_count=len(frame['getters'])))
                    for case in chain.cases:
                        if case['operation'] == 'swap' and source_id(scope, case) == frame['id']:
                            for actor in ('router1', 'router2'):
                                last_quotes[actor] = quote(actor, case, snapshot, 'source')
                    if scope == 'shared' and frame['id'] == chain.frames[0]['id']:
                        first_case=next(case for case in chain.cases if case['operation']=='swap')
                        for actor in ('router1','router2'):
                            quote(actor,first_case,snapshot,'capacity-split',amount='5000000000000000000000')
                    write(scope, report)
                require(len(report['source_frames']) == (10 if scope == 'catalog' else 7), 'incomplete native banks')
                require(sum(p['phase'] == 'source' and p.get('passed') is True for p in report['positive_plans']) == 8,
                        'both routers must execute all four source swaps')
                report['checks'].append('all-native-full-banks-and-eight-two-replica-positive-plans')
                if scope == 'shared':
                    report['checks'].append('real-api-selected-plans-independent-vm-full-getters-and-wallet-differences')
                    require({row['actor'] for row in report['positive_plans'] if row['phase']=='capacity-split'} == {'router1','router2'},
                            'both replicas must execute the native-funded capacity split')
                    report['checks'].append('both-replicas-capacity-split-shared-physical-pools-real-vm')
                else:
                    report['checks'].append('four-native-swaps-and-four-30day-collections-complete-observed-after')

                first_case = next(c for c in chain.cases if c['operation'] == 'swap')
                for fault in faults_for(scope):
                    # A real ancestor -> a different native next H forces a new
                    # acquisition. Never fault a head already in the cache.
                    chain.select(0)
                    before = complete_bank()
                    generation = before['reference']['online_generation']
                    chain.select(1, fault)
                    evidence = copy.deepcopy(chain.fault_evidence)
                    fault_row = dict(evidence=evidence, ancestor_reference=before['reference'], recovered=False)
                    report['faults'].append(fault_row)
                    write(scope, report)
                    unrelated = []
                    for actor in ('router1', 'router2'):
                        base.await_condition(lambda: status(actor)[0] == 503, 'invalid next source stayed ready: '+fault, timeout=60)
                        code, stale = http(actor, '/v1/quote', native_request(first_case))
                        require(code != 200, 'withdrawn source still served a quote')
                        fault_row.setdefault('quote_rejections', []).append(dict(actor=actor, status=code, response=stale))
                        unrelated.append(solana_still_quotes(actor))
                    require(chain.fault_hits > 0, 'fault did not alter any actual requested RPC result')
                    fault_row.update(actual_fault_rpc_hits=chain.fault_hits, unrelated_solana=unrelated)
                    chain.select(1)
                    recovered = complete_bank()
                    require(recovered['reference']['online_generation'] > generation, 'recovery did not acquire a new generation')
                    fault_row.update(recovered=True, recovered_reference=recovered['reference'])
                    # Return to the real first source for a native-bound recovery
                    # quote; epoch/generation changes never rewrite expected H.
                    chain.select(0)
                    recovered = complete_bank()
                    quote('router1', first_case, recovered, 'fault-recovery:'+fault)
                    report['checks'].append('actual-next-bank-withdrawal-and-new-generation:'+fault)
                    write(scope, report)

                # Restart one replica while both scopes still use their complete
                # registration bank, and require full Kafka/bootstrap restoration.
                chain.select(0)
                complete_bank()
                processes.stop('router2')
                start('router2')
                base.wait(urls['router2']+'/health/live')
                snapshot = complete_bank()
                report['checks'].append('router2-restart-complete-native-bank-restore')
                for actor in ('router1', 'router2'):
                    q, request = quote(actor, first_case, snapshot, 'restart')
                    verified = dict(request, verification='VERIFIED')
                    code, response = http(actor, '/v1/quote', verified)
                    require(code == 422 and 'INDEPENDENT_SIMULATION_UNAVAILABLE' in json.dumps(response), 'VERIFIED unexpectedly admitted')
                    build_code, build_response = http(actor, '/v1/quotes/'+q['quoteId']+'/build', {})
                    require(build_code == 422, 'public build unexpectedly admitted')
                    report['reverts'].append(dict(actor=actor, verified_status=code, verified_response=response,
                                                  build_status=build_code, build_response=build_response))
                report['checks'].append('verified-and-build-rejected-on-both-replicas')
        completed_fingerprint, completed_inputs = base.runtime_fingerprint(), base.snapshot_inputs()
        require(fingerprint == completed_fingerprint, 'source/binaries changed during process run')
        require(inputs == completed_inputs, 'execution inputs changed during process run')
        report.update(source_binding_completed=completed_fingerprint, input_hashes_completed=completed_inputs,
            inputs_unchanged=True, full_service_run=True, passed=True, state='PASSED',
            completed_at_unix_ms=int(time.time()*1000), fault_count=len(report['faults']),
            source_frame_count=len(report['source_frames']),
            limitation='Frozen local original RPC replay; selected shared plans use original signed-history bytecode VM. '
                       'Route count is optimizer output, not proof that every physical route was selected. '
                       'No independent live-chain trust, real execution gas reference, VERIFIED or public build admission.')
        return report
    finally:
        report.update(read_counts=copy.deepcopy(chain.reads), getter_read_counts=copy.deepcopy(chain.getter_reads),
                      code_read_counts=copy.deepcopy(chain.code_reads), receipt_reads=chain.receipt_reads,
                      getter_reads=sum(chain.getter_reads.values()), code_reads=sum(chain.code_reads.values()))
        write(scope, report)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--scope', choices=(*SCOPES, 'both'), default='both',
                        help='default: catalog then shared, each with separate PG/Kafka/services and report')
    args = parser.parse_args(argv)
    scopes = SCOPES if args.scope == 'both' else (args.scope,)
    reports = {scope: initial(scope) for scope in scopes}
    # Invalidate every requested previous pass before any setup can fail.
    for scope in scopes:
        write(scope, reports[scope])
    failed = False
    for scope in scopes:
        report = reports[scope]
        try:
            run_scope(scope, report)
        except BaseException as error:
            failed = True
            report.update(passed=False, state='FAILED', full_service_run=False, error=str(error),
                          error_type=type(error).__name__, completed_at_unix_ms=int(time.time()*1000))
            write(scope, report)
            print(f'{scope}: FAILED: {error}', file=sys.stderr)
            if isinstance(error, (KeyboardInterrupt, SystemExit)):
                for pending in scopes:
                    if reports[pending]['state'] == 'RUNNING':
                        reports[pending].update(state='FAILED', passed=False, error='interrupted before completion')
                        write(pending, reports[pending])
                raise
        else:
            write(scope, report)
        print(json.dumps(dict(scope=scope, passed=report['passed'], state=report['state'],
                              report=str(REPORTS[scope]), positive_plans=len(report['positive_plans']))))
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main())
