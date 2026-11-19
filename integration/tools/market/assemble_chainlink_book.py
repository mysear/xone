#!/usr/bin/env python3
"""Verify detached signer responses and emit a new, offline candidate book."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import time

from capture_chainlink import digest, file_digest, load, require


def read(path, limit=1 << 20):
    with Path(path).open('rb') as stream:
        raw = stream.read(limit + 1)
    require(len(raw) <= limit, 'MARKET_ASSEMBLY_INPUT_LIMIT')
    return raw


def run(args, value):
    r = subprocess.run(args, input=json.dumps(value, separators=(',', ':')).encode(),
                       capture_output=True, timeout=15)
    if r.returncode:
        raise ValueError(load(r.stderr)['error'])
    return load(r.stdout)


def assemble(config, request, receipts, validator, publication_cli, output, *, online_candidate=False):
    raw_config, raw_request = read(config), read(request, 2 << 20)
    policy, bundle, responses = load(raw_config), load(raw_request), load(read(receipts))
    revision, anchor = bundle['publication_revision'], bundle['anchor']
    require(type(revision) is int and 0 < revision < 2**64 and bundle['schema_version'] == 1
            and bundle['authorized'] is False and bundle['config_sha256'] == digest(raw_config)
            and bundle['validator_sha256'] == file_digest(validator), 'MARKET_ASSEMBLY_REQUEST')
    require(responses['schema_version'] == 1 and responses['request_sha256'] == digest(raw_request),
            'MARKET_ASSEMBLY_RECEIPT_REQUEST')
    policies, rows = policy['policies'], bundle['signing_requests']
    require(1 <= len(policies) == len(rows) <= 128, 'MARKET_ASSEMBLY_COVERAGE')
    signatures = {p['token']: {} for p in policies}
    require(len(signatures) == len(rows), 'MARKET_ASSEMBLY_COVERAGE')
    for receipt in responses['signatures']:
        token, key = receipt['token'], receipt['key_id']
        require(token in signatures and key not in signatures[token], 'MARKET_ASSEMBLY_SIGNER')
        signatures[token][key] = dict(key_id=key, signature=receipt['signature'])
    prices, latest_read, expires = [], 0, None
    for i, (p, row) in enumerate(zip(policies, rows)):
        source = Path(request).parent / f'capture-{i}'
        report_bytes = read(source / 'report.json')
        report = load(report_bytes)
        candidate_bytes = read(source / 'candidate.json')
        require(row['capture_report'] == f'capture-{i}/report.json'
                and row['capture_report_sha256'] == digest(report_bytes) and report['passed'] is True
                and report['artifacts']['candidate.json'] == digest(candidate_bytes), 'MARKET_ASSEMBLY_CAPTURE')
        candidate = load(candidate_bytes)
        require(candidate['policy'] == p and candidate['anchor'] == anchor
                and candidate['publication_revision'] == revision, 'MARKET_ASSEMBLY_MEMBER')
        unsigned = run([validator], candidate)
        o = row['observation']
        require(unsigned['observation'] == o and unsigned['signing_message_hex'] == row['signing_message_hex']
                and o['token'] == p['token'], 'MARKET_ASSEMBLY_MESSAGE')
        latest_read = max(latest_read, o['market_feed']['source_read_at_ms'])
        end = min(o['valid_until_ms'], o['observed_at_ms'] + p['max_age_ms'] + 1)
        expires = end if expires is None else min(expires, end)
        sigs = signatures[p['token']]
        prices.append(dict(observation=o, signatures=[sigs[k] for k in sorted(sigs)]))

    def check_time():
        now = time.time_ns() // 1_000_000
        require(bundle['valid_until_ms'] == expires and latest_read <= now < expires, 'MARKET_ASSEMBLY_TIME')
        return now

    base = Path(config).parent
    journal = base / policy['publication_journal']
    destination = Path(output)
    require(destination.resolve() not in {journal.resolve(), (base / policy['observations_file']).resolve()},
            'MARKET_ASSEMBLY_ACTIVE_PATH')
    book = dict(schema_version=1, prices=prices)
    command = [publication_cli, 'verify'] if online_candidate else [publication_cli, 'preview', journal]
    preview = run(command, dict(policy=policy, anchor=anchor, book=book,
                                                             execution_timestamp_ms=check_time()))
    require(preview['revision'] == revision and preview['published'] is False and
            (preview.get('journalChecked') is False if online_candidate else preview.get('wouldAdvance') is True),
            'MARKET_ASSEMBLY_PREVIEW')
    with tempfile.NamedTemporaryFile(dir=destination.parent) as tmp:
        tmp.write(json.dumps(book, separators=(',', ':')).encode())
        tmp.flush()
        os.fsync(tmp.fileno())
        check_time()
        os.link(tmp.name, destination)  # Atomic visibility; never replace an existing output.
    return preview


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['config', 'request', 'receipts', 'validator', 'publication-cli', 'output']:
        parser.add_argument('--' + name, required=True, type=Path)
    parser.add_argument('--online-candidate', action='store_true', help='Verify signatures without opening the journal; Router must preview and apply')
    try:
        print(json.dumps(assemble(**vars(parser.parse_args()))))
    except Exception as error:
        print(json.dumps(dict(error=str(error) if isinstance(error, ValueError)
                              else type(error).__name__)), file=sys.stderr)
        sys.exit(1)
