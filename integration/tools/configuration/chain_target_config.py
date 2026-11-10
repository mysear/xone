#!/usr/bin/env python3
"""Render a service config from design targets, after a bounded identity probe.

Public endpoints are development defaults. Provider observations establish neither
consensus trust, historical coverage, independence nor protocol deployment admission.
Infrastructure/authentication and already admitted same-chain pools stay in the base.
"""
from pathlib import Path
import argparse
import copy
import hashlib
import json
import os
import re
import secrets
import sys
import subprocess
import time
import urllib.error
import urllib.parse
import urllib.request

ROOT = Path(__file__).resolve().parents[3]
CATALOG = ROOT / 'integration/config/chain-targets.json'
MAX_CONFIG_BYTES = 4 * 1024 * 1024
MAX_RPC_BYTES = 256 * 1024
PROBE_TIMEOUT_SECONDS = 5

def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError('DUPLICATE_JSON_FIELD')
        result[key] = value
    return result

def load_targets():
    doc = json.loads(CATALOG.read_bytes(), object_pairs_hook=unique_object)
    if doc['schema_version'] != 1 or doc['design_clause'] != '3':
        raise ValueError('TARGET_CATALOG_VERSION')
    return doc['targets']

def choose_rpc_urls(base, target, explicit=None):
    if explicit is not None:
        if not explicit:
            raise ValueError('TARGET_RPC_LIST_EMPTY')
        return list(explicit)
    existing = next((ch for ch in base.get('chains', []) if ch.get('key') == target['chain_key']), None)
    return list(existing['rpc_urls']) if existing and existing.get('rpc_urls') else list(target['rpc_urls'])

def go_runtime_defaults(config, chain, genesis, base_directory):
    # This profile selects each known chain's native header/body/receipt rules,
    # including successor commitments for asynchronous execution windows.
    keys = {'evm:1', 'evm:10', 'evm:8453', 'evm:56', 'evm:137',
            'evm:42161', 'evm:43114'}
    if chain['key'] in keys:
        chain.setdefault('block_integrity', 'known-evm-native-commitments-v2')
    if chain.get('block_integrity') == 'known-evm-native-commitments-v2':
        chain.setdefault('native_proof', {'max_bytes':16777216, 'max_headers':64,
            'max_window_blocks':16, 'max_transactions':4096, 'max_lookahead':32})
    chain.setdefault('receipts_mode', 'bulk-auto')
    # The pinned public genesis is a reproducible historical probe boundary.
    # It grants no claim about arbitrary historical heights or endpoint domains.
    chain.setdefault('provider_probe', {
        'historical_height':0, 'historical_hash':genesis,
        'account':'0x000000000000000000000000000000000000dead',
        'timeout_ms':5000, 'interval_ms':30000, 'max_backoff_ms':60000,
        'max_age_ms':120000, 'max_providers':min(64, max(1, len(chain['rpc_urls'])))})
    journal = config['journal']
    raw = journal['raw_evidence']
    journal.setdefault('raw_maintenance', {
        'sink_path':str(Path(config['state']['path']).parent / 'raw-archive'),
        'sink_max_bytes_per_chain':max(536870912, raw['max_bytes']),
        'sink_max_records_per_chain':max(2, min(1000000, raw['max_records'])),
        'interval_ms':30000, 'timeout_ms':5000, 'replay_timeout_ms':5000,
        'keep_ms':86400000, 'target_bytes':raw['max_bytes']//2,
        'max_scan':64, 'max_archive':8, 'max_reclaims':8,
        'max_raw_bytes':min(raw['max_record_bytes'], 32*1024*1024),
        'max_decoded_bytes':min(raw['max_record_bytes'], 32*1024*1024),
        'max_page_records':100, 'max_request_bytes':8192,
        'max_pins_per_capture':8192, 'decoder_version':'staged-state/public-adapter-v1'})
    sink = journal['raw_maintenance'].get('sink_path')
    if sink and not Path(sink).is_absolute():
        journal['raw_maintenance']['sink_path'] = str((Path(base_directory) / sink).resolve())
    api = config.setdefault('api', {})
    generated = not api.get('auth_token')
    if generated:
        # This credential belongs to the local management API. Public RPCs do
        # not need a third-party key. The output is written with mode 0600.
        api['auth_token'] = secrets.token_urlsafe(32)
    return generated

def render(base, target, call, base_directory, rpc_urls=None):
    owner = target['owner']
    if not target.get('genesis_hash'):
        raise ValueError('TARGET_GENESIS_PIN_REQUIRED')
    if base.get('service', {}).get('name') != owner:
        raise ValueError('TARGET_WRONG_SERVICE_OWNER')
    startup, history = base.get('startup', {}), base.get('history', {})
    if (startup.get('mode') != 'HEAD_FIRST'
        or startup.get('resume_previous_online_cursor') is not False
        or startup.get('history_catchup_blocks_online_readiness') is not False
        or history.get('namespace') != 'history'
        or history.get('online_write_permission') is not False
        or not 0 < history.get('max_worker_share', 0) <= .15):
        raise ValueError('TARGET_RECOVERY_POLICY_CONFLICT')
    previous = base.get('chains', [])
    if any(ch.get('key') != target['chain_key'] and ch.get('pools') for ch in previous):
        raise ValueError('TARGET_FOREIGN_POOL_CATALOG')
    pools = [ch['pools'] for ch in previous if ch.get('key') == target['chain_key']]
    if len(pools) > 1:
        raise ValueError('TARGET_DUPLICATE_CHAIN')
    observations = []
    def observe(method, params):
        value = call(method, params)
        observations.append({'method':method, 'params':params, 'result':value})
        return value
    if target['family'] == 'evm':
        if observe('eth_chainId', []) != hex(target['chain_id']):
            raise ValueError('TARGET_RPC_WRONG_CHAIN_ID')
        block = observe('eth_getBlockByNumber', ['0x0', False])
        if not isinstance(block, dict) or block.get('number') != '0x0':
            raise ValueError('TARGET_GENESIS_MISSING')
        genesis = block.get('hash', '')
        if not isinstance(genesis, str) or not re.fullmatch(r'0x[0-9a-fA-F]{64}', genesis) or int(genesis, 16) == 0:
            raise ValueError('TARGET_GENESIS_INVALID')
        genesis = genesis.lower()
        if target.get('genesis_hash') and genesis != target['genesis_hash']:
            raise ValueError('TARGET_GENESIS_MISMATCH')
    else:
        genesis = observe('getGenesisHash', [])
        if genesis != target['genesis_hash']:
            raise ValueError('TARGET_GENESIS_MISMATCH')
    pinned = next((ch.get('genesis_hash') for ch in previous if ch.get('key') == target['chain_key']), None)
    if pinned and pinned != genesis:
        raise ValueError('TARGET_EXISTING_GENESIS_MISMATCH')
    config = copy.deepcopy(base)
    if not 0 < startup.get('max_baseline_age_ms', 0) or not 0 < startup.get('max_tail_blocks', 0):
        raise ValueError('TARGET_RECOVERY_BUDGET_MISSING')
    config['startup']['max_baseline_age_ms'] = min(startup['max_baseline_age_ms'], 120000)
    config['startup']['max_tail_blocks'] = min(startup['max_tail_blocks'], 128)
    chain = {'key':target['chain_key'], 'owner_service':owner, 'assignment_version':1,
             'genesis_hash':genesis, 'rpc_urls':choose_rpc_urls(base, target, rpc_urls),
             'pools':copy.deepcopy(pools[0]) if pools else [], 'confirmation_tag':'finalized',
             'auto_reorg_window':128}
    # Preserve versioned same-network settings, including monotone assignment,
    # source transports and explicitly admitted protocol configurations.
    existing = next((ch for ch in previous if ch.get('key') == target['chain_key']), None)
    if existing:
        chain.update(copy.deepcopy(existing))
        chain['rpc_urls'] = choose_rpc_urls(base, target, rpc_urls)
        chain['genesis_hash'] = genesis
        chain['confirmation_tag'] = 'finalized'
    config['chains'] = [chain]
    slug = target['chain_key'].replace(':', '-')
    genesis_namespace = genesis.removeprefix('0x')
    config['state']['path'] = str(ROOT / 'integration/.runtime/chain-targets' / slug / genesis_namespace / owner / 'state')
    for section, key in [('control','dsn_file'), ('simulation','state_file')]:
        value = config.get(section, {}).get(key)
        if value and not Path(value).is_absolute():
            config[section][key] = str((Path(base_directory) / value).resolve())
    kafka = config['journal']['kafka']
    topic = kafka.get('topics', {}).get(target['chain_key']) if existing else None
    kafka['topics'] = {target['chain_key']:topic or 'xone.canonical.' + slug + '.' + genesis_namespace}
    config['control']['instance_id'] = owner + '-' + slug
    kafka['replica_id'] = config['control']['instance_id']
    if owner == 'chain-parser-go' and 'raw_evidence' not in config['journal']:
        record_bytes = max(33570816, config['journal']['max_payload_bytes']*4+16384)
        if record_bytes > 128*1024*1024:
            raise ValueError('TARGET_RAW_BUDGET_INCOMPATIBLE')
        config['journal']['raw_evidence'] = {'max_bytes':536870912, 'max_records':1000000, 'max_record_bytes':record_bytes}
    management_token_generated = False
    if owner == 'chain-parser-go':
        management_token_generated = go_runtime_defaults(config, chain, genesis, base_directory)
    report = {'schema_version':1, 'design_clause':'3', 'chain_key':target['chain_key'],
              'management_token_generated':management_token_generated,
              'dynamic_catalog_default':('explicit-admitted-sources-preserved' if chain.get('dynamic_catalog')
                                         else 'qualified-deployment-required'),
              'authority':'bounded-public-provider-identity-observation',
              'production_ready':False, 'consensus_verified':False,
              'provider_failure_domain_verified':False, 'protocol_admission_granted':False,
              'observations':observations, 'public_defaults_are_development_only':True,
              'remaining_qualification':['deployed protocol/version evidence',
                  'chain-specific completeness and finality', 'provider historical/replay coverage',
                  'online service integration and recovery acceptance']}
    return config, report

def validate_rpc_url(url):
    parsed = urllib.parse.urlparse(url)
    if parsed.scheme != 'https' or not parsed.hostname or parsed.username or parsed.password or parsed.query or parsed.fragment or len(url) > 4096:
        raise ValueError('TARGET_RPC_REQUIRES_CREDENTIAL_FREE_HTTPS')

class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise ValueError('TARGET_RPC_HTTP_REDIRECT')

def request_identity(payload):
    # This child only performs the three fixed read-only identity probes.
    url, method, params = payload['url'], payload['method'], payload['params']
    validate_rpc_url(url)
    if not ((method in ['eth_chainId','getGenesisHash'] and params == []) or (method == 'eth_getBlockByNumber' and params == ['0x0', False])):
        raise ValueError('TARGET_PROBE_REQUEST_INVALID')
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    raw = json.dumps({'jsonrpc':'2.0', 'id':1, 'method':method, 'params':params}).encode()
    request = urllib.request.Request(url, data=raw, headers={'Content-Type':'application/json', 'User-Agent':'XONE-source-evidence/1.0'}, method='POST')
    try:
        with opener.open(request, timeout=PROBE_TIMEOUT_SECONDS) as response:
            if response.status != 200:
                raise ValueError('TARGET_RPC_HTTP_STATUS')
            body = response.read(MAX_RPC_BYTES+1)
    except urllib.error.HTTPError as error:
        raise ValueError('TARGET_RPC_HTTP_'+str(error.code)) from None
    except (OSError, urllib.error.URLError):
        raise ValueError('TARGET_RPC_UNAVAILABLE') from None
    if len(body) > MAX_RPC_BYTES:
        raise ValueError('TARGET_RPC_RESPONSE_LIMIT')
    result = json.loads(body, object_pairs_hook=unique_object)
    if result.get('jsonrpc') != '2.0' or type(result.get('id')) is not int or result['id'] != 1 or 'error' in result or 'result' not in result:
        raise ValueError('TARGET_RPC_INVALID_ENVELOPE')
    return result['result']

def run_worker(command, payload, timeout):
    try:
        completed = subprocess.run(command, input=payload, stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=timeout, check=False)
    except subprocess.TimeoutExpired:
        # subprocess.run kills and waits for this owned worker; no stalled
        # network thread or orphan reader survives into fallback.
        raise ValueError('TARGET_RPC_DEADLINE') from None
    if completed.returncode or len(completed.stdout) > MAX_RPC_BYTES*8:
        raise ValueError('TARGET_RPC_WORKER_FAILED')
    return completed.stdout

def remote_call(url):
    validate_rpc_url(url)
    def call(method, params):
        payload = json.dumps({'url':url, 'method':method, 'params':params}).encode()
        raw = run_worker([sys.executable, '-I', str(Path(__file__).resolve()), '--probe-worker'], payload, PROBE_TIMEOUT_SECONDS)
        outcome = json.loads(raw, object_pairs_hook=unique_object)
        if 'error' in outcome:
            raise ValueError(outcome['error'])
        return outcome['result']
    return call

def retained_call(path, target):
    if path.stat().st_size > MAX_CONFIG_BYTES:
        raise ValueError('TARGET_EVIDENCE_LIMIT')
    raw = path.read_bytes()
    if hashlib.sha256(raw).hexdigest() != target.get('genesis_evidence_sha256'):
        raise ValueError('TARGET_EVIDENCE_DIGEST')
    evidence = json.loads(raw, object_pairs_hook=unique_object)
    if evidence.get('schema_version') != 1:
        raise ValueError('TARGET_EVIDENCE_VERSION')
    expected = next((value for value in evidence.get('targets', {}).values() if value.get('chain_key') == target['chain_key']), None)
    if expected is None or expected.get('genesis_hash') != target['genesis_hash']:
        raise ValueError('TARGET_EVIDENCE_IDENTITY')
    name = next((name for name, value in evidence['targets'].items() if value['chain_key'] == target['chain_key']), None)
    values = {}
    for row in evidence.get('rows', []):
        if row.get('target') != name:
            continue
        for observation in row.get('observations', []):
            method = observation['method']
            result = observation['raw_response']['result'] if 'raw_response' in observation else observation['parsed_response_result']
            if method == 'eth_chainId' and result != hex(target['chain_id']):
                raise ValueError('TARGET_EVIDENCE_IDENTITY')
            if method == 'eth_getBlockByNumber' and (not isinstance(result, dict) or result.get('number') != '0x0' or result.get('hash') != target['genesis_hash']):
                raise ValueError('TARGET_EVIDENCE_IDENTITY')
            if method == 'getGenesisHash' and result != target['genesis_hash']:
                raise ValueError('TARGET_EVIDENCE_IDENTITY')
            values[method] = result
    def call(method, params):
        if method not in values:
            raise ValueError('TARGET_EVIDENCE_MISSING_METHOD')
        return values[method]
    return call

def write_private(path, value):
    raw = (json.dumps(value, ensure_ascii=False, indent=2)+'\n').encode()
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
    with os.fdopen(fd, 'wb') as output:
        output.write(raw); output.flush(); os.fsync(output.fileno())

def main():
    if sys.argv[1:] == ['--probe-worker']:
        raw = sys.stdin.buffer.read(8193)
        if len(raw) > 8192:
            print(json.dumps({'error':'TARGET_PROBE_REQUEST_INVALID'})); return
        try:
            result = request_identity(json.loads(raw, object_pairs_hook=unique_object))
            print(json.dumps({'result':result}))
        except (OSError, ValueError, KeyError, TypeError) as error:
            reason = str(error) if re.fullmatch(r'[A-Z][A-Z0-9_]{1,100}', str(error)) else 'TARGET_RPC_INVALID_RESPONSE'
            print(json.dumps({'error':reason}))
        return
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--list', action='store_true')
    parser.add_argument('--target')
    parser.add_argument('--base-config', type=Path)
    parser.add_argument('--output', type=Path)
    parser.add_argument('--rpc-url', action='append')
    parser.add_argument('--identity-evidence', type=Path, help='render from retained fixed-genesis identity observations without live network access')
    args = parser.parse_args()
    targets = load_targets()
    if args.list:
        print(json.dumps({'design_clause':'3', 'targets':targets, 'production_ready':False}, ensure_ascii=False, indent=2)); return
    if args.target not in targets or args.base_config is None or args.output is None:
        parser.error('--target, --base-config and --output are required; --list shows design targets')
    target = targets[args.target]
    if args.output.exists() or Path(str(args.output)+'.identity.json').exists():
        raise ValueError('TARGET_OUTPUT_EXISTS')
    if args.base_config.stat().st_size > MAX_CONFIG_BYTES:
        raise ValueError('TARGET_CONFIG_LIMIT')
    base = json.loads(args.base_config.read_bytes(), object_pairs_hook=unique_object)
    urls = choose_rpc_urls(base, target, args.rpc_url)
    if args.identity_evidence:
        config, report = render(base, target, retained_call(args.identity_evidence, target), args.base_config.resolve().parent, urls)
        report.update(authority='retained-public-provider-genesis-observation', live_probe_executed=False, unprobed_provider_count=len(urls),
                      evidence_sha256=hashlib.sha256(args.identity_evidence.read_bytes()).hexdigest())
    else:
        for url in urls:
            remote_call(url)  # Validate every final configured URL, including fallback entries.
        attempts = []
        selected = None
        for url in urls:
            try:
                config, report = render(base, target, remote_call(url), args.base_config.resolve().parent, urls)
                selected = url
                break
            except ValueError as error:
                reason = str(error)
                if not (reason in ['TARGET_RPC_UNAVAILABLE','TARGET_RPC_DEADLINE'] or reason.startswith('TARGET_RPC_HTTP_')):
                    raise
                attempts.append({'provider_id':hashlib.sha256(url.encode()).hexdigest(), 'failure':reason})
        if selected is None:
            print(json.dumps({'identity_probe_failed':True, 'attempts':attempts, 'production_ready':False}))
            raise ValueError('TARGET_ALL_PUBLIC_RPC_UNAVAILABLE')
        report['provider_id'] = hashlib.sha256(selected.encode()).hexdigest()
        report['failed_attempts'] = attempts
        report['live_probe_executed'] = True
        report['unprobed_provider_count'] = len(urls)-len(attempts)-1
        report['identity_probe_provider_count'] = 1
    report['config_sha256'] = hashlib.sha256((json.dumps(config, ensure_ascii=False, indent=2)+'\n').encode()).hexdigest()
    report['observed_at_unix_ms'] = int(time.time()*1000)
    write_private(args.output, config)
    write_private(Path(str(args.output)+'.identity.json'), report)
    print(json.dumps({'written':str(args.output.resolve()), 'chain_key':target['chain_key'],
                      'production_ready':False, 'next_check':'run the normal parser --config FILE --check-config'}))

if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, KeyError, TypeError) as error:
        reason = str(error) if re.fullmatch(r"[A-Z][A-Z0-9_]{1,100}", str(error)) else "INVALID_INPUT_OR_SOURCE"
        print("CHAIN_TARGET_CONFIG_FAILED: "+reason, file=sys.stderr)
        sys.exit(1)
