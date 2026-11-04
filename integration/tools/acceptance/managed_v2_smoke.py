#!/usr/bin/env python3
"""Original Managed V2 current-date banks through RPC/PG/Kafka/two routers.

Every getter, runtime, block and receipt comes from the frozen local native run.
The server is a replay fixture, not an independent trusted chain or a VERIFIED
execution backend. Future original frames remain excluded from positive service
coverage; neither their timestamps nor service admission rules are rewritten.
"""
import copy
import importlib.util
import json
import secrets
import subprocess
import tempfile
import threading
import time
import urllib.request
from pathlib import Path

import managed_v2_projection as projection
import curve_crypto_smoke as base

ROOT = projection.ROOT
OUT = ROOT / 'integration/artifacts/local'
TARGET = OUT / 'managed-v2-aum-three-service-smoke.json'
spec = importlib.util.spec_from_file_location('managed_v2_wire', ROOT / 'integration/tools/ci/managed-v2-aum-wire-golden.py')
wire = importlib.util.module_from_spec(spec)
spec.loader.exec_module(wire)
FAULTS = ('missing-getter', 'actual-supply', 'factory-membership', 'provider-cap',
          'vault-bpt-cash', 'token-balance', 'factory-code', 'helper-code',
          'fee-id-width', 'future-head')


class Chain:
    provider = base.Chain.provider

    def __init__(self, *, now=None, paused_branch=False):
        self.bank = projection.load(ROOT / 'integration/test/contracts/managed-v2-aum.json')
        self.lock = threading.RLock()
        self.reads = {}
        self.receipt_reads = 0
        self.headers = {}
        self.receipts = {}
        self.codes = {}
        self.births = {}
        self.identities = {row['label']: row for row in self.bank._identities}
        for row in self.bank['objects'].values():
            value = row['value']
            if row['kind'] == 'header':
                number = int(value['number'], 16)
                projection.require(number not in self.headers or self.headers[number] == value,
                                   'conflicting original headers')
                self.headers[number] = value
            elif row['kind'] == 'transaction':
                self.receipts[value['transaction']['hash']] = value['receipt']
        for ref in self.bank['reference']['identities']:
            identity = projection.resolve(self.bank, ref, 'identity')
            tx = projection.resolve(self.bank, identity['creation_transaction'], 'transaction')
            self.codes[identity['address']] = projection.resolve(self.bank, identity['code'], 'code')['hex']
            self.births[identity['address']] = int(tx['receipt']['blockNumber'], 16)
        for row in self.bank['reference']['factory_code_storage']['contracts']:
            self.codes[row['address']] = projection.resolve(self.bank, row['code'], 'code')['hex']
            self.births[row['address']] = self.births[self.identities['ManagedPoolFactory']['address']]
        self.cases = [projection.project_case(self.bank, c) for c in self.bank['cases']]
        self.future_cases = [c for c in self.cases if not projection.service_eligible(c['source_pool'], now=now)]
        first = self.cases[0]['source_pool']['pool_id']
        frames = {}
        for case in self.bank['cases']:
            for ref in (case['execution']['before'], case['execution']['after']):
                projected = projection.project_snapshot(self.bank, ref)
                pool = projected['pool']
                if pool['pool_id'] != first or not projection.service_eligible(pool, now=now):
                    continue
                snap = projection.resolve(self.bank, ref, 'snapshot')
                header = projection.resolve(self.bank, snap['header'], 'header')
                getters, methods = {}, {}
                for getter in snap['getters']:
                    g = projection.resolve(self.bank, getter, 'getter')
                    key = (g['to'], g['data'])
                    projection.require(key not in getters or getters[key] == g['result'], 'conflicting original getters')
                    getters[key] = g['result']
                    methods[key] = g['method']
                codes = {}
                for account in snap['accounts']:
                    account = projection.resolve(self.bank, account, 'account')
                    codes[account['address']] = projection.resolve(self.bank, account['code'], 'code')['hex']
                number = int(header['number'], 16)
                frame = dict(ref=ref, pool=pool, header=header, getters=getters, methods=methods, codes=codes)
                projection.require(number not in frames or frames[number] == frame, 'conflicting source frame')
                frames[number] = frame
        self.frames = [frames[n] for n in sorted(frames)]
        projection.require(len(self.frames) >= 6, 'current-date native service frame inventory incomplete')
        self.config = self.cases[0]['config']
        self.paused_branch_sha256 = None
        if paused_branch:
            self.include_paused_branch()
        self.select(0)

    def include_paused_branch(self):
        raw = (ROOT / 'integration/test/contracts/managed-v2-aum-recovery.json').read_bytes()
        branch = json.loads(raw)
        digest = base.sha(raw)
        projection.require(branch['public_adapter_version'] == projection.ADAPTER
            and branch['original_bank_sha256'] == projection.BANK_SHA256
            and branch['reference']['original_prefix_receipts_and_headers_matched'] is True,
            'unqualified original paused Vault branch')
        projection.require(branch['reference']['source_script_sha256'] == base.sha(
            (ROOT / 'integration/tools/ci/managed-v2-aum-paused-vault-golden.cjs').read_bytes()),
            'paused Vault reference producer changed')
        self.headers = {number: header for number, header in self.headers.items() if number <= 28}
        self.receipts = {tx: receipt for tx, receipt in self.receipts.items() if int(receipt['blockNumber'], 16) <= 28}
        for header in branch['headers']:
            number = int(header['number'], 16)
            projection.require(number not in self.headers or self.headers[number] == header, 'original prefix header changed')
            self.headers[number] = header
        for receipt in branch['receipts']:
            self.receipts[receipt['transactionHash']] = receipt
        frames = {int(frame['header']['number'], 16): frame for frame in self.frames if int(frame['header']['number'], 16) <= 28}

        def add(pool, header, getters):
            number = int(header['number'], 16)
            state = pool['multi_asset']['managed_v2']
            projection.require(state['block_hash'] == header['hash'] and state['block_number'] == number
                and state['timestamp'] == int(header['timestamp'], 16) and 'execution_context' not in state,
                'paused reference source H/getter anchor differs')
            ref = {'native_pause_branch_sha256': digest, 'block_number': number}
            frame = dict(ref=ref, pool=pool, header=header,
                         getters={(g['to'], g['data']):g['result'] for g in getters},
                         methods={(g['to'], g['data']):g['method'] for g in getters}, codes={})
            projection.require(len(frame['getters']) == len(getters), 'duplicate paused source getter')
            projection.require(number not in frames or frames[number] == frame, 'conflicting paused reference source')
            frames[number] = frame
            return ref

        for case in branch['cases']:
            add(case['source_pool'], case['source_header'], case['getters_before'])
            add(case['observed_pool_after'], case['execution_header'], case['getters_after'])
            if 'execution_source_pool' in case:
                add(case['execution_source_pool'], case['execution_source_header'], case['execution_source_getters'])
            if case['operation'] == 'swap' and case['expected_native']['status'] == 1:
                source = case.get('execution_source_pool', case['source_pool'])
                before = add(source,case.get('execution_source_header',case['source_header']),
                             case.get('execution_source_getters',case['getters_before']))
                c = dict(name=case['name'],operation='swap',source_pool=source,params=case['params'],
                         expected_native=case['expected_native'],source_refs={'before':before,'native_pause_branch_sha256':digest},
                         native_request_identity={'caller':case['transaction']['transaction']['from'],
                                                  'recipient':case['transaction']['transaction']['from']})
                self.cases.append(c)
        self.frames = [frames[number] for number in sorted(frames)]
        self.paused_branch_sha256 = digest

    def select(self, index, fault=None):
        with self.lock:
            projection.require(fault is None or fault in FAULTS, 'unknown fault')
            self.frame = self.frames[index]
            self.head = int(self.frame['header']['number'], 16)
            self.timestamp = int(self.frame['header']['timestamp'], 16)
            self.fault = fault

    def anchor(self):
        return {'blockHash': self.frame['header']['hash'], 'requireCanonical': True}

    def rpc(self, method, args):
        with self.lock:
            self.reads[method] = self.reads.get(method, 0) + 1
            if method == 'eth_chainId':
                return hex(self.bank['reference']['chain_options']['chain']['chainId'])
            if method == 'eth_getBlockByNumber':
                number = self.head if args[0] in ('latest', 'safe', 'finalized') else int(args[0], 16)
                projection.require(number <= self.head and number in self.headers, 'unknown original header')
                result = copy.deepcopy(self.headers[number])
                if self.fault == 'future-head' and number == self.head:
                    result['timestamp'] = hex(int(time.time()) + 120)
                return result
            if method == 'eth_getTransactionReceipt':
                receipt = self.receipts[args[0]]
                projection.require(int(receipt['blockNumber'], 16) <= self.head, 'future receipt')
                self.receipt_reads += 1
                return copy.deepcopy(receipt)
            projection.require(method in ('eth_call', 'eth_getCode'), 'unexpected RPC ' + method)
            projection.require(type(args[1]) is dict and set(args[1]) == {'blockHash', 'requireCanonical'}
                               and args[1]['requireCanonical'] is True, 'exact canonical anchor required')
            frame = next((f for f in self.frames if f['header']['hash'] == args[1]['blockHash']
                          and int(f['header']['number'], 16) <= self.head), None)
            projection.require(frame is not None, 'unknown source bank')
            if method == 'eth_getCode':
                address = args[0].lower()
                value = frame['codes'].get(address)
                if value is None:
                    projection.require(address in self.codes and self.births[address] <= int(frame['header']['number'], 16),
                                       'unknown or future runtime')
                    value = self.codes[address]
                if ((self.fault == 'factory-code' and address == self.identities['ManagedPoolFactory']['address'])
                        or (self.fault == 'helper-code' and address == self.identities['RecoveryModeHelper']['address'])):
                    value = value[:-2] + ('00' if value[-2:] != '00' else '01')
                return value
            key = (args[0]['to'].lower(), args[0]['data'].lower())
            projection.require(key in frame['getters'], 'getter absent from original snapshot')
            value = frame['getters'][key]
            getter = frame['methods'][key]
            if self.fault == 'missing-getter' and getter == 'getActualSupply':
                raise ValueError('injected unavailable original getter')
            zero = ((self.fault == 'factory-membership' and getter == 'isPoolFromFactory')
                    or (self.fault == 'provider-cap' and getter == 'getFeeTypeMaximumPercentage')
                    or (self.fault == 'token-balance' and getter == 'balanceOf' and key[0] != frame['pool']['pool_id']))
            if zero:
                return '0x' + base.word(0)
            if self.fault == 'actual-supply' and getter == 'getActualSupply':
                return '0x' + base.word(int(value, 16) + 1)
            if self.fault == 'vault-bpt-cash' and getter == 'getPoolTokenInfo' and key[1].endswith(frame['pool']['pool_id'][2:]):
                return '0x' + base.word(int(value[2:66], 16) + 1) + value[66:]
            if self.fault == 'fee-id-width' and getter == 'getProviderFeeId':
                return '0x' + base.word(256)
            return value


def initial():
    return dict(passed=False, state='RUNNING', verified=False, production_ready=False,
                all_requested_features_complete=False, checks=[],
                trust='FROZEN_LOCAL_ORIGINAL_RPC_REPLAY_NOT_TRUSTED_CHAIN_OR_EXECUTION')


def write(report):
    base.TARGET = TARGET
    base.write(report)


def run():
    report = initial()
    chain = Chain(paused_branch=True)
    servers = []
    processes, logs = {}, {}
    started = int(time.time() * 1000)
    try:
        for _ in range(2):
            servers.append(chain.provider())
        fingerprint = base.runtime_fingerprint()
        inputs = base.snapshot_inputs()
        report.update(source_binding=fingerprint, started_at_unix_ms=started, original_bank_sha256=projection.BANK_SHA256,
                      source_frames_checked=[], quote_checks=[], paused_quote_rejections=[], recovery_source_frames=[],
                  paused_branch_sha256=chain.paused_branch_sha256,
                      future_original_cases_excluded=[c['name'] for c in chain.future_cases])
        write(report)
        with base.Infrastructure() as infra, tempfile.TemporaryDirectory(prefix='xone-managed-v2-') as folder:
            root = Path(folder)
            token = secrets.token_urlsafe(32)
            names = {'go': 'chain-parser-go', 'rust': 'chain-parser-rust', 'router1': 'dex-router', 'router2': 'dex-router'}
            ports = {actor: base.port() for actor in names}
            sources = [f'http://127.0.0.1:{ports[actor]}' for actor in ('go', 'rust')]
            configs = {}
            for actor, name in names.items():
                config = base.config_for(name, root / actor, token, ports[actor])
                config['journal'].update(transport='kafka', source_urls=sources, poll_interval_ms=100, request_timeout_ms=15000)
                config['journal']['kafka'].update(brokers=[infra.broker], topics={'evm:31337': 'canonical.evm-31337.v1',
                    'solana:localnet': 'canonical.solana-localnet.v1'}, replica_id=actor, transaction_timeout_ms=4000)
                config['control'].update(backend='disabled' if name == 'dex-router' else 'postgres',
                    dsn_file='' if name == 'dex-router' else str(infra.dsnfile), instance_id=actor,
                    lease_ttl_ms=30000, renewal_ms=1000, min_commit_remaining_ms=7000)
                config['startup']['max_baseline_age_ms'] = base.FRESHNESS
                if actor == 'go':
                    config['chains'][0].update(genesis_hash=chain.headers[0]['hash'], confirmation_tag='latest', auto_reorg_window=4,
                        rpc_urls=[f'http://127.0.0.1:{s.server_port}' for s in servers], pools=[chain.config])
                elif actor == 'rust':
                    base.sample_for('solana:localnet', root / actor / 'fixtures', chain.timestamp * 1000)
                else:
                    config['routing'].update(max_state_age_ms=base.FRESHNESS, quote_ttl_ms=60000)
                configs[actor] = base.save_config(root / actor / 'config', config)

            def url(actor):
                return f'http://127.0.0.1:{ports[actor]}'

            def start(actor):
                logs[actor] = (OUT / f'managed-v2-aum-{actor}.log').open('ab')
                processes[actor] = subprocess.Popen(base.service_command(names[actor], configs[actor],
                    root / actor / 'fixtures' if actor == 'rust' else None), cwd=ROOT / names[actor],
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
                return base.http(url(actor) + '/internal/chains/' + key + '/status', token)

            def complete_bank():
                base.await_condition(lambda: status('go')[1].get('snapshot_ref', {}).get('block', {}).get('hash')
                                     == chain.frame['header']['hash'], 'Managed owner did not reach native source bank')
                code, snapshot = base.http(url('go') + '/internal/chains/evm:31337/baseline', token)
                projection.require(code == 200, ('Managed baseline unavailable', code, snapshot))
                projection.require(snapshot['pools'] == [chain.frame['pool']], 'complete original Managed source state differs')
                request = urllib.request.Request(url('go') + '/internal/chains/evm:31337/bootstrap',
                                                 headers={'Authorization': 'Bearer ' + token})
                with urllib.request.urlopen(request, timeout=10) as response:
                    projection.require(response.headers['content-type'] == 'application/x-protobuf', 'Managed bootstrap not protobuf')
                    raw = response.read()
                encoded = subprocess.run(['protoc', '-I', str(ROOT / 'common/protoc/proto'),
                    '--encode=dex.v1.OnlineSnapshot', str(ROOT / 'common/protoc/proto/dex/v1/state.proto')],
                    input=wire.encode(snapshot).encode(), capture_output=True, check=True).stdout
                projection.require(base.bootstrap_snapshot(raw) == encoded, 'live Managed C++ protobuf differs')
                for actor in ('router1', 'router2'):
                    base.await_condition(lambda: status(actor)[0] == 200 and status(actor)[1].get('snapshot_ref')
                                         == snapshot['reference'], 'Managed replica source bank mismatch')
                return snapshot

            def quote(actor, case):
                pool = case['source_pool']
                assets = pool['multi_asset']['assets']
                params = case['params']
                identity = case.get('native_request_identity')
                if identity is None:
                    native = projection.resolve(chain.bank, case['source_refs']['transaction'], 'transaction')
                    source = projection.resolve(chain.bank, case['source_refs']['before'], 'snapshot')
                    identity = dict(caller=native['transaction']['from'], recipient=source['state']['roles']['trader'])
                request = dict(chainKey='evm:31337', kind='EXACT_IN', sellToken=assets[params['index_in']]['token'],
                    buyToken=assets[params['index_out']]['token'], sellAmount=params['amount'],
                    caller=identity['caller'], recipient=identity['recipient'], slippageBps=50, verification='FAST')
                code, q = base.http(url(actor) + '/v1/quote', token, request)
                projection.require(code == 200, ('Managed quote failed', case['name'], code, q))
                projection.require(q['grossBuyAmount'] == case['expected_native']['quantity']
                    and q['consumedIn'] == params['amount'] and q['refund'] == '0', 'original Managed transaction amount differs')
                projection.require(q['verification']['level'] == 'INDICATIVE' and q['snapshotRef'] == complete_bank()['reference'],
                                   'Managed quote source or verification differs')
                projection.require(q['quality']['candidateScope'] == [pool['pool_id']], 'Managed BPT/foreign pool entered graph')
                for route in q['routes']:
                    projection.require(len(route['path']['edges']) == 1 and route['path']['edges'][0]['pool_id'] == pool['pool_id'],
                                       'Managed native physical path differs')
                return q, request

            try:
                for actor in ('go', 'rust'):
                    start(actor)
                    base.wait(url(actor) + '/health/ready')
                for actor in ('router1', 'router2'):
                    start(actor)
                    base.wait(url(actor) + '/health/live')
                    base.wait_scope(url(actor), token, 'evm:31337')
                    base.wait_scope(url(actor), token, 'solana:localnet')
                quotes = report['quote_checks']
                for index, frame in enumerate(chain.frames):
                    chain.select(index)
                    complete_bank()
                    report['source_frames_checked'].append(dict(snapshot=frame['ref'], header=frame['header']['hash']))
                    mode = frame['pool']['multi_asset']['managed_v2']
                    if mode['recovery_mode']:
                        projection.require(mode['management_aum_fee'] == '0' and mode['pending_aum'] == '0'
                                           and mode['actual_supply'] == mode['virtual_supply'], 'Recovery source effective fee/view differs')
                        report['recovery_source_frames'].append(dict(header=frame['header']['hash'],effective_fee=mode['management_aum_fee']))
                    for case in chain.cases:
                        if case['operation'] == 'swap' and case['source_refs']['before'] == frame['ref']:
                            for actor in ('router1', 'router2'):
                                q, request = quote(actor, case)
                                quotes.append(dict(actor=actor, case=case['name'], buy_amount=q['grossBuyAmount'],
                                    source_ref=case['source_refs'], quote_id=q['quoteId'], request=request, quote=q))
                    if frame['pool']['multi_asset']['managed_v2']['vault_paused']:
                        case = next(c for c in chain.cases if c['name'] == 'two:swap-forward')
                        native = projection.resolve(chain.bank, case['source_refs']['transaction'], 'transaction')
                        assets = frame['pool']['multi_asset']['assets']
                        request = dict(chainKey='evm:31337',kind='EXACT_IN',sellToken=assets[0]['token'],
                            buyToken=assets[1]['token'],sellAmount='100000000000000000000',caller=native['transaction']['from'],
                            recipient=native['transaction']['from'],slippageBps=50,verification='FAST')
                        for actor in ('router1','router2'):
                            code, rejected = base.http(url(actor) + '/v1/quote',token,request)
                            projection.require(code in (400,404,422) and 'quoteId' not in rejected,
                                               'paused Vault produced an executable quote')
                            projection.require(status(actor)[0] == 200, 'valid paused bank revoked source readiness')
                            report['paused_quote_rejections'].append(dict(actor=actor,header=frame['header']['hash'],status=code))
                    write(report)
                report['checks'] += ['normal-original-rpc-postgres-kafka-two-routers', 'complete-native-state-across-swap-and-aum-collection',
                                     'native-token-swaps-without-bpt-or-collection-side-effects']
                native_input = dict(schema_version=1, original_bank_sha256=projection.BANK_SHA256,
                    quotes=[dict(case=row['case'], request=row['request'], quote=row['quote']) for row in quotes
                            if row['case'] in ('two:swap-forward','two:swap-reverse')])
                replay = subprocess.run(['node', str(ROOT / 'integration/tools/ci/managed-v2-aum-plan-replay.cjs')],
                    input=json.dumps(native_input).encode(), capture_output=True, timeout=120, cwd=ROOT)
                report['independent_native_plan_replay'] = json.loads(replay.stdout)
                write(report)
                projection.require(replay.returncode == 0 and report['independent_native_plan_replay'].get('passed') is True,
                                   ('independent native selected-hop replay failed', report['independent_native_plan_replay']))
                report['checks'].append('selected-api-single-hops-reexecuted-by-original-vault-with-real-balances')
                projection.require(len(report['paused_quote_rejections']) == 6, 'paused native bank rejection inventory incomplete')
                projection.require({row['actor'] for row in quotes if row['case'] == 'swap-after-vault-buffer'} == {'router1','router2'},
                                   'independent Vault pause expiry was not quoted by both replicas')
                report['checks'].append('original-vault-pause-keeps-canonical-state-but-blocks-swap-until-own-buffer-expires')
                projection.require(len(report['recovery_source_frames']) == 5, 'Recovery source bank coverage incomplete')
                projection.require({row['actor'] for row in quotes if row['case'] == 'recovery-token-swap'} == {'router1','router2'},
                                   'Recovery original swap was not quoted by both replicas')
                restored = chain.frame['pool']['multi_asset']['managed_v2']
                projection.require(not restored['recovery_mode'] and restored['management_aum_fee'] == '10000000000000000',
                                   'normal configured fee did not return after original disableRecoveryMode')
                report['recovery_exit_source'] = dict(header=chain.frame['header']['hash'],recovery_mode=False,
                                                      effective_fee=restored['management_aum_fee'])
                report['checks'].append('original-recovery-effective-zero-fee-swap-zero-collection-and-normal-exit')
                for fault in FAULTS:
                    # Rewind to a real recorded ancestor, then fault the next
                    # original block. An unchanged head has no new state to read.
                    chain.select(0)
                    generation = complete_bank()['reference']['online_generation']
                    chain.select(1, fault=fault)
                    for actor in ('router1', 'router2'):
                        base.await_condition(lambda: status(actor)[0] == 503, 'invalid Managed source stayed ready: ' + fault)
                        projection.require(status(actor, 'solana:localnet')[0] == 200, 'unrelated Solana scope withdrawn')
                        code, unrelated = base.http(url(actor) + '/v1/quote', token, dict(chainKey='solana:localnet',
                            kind='EXACT_IN', sellToken='A', buyToken='B', sellAmount='10003', caller='A', recipient='B',
                            slippageBps=50, verification='FAST'))
                        projection.require(code == 200 and unrelated['verification']['level'] == 'INDICATIVE'
                                           and int(unrelated['grossBuyAmount']) > 0, 'unrelated Solana quote failed')
                    chain.select(1)
                    snapshot = complete_bank()
                    projection.require(snapshot['reference']['online_generation'] > generation, 'recovery reused generation')
                    quote('router1', next(c for c in chain.cases if c['name'] == 'two:swap-reverse'))
                    report['checks'].append('scope-withdraw-and-new-generation:' + fault)
                    write(report)
                stop('router2')
                start('router2')
                base.wait(url('router2') + '/health/live')
                complete_bank()
                report['checks'].append('router-restart-restores-complete-native-managed-state')
                reverse = next(c for c in chain.cases if c['name'] == 'two:swap-reverse')
                for actor in ('router1', 'router2'):
                    quoted, request = quote(actor, reverse)
                    request['verification'] = 'VERIFIED'
                    code, response = base.http(url(actor) + '/v1/quote', token, request)
                    projection.require(code == 422 and 'INDEPENDENT_SIMULATION_UNAVAILABLE' in json.dumps(response),
                                       'Managed VERIFIED enabled')
                    code, _ = base.http(url(actor) + '/v1/quotes/' + quoted['quoteId'] + '/build', token, {})
                    projection.require(code == 422, 'Managed build enabled')
                report['checks'].append('verified-and-build-remain-disabled')
                projection.require(fingerprint == base.runtime_fingerprint(), 'source/binaries changed during service run')
                projection.require(inputs == base.snapshot_inputs(), 'execution inputs changed during service run')
                report.update(passed=True, state='PASSED', source_binding=fingerprint, input_hashes=inputs,
                    inputs_unchanged=True, started_at_unix_ms=started, completed_at_unix_ms=int(time.time() * 1000),
                    original_bank_sha256=projection.BANK_SHA256, quote_checks=quotes, source_frame_count=len(chain.frames),
                    fault_count=len(FAULTS), future_original_cases_excluded=[c['name'] for c in chain.future_cases],
                    read_counts=chain.reads, receipt_reads=chain.receipt_reads,
                    limitation='Current-date two-token original plus separately executed Vault pause and Recovery branches;  future original frames remain math/parser evidence. '
                               'Independent replay covers selected original single-hop Vault transactions only; '
                               'no public executor, trusted chain proof or VERIFIED/build admission.')
                return report
            finally:
                for actor in list(processes):
                    stop(actor)
    finally:
        for server in servers:
            server.shutdown()
            server.server_close()


def main():
    write(initial())
    try:
        report = run()
        write(report)
        print(json.dumps(dict(passed=report['passed'], checks=len(report['checks']), quotes=len(report['quote_checks']))))
    except BaseException as error:
        report = json.loads(TARGET.read_text()) if TARGET.is_file() else initial()
        report.update(passed=False, state='FAILED', error=str(error), error_type=type(error).__name__)
        write(report)
        raise


if __name__ == '__main__':
    main()
