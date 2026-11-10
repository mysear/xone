#!/usr/bin/env python3
"""Real normal binaries + loopback PG/Kafka, controlled large height gaps.

Hours are equivalent 12-second-block gap sizes, not real elapsed wall time.
Persistent stale-time admission is tested separately in Go. No mainnet writes.
"""
from pathlib import Path
import hashlib
import json
import secrets
import shutil
import subprocess
import tempfile
import time
import urllib.request

from infrastructure import Infrastructure, ROOT, port
from configuration import config_for, save_config, service_command
from receipt_witness_smoke import Chain, GENESIS, NAMES, profile, oracle
from reorg_smoke import await_condition
from smoke import http, wait_scope
from kafka_smoke import wait
from weighted_evidence import source_hash, write_result

OUT = ROOT/'integration/artifacts/local'
TARGET = OUT/'r01-r04-recovery-normal-process.json'
BINARIES = ['chain-parser-go/bin/chain-parser-go', 'dex-router/target/debug/dex-router']

def binary_fingerprint():
    return {name:hashlib.sha256((ROOT/name).read_bytes()).hexdigest() for name in BINARIES}

def require(ok, message):
    if not ok:
        raise AssertionError(message)

def raw_options(path, payload):
    return {'sink_path':str(path), 'sink_max_bytes_per_chain':2*1024**3,
        'sink_max_records_per_chain':100000, 'interval_ms':500,
        'timeout_ms':3000, 'replay_timeout_ms':15000, 'keep_ms':0,
        'target_bytes':384*1024**2, 'max_scan':64, 'max_archive':64,
        'max_reclaims':64, 'max_raw_bytes':min(payload*2,32*1024**2),
        'max_decoded_bytes':min(payload*2,32*1024**2), 'max_page_records':100,
        'max_request_bytes':8192, 'max_pins_per_capture':4096,
        'decoder_version':'staged-state/public-adapter-v1'}

def run():
    OUT.mkdir(exist_ok=True)
    report={'schema_version':1, 'state':'RUNNING', 'passed':False,
        'all_requested_features_complete':False, 'production_ready':False,
        'authority':'controlled public-style RPC; actual PG/Kafka and normal Go/quote binaries',
        'source_tree_sha256':source_hash(), 'started_at_unix_ms':int(time.time()*1000),
        'hours_are_height_gap_equivalents':True, 'cases':[],
        'binary_sha256':binary_fingerprint(),
        'runner_sha256':hashlib.sha256(Path(__file__).read_bytes()).hexdigest()}
    write_result(TARGET,report)
    chain=Chain();servers=[chain.provider(),chain.provider(True)]
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
                cfg['journal']['kafka'].update(brokers=[infra.broker],topics={'evm:31337':'canonical.evm-31337.v1'},replica_id=alias,transaction_timeout_ms=4000)
                cfg['control'].update(backend='disabled'if name=='dex-router'else'postgres',dsn_file=''if name=='dex-router'else str(infra.dsnfile),instance_id=alias,lease_ttl_ms=30000,renewal_ms=1000,min_commit_remaining_ms=7000)
                cfg['startup'].update(max_baseline_age_ms=180000,max_tail_blocks=128)
                cfg['history']['enabled']=False
                if alias=='go':
                    cfg['chains'][0].update(genesis_hash=GENESIS,confirmation_tag='latest',auto_reorg_window=4,rpc_urls=[f'http://127.0.0.1:{s.server_port}'for s in servers],pools=[profile(n)[0]for n in NAMES])
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
            def quote(alias):
                pool,_=profile('uniswap-v2')
                body={'chainKey':'evm:31337','kind':'EXACT_IN','sellToken':pool['token0'],'buyToken':pool['token1'],'sellAmount':'10000','caller':pool['token0'],'recipient':pool['token1'],'slippageBps':50,'verification':'FAST'}
                code,value=http(base(alias)+'/v1/quote',token,body)
                require(code==200,(alias,code,value));require(value['grossBuyAmount']==oracle('uniswap-v2',True),value)
                return value
            try:
                start('go');wait(base('go')+'/health/ready')
                for alias in ['router1','router2']:start(alias);wait(base(alias)+'/health/live');wait_scope(base(alias),token,'evm:31337')
                old=quote('router1')
                for hours,empty in [(1,False),(24,False),(720,False),(720,True)]:
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
                    report['cases'].append(case);write_result(TARGET,report);old=quoted
                request=urllib.request.Request(base('router1')+'/metrics',headers={'Authorization':'Bearer '+token})
                with urllib.request.urlopen(request,timeout=3)as response:metrics=response.read(65536).decode()
                require('xone_first_successful_quote_seconds' in metrics,'normal quote recovery metric missing')
                report['metrics_observation']=metrics[:4096]
                require(source_hash()==report['source_tree_sha256'],'core source changed during recovery process run')
                require(binary_fingerprint()==report['binary_sha256'],'normal binaries changed during recovery process run')
                report.update(passed=True,state='LOCAL_RECOVERY_PROCESS_SCOPE_VERIFIED',completed_at_unix_ms=int(time.time()*1000))
                write_result(TARGET,report);print(json.dumps({'passed':True,'cases':len(report['cases']),'all_requested_features_complete':False}))
                return report
            finally:
                for alias in list(processes):stop(alias)
    except BaseException as error:
        report.update(state='FAILED',error=str(error),error_type=type(error).__name__,completed_at_unix_ms=int(time.time()*1000));write_result(TARGET,report);raise
    finally:
        for server in servers:server.shutdown();server.server_close();server.upstream.shutdown();server.upstream.server_close()

if __name__=='__main__':run()
