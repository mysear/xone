"""Exercise signed finality admission and revocation on the ordinary Go owner."""
import argparse
import json
import secrets
import subprocess
import time
from pathlib import Path
from urllib.parse import urlsplit

from beacon_owner_archive import LIMITS, audit_archive
from current_v2_evidence import ENDPOINTS, GENESIS, POOL, config_for, free_port, http, pairs, provider_id, stop


RETRYABLE_REFRESH_ERRORS = frozenset({'PROVENANCE_BEACON_REFRESH_NETWORK',
                                     'PROVENANCE_BEACON_UPDATE_PARTICIPATION'})


def wait_for_next_finality(binary, config, path, endpoint, child, first_number, on_retry=None):
    """Wait for a verified advance; rejected refreshes cannot replace the proof."""
    deadline = time.monotonic() + 900
    while True:
        if child.poll() is not None:
            raise ValueError('OWNER_NEXT_FINALITY_CHILD_EXITED')
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise ValueError('OWNER_NEXT_FINALITY_TIMEOUT')
        with path.open('rb') as stream:
            before = stream.read((16 << 20) + 1)
        if len(before) > 16 << 20:
            raise ValueError('OWNER_REFRESH_PROOF_SIZE')
        try:
            reply = subprocess.run([binary, '--config', config, '--refresh-beacon-proof', endpoint],
                                   check=False, capture_output=True, timeout=min(70, remaining))
        except subprocess.TimeoutExpired:
            if time.monotonic() >= deadline:
                raise ValueError('OWNER_NEXT_FINALITY_TIMEOUT')
            raise
        if child.poll() is not None:
            raise ValueError('OWNER_NEXT_FINALITY_CHILD_EXITED')
        if time.monotonic() >= deadline:
            raise ValueError('OWNER_NEXT_FINALITY_TIMEOUT')
        if reply.returncode != 0:
            try:
                failure = json.loads(reply.stderr, object_pairs_hook=pairs)
                code = failure['fields']['error_code'] if failure['event'] == 'service_start_failed' else None
            except (ValueError, KeyError, TypeError):
                code = None
            if not isinstance(code, str) or code not in RETRYABLE_REFRESH_ERRORS:
                raise subprocess.CalledProcessError(reply.returncode, reply.args,
                                                    output=reply.stdout, stderr=reply.stderr)
            with path.open('rb') as stream:
                unchanged = stream.read(len(before) + 1) == before
            if not unchanged:
                raise ValueError('OWNER_REFRESH_REJECTED_PROOF_CHANGED')
            if on_retry:
                on_retry(code)
        else:
            with path.open('rb') as stream:
                raw = stream.read((16 << 20) + 1)
            if len(raw) > 16 << 20:
                raise ValueError('OWNER_REFRESH_PROOF_SIZE')
            try:
                following = json.loads(raw, object_pairs_hook=pairs)
                binding = json.loads(reply.stdout, object_pairs_hook=pairs)
                execution = following['finality']['data']['finalized_header']['execution']
                number = binding['execution_number']
                matches = (type(number) is int and isinstance(execution['block_number'], str)
                           and str(number) == execution['block_number']
                           and isinstance(binding['execution_hash'], str)
                           and binding['execution_hash'] == execution['block_hash'] and number >= first_number)
            except (ValueError, KeyError, TypeError):
                raise ValueError('OWNER_REFRESH_EXECUTION_BINDING') from None
            if not matches:
                raise ValueError('OWNER_REFRESH_EXECUTION_BINDING')
            if number > first_number:
                return following
        time.sleep(min(12, max(0, deadline - time.monotonic())))


def execution_endpoints(config_path):
    if config_path is None:
        return ENDPOINTS
    with config_path.open('rb') as stream:
        raw = stream.read((1 << 20) + 1)
    if len(raw) > 1 << 20:
        raise ValueError('EXECUTION_CONFIG_SIZE')
    chains = json.loads(raw, object_pairs_hook=pairs)['chains']
    mainnet = [chain for chain in chains if chain['key'] == 'evm:1']
    if len(mainnet) != 1 or mainnet[0].get('genesis_hash') != GENESIS:
        raise ValueError('EXECUTION_CONFIG_CHAIN')
    urls = mainnet[0].get('rpc_urls')
    if not isinstance(urls, list) or len(urls) != 2:
        raise ValueError('EXECUTION_CONFIG_WITNESSES')
    proof = mainnet[0].get('proof_rpc_url', '')
    if proof != '':
        urls = [*urls, proof]
    for url in urls:
        if not isinstance(url, str) or not url or any(c.isspace() for c in url):
            raise ValueError('EXECUTION_CONFIG_URL')
        parsed = urlsplit(url)
        if (parsed.scheme != 'https' or not parsed.hostname or parsed.username is not None
                or parsed.password is not None or parsed.fragment):
            raise ValueError('EXECUTION_CONFIG_URL')
    if len(set(urls[:2])) != 2:
        raise ValueError('EXECUTION_CONFIG_WITNESSES')
    return tuple(urls)


def verify_advancement(owner, token, child, start, execution, max_tail, on_commit=None):
    cursor = start
    target = int(execution['block_number'])
    gap = target - start['block']['number_or_slot']
    if not 1 <= gap <= max_tail:
        raise ValueError('OWNER_ADVANCE_OUTSIDE_SHORT_TAIL')
    records = []
    deadline = time.monotonic() + 600
    while child.poll() is None and time.monotonic() < deadline:
        batch = http(owner + '/internal/chains/evm:1/journal?after=' + str(cursor['journal_seq']), token)
        if batch['status'] != 200:
            raise ValueError('OWNER_JOURNAL_UNAVAILABLE')
        for record in batch['body']:
            ref = record['reference']
            block = ref['block']
            if (record['kind'] != 'BLOCK_COMMIT' or block['number_or_slot'] != cursor['block']['number_or_slot'] + 1
                    or block['chain_key'] != 'evm:1' or block['parent_hash'] != cursor['block']['hash']
                    or ref['journal_seq'] != cursor['journal_seq'] + 1
                    or any(ref[k] != start[k] for k in ('online_generation', 'canonical_epoch', 'writer_epoch', 'scope_hash'))):
                raise ValueError('OWNER_JOURNAL_DISCONTINUITY')
            records.append({'reference': ref, 'record_id': record['record_id'], 'kind': record['kind']})
            if on_commit:
                on_commit(records[-1])
            cursor = ref
        if len(records) == gap:
            baseline = http(owner + '/internal/chains/evm:1/baseline', token)
            if (cursor['block']['hash'] != execution['block_hash'] or baseline['status'] != 200
                    or baseline['body']['reference'] != cursor):
                raise ValueError('OWNER_ADVANCE_FINAL_BASELINE')
            return records
        if len(records) > gap:
            raise ValueError('OWNER_ADVANCE_OVERSHOOT')
        time.sleep(0.5)
    raise ValueError('OWNER_ADVANCE_TIMEOUT')


def exercise(binary, output, bootstrap, updates, finality, root, next_proof=None, refresh_endpoint=None, execution_config=None):
    endpoints = execution_endpoints(execution_config)
    output.mkdir(mode=0o700, exist_ok=False, parents=True)
    proof = {'bootstrap': json.loads(bootstrap.read_bytes()), 'finality': json.loads(finality.read_bytes())}
    if updates:
        proof['updates'] = json.loads(updates.read_bytes())
    execution = proof['finality']['data']['finalized_header']['execution']
    path = output / 'proof.json'
    def install(data):
        tmp = path.with_suffix('.tmp')
        tmp.write_text(json.dumps(data))
        tmp.replace(path)
    install(proof)
    token, port = secrets.token_urlsafe(32), free_port()
    owner = 'http://127.0.0.1:' + str(port)
    cfg = config_for('chain-parser-go', output/'owner', token, port)
    cfg['chains'] = [dict(key='evm:1', owner_service='chain-parser-go', assignment_version=1,
                          genesis_hash=GENESIS, rpc_urls=list(endpoints[:2]), pools=[POOL],
                          confirmation_tag='finalized', auto_reorg_window=128,
                          block_integrity='known-evm-native-commitments-v2', receipts_mode='bulk-auto',
                          native_proof=dict(max_bytes=64<<20, max_headers=64, max_transactions=4096,
                                            max_lookahead=32, max_window_blocks=16),
                          beacon_finality=dict(checkpoint_root=root, proof_file=str(path),
                                               max_checkpoint_age_ms=7*86400000))]
    if len(endpoints) == 3:
        cfg['chains'][0]['proof_rpc_url'] = endpoints[2]
    advancing = bool(next_proof or refresh_endpoint)
    if advancing:
        cfg['journal']['raw_evidence'] = LIMITS
        cfg['chains'][0]['rpc_retry'] = dict(max_attempts=3, base_backoff_ms=150, max_backoff_ms=1500)
    cfg['history']['enabled'] = False
    cfg['startup']['max_baseline_age_ms'] = 3600000
    cfg['journal'].update(transport='http-development', source_urls=[owner], poll_interval_ms=2000,
                          request_timeout_ms=60000, max_payload_bytes=16<<20)
    config = output/'owner-config.json'
    config.touch(mode=0o600, exist_ok=False)
    config.write_text(json.dumps(cfg))
    progress = {'passed': False, 'commits': [], 'provider_ids': [provider_id(url) for url in endpoints]}
    def checkpoint():
        tmp = output / 'report.tmp'
        tmp.write_text(json.dumps(progress, indent=2))
        tmp.replace(output / 'report.json')
    def on_commit(record):
        progress['commits'].append(record)
        checkpoint()
    def on_refresh_retry(code):
        progress.setdefault('refresh_retry_codes', []).append(code)
        checkpoint()
    with (output/'owner.log').open('w') as stream:
        child = subprocess.Popen([binary, '--config', config], stdout=stream, stderr=subprocess.STDOUT,
                                 start_new_session=True)
        def baseline(status):
            until = time.monotonic()+150
            while time.monotonic()<until and child.poll() is None:
                res = http(owner+'/internal/chains/evm:1/baseline', token)
                if res['status']==status:
                    return res['body']
                time.sleep(.2)
            raise ValueError('OWNER_BASELINE_STATUS_'+str(status))
        try:
            first = baseline(200)['reference']
            if first['block']['hash'] != execution['block_hash'] or first['block']['number_or_slot'] != int(execution['block_number']):
                raise ValueError('OWNER_FINALIZED_BLOCK')
            if advancing:
                progress['first'] = first
                checkpoint()
                if next_proof:
                    following = json.loads(next_proof.read_bytes())
                    install(following)
                else:
                    following = wait_for_next_finality(binary, config, path, refresh_endpoint, child,
                                                       first['block']['number_or_slot'], on_refresh_retry)
                target = following['finality']['data']['finalized_header']['execution']
                progress['target'] = {k: target[k] for k in ('block_hash', 'block_number')}
                checkpoint()
                verify_advancement(owner, token, child, first, target, cfg['startup']['max_tail_blocks'], on_commit)
                progress['journal_verified'] = True
            else:
                install({})
                baseline(503)
                install(proof)
                recovered = baseline(200)['reference']
                if recovered['block']!=first['block'] or recovered['journal_seq']<=first['journal_seq']:
                    raise ValueError('OWNER_RECOVERY')
                return {'passed':True, 'first':first,'revoked_status':503,'recovered':recovered}
        except (ValueError, OSError, subprocess.SubprocessError) as exc:
            progress['failure'] = str(exc)
            checkpoint()
            raise
        finally:
            stop(child)
            if advancing:
                blocks = [progress['first']['block']] if 'first' in progress else []
                blocks.extend(record['reference']['block'] for record in progress['commits'])
                archive_dir = output / 'owner/state/raw-evidence/evm-1'
                progress['archive'] = audit_archive(archive_dir, blocks, endpoints)
                checkpoint()
    if progress['archive']['coverage_gaps']:
        raise ValueError('OWNER_ARCHIVE_COVERAGE')
    progress['passed'] = True
    checkpoint()
    return progress


if __name__=='__main__':
    parser=argparse.ArgumentParser(description=__doc__)
    for key in ('binary','output','bootstrap','finality'):
        parser.add_argument(key,type=Path)
    parser.add_argument('root')
    parser.add_argument('--updates',type=Path)
    parser.add_argument('--execution-config', type=Path,
                        help='read the two body witnesses and optional proof RPC URL from the evm:1 chain of an owner JSON config')
    advance = parser.add_mutually_exclusive_group()
    advance.add_argument('--next-proof', type=Path, help='exercise continuous publication to a later signed head')
    advance.add_argument('--refresh-endpoint', help='refresh the installed proof until the finalized head advances')
    args=parser.parse_args()
    result=exercise(args.binary.resolve(), args.output.resolve(),args.bootstrap,args.updates,args.finality,args.root,args.next_proof,args.refresh_endpoint,args.execution_config)
    (args.output/'report.json').write_text(json.dumps(result,indent=2))
