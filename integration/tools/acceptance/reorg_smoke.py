#!/usr/bin/env python3
from configuration import config_for,save_config,sample_for,service_command
"""Synthetic EVM forks, real PG/Kafka and both independent router replicas.

The two loopback RPC endpoints validate protocol behavior, not independence of
real chain providers or production chain correctness.
"""
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
import argparse
from contextlib import nullcontext
from types import SimpleNamespace
import hashlib
import json
import secrets
import os
import shutil
import subprocess
import tempfile
import threading
import time

from receipt_reference import EMPTY_ROOT, seal_receipts
from infrastructure import Infrastructure, ROOT, port
from smoke import http, quote, wait_scope
from kafka_smoke import wait

OUT = ROOT / 'integration/artifacts/local'
TOKEN0 = '0x' + '01' * 20
TOKEN1 = '0x' + '02' * 20
POOL = '0x' + '03' * 20
GENESIS = '0x' + '99' * 32


class Chain:
    def __init__(self):
        self.head = 100
        self.branch = 0
        self.fork_start = 101
        self.disagree = False
        self.unavailable = False
        self.complex_block = None
        self.hold = False
        self.release = threading.Event()
        self.timestamp = int(time.time())

    def block(self, number, secondary=False):
        def digest(n):
            branch = self.branch if n >= self.fork_start else 0
            return '0x' + hashlib.sha256(f'{branch}/{n}'.encode()).hexdigest()
        block = {'number': hex(number), 'hash': GENESIS if number == 0 else digest(number),
                 'parentHash': digest(max(0, number - 1)), 'timestamp': hex(self.timestamp), 'transactions': []}
        if number > 0:
            block['transactions'] = ['0x' + hashlib.sha256(('tx/' + block['hash']).encode()).hexdigest()]
        block['receiptsRoot'] = self.receipt(block)[0] if number else EMPTY_ROOT
        if secondary and self.disagree and number != 0:
            block['hash'] = '0x' + 'ff' * 32
        return block

    def receipt(self, block):
        number = int(block['number'], 16)
        branch = self.branch if number >= self.fork_start else 0
        logs = []
        if branch != 2:
            price = (20 if number == 101 else 100) if branch == 0 else (30 if number == 101 else 40)
            logs = [{'blockHash': block['hash'], 'transactionHash': block['transactions'][0], 'transactionIndex': '0x0',
                     'logIndex': '0x0', 'removed': False, 'address': POOL,
                     'topics': ['0xd78ad95fa46c994b6551d0da85fc275fe613ce37657fb8d5e3d130840159d822', '0x' + '00' * 32, '0x' + '00' * 32],
                     'data': '0x' + f'{1:064x}{(1 if self.complex_block == number else 0):064x}{0:064x}{price:064x}'}]
        root, receipts = seal_receipts([{'blockHash': block['hash'], 'transactionHash': block['transactions'][0], 'transactionIndex': '0x0', 'logs': logs}])
        return root, receipts[0]

    def provider(self, secondary):
        chain = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_POST(self):
                query = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                method, params = query['method'], query['params']
                error = None
                result = None
                if method == 'eth_chainId':
                    result = '0x7a69'
                elif method == 'eth_getBlockByNumber':
                    number = chain.head if params[0] in ['latest', 'safe', 'finalized'] else int(params[0], 16)
                    if chain.hold and number == 100 and chain.branch:
                        chain.release.wait(timeout=8)
                    result = chain.block(number, secondary)
                elif method == 'eth_getTransactionReceipt':
                    known = [chain.block(n) for n in range(1, chain.head + 1)]
                    block = next((b for b in known if params[0] in b['transactions']), None)
                    if block is not None:
                        result = chain.receipt(block)[1]
                elif method == 'eth_call':
                    anchor = params[1]
                    canonical = {chain.block(n)['hash']: n for n in range(chain.head + 1)}
                    if anchor.get('requireCanonical') is not True or anchor.get('blockHash') not in canonical:
                        error = {'code': -32000, 'message': 'not canonical'}
                    else:
                        selector = params[0]['data']
                        if selector in ['0x0dfe1681', '0xd21220a7']:
                            result = '0x' + (TOKEN0 if selector == '0x0dfe1681' else TOKEN1)[2:].rjust(64, '0')
                        else:
                            number = canonical[anchor['blockHash']]
                            branch = chain.branch if number >= chain.fork_start else 0
                            result = '0x' + f'{1000000 + branch * 1000 + number:064x}{1000000:064x}{0:064x}'
                else:
                    error = {'code': -32601, 'message': 'unsupported fixture method'}
                if chain.unavailable:
                    error = {'code': -32001, 'message': 'fixture temporary outage'}
                response = {'jsonrpc': '2.0', 'id': query['id']}
                response.update({'error': error} if error else {'result': result})
                raw = json.dumps(response).encode()
                self.send_response(200)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(raw)))
                self.end_headers()
                self.wfile.write(raw)

        server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        return server


def await_condition(predicate, message, timeout=25):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            if predicate():
                return
        except (OSError, ValueError):
            pass
        time.sleep(.05)
    raise RuntimeError(message)


def run(transport):
    report = {'profile': 'synthetic-evm-fork-' + transport, 'production_ready': False, 'checks': []}
    chain = Chain()
    providers = [chain.provider(False), chain.provider(True)]
    processes, logs = {}, {}
    try:
        with (Infrastructure() if transport == 'kafka' else nullcontext(SimpleNamespace(broker='127.0.0.1:9092', dsnfile=''))) as infra, tempfile.TemporaryDirectory(prefix='xone-reorg-') as folder:
            if transport == 'kafka':
                env = os.environ.copy()
                env['PATH'] = str(Path.home() / '.cargo/bin') + ':' + env['PATH']
                env['XONE_TEST_KAFKA_BROKER'] = infra.broker
                command = [str(Path.home() / '.cargo/bin/cargo'), '+1.90.0', 'test', '--locked', '-p', 'xone-quote-api', '--lib', 'kafka_poll_failure_between_barrier_and_commit_resumes_exact_applied_cursor', '--', '--nocapture']
                result = subprocess.run(command, cwd=ROOT / 'dex-router', env=env, capture_output=True, text=True, timeout=90)
                (OUT / 'reorg-reader-recovery.log').write_text(result.stdout + result.stderr)
                if result.returncode:
                    raise RuntimeError('Kafka reader recovery regression failed: ' + (result.stdout + result.stderr)[-3000:])
                report['checks'].append('kafka-poll-failure-after-begin-resumes-applied-cursor-and-matching-commit-without-restart')
            # Infrastructure startup is not simulated chain downtime. Anchor
            # the frozen RPC fixture only when services are about to start.
            chain.timestamp = int(time.time())
            root = Path(folder)
            token = secrets.token_urlsafe(32)
            names = {'go': 'chain-parser-go', 'rust': 'chain-parser-rust', 'router1': 'dex-router', 'router2': 'dex-router'}
            ports = {alias: port() for alias in names}
            configs = {}
            sources = [f'http://127.0.0.1:{ports[a]}' for a in ['go', 'rust']]
            for alias, name in names.items():
                directory=root/alias
                service=config_for(name,directory,token,ports[alias])
                if alias=='go':
                    service['chains'][0].update(genesis_hash=GENESIS,confirmation_tag='latest',auto_reorg_window=2,
                        rpc_urls=[f'http://127.0.0.1:{p.server_port}' for p in providers],
                        pools=[{'id':POOL,'protocol':'uniswap-v2','token0':TOKEN0,'token1':TOKEN1,
                                'fee_numerator':3,'fee_denominator':1000,'program_id':'','accounts':[]}])
                elif alias=='rust':sample_for('solana:localnet',directory/'fixtures',chain.timestamp*1000)
                service['journal'].update(transport=transport,source_urls=sources,poll_interval_ms=100,request_timeout_ms=10000)
                service['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1','solana:localnet':'canonical.solana-localnet.v1'},replica_id=alias,transaction_timeout_ms=4000)
                service['control'].update(backend='disabled' if name=='dex-router' or transport!='kafka' else 'postgres',dsn_file='' if name=='dex-router' else str(infra.dsnfile),instance_id=alias,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000)
                service['logging']['level']='debug';service['startup']['max_baseline_age_ms']=120000
                if name=='dex-router':service['routing'].update(max_state_age_ms=120000,quote_ttl_ms=60000)
                configs[alias]=save_config(directory/'config',service)

            analysis_runs=0
            if transport=='kafka':
                value=json.loads(configs['go'].read_text())
                value['service'].update(role='analytics',mode='journal')
                value['state']['path']=str(root/'analytics/state')
                value['chains'][0].update(analysis_interval_ms=60000,rpc_urls=[])
                value['control'].update(backend='disabled',dsn_file='')
                analysis_config=save_config(root/'analytics/config',value)

            def analyze(expect_success=True):
                nonlocal analysis_runs
                analysis_runs += 1
                process = subprocess.run([str(ROOT / 'chain-parser-go/bin/chain-parser-go'), '--config', str(analysis_config)],
                    cwd=ROOT / 'chain-parser-go', capture_output=True, text=True, timeout=20)
                (OUT / f'analytics-journal-{analysis_runs}.log').write_text(process.stdout + process.stderr)
                assert (process.returncode == 0) == expect_success, (process.returncode, process.stdout, process.stderr)
                checkpoint = json.loads((root / 'analytics/state/analytics/evm:31337.journal.json').read_text())
                (OUT / f'analytics-journal-{analysis_runs}.json').write_text(json.dumps(checkpoint, indent=2) + '\n')
                return checkpoint

            def start(alias):
                name = names[alias]
                logs[alias] = (OUT / f'reorg-{transport}-{alias}.log').open('ab')
                processes[alias] = subprocess.Popen(service_command(name,configs[alias],root/alias/'fixtures' if alias=='rust' else None),cwd=ROOT/name,stdout=logs[alias],stderr=subprocess.STDOUT)

            def stop(alias):
                process = processes.pop(alias, None)
                if process:
                    process.terminate()
                    try:
                        process.wait(timeout=10)
                    except subprocess.TimeoutExpired:
                        process.kill()
                        process.wait()
                if alias in logs:
                    logs.pop(alias).close()

            def base(alias):
                return f'http://127.0.0.1:{ports[alias]}'

            def status(alias):
                return http(base(alias) + '/internal/chains/evm:31337/status', token)

            def evm_quote(alias):
                body = {'chainKey': 'evm:31337', 'kind': 'EXACT_IN', 'sellToken': TOKEN0, 'buyToken': TOKEN1,
                        'sellAmount': '10003', 'caller': TOKEN0, 'recipient': TOKEN1, 'slippageBps': 50, 'verification': 'FAST'}
                code, value = http(base(alias) + '/v1/quote', token, body)
                assert code == 200, (code, value)
                return value

            def assert_revoked(alias, old):
                await_condition(lambda: status(alias)[0] == 503, f'{alias} did not revoke')
                code, error = http(base(alias) + f'/v1/quotes/{old["quoteId"]}/build', token, {}, 'POST')
                assert code == 409 and error['error'] == 'SNAPSHOT_REORGED', (code, error)
                quote(base(alias), token, 'solana:localnet', 'B')

            try:
                for alias in ['go', 'rust']:
                    start(alias)
                    wait(base(alias) + '/health/ready')
                for alias in ['router1', 'router2']:
                    start(alias)
                    wait(base(alias) + '/health/live')
                    wait_scope(base(alias), token, 'evm:31337')
                    wait_scope(base(alias), token, 'solana:localnet')
                before_outage = {a: evm_quote(a) for a in ['router1','router2']}
                chain.unavailable = True
                for alias in before_outage:
                    assert_revoked(alias, before_outage[alias])
                chain.unavailable = False
                for alias in before_outage:
                    wait_scope(base(alias),token,'evm:31337')
                    recovered = evm_quote(alias)
                    assert recovered['onlineGeneration'] > before_outage[alias]['onlineGeneration']
                    assert recovered['snapshotRef']['canonical_epoch'] == before_outage[alias]['snapshotRef']['canonical_epoch']
                report['checks'].append('temporary-source-outage-revokes-scope-but-recovery-does-not-invent-fork-epoch')
                chain.head = 102
                for alias in ['router1', 'router2']:
                    await_condition(lambda: status(alias)[1].get('snapshot_ref', {}).get('block', {}).get('number_or_slot') == 102, 'initial tail did not reach 102')
                if transport == 'kafka':
                    original_analysis = analyze()
                    candle = original_analysis['release']['candles'][0]
                    assert original_analysis['release']['publishable'] and candle['high_rational'] == '100' and candle['low_rational'] == '20' and candle['count'] == 2
                    duplicate = analyze()
                    assert duplicate == original_analysis
                    report['checks'].append('isolated-analytics-job-consumes-normalized-swaps-and-restart-is-idempotent')
                old = {a: evm_quote(a) for a in ['router1', 'router2']}
                chain.hold = True
                chain.branch = 1
                for alias in old:
                    assert_revoked(alias, old[alias])
                assert http(base('go') + '/internal/chains/evm:31337/bootstrap', token)[0] == 503
                report['checks'].append('scope-revocation-invalidates-both-router-quote-handles-before-recovery')
                if transport == 'kafka':
                    suspended = analyze()
                    assert not suspended['release']['publishable'] and not suspended['release']['candles']
                    report['checks'].append('analytics-durable-revocation-withholds-half-reorg-release')
                chain.release.set()
                chain.hold = False
                for alias in old:
                    wait_scope(base(alias), token, 'evm:31337')
                    fresh = evm_quote(alias)
                    assert fresh['snapshotRef']['canonical_epoch'] == old[alias]['snapshotRef']['canonical_epoch'] + 1
                    assert fresh['onlineGeneration'] == old[alias]['onlineGeneration']
                    assert fresh['snapshotRef']['journal_seq'] == old[alias]['snapshotRef']['journal_seq'] + 3
                report['checks'].append('two-block-undo-and-ordered-replacement-commit-preserve-generation')
                if transport == 'kafka':
                    corrected = analyze()
                    candle = corrected['release']['candles'][0]
                    assert corrected['release']['publishable'] and candle['high_rational'] == '40' and candle['low_rational'] == '30' and candle['count'] == 2
                    assert candle['revision'] > original_analysis['release']['candles'][0]['revision']
                    report['checks'].append('reorg-journal-removes-old-high-low-and-matches-independent-window-recomputation')
                previous = evm_quote('router1')
                chain.branch = 2
                await_condition(lambda: status('router1')[1].get('snapshot_ref', {}).get('canonical_epoch') == previous['snapshotRef']['canonical_epoch'] + 1, 'second fork did not complete')
                report['checks'].append('replacement-branch-undo-supports-second-fork')
                if transport == 'kafka':
                    empty = analyze()
                    candle = empty['release']['candles'][0]
                    assert empty['release']['publishable'] and candle['deleted'] and candle['count'] == 0
                    assert candle['revision'] > corrected['release']['candles'][0]['revision']
                    report['checks'].append('reorg-removing-all-trades-emits-explicit-window-delete-revision')
                for alias in old:
                    wait_scope(base(alias), token, 'evm:31337')
                before = {a: evm_quote(a) for a in old}
                chain.disagree = True
                chain.branch = 3
                chain.fork_start = 100
                for alias in before:
                    assert_revoked(alias, before[alias])
                stop('go')
                start('go')
                def unconfirmed_owner_is_closed():
                    if processes['go'].poll() is not None:
                        return processes['go'].returncode != 0
                    # Cancellation can leave the previous lease until expiry.
                    # A fenced standby is correct; data readiness is the gate.
                    return (http(base('go') + '/health/startup')[0] == 200
                            and http(base('go') + '/health/ready')[0] == 503
                            and http(base('go') + '/internal/chains/evm:31337/bootstrap', token)[0] == 503)
                await_condition(unconfirmed_owner_is_closed, 'unconfirmed restart became data ready')
                report['unconfirmed_restart_outcome'] = 'REJECTED' if processes['go'].poll() is not None else 'FENCED_STANDBY'
                for alias in before:
                    assert status(alias)[0] == 503
                report['checks'].append('provider-disagreement-and-parser-restart-cannot-clear-durable-revocation')
                stop('go')
                chain.disagree = False
                start('go')
                wait(base('go') + '/health/ready')
                for alias in before:
                    wait_scope(base(alias), token, 'evm:31337')
                    fresh = evm_quote(alias)
                    assert fresh['onlineGeneration'] > before[alias]['onlineGeneration']
                    assert fresh['snapshotRef']['canonical_epoch'] == before[alias]['snapshotRef']['canonical_epoch'] + 1
                report['checks'].append('qualified-head-first-recovery-advances-generation-and-confirmed-canonical-epoch')
                if transport == 'kafka':
                    recovered_analysis = analyze()
                    assert recovered_analysis['release']['publishable'] and recovered_analysis['release']['history_gaps']
                    assert recovered_analysis['state']['snapshot_ref']['online_generation'] > empty['state']['snapshot_ref']['online_generation']
                    report['checks'].append('analytics-head-first-recovery-discloses-history-gap-without-reading-online-kv')
                    last_cursor = recovered_analysis['state']['cursor']
                    chain.complex_block = chain.head + 1
                    chain.head += 1
                    for alias in before:
                        await_condition(lambda: status(alias)[1].get('snapshot_ref', {}).get('block', {}).get('number_or_slot') == chain.head, 'poison block did not reach router')
                    failed_analysis = analyze(expect_success=False)
                    assert not failed_analysis['release']['publishable'] and failed_analysis['state']['cursor'] == last_cursor
                    assert failed_analysis['release']['reason'] == 'ANALYTICS_COMPLEX_SWAP_UNSUPPORTED'
                    for alias in before:
                        evm_quote(alias)
                    report['checks'].append('analytics-poison-fact-stops-only-job-retains-cursor-and-durably-revokes-release')
                report['passed'] = True
            finally:
                chain.release.set()
                for alias in list(processes):
                    stop(alias)
    finally:
        for provider in providers:
            provider.shutdown()
            provider.server_close()
    report.update(measured_at_unix_ms=int(time.time() * 1000), tool_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest())
    return report


if __name__ == '__main__':
    OUT.mkdir(parents=True, exist_ok=True)
    parser = argparse.ArgumentParser()
    parser.add_argument('--transport', choices=['kafka', 'http-development'], default='kafka')
    args = parser.parse_args()
    output = OUT / ('reorg-smoke.json' if args.transport == 'kafka' else 'reorg-http-smoke.json')
    try:
        result = run(args.transport)
    except Exception as error:
        result = {'passed': False, 'production_ready': False, 'error_type': type(error).__name__, 'error': str(error)}
        output.write_text(json.dumps(result, indent=2) + '\n')
        if args.transport == 'kafka':
            (OUT / 'analytics-journal-smoke.json').write_text(json.dumps(result, indent=2) + '\n')
        raise
    output.write_text(json.dumps(result, indent=2) + '\n')
    if args.transport == 'kafka':
        analytics = dict(result)
        analytics['scope'] = 'normalized-v2-facts-isolated-analytics-job'
        analytics['checks'] = [c for c in result.get('checks', []) if 'analytics' in c or c.startswith('reorg-journal-') or c.startswith('reorg-removing-')]
        (OUT / 'analytics-journal-smoke.json').write_text(json.dumps(analytics, indent=2) + '\n')
    print(json.dumps(result, indent=2))
