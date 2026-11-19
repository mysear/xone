"""Exercise a beacon root release change over the same durable owner store."""
import argparse
import copy
import json
from pathlib import Path
import secrets
import subprocess
import time
import urllib.request

from current_v2_evidence import free_port, http, require, stop
from verify_beacon_owner import execution_endpoints


def store_config(path, config):
    path.touch(mode=0o600, exist_ok=False)
    path.write_text(json.dumps(config))


def read_baseline(binary, path, url, token, log, after=None):
    with log.open('x') as stream:
        child = subprocess.Popen([binary, '--config', path], stdout=stream, stderr=subprocess.STDOUT,
                                 start_new_session=True)
        try:
            deadline = time.monotonic() + 150
            while time.monotonic() < deadline and child.poll() is None:
                response = http(url + '/internal/chains/evm:1/baseline', token)
                if response['status'] == 200:
                    snapshot = response['body']
                    if after is None:
                        return snapshot
                    records = http(url + '/internal/chains/evm:1/journal?after=' + str(after), token)
                    require(records['status'] == 200, 'ROTATION_JOURNAL_UNAVAILABLE')
                    require(any(r['kind'] == 'ONLINE_BASELINE_COMMIT' and r['reference'] == snapshot['reference']
                                for r in records['body']), 'ROTATION_BASELINE_COMMIT')
                    return snapshot
                time.sleep(.2)
            raise ValueError('ROTATION_BASELINE_UNAVAILABLE')
        finally:
            stop(child)


def verify_switch(first, second, binding):
    before, after = first['reference'], second['reference']
    require(before['block']['number_or_slot'] == binding['execution_number'] and
            before['block']['hash'] == binding['execution_hash'], 'ROTATION_SIGNED_BLOCK')
    require(after['online_generation'] == before['online_generation'] + 1 and
            after['writer_epoch'] == before['writer_epoch'] + 1 and after['journal_seq'] > before['journal_seq'],
            'ROTATION_DURABLE_CURSOR')
    require(after == dict(before, **{k: after[k] for k in ('online_generation', 'writer_epoch', 'journal_seq')})
            and first['pools'] == second['pools'] and bool(first['pools']), 'ROTATION_STATE_CHANGED')


def exercise(binary, template, endpoint, output):
    execution_endpoints(template)
    cfg = json.loads(template.read_bytes())
    require(len(cfg['chains']) == 1 and cfg['journal']['transport'] == 'http-development'
            and cfg['control']['backend'] == 'disabled' and not cfg['history']['enabled'], 'ROTATION_LOCAL_PROFILE')
    output.mkdir(parents=True, exist_ok=False, mode=0o700)
    port, token = free_port(), secrets.token_urlsafe(32)
    url = 'http://127.0.0.1:' + str(port)
    cfg['api'].update(listen=f'127.0.0.1:{port}', auth_token=token)
    cfg['journal']['source_urls'] = [url]
    cfg['state']['path'] = str(output/'state')
    policy = cfg['chains'][0]['beacon_finality']
    policy['proof_file'] = str(output/'old-proof.json')
    old, new = output/'old-config.json', output/'next-config.json'
    store_config(old, cfg)
    def cli(*args):
        return json.loads(subprocess.run([binary, '--config', old, *args], capture_output=True,
                                         check=True, timeout=80).stdout)
    binding = cli('--refresh-beacon-proof', endpoint)
    successor = copy.deepcopy(cfg)
    root = binding['finalized_root']
    successor['chains'][0]['beacon_finality'].update(checkpoint_root=root, proof_file=str(output/'next-proof.json'))
    store_config(new, successor)
    bootstrap = output/'next-bootstrap.json'
    with urllib.request.urlopen(endpoint.rstrip('/') + '/eth/v1/beacon/light_client/bootstrap/' + root, timeout=20) as response:
        data = response.read((128 << 10) + 1)
    require(len(data) <= 128 << 10, 'ROTATION_BOOTSTRAP_SIZE')
    bootstrap.write_bytes(data)
    rotated = cli('--prepare-beacon-rotation', new, '--beacon-bootstrap', bootstrap)
    require(rotated == binding, 'ROTATION_FINALITY_CHANGED')
    first = read_baseline(binary, old, url, token, output/'old.log')
    second = read_baseline(binary, new, url, token, output/'next.log', first['reference']['journal_seq'])
    verify_switch(first, second, binding)
    return dict(passed=True, predecessor=first['reference'], successor=second['reference'],
                pools=len(first['pools']), finality=binding)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for arg in ('binary', 'template', 'output'):
        parser.add_argument(arg, type=Path)
    parser.add_argument('beacon_endpoint')
    args = parser.parse_args()
    report = exercise(args.binary.resolve(), args.template.resolve(), args.beacon_endpoint, args.output.resolve())
    (args.output/'report.json').write_text(json.dumps(report, indent=2))
