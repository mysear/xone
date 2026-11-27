#!/usr/bin/env python3
"""Normal binaries with local PG/Kafka, controlled height gaps and retention loss.

Hours are equivalent 12-second-block gap sizes, not real elapsed wall time.
Persistent stale-time admission is tested separately in Go. No mainnet writes.
"""
from pathlib import Path
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import argparse
import hashlib
import json
import re
import secrets
import shutil
import subprocess
import tempfile
import threading
import time
import urllib.request

from infrastructure import Infrastructure, ROOT, port
from configuration import config_for, save_config, service_command
from smoke import http, wait_scope
from kafka_smoke import wait
from weighted_evidence import source_hash, write_result

OUT = ROOT/'integration/artifacts/local'
TARGET = OUT/'r01-r04-recovery-normal-process.json'
BINARIES = ['chain-parser-go/bin/chain-parser-go', 'dex-router/target/debug/dex-router']
TOPIC = 'canonical.evm-31337.v1'
GENESIS = '0x' + '99' * 32
EMPTY_RECEIPTS_ROOT = '0x56e81f171bcc55a6ff8345e692c0f86e5b48e01b996cadc001622fb5e363b421'
RETENTION_POOL = {
    'id': '0x' + '03' * 20,
    'protocol': 'uniswap-v2',
    'token0': '0x' + '01' * 20,
    'token1': '0x' + '02' * 20,
    'fee_numerator': 3,
    'fee_denominator': 1000,
    'program_id': '',
    'accounts': [],
}

class RetentionChain:
    def __init__(self):
        self.head = 100
        self.timestamp = int(time.time())
        self.issued_headers = set()

    def provider(self):
        chain = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def do_POST(self):
                query = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
                method, params = query['method'], query['params']
                result = None
                if method == 'eth_chainId':
                    result = '0x7a69'
                elif method == 'eth_getBlockByNumber':
                    number = chain.head if params[0] == 'latest' else int(params[0], 16)
                    block_hash = GENESIS if number == 0 else f'0x{number:064x}'
                    chain.issued_headers.add(block_hash)
                    result = {
                        'number': hex(number), 'hash': block_hash,
                        'parentHash': GENESIS if number <= 1 else f'0x{number - 1:064x}',
                        'timestamp': hex(chain.timestamp), 'transactions': [],
                        'receiptsRoot': EMPTY_RECEIPTS_ROOT,
                    }
                elif method == 'eth_call':
                    call, anchor = params
                    if (call['to'] == RETENTION_POOL['id'] and anchor.get('requireCanonical') is True
                            and anchor.get('blockHash') in chain.issued_headers):
                        tokens = {'0x0dfe1681': 'token0', '0xd21220a7': 'token1'}
                        if call['data'] in tokens:
                            result = '0x' + RETENTION_POOL[tokens[call['data']]][2:].rjust(64, '0')
                        elif call['data'] == '0x0902f1ac':
                            result = f'0x{1000000:064x}{1000000:064x}{chain.timestamp:064x}'
                elif method == 'eth_getBlockReceipts':
                    result = []
                response = {'jsonrpc': '2.0', 'id': query['id']}
                if result is None:
                    response['error'] = {'code': -32601, 'message': 'unsupported fixture request'}
                else:
                    response['result'] = result
                data = json.dumps(response).encode()
                self.send_response(200)
                self.send_header('Content-Type', 'application/json')
                self.send_header('Content-Length', str(len(data)))
                self.end_headers()
                self.wfile.write(data)

        server = ThreadingHTTPServer(('127.0.0.1', 0), Handler)
        threading.Thread(target=server.serve_forever, daemon=True).start()
        return server

def binary_fingerprint():
    return {name:hashlib.sha256((ROOT/name).read_bytes()).hexdigest() for name in BINARIES}

def require(ok, message):
    if not ok:
        raise AssertionError(message)

def owner_cursor(base, token):
    request = urllib.request.Request(
        base + '/internal/chains/evm:31337/bootstrap',
        headers={'Authorization': 'Bearer ' + token},
    )
    with urllib.request.urlopen(request, timeout=3) as response:
        raw = response.read(8 * 1024**2 + 1)
    require(len(raw) <= 8 * 1024**2, 'owner bootstrap exceeded the payload limit')
    proto = ROOT / 'common/protoc/proto'
    decoded = subprocess.run(
        ['protoc', '--proto_path=' + str(proto), '--decode=dex.v1.OwnerBootstrap',
         str(proto / 'dex/v1/state.proto')],
        input=raw, capture_output=True, check=True,
    ).stdout.decode()
    cursor = re.search(r'^cursor \{\n  topic: "([^"]+)"\n  next_offset: ([0-9]+)\n\}', decoded, re.M)
    require(cursor and cursor[1] == TOPIC, 'missing partition-zero owner cursor')
    return int(cursor[2])

def topic_offset(infra, time_option):
    output = infra.kafka_command([
        'kafka-get-offsets.sh', '--bootstrap-server', infra.broker,
        '--topic', TOPIC, '--time', time_option,
    ])
    match = re.search(rf'^{re.escape(TOPIC)}:0:([0-9]+)$', output, re.M)
    require(match, f'Kafka offset missing for {time_option}: {output}')
    return int(match[1])

def expire_owner_cursor(infra, root, cursor):
    low_before = topic_offset(infra, 'earliest')
    end = topic_offset(infra, 'latest')
    require(low_before <= cursor < end, (low_before, cursor, end))
    offsets = root / 'retention-offsets.json'
    offsets.write_text(json.dumps({
        'partitions': [{'topic': TOPIC, 'partition': 0, 'offset': end}],
        'version': 1,
    }))
    path = str(offsets)
    if infra.kafka_container:
        path = '/tmp/xone-retention-offsets.json'
        infra.docker(['cp', offsets, f'{infra.kafka_container}:{path}'])
    infra.kafka_command([
        'kafka-delete-records.sh', '--bootstrap-server', infra.broker,
        '--offset-json-file', path,
    ])
    low_after = topic_offset(infra, 'earliest')
    require(low_after == end and low_after > cursor, (cursor, low_after, end))
    return low_before, low_after

def raw_options(path, payload):
    return {'sink_path':str(path), 'sink_max_bytes_per_chain':2*1024**3,
        'sink_max_records_per_chain':100000, 'interval_ms':500,
        'timeout_ms':3000, 'replay_timeout_ms':15000, 'keep_ms':0,
        'target_bytes':384*1024**2, 'max_scan':64, 'max_archive':64,
        'max_reclaims':64, 'max_raw_bytes':min(payload*2,32*1024**2),
        'max_decoded_bytes':min(payload*2,32*1024**2), 'max_page_records':100,
        'max_request_bytes':8192, 'max_pins_per_capture':4096,
        'decoder_version':'staged-state/public-adapter-v1'}

def run(retention_only=False):
    OUT.mkdir(exist_ok=True)
    report={'schema_version':1, 'state':'RUNNING', 'passed':False,
        'all_requested_features_complete':False, 'production_ready':False,
        'authority':'controlled public-style RPC; actual PG/Kafka and normal Go/quote binaries',
        'source_tree_sha256':source_hash(), 'started_at_unix_ms':int(time.time()*1000),
        'hours_are_height_gap_equivalents':True, 'cases':[],
        'binary_sha256':binary_fingerprint(),
        'runner_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    if retention_only:
        target = OUT / 'r04-retention-normal-process.json'
        chain = RetentionChain()
        servers = [chain.provider(), chain.provider()]
        pools = [RETENTION_POOL]
        expected_amount = str(10000 * 997 * 1000000 // (1000000 * 1000 + 10000 * 997))
        report['authority'] = 'controlled single-pool RPC with empty blocks; actual PG/Kafka and normal Go/quote binaries'
    else:
        from receipt_witness_smoke import Chain, NAMES, profile, oracle
        from reorg_smoke import await_condition
        target = TARGET
        chain = Chain()
        servers = [chain.provider(), chain.provider(True)]
        pools = [profile(name)[0] for name in NAMES]
        expected_amount = oracle('uniswap-v2', True)
    write_result(target, report)
    processes={};logs={}
    try:
        with Infrastructure() as infra, tempfile.TemporaryDirectory(prefix='xone-r01-r04-rto-') as directory:
            root=Path(directory);token=secrets.token_urlsafe(32)
            names={'go':'chain-parser-go','router1':'dex-router','router2':'dex-router'}
            ports={alias:port()for alias in names};configs={}
            def base(alias):return f'http://127.0.0.1:{ports[alias]}'
            for alias,name in names.items():
                cfg=config_for(name,root/alias,token,ports[alias])
                cfg['chains']=[c for c in cfg['chains']if c['key']=='evm:31337']
                cfg['journal'].update(transport='kafka',source_urls=[base('go')],poll_interval_ms=100,request_timeout_ms=15000)
                cfg['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':TOPIC},replica_id=alias,transaction_timeout_ms=4000)
                cfg['control'].update(backend='disabled'if name=='dex-router'else'postgres',dsn_file=''if name=='dex-router'else str(infra.dsnfile),instance_id=alias,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000)
                cfg['startup'].update(max_baseline_age_ms=180000,max_tail_blocks=128)
                cfg['history']['enabled']=False
                if alias=='go':
                    cfg['chains'][0].update(genesis_hash=GENESIS,confirmation_tag='latest',auto_reorg_window=4,rpc_urls=[f'http://127.0.0.1:{s.server_port}'for s in servers],pools=pools)
                    payload=cfg['journal']['max_payload_bytes']
                    cfg['journal']['raw_evidence']={'max_bytes':512*1024**2,'max_records':100000,'max_record_bytes':payload*4+16384}
                    cfg['journal']['raw_maintenance']=raw_options(root/'cold-evidence',payload)
                else:
                    cfg['routing'].update(max_state_age_ms=180000,quote_ttl_ms=60000)
                configs[alias]=save_config(root/alias/'config',cfg)
            def start(alias):
                logs[alias]=(OUT/f'r01-r04-rto-{alias}.log').open('ab')
                processes[alias]=subprocess.Popen(service_command(names[alias],configs[alias]),cwd=ROOT/names[alias],stdout=logs[alias],stderr=subprocess.STDOUT)
            def stop(alias):
                p=processes.pop(alias,None)
                if p:
                    p.terminate()
                    try:p.wait(timeout=15)
                    except subprocess.TimeoutExpired:p.kill();p.wait()
                stream=logs.pop(alias,None)
                if stream:stream.close()
            def status(alias):return http(base(alias)+'/internal/chains/evm:31337/status',token)[1]
            def quote(alias, expected=200):
                pool = pools[0]
                body={'chainKey':'evm:31337','kind':'EXACT_IN','sellToken':pool['token0'],'buyToken':pool['token1'],'sellAmount':'10000','caller':pool['token0'],'recipient':pool['token1'],'slippageBps':50,'verification':'FAST'}
                code,value=http(base(alias)+'/v1/quote',token,body)
                require(code==expected,(alias,code,value))
                if expected == 200:
                    require(value['grossBuyAmount'] == expected_amount, value)
                return value
            try:
                start('go');wait(base('go')+'/health/ready')
                for alias in ['router1','router2']:start(alias);wait(base(alias)+'/health/live');wait_scope(base(alias),token,'evm:31337')
                old=quote('router1')
                cases = [] if retention_only else [(1, False), (24, False), (720, False), (720, True)]
                for hours,empty in cases:
                    stop('go')
                    if empty:
                        state=root/'go/state/evm-31337'
                        require(state.is_dir(),'empty-store experiment missing existing owner KV')
                        shutil.rmtree(state)
                    old_head=chain.head;chain.head+=hours*300;chain.timestamp=int(time.time())
                    before_receipts=chain.receipts;started=time.monotonic();start('go');wait(base('go')+'/health/ready')
                    scope_ms=int((time.monotonic()-started)*1000)
                    for alias in ['router1','router2']:
                        await_condition(lambda alias=alias:status(alias).get('snapshot_ref',{}).get('block',{}).get('number_or_slot')==chain.head,'HEAD_FIRST scope did not converge')
                    quoted=quote('router1');quote_ms=int((time.monotonic()-started)*1000)
                    require(quoted['onlineGeneration']>old['onlineGeneration'],'restart reused old generation')
                    require(chain.receipts-before_receipts<=8,'restart caught up the historical receipt range')
                    source=http(base('go')+'/internal/chains/evm:31337/source-status',token)[1]
                    history=source.get('history',{})
                    require(history.get('Enabled')is False or history.get('enabled')is False,('history pause was not visible',history))
                    require(http(base('go')+'/internal/chains/evm:31337/raw/status',token)[0]==200,'raw lifecycle not mounted')
                    case={'equivalent_offline_hours':hours,'empty_owner_kv':empty,'old_height':old_head,'current_height':chain.head,'first_qualified_scope_ms':scope_ms,'first_successful_quote_ms':quote_ms,'online_generation':quoted['onlineGeneration'],'receipt_calls_during_restore':chain.receipts-before_receipts,'history_paused':True,'quote_verification':quoted['verification']['level'],'wallclock_downtime_simulated':False}
                    report['cases'].append(case);write_result(target,report);old=quoted
                expired = owner_cursor(base('go'), token)
                old_height = chain.head
                for alias in ['router1', 'router2', 'go']:
                    stop(alias)
                low_before, low_after = expire_owner_cursor(infra, root, expired)
                start('router1')
                wait(base('router1') + '/health/live')
                require(http(base('router1') + '/health/ready')[0] == 503, 'cold router ready without owner')
                quote('router1', 503)
                chain.head += 300
                chain.timestamp = int(time.time())
                started = time.monotonic()
                start('go')
                wait(base('go') + '/health/ready')
                wait_scope(base('router1'), token, 'evm:31337')
                scope_ms = int((time.monotonic() - started) * 1000)
                quoted = quote('router1')
                quote_ms = int((time.monotonic() - started) * 1000)
                restored = owner_cursor(base('go'), token)
                owner = status('go')
                gaps = owner['history_gaps']
                require(restored > low_after, (restored, low_after))
                require(quoted['onlineGeneration'] > old['onlineGeneration'], 'retention restart reused a generation')
                require(quoted['snapshotRef'] == owner['snapshot_ref'], 'quote did not bind the owner reference')
                require(gaps and quoted['quality']['historyGaps'] == gaps, 'owner history gap missing from quote')
                chain.head += 1
                deadline = time.monotonic() + 25
                while True:
                    _, state = http(base('router1') + '/internal/chains/evm:31337/status', token)
                    if state.get('snapshot_ref', {}).get('block', {}).get('number_or_slot') == chain.head:
                        break
                    require(time.monotonic() < deadline, 'router did not follow the journal after retention recovery')
                    time.sleep(0.05)
                continued = quote('router1')
                require(continued['onlineGeneration'] == quoted['onlineGeneration'], 'live block changed generation')
                require(continued['quality']['historyGaps'] == gaps, 'live block lost the history gap')
                report['retention'] = {
                    'expired_cursor': expired, 'restored_cursor': restored,
                    'low_watermark_before': low_before, 'low_watermark_after': low_after,
                    'old_height': old_height, 'current_height': chain.head - 1,
                    'followed_height': chain.head, 'history_gaps': gaps,
                    'online_generation': quoted['onlineGeneration'],
                    'history_paused': True, 'wallclock_downtime_simulated': False,
                    'first_qualified_scope_ms': scope_ms, 'first_successful_quote_ms': quote_ms,
                    'quote_verification': quoted['verification']['level'],
                }
                write_result(target, report)
                request=urllib.request.Request(base('router1')+'/metrics',headers={'Authorization':'Bearer '+token})
                with urllib.request.urlopen(request,timeout=3)as response:metrics=response.read(65536).decode()
                require('xone_first_successful_quote_seconds' in metrics,'normal quote recovery metric missing')
                report['metrics_observation']=metrics[:4096]
                require(source_hash()==report['source_tree_sha256'],'core source changed during recovery process run')
                require(binary_fingerprint()==report['binary_sha256'],'normal binaries changed during recovery process run')
                report.update(passed=True,state='LOCAL_RECOVERY_PROCESS_SCOPE_VERIFIED',completed_at_unix_ms=int(time.time()*1000))
                write_result(target,report);print(json.dumps({'passed':True,'cases':len(report['cases']),'retention':report['retention'],'all_requested_features_complete':False}))
                return report
            finally:
                for alias in list(processes):stop(alias)
    except BaseException as error:
        report.update(state='FAILED',error=str(error),error_type=type(error).__name__,completed_at_unix_ms=int(time.time()*1000));write_result(target,report);raise
    finally:
        for server in servers:
            server.shutdown()
            server.server_close()
            if hasattr(server, 'upstream'):
                server.upstream.shutdown()
                server.upstream.server_close()

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--retention-only', action='store_true', help='run the single-pool retention scenario')
    run(parser.parse_args().retention_only)
