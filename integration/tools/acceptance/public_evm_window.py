#!/usr/bin/env python3
"""Capture a public EVM owner's contiguous journal and replay its numbered blocks."""
import argparse
import copy
from collections import defaultdict
from contextlib import ExitStack
from datetime import datetime
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import time
import urllib.error

from current_v2_evidence import canonical, pairs, provider_id, require, rpc_observation, sealed_payload, sha
from public_rpc_smoke import ROOT, check_snapshot, owner_process, probe_owner_chain
from replay_beacon_window import ArchiveRPC, FILE_LIMIT, REPLAY_METHODS, read_window


class FinalizedObservations:
    """Index the owner's sealed moving-tag observations by the full returned block reference."""

    def __init__(self, directory, chain, bounds):
        self.directory = directory
        self.chain = chain['key']
        self.providers = {provider_id(url) for url in chain['rpc_urls']}
        self.bounds = bounds
        self.lookahead = chain['native_proof']['max_lookahead']
        self.files = set()
        self.bytes = 0
        self.finalized = defaultdict(list)
        self.numbered = {}

    def scan(self):
        for path in self.directory.glob('*.raw'):
            if path in self.files:
                continue
            self.files.add(path)
            self.bytes += path.lstat().st_size
            require(len(self.files) <= self.bounds['max_records'] and self.bytes <= self.bounds['max_bytes'],
                    'PUBLIC_WINDOW_ARCHIVE_BUDGET')
            owner, payload, identity = sealed_payload(path, FILE_LIMIT, self.chain, with_capture_time=True)
            if owner['ProviderID'] == 'derived-state-from-recorded-rpc':
                continue
            require(owner['ProviderID'] in self.providers, 'PUBLIC_WINDOW_ARCHIVE_PROVIDER')
            method, params, body = rpc_observation(owner, payload, identity, REPLAY_METHODS)
            if (method != 'eth_getBlockByNumber' or not isinstance(params, list) or len(params) != 2
                    or params[1] is not False or not isinstance(params[0], str)
                    or (params[0] != 'finalized' and not params[0].startswith('0x'))):
                continue
            try:
                reply = json.loads(body, object_pairs_hook=pairs)
            except (UnicodeDecodeError, json.JSONDecodeError):
                continue
            if (not isinstance(reply, dict) or reply.get('jsonrpc') != '2.0' or type(reply.get('id')) is not int
                    or reply['id'] != 1 or 'error' in reply or not isinstance(reply.get('result'), dict)):
                continue
            result = reply['result']
            block = dict(chain_key=self.chain, number_or_slot=int(result['number'], 16), hash=result['hash'],
                         parent_hash=result['parentHash'], timestamp_ms=int(result['timestamp'], 16) * 1000)
            observation = dict(block=block, provider_id=owner['ProviderID'], record_sha256=identity['sha256'],
                               response_sha256=sha(body), captured_at=identity['captured_at'])
            height = block['number_or_slot']
            if params[0] == 'finalized':
                settled = height
                if self.chain == 'evm:43114' and 'settledHeight' in result:
                    settled = int(result['settledHeight'], 16)
                require(0 <= height - settled <= self.lookahead, 'PUBLIC_WINDOW_FINALIZED_SETTLEMENT_BOUND')
                self.finalized[settled].append(observation)
            elif params[0] == hex(height):
                self.numbered.setdefault((owner['ProviderID'], height, block['hash']), observation)

    def find(self, snapshot):
        target = snapshot['reference']['block']
        heads = self.finalized.get(target['number_or_slot'], [])
        for head in sorted(heads, key=lambda row: (datetime.fromisoformat(row['captured_at']), row['provider_id'],
                                                 row['record_sha256'])):
            path = [head]
            block = head['block']
            while block['number_or_slot'] > target['number_or_slot']:
                ancestor = self.numbered.get((head['provider_id'], block['number_or_slot'] - 1, block['parent_hash']))
                if ancestor is None:
                    break
                path.append(ancestor)
                block = ancestor['block']
            if block == target:
                return path
        return None


class NumberedArchiveRPC(ArchiveRPC):
    def add(self, path, method, params, body):
        if (method == 'eth_getBlockByNumber' and isinstance(params, list) and params
                and params[0] in ('latest', 'safe', 'finalized', 'pending')):
            return
        super().add(path, method, params, body)


def config_for(target):
    config = json.loads((ROOT / target['owner'] / 'config/config-dev.yaml').read_bytes())
    base, = config['chains']
    chain = probe_owner_chain(base, target)
    require(len(set(chain['rpc_urls'])) == 2 and all(url.startswith('https://') for url in chain['rpc_urls'])
            and chain['confirmation_tag'] == 'finalized' and not chain.get('proof_rpc_url'), 'PUBLIC_WINDOW_CONFIG')
    config['chains'] = [chain]
    # Sample short finalized tails even on chains whose blocks arrive sub-second.
    config['journal']['poll_interval_ms'] = 2000
    config['journal']['raw_evidence'] = dict(max_bytes=512 << 20, max_records=8192,
                                             max_record_bytes=4 * config['journal']['max_payload_bytes'] + 16384)
    return config, chain


def verify_journal(first, last, records, minimum):
    start = first['reference']
    require(start['journal_seq'] == 1 and len(records) > minimum and records[0]['kind'] == 'ONLINE_BASELINE_COMMIT'
            and records[0]['reference'] == start and records[0]['pools'] == first['pools'], 'PUBLIC_WINDOW_BASELINE')
    previous = start
    for index, record in enumerate(records):
        ref = record['reference']
        block = ref['block']
        if index:
            prior = previous['block']
            require(record['kind'] == 'BLOCK_COMMIT' and block['chain_key'] == prior['chain_key']
                    and block['number_or_slot'] == prior['number_or_slot'] + 1 and block['parent_hash'] == prior['hash']
                    and ref == dict(previous, block=block, journal_seq=previous['journal_seq'] + 1),
                    'PUBLIC_WINDOW_JOURNAL_DISCONTINUITY')
        record_id = f"{block['chain_key']}/{ref['online_generation']}/{ref['journal_seq']}"
        require(record['record_id'] == record_id and re.fullmatch('[0-9a-f]{64}', record['payload_hash']),
                'PUBLIC_WINDOW_RECORD_IDENTITY')
        previous = ref
    require(last == dict(first, reference=previous, pools=records[-1]['pools']), 'PUBLIC_WINDOW_FINAL_SNAPSHOT')


def read_journal(read, path, last):
    records = []
    end = last['reference']['journal_seq']
    while len(records) < end:
        batch = read(f'{path}/journal?after={len(records)}')
        require(isinstance(batch, list) and batch, 'PUBLIC_WINDOW_JOURNAL_UNAVAILABLE')
        records.extend(batch[:end - len(records)])
    return records


def capture(binary, config, target, directory, minimum, timeout):
    chain, = config['chains']
    witness = dict(chain, rpc_urls=[chain['rpc_urls'][1]])
    path = f"/internal/chains/{chain['key']}"
    first = None
    with owner_process(binary, config, directory) as (process, read, log):
        archive = Path(config['state']['path']) / 'raw-evidence' / chain['key'].replace(':', '-')
        heads = FinalizedObservations(archive, chain, config['journal']['raw_evidence'])
        deadline = time.monotonic() + 150
        while process.poll() is None and time.monotonic() < deadline:
            try:
                current = read(f'{path}/baseline')
            except (urllib.error.URLError, TimeoutError, ConnectionError):
                time.sleep(0.6)
                continue
            if first is None:
                first = current
                require(first['reference']['journal_seq'] == 1, 'PUBLIC_WINDOW_INITIAL_OWNER_BASELINE')
                heads.scan()
                require(heads.find(first) is not None, 'PUBLIC_WINDOW_INITIAL_FINALIZED_OBSERVATION')
                deadline = time.monotonic() + timeout
                first_check = check_snapshot(target, witness, first, config['startup']['max_baseline_age_ms'])
            distance = current['reference']['journal_seq'] - first['reference']['journal_seq']
            require(0 <= distance <= config['startup']['max_tail_blocks'], 'PUBLIC_WINDOW_TAIL_BOUND')
            require(current['reference'] == dict(first['reference'], block=current['reference']['block'],
                                                 journal_seq=current['reference']['journal_seq']),
                    'PUBLIC_WINDOW_JOURNAL_DISCONTINUITY')
            if distance >= minimum:
                heads.scan()
                if heads.find(current) is not None:
                    records = read_journal(read, path, current)
                    verify_journal(first, current, records, minimum)
                    return first, current, records, first_check
            time.sleep(0.6)
        size = log.seek(0, 2)
        log.seek(max(0, size - 2048))
        phase = 'startup' if first is None else 'continuous journal'
        raise RuntimeError(f"public EVM {phase} incomplete (exit {process.poll()}) {log.read().decode(errors='replace')}")


def verify_replay(decoded, first, last, records, staged):
    require(decoded['initial_snapshot'] == first and decoded['final_snapshot'] == last, 'PUBLIC_WINDOW_REPLAY_SNAPSHOT')
    require(decoded['initial_record'] == records[0], 'PUBLIC_WINDOW_REPLAY_BASELINE_RECORD')
    rows = decoded['rows']
    require(len(rows) == len(records) - 1, 'PUBLIC_WINDOW_REPLAY_COUNT')
    for row, record in zip(rows, records[1:]):
        block = record['reference']['block']
        for field in ('reference', 'kind', 'record_id', 'payload_hash'):
            require(row[field] == record[field], 'PUBLIC_WINDOW_REPLAY_JOURNAL')
        require(row['block'] == block and row['staged_sha256'] == staged.get((block['number_or_slot'], block['hash']))
                and re.fullmatch('[0-9a-f]{64}', row['staged_sha256'])
                and type(row['swap_count']) is int and row['swap_count'] >= 0, 'PUBLIC_WINDOW_REPLAY_STAGED_FACTS')
    return rows


def replay(binary, directory, config, chain, first, last, records):
    archive = Path(config['state']['path']) / 'raw-evidence' / chain['key'].replace(':', '-')
    heads = FinalizedObservations(archive, chain, config['journal']['raw_evidence'])
    heads.scan()
    begin, end = heads.find(first), heads.find(last)
    require(begin and end and datetime.fromisoformat(begin[0]['captured_at']) < datetime.fromisoformat(end[0]['captured_at']),
            'PUBLIC_WINDOW_FINALIZED_ORDER')
    sources, staged = read_window(archive, chain['rpc_urls'], chain['key'], rpc_type=NumberedArchiveRPC)
    loopback = copy.deepcopy(config)
    loopback['state']['path'] = str(directory / 'reconstruction')
    start = first['reference']['block']['number_or_slot']
    finish = last['reference']['block']['number_or_slot']
    with ExitStack() as servers:
        local = []
        for url in chain['rpc_urls']:
            source = sources[provider_id(url)]
            servers.callback(source.close)
            local.append(source.start())
        loopback['chains'][0]['rpc_urls'] = local
        path = directory / 'offline-config.yaml'
        path.touch(mode=0o600, exist_ok=False)
        path.write_text(json.dumps(loopback))
        result = subprocess.run([str(binary), '--config', str(path), '--provider-finalized',
                                 '--first', str(start), '--last', str(finish)], capture_output=True, timeout=625)
        misses = [entry for source in sources.values() for entry in source.trace]
        if result.returncode:
            raise RuntimeError(f"public EVM replay failed {result.stderr.decode(errors='replace')[:400]} {misses[:3]}")
        require(not misses and all(source.served for source in sources.values()), 'PUBLIC_WINDOW_REPLAY_COVERAGE')
        decoded = json.loads(result.stdout, object_pairs_hook=pairs)
    rows = verify_replay(decoded, first, last, records, staged)
    observed_window = dict(initial_snapshot=first, records=records, final_snapshot=last)
    return dict(chain_key=chain['key'], scope='public-provider-finalized-journal',
                first=first['reference'], last=last['reference'], commits=rows,
                window_sha256=sha(canonical(observed_window).encode()), finalized=dict(first=begin, last=end),
                archive=dict(records=len(heads.files), bytes=heads.bytes),
                providers=[dict(rpc=url, provider_id=source.source, requests=source.served,
                                replayed_batches=source.served_batches) for url, source in zip(chain['rpc_urls'], sources.values())])


def main(argv=None):
    targets = json.loads((ROOT / 'integration/config/chain-targets.json').read_bytes())['targets']
    eligible = [name for name, target in targets.items() if target['family'] == 'evm' and 'public_pool_probe' in target]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--chain', choices=eligible, default='bsc')
    parser.add_argument('--commits', type=int, default=3, help='minimum consecutive commits after the baseline')
    parser.add_argument('--window-timeout', type=int, default=120, help='seconds to observe the owner after startup')
    parser.add_argument('--go-binary', type=Path, default=ROOT / 'chain-parser-go/bin/chain-parser-go')
    parser.add_argument('--replay-binary', type=Path, default=ROOT / 'chain-parser-go/bin/replay-beacon-window')
    args = parser.parse_args(argv)
    target = targets[args.chain]
    try:
        config, chain = config_for(target)
        if not (2 <= args.commits <= config['startup']['max_tail_blocks'] and 1 <= args.window_timeout <= 600):
            parser.error('--commits must fit a multi-block tail and --window-timeout must be 1 to 600 seconds')
        with tempfile.TemporaryDirectory(prefix='xone-public-evm-window-') as tmp:
            directory = Path(tmp)
            first, last, records, first_check = capture(args.go_binary.resolve(), config, target, directory,
                                                        args.commits, args.window_timeout)
            witness = dict(chain, rpc_urls=[chain['rpc_urls'][1]])
            last_check = check_snapshot(target, witness, last, config['startup']['max_baseline_age_ms'])
            result = replay(args.replay_binary.resolve(), directory, config, chain, first, last, records)
            result['pool_checks'] = [first_check, last_check]
            print(json.dumps(result), flush=True)
    except (KeyError, TypeError, OSError, RuntimeError, ValueError, subprocess.TimeoutExpired) as error:
        print(f'{args.chain} public EVM window FAILED {error}', file=sys.stderr, flush=True)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
