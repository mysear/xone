"""Rebuild an observed owner window from sealed historical RPC responses on loopback."""
import argparse
from contextlib import ExitStack
import json
from pathlib import Path
import subprocess
import tempfile

from beacon_owner_archive import LIMITS
from current_v2_evidence import (METHODS, RecordedRPC, canonical, pairs, provider_id, require, sha,
                                 rpc_observation, sealed_payload)
from verify_beacon_owner import execution_endpoints


BATCH_METHOD = 'jsonrpc_batch'
REPLAY_METHODS = METHODS | {'eth_getRawTransactionByHash', BATCH_METHOD}
FILE_LIMIT = LIMITS['max_record_bytes'] * 2 + (256 << 10)


def valid_call_batch(request):
    return isinstance(request, list) and 1 <= len(request) <= 16 and all(
        isinstance(call, dict) and set(call) == {'jsonrpc', 'id', 'method', 'params'}
        and call['jsonrpc'] == '2.0' and type(call['id']) is int and call['id'] == i
        and call['method'] == 'eth_call' and isinstance(call['params'], list) and len(call['params']) == 2
        for i, call in enumerate(request, 1))


class ArchiveRPC(RecordedRPC):
    """Index successful reads by provider, method and params, retaining bodies on disk."""
    def __init__(self, source, expected_chain='evm:1'):
        super().__init__([])
        self.source = source
        self.expected_chain = expected_chain
        self.index = {}
        self.errors = 0
        self.served = 0
        self.served_batches = 0

    def add(self, path, method, params, body):
        try:
            reply = json.loads(body, object_pairs_hook=pairs)
        except (UnicodeDecodeError, json.JSONDecodeError):
            reply = None
        if method == BATCH_METHOD:
            require(valid_call_batch(params), 'REPLAY_INVALID_CALL_BATCH')
            if (not isinstance(reply, list) or len(reply) != len(params)
                    or any(not isinstance(r, dict) or r.get('jsonrpc') != '2.0' or type(r.get('id')) is not int
                           or 'error' in r or not isinstance(r.get('result'), str) for r in reply)
                    or sorted(r['id'] for r in reply) != list(range(1, len(params)+1))):
                self.errors += 1
                return
            # IDs associate results even when a server returns them out of order.
            reply.sort(key=lambda r: r['id'])
        elif (not isinstance(reply, dict) or reply.get('jsonrpc') != '2.0' or type(reply.get('id')) is not int
                or reply['id'] != 1 or 'error' in reply or reply.get('result') is None):
            self.errors += 1
            return
        key = (method, canonical(params))
        digest = sha(canonical(reply).encode())
        if key in self.index:
            require(self.index[key][1] == digest, 'AMBIGUOUS_CAPTURE_RESULT')
        else:
            self.index[key] = path, digest, sha(body)

    def dispatch(self, request):
        if isinstance(request, list):
            require(valid_call_batch(request), 'REPLAY_INVALID_CALL_BATCH')
            return self.response(BATCH_METHOD, request)
        require(not isinstance(request, dict) or request.get('method') != BATCH_METHOD, 'REPLAY_INVALID_CALL_BATCH')
        return super().dispatch(request)

    def response(self, method, params):
        key = (method, canonical(params))
        require(key in self.index, 'UNRECORDED_RPC_REQUEST')
        path, _, raw_digest = self.index[key]
        owner, data, identity = sealed_payload(path, FILE_LIMIT, self.expected_chain)
        require(owner['ProviderID'] == self.source, 'REPLAY_UNKNOWN_PROVIDER')
        recorded, arguments, body = rpc_observation(owner, data, identity, REPLAY_METHODS)
        require((recorded, canonical(arguments)) == key, 'REPLAY_REQUEST_CHANGED')
        require(sha(body) == raw_digest, 'REPLAY_RESPONSE_CHANGED')
        with self.lock:
            self.served += 1
            self.served_batches += method == BATCH_METHOD
        return body


def read_window(directory, endpoints, expected_chain='evm:1', rpc_type=ArchiveRPC):
    sources = {provider_id(url): rpc_type(provider_id(url), expected_chain) for url in endpoints}
    staged = {}
    total = 0
    for count, path in enumerate(directory.glob('*.raw'), start=1):
        total += path.lstat().st_size
        require(count <= LIMITS['max_records'] and total <= LIMITS['max_bytes'], 'REPLAY_INPUT_BUDGET')
        owner, data, identity = sealed_payload(path, FILE_LIMIT, expected_chain)
        source = owner['ProviderID']
        if source == 'derived-state-from-recorded-rpc':
            block = json.loads(data, object_pairs_hook=pairs)['block']
            require(block['chain_key'] == owner['Chain'] and block['number_or_slot'] == owner['Height']
                    and block['hash'] == owner['Hash'], 'REPLAY_STAGED_IDENTITY')
            staged[(owner['Height'], owner['Hash'])] = identity['data_sha256']
        else:
            require(source in sources, 'REPLAY_UNKNOWN_PROVIDER')
            method, params, body = rpc_observation(owner, data, identity, REPLAY_METHODS)
            sources[source].add(path, method, params, body)
    require(all(rpc.index for rpc in sources.values()), 'REPLAY_SOURCE_COVERAGE')
    return sources, staged


def check_rows(rows, report, staged):
    refs = [report['first']] + [r['reference'] for r in report['commits']]
    require(len(rows) == len(refs) and bool(rows), 'REPLAY_DECODE_COUNT')
    for i, (row, ref) in enumerate(zip(rows, refs)):
        block = row['block']
        require(block == ref['block'], 'REPLAY_REFERENCE')
        if i:
            ancestor = refs[i - 1]
            require(report['commits'][i-1]['kind'] == 'BLOCK_COMMIT'
                    and block['parent_hash'] == ancestor['block']['hash']
                    and block['number_or_slot'] == ancestor['block']['number_or_slot'] + 1
                    and ref == dict(ancestor, journal_seq=ancestor['journal_seq'] + 1, block=block),
                    'REPLAY_JOURNAL')
            require(row['staged_sha256'] == staged.get((block['number_or_slot'], block['hash'])),
                    'REPLAY_STATE_OR_FACTS_CHANGED')


def replay(binary, directory, trusted_root, retained_signed=False):
    config_path = directory / 'owner-config.json'
    endpoints = execution_endpoints(config_path, retained_signed=retained_signed)
    cfg = json.loads(config_path.read_bytes(), object_pairs_hook=pairs)
    require(len(cfg['chains']) == 1, 'REPLAY_SINGLE_CHAIN')
    report = json.loads((directory / 'report.json').read_bytes(), object_pairs_hook=pairs)
    first = report['first']['block']['number_or_slot']
    require(0 < len(report['commits']) <= 128, 'REPLAY_COMMIT_COUNT')
    sources, staged = read_window(directory / 'owner/state/raw-evidence/evm-1', endpoints)
    with ExitStack() as stack:
        local = []
        for url in dict.fromkeys(endpoints):
            rpc = sources[provider_id(url)]
            local.append(rpc)
            stack.callback(rpc.close)
        chain = cfg['chains'][0]
        loopback = {rpc.source: rpc.start() for rpc in local}
        chain['rpc_urls'] = [loopback[provider_id(url)] for url in endpoints[:2]]
        if len(endpoints) == 3:
            chain['proof_rpc_url'] = loopback[provider_id(endpoints[2])]
        chain['beacon_finality'].update(checkpoint_root=trusted_root, proof_file=str((directory / 'proof.json').resolve()))
        with tempfile.TemporaryDirectory() as tmp:
            config = Path(tmp) / 'replay.json'
            config.write_text(json.dumps(cfg))
            result = subprocess.run([binary, '--config', config, '--first', str(first),
                                     '--last', str(first + len(report['commits']))],
                                    capture_output=True, timeout=625)
        if result.returncode:
            raise ValueError('REPLAY_DECODER_FAILED ' + result.stderr.decode(errors='replace')[:300])
        counts = []
        for rpc in local:
            require(rpc.served and not rpc.trace, 'REPLAY_MISSING_READ')
            counts.append(rpc.served)
    decoded = json.loads(result.stdout)
    require(decoded['signed_height'] == int(report['target']['block_number'])
            and decoded['signed_hash'] == report['target']['block_hash'], 'REPLAY_SIGNED_TARGET')
    rows = decoded['rows']
    check_rows(rows, report, staged)
    result = dict(scope='historical-published-prefix', signed_target=report['target'], verified_blocks=rows,
                  matched_staged_records=len(rows)-1, reads_by_source=dict(zip(sources, counts)),
                  excluded_error_responses={key: rpc.errors for key, rpc in sources.items()})
    if retained_signed:
        result.update(source_mode='RETAINED_RECENT_SIGNED_RPC_BYTES', production_ready=False,
                      consensus_verified=False)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('binary', type=Path, help='built replay-beacon-window executable')
    parser.add_argument('directory', type=Path, help='stopped owner acceptance directory')
    parser.add_argument('trusted_root', help='externally selected mainnet beacon checkpoint root')
    parser.add_argument('--retained-signed', action='store_true',
                        help='require the recorded owner config to contain only path-free loopback recent signed sources')
    args = parser.parse_args()
    print(json.dumps(replay(args.binary.resolve(), args.directory.resolve(), args.trusted_root,
                            retained_signed=args.retained_signed), indent=2))


if __name__ == '__main__':main()
