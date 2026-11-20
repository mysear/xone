"""Replay an OP owner's captured baseline and verify rejection of a missing span."""
import argparse
from contextlib import ExitStack
import json
from pathlib import Path
import secrets
import subprocess
import tempfile
import time

from current_v2_evidence import free_port, http, provider_id, require, stop
from replay_beacon_window import read_window


def replay(config_file, archive, binary, baseline):
    original = json.loads(config_file.read_bytes())
    chain, = original['chains']
    require(chain['key'] == 'evm:10', 'OP_OWNER_REQUIRED')
    policy = chain['op_l1_origin']
    beacon = (config_file.parent / policy['proof_file']).resolve()
    observation = (beacon.parent / policy['batch_proof_file']).resolve()
    endpoints = [*chain['rpc_urls'], chain['proof_rpc_url'], policy['l1_rpc_url']]
    sources, _ = read_window(archive, endpoints, 'evm:10')
    with ExitStack() as stack:
        local = {}
        for source, server in sources.items():
            local[source] = server.start()
            stack.callback(server.close)
        chain['rpc_urls'] = [local[provider_id(url)] for url in chain['rpc_urls']]
        chain['proof_rpc_url'] = local[provider_id(chain['proof_rpc_url'])]
        policy['l1_rpc_url'] = local[provider_id(policy['l1_rpc_url'])]
        policy['proof_file'] = str(beacon)
        expected = json.loads(baseline.read_bytes())
        for fault in (False, True):
            with tempfile.TemporaryDirectory() as directory, ExitStack() as run:
                root = Path(directory)
                missing = root / 'empty.json'
                missing.write_text('{"submissions":[],"blocks":[]}')
                policy['batch_proof_file'] = str(missing if fault else observation)
                addr = f'127.0.0.1:{free_port()}'
                base = 'http://' + addr
                token = secrets.token_urlsafe(32)
                original['api'].update(listen=addr, auth_token=token)
                original['journal']['source_urls'] = [base]
                original['state']['path'] = str(root / 'state')
                cfg = root / 'owner.json'
                cfg.touch(mode=0o600)
                cfg.write_text(json.dumps(original))
                log = root / 'owner.log'
                stream = run.enter_context(log.open('w'))
                proc = subprocess.Popen([binary, '--config', cfg], stdout=stream,
                                        stderr=stream, start_new_session=True)
                run.callback(stop, proc)
                deadline = time.monotonic() + 60
                while proc.poll() is None and time.monotonic() < deadline:
                    result = http(base + '/internal/chains/evm:10/baseline', token)
                    if result['status'] == 200:
                        require(not fault, 'EMPTY_SPAN_PUBLISHED')
                        actual = result['body']
                        require(actual['reference']['block'] == expected['reference']['block']
                                and actual['pools'] == expected['pools'], 'OP_BASELINE_MISMATCH')
                        break
                    time.sleep(.15)
                else:
                    require(fault and proc.poll() not in (None, 0)
                            and 'OP_SPAN_BLOCKS_REQUIRED' in log.read_text(), 'OP_OWNER_REPLAY_FAILED')
        require(all(server.served and not server.trace for server in sources.values()),
                'OP_REPLAY_COVERAGE')
        return {'block': expected['reference']['block'], 'pools': len(expected['pools']),
                'rpc_reads': sum(server.served for server in sources.values()),
                'empty_span_rejected': True}


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for arg in ('config', 'archive', 'binary', 'baseline'):
        parser.add_argument('--' + arg, required=True, type=Path)
    args = parser.parse_args()
    print(json.dumps(replay(args.config, args.archive, args.binary.resolve(), args.baseline), indent=2))
