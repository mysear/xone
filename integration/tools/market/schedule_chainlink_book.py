#!/usr/bin/env python3
"""Advance bounded market capture, external signature handoff and local publication rounds."""
import argparse
import fcntl
import json
import os
from pathlib import Path
import re
import sys
import tempfile
import time

from assemble_chainlink_book import assemble, read
from capture_chainlink import GENESIS, digest, file_digest, load, require
from capture_chainlink_book import capture_book
from publish_chainlink_book import head, local_client, publish, reconcile, sync_dir


def checkpoint(root, state):
    with tempfile.NamedTemporaryFile(dir=root, delete=False) as tmp:
        path = Path(tmp.name)
        try:
            tmp.write(json.dumps(state).encode())
            tmp.flush()
            os.fsync(tmp.fileno())
            os.replace(path, root/'state.json')
            sync_dir(root)
        finally:
            path.unlink(missing_ok=True)


def bank_anchor(base, block_hash):
    anchor = base.get('bankAnchor')
    require(isinstance(anchor, dict) and base.get('bankConsensusAttested') is False and
            anchor.get('chain_key') == 'evm:1' and anchor.get('genesis_hash') == GENESIS and
            all(type(anchor.get(k)) is int and 0 < anchor[k] < 2**64 for k in ('timestamp_ms', 'block_number')) and
            all(isinstance(anchor.get(k), str) and re.fullmatch('0x[0-9a-f]{64}', anchor[k])
                for k in ('block_hash', 'parent_hash')), 'MARKET_SCHEDULE_BANK_ANCHOR')
    require(block_hash is None or block_hash == anchor['block_hash'], 'MARKET_SCHEDULE_BLOCK_CONFLICT')
    return anchor


def step(root, config, rpc_url, validator, publication_cli, router_origin, call, block_hash=None):
    root = Path(root)
    root.mkdir(mode=0o700, exist_ok=True)
    sync_dir(root.parent)
    fd = os.open(root, os.O_RDONLY | os.O_DIRECTORY)
    try:
        fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        binding = dict(config_path=str(Path(config).resolve()), config_hash=digest(read(config)),
                       rpc_hash=digest(rpc_url.encode()), router=router_origin, block_hash=block_hash,
                       validator_hash=file_digest(validator), publication_cli_hash=file_digest(publication_cli))
        if (root/'state.json').exists():
            state = load(read(root/'state.json'))
            require(state['schema_version'] == 1 and state['binding'] == binding, 'MARKET_SCHEDULE_BINDING')
        else:
            require(not any(root.iterdir()), 'MARKET_SCHEDULE_MISSING_STATE')
            state = dict(schema_version=1, binding=binding, revision=0, phase='NEW')
        phase = state['phase']
        if phase in ('NEW', 'CONFIRMED', 'EXPIRED', 'CAPTURING', 'SUPERSEDED'):
            base = head(call)
            anchor = bank_anchor(base, block_hash)
            revision = max(state['revision'], base['publicationRevision']) + 1
            require(revision < 2**64, 'MARKET_SCHEDULE_REVISION_LIMIT')
            state = dict(schema_version=1, binding=binding, phase='CAPTURING', revision=revision,
                         base=base)
            checkpoint(root, state)  # Never reuse a revision after an interrupted capture.
            directory = root / f'round-{revision}'
            directory.mkdir()
            sync_dir(root)
            capture_book(config, rpc_url, validator, directory/'capture', revision, anchor['block_hash'], expected_anchor=anchor)
            request = directory/'capture/signing-request.json'
            state.update(phase='WAITING', request_hash=digest(read(request, 2 << 20)))
        elif phase == 'WAITING':
            directory = root / f"round-{state['revision']}"
            request = directory/'capture/signing-request.json'
            raw = read(request, 2 << 20)
            require(digest(raw) == state['request_hash'], 'MARKET_SCHEDULE_REQUEST_CHANGED')
            bundle = load(raw)
            if not state['base'].get('bankAnchor') or bundle['anchor'] != state['base']['bankAnchor']:
                # A pre-anchor WAITING checkpoint cannot be upgraded using fresh authority.
                state['phase'] = 'SUPERSEDED'
            elif time.time_ns() // 1_000_000 >= bundle['valid_until_ms']:
                state['phase'] = 'EXPIRED'
            elif not (directory/'receipts.json').exists():
                return state
            elif (current := head(call))['journalEntryHash'] != state['base']['journalEntryHash'] or bank_anchor(current, block_hash) != bundle['anchor']:
                state['phase'] = 'SUPERSEDED'
            else:
                # An assembly interrupted before PUBLISHING has no authority to install anything.
                with tempfile.TemporaryDirectory(dir=directory) as scratch:
                    candidate = Path(scratch)/'book.json'
                    checked = assemble(config, request, directory/'receipts.json', validator, publication_cli,
                                       candidate, online_candidate=True)
                    require(checked['revision'] == state['revision'], 'MARKET_SCHEDULE_REVISION')
                    state.update(phase='PUBLISHING', intent=dict(base=state['base'], candidateRevision=checked['revision'],
                                 candidateBookHash=checked['bookHash']))
                    checkpoint(root, state)  # From this point an interrupted round is read-only on resume.
                    result = publish(config, candidate, checked['bookHash'], publication_cli, directory/'recovery', call,
                                     expected_base=state['base']['journalEntryHash'])
                    state.update(phase='CONFIRMED', confirmed=result['head'])
        elif phase == 'PUBLISHING':
            state.update(phase='CONFIRMED', confirmed=reconcile(state['intent'], call))
        else:
            raise ValueError('MARKET_SCHEDULE_INVALID_PHASE')
        checkpoint(root, state)
        return state
    finally:
        os.close(fd)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ('root', 'config', 'rpc-url', 'validator', 'publication-cli', 'router-origin'):
        parser.add_argument('--' + name, required=True)
    parser.add_argument('--block-hash')
    parser.add_argument('--steps', type=int, default=1)
    parser.add_argument('--interval-seconds', type=int, default=5)
    args = parser.parse_args()
    try:
        require(1 <= args.steps <= 100 and 1 <= args.interval_seconds <= 3600, 'MARKET_SCHEDULE_LIMIT')
        call = local_client(args.router_origin, os.environ.get('XONE_ROUTER_TOKEN'))
        for i in range(args.steps):
            if i:
                time.sleep(args.interval_seconds)
            state = step(args.root, args.config, args.rpc_url, args.validator, args.publication_cli,
                         args.router_origin, call, args.block_hash)
            print(json.dumps(dict(phase=state['phase'], revision=state['revision'])), flush=True)
    except Exception as error:
        print(json.dumps(dict(error=str(error) if isinstance(error, ValueError) else type(error).__name__)), file=sys.stderr)
        sys.exit(1)
