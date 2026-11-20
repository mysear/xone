"""Exercise OP owner startup, withdrawal and recovery with archived mainnet reads."""
import argparse
from contextlib import ExitStack
import json
from pathlib import Path
import secrets
import subprocess
import tempfile
import time

from current_v2_evidence import canonical, free_port, http, provider_id, require, stop
from replay_beacon_window import ArchiveRPC, read_window


class OPArchiveRPC(ArchiveRPC):
    projected = 0

    def response(self, method, params):
        # The original baseline capture includes full bodies from both witnesses.
        # Reanchor additionally requests the hash-only form from each same witness.
        if (method == 'eth_getBlockByNumber' and len(params) == 2 and params[1] is False
                and (method, canonical(params)) not in self.index):
            body = json.loads(super().response(method, [params[0], True]))
            block = body['result']
            block['transactions'] = [tx['hash'] for tx in block['transactions']]
            with self.lock:
                self.projected += 1
            return json.dumps(body).encode()
        return super().response(method, params)


def wait_baseline(proc, url, token, status):
    deadline = time.monotonic() + 30
    while time.monotonic() < deadline and proc.poll() is None:
        result = http(url + '/baseline', token)
        if result['status'] == status:
            return result['body']
        time.sleep(.25)
    raise ValueError(f'OP_OWNER_BASELINE_{status}_TIMEOUT')


def replay(config_file, archive, binary, baseline):
    original = json.loads(config_file.read_bytes())
    chain, = original['chains']
    require(chain['key'] == 'evm:10', 'OP_OWNER_REQUIRED')
    policy = chain['op_l1_origin']
    beacon = (config_file.parent / policy['proof_file']).resolve()
    observation = (beacon.parent / policy['batch_proof_file']).resolve().read_bytes()
    endpoints = [*chain['rpc_urls'], chain['proof_rpc_url'], policy['l1_rpc_url']]
    sources, _ = read_window(archive, endpoints, 'evm:10', OPArchiveRPC)
    expected = json.loads(baseline.read_bytes())
    with ExitStack() as stack:
        local = {}
        for source, server in sources.items():
            local[source] = server.start()
            stack.callback(server.close)
        chain['rpc_urls'] = [local[provider_id(url)] for url in chain['rpc_urls']]
        chain['proof_rpc_url'] = local[provider_id(chain['proof_rpc_url'])]
        policy['l1_rpc_url'] = local[provider_id(policy['l1_rpc_url'])]
        policy['proof_file'] = str(beacon)
        root = Path(stack.enter_context(tempfile.TemporaryDirectory()))
        active = root / 'channel.json'
        policy['batch_proof_file'] = str(active)
        addr = f'127.0.0.1:{free_port()}'
        base = 'http://' + addr
        url = base + '/internal/chains/evm:10'
        token = secrets.token_urlsafe(32)
        original['api'].update(listen=addr, auth_token=token)
        original['journal']['source_urls'] = [base]
        original['state']['path'] = str(root / 'state')
        cfg = root / 'owner.json'
        cfg.touch(mode=0o600)
        cfg.write_text(json.dumps(original))
        log = root / 'owner.log'
        stream = stack.enter_context(log.open('w'))
        command = [binary, '--config', cfg]
        empty = b'{"submissions":[],"blocks":[]}'
        active.write_bytes(empty)
        rejected = subprocess.run(command, stdout=stream, stderr=stream, timeout=60)
        require(rejected.returncode != 0 and 'OP_SPAN_BLOCKS_REQUIRED' in log.read_text(),
                'OP_EMPTY_SPAN_STARTUP')
        active.write_bytes(observation)
        proc = subprocess.Popen(command, stdout=stream, stderr=stream, start_new_session=True)
        stack.callback(stop, proc)
        before = wait_baseline(proc, url, token, 200)
        require(before['reference']['block'] == expected['reference']['block']
                and before['pools'] == expected['pools'], 'OP_BASELINE_MISMATCH')
        temp = root / 'next.json'
        temp.write_bytes(empty)
        temp.replace(active)
        wait_baseline(proc, url, token, 503)
        require(http(url + '/bootstrap', token)['status'] == 503, 'OP_REVOKED_BOOTSTRAP')
        seq = before['reference']['journal_seq']
        journal_url = url + f'/journal?after={seq}'
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            rows = http(journal_url, token)
            if rows['status'] == 200 and rows['body']:
                break
            time.sleep(.1)
        require(rows['status'] == 200 and [r['kind'] for r in rows['body']] == ['ONLINE_SCOPE_REVOKED']
                and rows['body'][0]['reference'] == dict(before['reference'], journal_seq=seq+1),
                'OP_REVOCATION_JOURNAL')
        with log.open() as events:
            failures = [event['fields']['error_code'] for line in events
                        if (event := json.loads(line)).get('event') == 'chain_sync_failed']
        require('OP_SPAN_BLOCKS_REQUIRED' in failures, 'OP_RUNNING_OWNER_REJECTION_REASON')
        temp.write_bytes(observation)
        temp.replace(active)
        after = wait_baseline(proc, url, token, 200)
        ref = before['reference']
        require(after['pools'] == before['pools'] and after['reference'] == dict(ref,
                online_generation=ref['online_generation'] + 1, writer_epoch=ref['writer_epoch'] + 1,
                journal_seq=seq + 2), 'OP_RECOVERED_BASELINE_MISMATCH')
        rows = http(journal_url, token)
        require(rows['status'] == 200 and [r['kind'] for r in rows['body']] == [
            'ONLINE_SCOPE_REVOKED', 'ONLINE_BASELINE_COMMIT'] and rows['body'][1]['reference'] == after['reference'],
            'OP_RECOVERY_JOURNAL')
        require(all(server.served and not server.trace for server in sources.values()), 'OP_REPLAY_COVERAGE')
        return {'block': ref['block'], 'pools': len(after['pools']), 'empty_span_rejected': True,
                'rpc_reads': sum(server.served for server in sources.values()),
                'hash_only_reads_projected_from_archived_bodies': sum(server.projected for server in sources.values()),
                'canonical_epoch': ref['canonical_epoch'], 'recovered_generation': after['reference']['online_generation'],
                'journal': [dict(kind=r['kind'], seq=r['reference']['journal_seq']) for r in rows['body']]}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for arg in ('config', 'archive', 'binary', 'baseline'):
        parser.add_argument('--' + arg, required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(replay(args.config, args.archive, args.binary.resolve(), args.baseline), indent=2))
