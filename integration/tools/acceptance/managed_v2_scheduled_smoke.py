#!/usr/bin/env python3
"""Original scheduled RPC banks through PG/Kafka/two routers.

The replay preserves January 2026 headers. Its explicitly broad fixture age
budget exercises service transport only; it is not current-chain evidence.
Importing or constructing ReplayChain cannot produce an acceptance report.
"""
import argparse
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import secrets
import subprocess
import tempfile
import threading
import time
import urllib.request

import managed_v2_scheduled_projection as projection
import managed_v2_smoke as legacy

base = legacy.base
ROOT = projection.ROOT
OUT = ROOT / 'integration/artifacts/local'
FAULTS = ('missing-word', 'reserved-token-bit', 'missing-getter', 'fee-getter', 'future-head')
spec = importlib.util.spec_from_file_location('scheduled_live_wire', ROOT / 'integration/tools/ci/managed-v2-scheduled-wire-golden.py')
wire = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wire)
require = projection.require


class ReplayChain:
    provider = base.Chain.provider

    def __init__(self, raw, thin, namespace):
        require(namespace in ('two', 'three', 'reschedule'), 'unknown original namespace')
        self.namespace = namespace
        self.lock = threading.RLock()
        self.headers, self.receipts, self.frames = {}, {}, {}
        self.reads, self.fault_hits = {}, 0
        branch = raw['branches'][namespace]
        def walk(value):
            if isinstance(value, dict):
                if {'hash', 'parentHash', 'number', 'timestamp', 'receiptsRoot', 'transactions'} <= value.keys():
                    ident = value['hash']
                    require(ident not in self.headers or self.headers[ident] == value, 'conflicting original header')
                    self.headers[ident] = copy.deepcopy(value)
                if {'transaction', 'receipt', 'signed_transaction'} <= value.keys():
                    receipt = value['receipt']
                    self.receipts.setdefault(receipt['transactionHash'], {})[receipt['blockHash']] = copy.deepcopy(receipt)
                for child in value.values():
                    walk(child)
            elif isinstance(value, list):
                for child in value:
                    walk(child)
        walk(branch)
        runtime = branch.get('native_code', branch.get('native_runtime'))
        for state in branch['states']:
            ident = state['id']
            p = copy.deepcopy(thin['states'][namespace + '/' + ident])
            self.frames[ident] = dict(id=ident, header=copy.deepcopy(state['header']), pool=p,
                getters={g['to'] + g['data']: g['result'] for g in state['getters']},
                methods={g['to'] + g['data']: g['method'] for g in state['getters']},
                codes={a['address']: runtime[a['code_sha256']] for a in state['accounts']},
                storage={r['address'] + r['key']: '0x' + r['value'][2:].rjust(64, '0') for r in state['storage']})
        self.by_hash = {}
        for frame in self.frames.values():
            h = frame['header']['hash']
            if h in self.by_hash:
                require(all(frame[k] == self.by_hash[h][k] for k in ('pool', 'getters', 'codes', 'storage')),
                        'conflicting original same-H observations')
            self.by_hash[h] = frame
        self.config = copy.deepcopy(thin['configs'][namespace + '/' + branch['states'][0]['id']])
        self.select(branch['states'][0]['id'])

    def select(self, ident, fault=None):
        with self.lock:
            require(ident in self.frames and (fault is None or fault in FAULTS), 'unknown source/fault')
            frame = copy.deepcopy(self.frames[ident])
            canonical, h = {}, frame['header']
            while True:
                n = int(h['number'], 16)
                require(n not in canonical, 'cyclic original ancestry')
                canonical[n] = h
                if n == 0:
                    break
                require(h['parentHash'] in self.headers, 'missing original ancestor')
                parent = self.headers[h['parentHash']]
                require(int(parent['number'], 16) == n-1, 'original ancestry gap')
                h = parent
            self.frame, self.canonical = frame, canonical
            self.head = int(frame['header']['number'], 16)
            self.timestamp = int(frame['header']['timestamp'], 16)
            self.fault, self.fault_hits = fault, 0

    def anchor(self):
        return {'blockHash': self.frame['header']['hash'], 'requireCanonical': True}

    def rpc(self, method, args):
        with self.lock:
            self.reads[method] = self.reads.get(method, 0) + 1
            if method == 'eth_chainId':
                return '0x7a69'
            if method == 'eth_getBlockByNumber':
                n = self.head if args[0] in ('latest', 'safe', 'finalized') else int(args[0], 16)
                require(n in self.canonical, 'unknown original canonical header')
                result = copy.deepcopy(self.canonical[n])
                if self.fault == 'future-head' and n == self.head:
                    result['timestamp'] = hex(int(time.time()) + 120)
                    self.fault_hits += 1
                return result
            if method == 'eth_getTransactionReceipt':
                candidates = self.receipts.get(args[0], {})
                rows = [r for h, r in candidates.items() if int(r['blockNumber'], 16) in self.canonical
                        and self.canonical[int(r['blockNumber'], 16)]['hash'] == h]
                require(len(rows) == 1, 'unknown/noncanonical original receipt')
                return copy.deepcopy(rows[0])
            require(method in ('eth_call', 'eth_getCode', 'eth_getStorageAt'), 'unexpected original RPC')
            anchor = args[2] if method == 'eth_getStorageAt' else args[1]
            require(type(anchor) is dict and set(anchor) == {'blockHash', 'requireCanonical'}
                    and anchor['requireCanonical'] is True, 'exact canonical anchor required')
            frame = self.by_hash.get(anchor['blockHash'])
            require(frame is not None and int(frame['header']['number'], 16) in self.canonical
                    and self.canonical[int(frame['header']['number'], 16)]['hash'] == anchor['blockHash'], 'unknown canonical source H')
            address = (args[0]['to'] if method == 'eth_call' else args[0]).lower()
            fault = self.fault if anchor['blockHash'] == self.frame['header']['hash'] else None
            if method == 'eth_getCode':
                require(address in frame['codes'], 'runtime absent from original source')
                return frame['codes'][address]
            if method == 'eth_getStorageAt':
                key = address + args[1]
                require(key in frame['storage'], 'storage absent from original source')
                if fault == 'missing-word' and address == frame['pool']['pool_id'] and args[1] == '0x9':
                    self.fault_hits += 1
                    raise ValueError('injected missing original pool word')
                value = frame['storage'][key]
                if fault == 'reserved-token-bit' and address == frame['pool']['pool_id'] and args[1] != '0x9':
                    self.fault_hits += 1
                    return '0x' + format(int(value, 16) | (1 << 133), '064x')
                return value
            key = address + args[0]['data'].lower()
            require(key in frame['getters'], 'getter absent from original source')
            getter = frame['methods'][key]
            if fault == 'missing-getter' and getter == 'getActualSupply':
                self.fault_hits += 1
                raise ValueError('injected missing original getter')
            if fault == 'fee-getter' and getter == 'getSwapFeePercentage':
                self.fault_hits += 1
                return '0x' + format(int(frame['getters'][key], 16)+1, '064x')
            return frame['getters'][key]


def _write(report):
    OUT.mkdir(parents=True, exist_ok=True)
    target = OUT / 'managed-v2-scheduled-three-service-smoke.json'
    temporary = target.with_name(target.name + '.' + secrets.token_hex(6) + '.tmp')
    try:
        temporary.write_text(json.dumps(report, indent=2) + '\n')
        temporary.replace(target)
    finally:
        temporary.unlink(missing_ok=True)


def validate_quote_reference(quote, reference):
    require(type(reference) is dict and quote.get('snapshotRef') == reference,
            'scheduled quote full source reference/generation differs')
    require(quote.get('onlineGeneration') == reference['online_generation']
            and quote.get('scopeHash') == reference['scope_hash'], 'scheduled quote generation/scope differs')


def validate_withdrawn_quote(code, response):
    require(code in (400, 404, 422, 503) and type(response) is dict
            and not any(key in response for key in ('quoteId', 'grossBuyAmount', 'routes')),
            'faulted scheduled source returned a stale FAST quote')


def run_namespace(raw, thin, namespace, report):
    chain = ReplayChain(raw, thin, namespace)
    servers, processes, logs = [], {}, {}
    observed = dict(namespace=namespace, passed=False, source_frames=[], quotes=[], faults=[],
                    restarted=False, full_service_run=False, admission_probes=[])
    report['namespaces'].append(observed)
    _write(report)
    branch = raw['branches'][namespace]
    native_cases = [c for c in thin['cases'] if c['id'].split('/')[0] == namespace]
    quote_sources = {c['before'].split('/')[1] for c in native_cases}
    quote_sources.update(c['after'].split('/')[1] for c in thin['collections'] if c['id'].split('/')[0] == namespace)
    try:
        for _ in range(2):
            servers.append(chain.provider())
        with base.Infrastructure() as infra, tempfile.TemporaryDirectory(prefix='xone-managed-scheduled-') as folder:
            root, token = Path(folder), secrets.token_urlsafe(32)
            names = {'go': 'chain-parser-go', 'rust': 'chain-parser-rust', 'router1': 'dex-router', 'router2': 'dex-router'}
            ports = {actor: base.port() for actor in names}
            urls = {actor: f'http://127.0.0.1:{port}' for actor, port in ports.items()}
            configs = {}
            for actor, project in names.items():
                config = base.config_for(project, root/actor, token, ports[actor])
                config['journal'].update(transport='kafka', source_urls=[urls[a] for a in ('go', 'rust')],
                                         poll_interval_ms=100, request_timeout_ms=30000)
                config['journal']['kafka'].update(brokers=[infra.broker], replica_id=actor,
                    topics={'evm:31337': 'canonical.evm-31337.v1', 'solana:localnet': 'canonical.solana-localnet.v1'},
                    transaction_timeout_ms=4000)
                config['control'].update(backend='disabled' if project == 'dex-router' else 'postgres',
                    dsn_file='' if project == 'dex-router' else str(infra.dsnfile), instance_id=actor,
                    lease_ttl_ms=30000, renewal_ms=1000, min_commit_remaining_ms=7000)
                config['startup']['max_baseline_age_ms'] = base.FRESHNESS
                if actor == 'go':
                    config['chains'][0].update(genesis_hash=chain.canonical[0]['hash'], confirmation_tag='latest',
                        auto_reorg_window=4, rpc_urls=[f'http://127.0.0.1:{s.server_port}' for s in servers], pools=[chain.config])
                elif actor == 'rust':
                    base.sample_for('solana:localnet', root/actor/'fixtures', chain.timestamp*1000)
                else:
                    config['routing'].update(max_hops=1, max_splits=1, max_state_age_ms=base.FRESHNESS, quote_ttl_ms=60000)
                    config['api']['quote_timeout_ms'] = 15000
                configs[actor] = base.save_config(root/actor/'config', config)

            def start(actor):
                logs[actor] = (OUT/f'managed-v2-scheduled-{namespace}-{actor}.log').open('ab')
                processes[actor] = subprocess.Popen(base.service_command(names[actor], configs[actor],
                    root/actor/'fixtures' if actor == 'rust' else None), cwd=ROOT/names[actor],
                    stdout=logs[actor], stderr=subprocess.STDOUT)

            def stop(actor):
                process = processes.pop(actor, None)
                if process is not None:
                    process.terminate()
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
                if actor in logs:
                    logs.pop(actor).close()

            def status(actor, key='evm:31337'):
                return base.http(urls[actor]+'/internal/chains/'+key+'/status', token)

            published_reference = None

            def complete_bank():
                nonlocal published_reference
                base.await_condition(lambda: status('go')[0] == 200 and
                    status('go')[1].get('snapshot_ref', {}).get('block', {}).get('hash') == chain.frame['header']['hash'],
                    'scheduled owner did not reach original source H')
                code, snapshot = base.http(urls['go']+'/internal/chains/evm:31337/baseline', token)
                require(code == 200 and snapshot.get('pools') == [chain.frame['pool']], 'complete scheduled original state differs')
                request = urllib.request.Request(urls['go']+'/internal/chains/evm:31337/bootstrap',
                                                 headers={'Authorization': 'Bearer '+token})
                with urllib.request.urlopen(request, timeout=10) as response:
                    require(response.headers['content-type'] == 'application/x-protobuf', 'scheduled bootstrap content type')
                    actual = base.bootstrap_snapshot(response.read())
                expected = subprocess.run(['protoc', '-I', str(ROOT/'common/protoc/proto'), '--encode=dex.v1.OnlineSnapshot',
                    str(ROOT/'common/protoc/proto/dex/v1/state.proto')], input=wire.encode(snapshot).encode(),
                    capture_output=True, check=True).stdout
                require(actual == expected, 'scheduled live independent C++ wire differs')
                replicas = {}
                for actor in ('router1', 'router2'):
                    base.await_condition(lambda: status(actor)[0] == 200 and
                        status(actor)[1].get('snapshot_ref') == snapshot['reference'], 'scheduled replica source H differs')
                    replicas[actor] = dict(ready=True, snapshot_ref=status(actor)[1]['snapshot_ref'])
                published_reference = copy.deepcopy(snapshot['reference'])
                observed['last_complete_bank'] = dict(source_hash=chain.frame['header']['hash'],
                    pool_sha256=hashlib.sha256(projection.base.canonical(snapshot['pools'])).hexdigest(),
                    protobuf_sha256=hashlib.sha256(actual).hexdigest(), protobuf_bytes=len(actual),
                    independent_cpp_identical=True, replicas=replicas)
                return snapshot

            def quote(actor, case):
                native = next(c for c in branch['cases'] if case['id'].split('/')[1] == c['id'])
                if namespace == 'two':
                    tx = native['execution']
                    recipient = tx['args'][1]['recipient'] if isinstance(tx['args'][1], dict) else tx['args'][1][2]
                else:
                    tx = next(t for t in branch['transactions'] if t['id'] == native['transaction'])
                    recipient = branch['configuration']['roles']['trader']
                request = dict(chainKey='evm:31337', kind='EXACT_IN', sellToken=case['token_in'], buyToken=case['token_out'],
                    sellAmount=case['amount_in'], caller=tx['transaction']['from'], recipient=recipient,
                    slippageBps=50, verification='FAST', constraints={'maxHops': 1})
                code, q = base.http(urls[actor]+'/v1/quote', token, request)
                require(code == 200 and q.get('grossBuyAmount') == case['expected_raw_out'],
                        ('scheduled original wallet amount differs', case['id'], actor, code, q))
                require(q.get('consumedIn') == request['sellAmount'] and q.get('refund') == '0', 'scheduled input conservation')
                require(q['verification']['level'] == 'INDICATIVE', 'scheduled verification promoted')
                validate_quote_reference(q, published_reference)
                require(len(q['routes']) == 1 and len(q['routes'][0]['path']['edges']) == 1 and
                        q['routes'][0]['path']['edges'][0]['pool_id'] == chain.frame['pool']['pool_id'], 'scheduled selected physical route')
                observed['quotes'].append(dict(case=case['id'], actor=actor, request=copy.deepcopy(request), quote=copy.deepcopy(q),
                    original_transaction=tx['transaction']['hash'], source=chain.frame['header']['hash']))
                return request, q

            try:
                for actor in names:
                    start(actor)
                for actor in names:
                    base.wait(urls[actor]+'/health/live')
                # Every boundary checkpoint and every source/after state are
                # checked in parser/domain tests. This process run selects all
                # actual swap sources, plus both real collection poststates.
                for state in branch['states']:
                    if state['id'] not in quote_sources:
                        continue
                    chain.select(state['id'])
                    snapshot = complete_bank()
                    observed['source_frames'].append(dict(id=namespace+'/'+state['id'], reference=snapshot['reference'],
                        complete_bank=copy.deepcopy(observed['last_complete_bank'])))
                    for case in native_cases:
                        if case['before'] == namespace+'/'+state['id']:
                            for actor in ('router1', 'router2'):
                                quote(actor, case)
                    _write(report)
                require(len(observed['quotes']) == 2*len(native_cases), 'incomplete scheduled original case service coverage')
                first, second = native_cases[0], native_cases[1]
                for fault in FAULTS:
                    chain.select(first['before'].split('/')[1])
                    generation = complete_bank()['reference']['online_generation']
                    chain.select(second['before'].split('/')[1], fault=fault)
                    withdrawals = {}
                    for actor in ('router1', 'router2'):
                        base.await_condition(lambda: status(actor)[0] == 503, 'faulted scheduled source remained ready: '+fault)
                        require(status(actor, 'solana:localnet')[0] == 200, 'scheduled fault revoked unrelated chain')
                        stale_request = next(row['request'] for row in observed['quotes'] if row['case'] == second['id'])
                        rejected_code, rejected = base.http(urls[actor]+'/v1/quote', token, stale_request)
                        validate_withdrawn_quote(rejected_code, rejected)
                        withdrawals[actor] = dict(evm_status=503, unrelated_solana_status=200,
                            fast_quote_status=rejected_code, fast_quote_response=rejected)
                    require(chain.fault_hits > 0, 'scheduled fault never exercised')
                    hits = chain.fault_hits
                    chain.select(second['before'].split('/')[1])
                    snapshot = complete_bank()
                    require(snapshot['reference']['online_generation'] > generation, 'scheduled recovery reused generation')
                    quote('router1', second)
                    observed['faults'].append(dict(fault=fault, hits=hits, previous_generation=generation, reference=snapshot['reference'], withdrawals=withdrawals))
                    _write(report)
                stop('router2')
                start('router2')
                base.wait(urls['router2']+'/health/live')
                complete_bank()
                for actor in ('router1', 'router2'):
                    request, q = quote(actor, second)
                    request['verification'] = 'VERIFIED'
                    code, result = base.http(urls[actor]+'/v1/quote', token, request)
                    require(code == 422 and 'INDEPENDENT_SIMULATION_UNAVAILABLE' in json.dumps(result), 'scheduled VERIFIED admission changed')
                    build_code, _ = base.http(urls[actor]+'/v1/quotes/'+q['quoteId']+'/build', token, {})
                    require(build_code == 422, 'scheduled public build admission changed')
                    observed['admission_probes'].append(dict(actor=actor, verified_status=code, build_status=build_code,
                        verified_reason='INDEPENDENT_SIMULATION_UNAVAILABLE'))
                observed['restart_reference'] = copy.deepcopy(observed['last_complete_bank'])
                observed.update(passed=True, full_service_run=True, restarted=True, reads=chain.reads,
                                original_cases=len(native_cases), fault_count=len(FAULTS))
                _write(report)
            finally:
                for actor in set(processes) | set(logs):
                    stop(actor)
    finally:
        for server in servers:
            server.shutdown()
            server.server_close()
    return observed


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--namespace', choices=('two', 'three', 'reschedule', 'all'), default='all')
    args = parser.parse_args()
    report = dict(schema_version=1, passed=False, state='RUNNING', namespaces=[], verified=False,
                  production_ready=False, all_requested_features_complete=False, native_plan_replay=False,
                  scope='local-original-scheduled-source-and-FAST-process-transport',
                  historical_fixture_age_budget_ms=base.FRESHNESS,
                  trust='JANUARY_2026_ORIGINAL_LOCAL_VM_REPLAY_NOT_CURRENT_CHAIN_TRUST',
                  started_at_unix_ms=int(time.time()*1000))
    _write(report)
    try:
        bank = projection.load(ROOT/'integration/test/contracts/managed-v2-scheduled.json')
        thin = projection.project(bank)
        report['original_raw'] = thin['original_raw']
        report['source_binding_started'] = base.runtime_fingerprint()
        report['input_hashes_started'] = base.snapshot_inputs()
        namespaces = ('two', 'three', 'reschedule') if args.namespace == 'all' else (args.namespace,)
        for namespace in namespaces:
            run_namespace(bank, thin, namespace, report)
        rows = [dict(case=row['case'], actor=row['actor'], request=row['request'], quote=row['quote'])
                for ns in report['namespaces'] for row in ns['quotes']]
        input_data = dict(schema_version=1, original_raw_sha256=thin['original_raw']['sha256'], quotes=rows)
        replay = subprocess.run(['node', str(ROOT/'integration/tools/ci/managed-v2-scheduled-plan-replay.cjs')],
            input=json.dumps(input_data).encode(), capture_output=True, timeout=600, cwd=ROOT)
        require(replay.stdout, 'selected scheduled original VM returned no receipt: '+replay.stderr.decode()[-2000:])
        report['independent_native_plan_replay'] = json.loads(replay.stdout)
        require(replay.returncode == 0 and report['independent_native_plan_replay'].get('passed') is True
                and report['independent_native_plan_replay'].get('vm_started') is True, 'scheduled selected-plan original VM replay failed: '+str(report['independent_native_plan_replay'].get('error')))
        report['native_plan_replay'] = True
        require(report['source_binding_started'] == base.runtime_fingerprint(), 'scheduled source/binary changed during run')
        require(report['input_hashes_started'] == base.snapshot_inputs(), 'scheduled source inputs changed during run')
        report.update(passed=True, state='PASSED', completed_at_unix_ms=int(time.time()*1000),
                      limitation='Single physical original pool per namespace; complete-case source/FAST checks. '
                                 'Original one-pool selected signed swaps replayed by independent original VM; cross-pool execution and final full-scope gate remain separate requirements.')
        _write(report)
    except Exception as error:
        report.update(state='FAILED', error=str(error), completed_at_unix_ms=int(time.time()*1000))
        _write(report)
        raise
    print(json.dumps({'passed': report['passed'], 'namespaces': list(namespaces),
                      'report': str(OUT/'managed-v2-scheduled-three-service-smoke.json'), 'production_ready': False}))


if __name__ == '__main__':
    main()
