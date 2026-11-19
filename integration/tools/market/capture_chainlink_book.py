#!/usr/bin/env python3
"""Collect one complete unsigned signing request from approved Ethereum policies."""
import argparse
import json
from pathlib import Path
import sys
import time

from capture_chainlink import GENESIS, collect, digest, file_digest, load, require, write


def capture_book(config, rpc_url, validator, output, revision, block_hash=None, *, allow_loopback=False, expected_anchor=None):
    require(type(revision) is int and 0 < revision < 2**64, 'MARKET_PUBLICATION_REVISION')
    if expected_anchor is not None:
        require(block_hash == expected_anchor['block_hash'], 'MARKET_BOOK_BANK_ANCHOR')
    with Path(config).open('rb') as stream:
        raw = stream.read((1 << 20) + 1)
    require(len(raw) <= 1 << 20, 'MARKET_CONFIG_BYTES')
    policies = load(raw)['policies']
    require(isinstance(policies, list) and 1 <= len(policies) <= 128, 'MARKET_BOOK_POLICIES')
    seen = set()
    for p in policies:
        require(p['chain_key'] == 'evm:1' and p['genesis_hash'] == GENESIS and p.get('market_feed'),
                'MARKET_ONLY_APPROVED_ETHEREUM')
        require(p['token'] not in seen, 'MARKET_BOOK_DUPLICATE_TOKEN')
        seen.add(p['token'])

    out = Path(output)
    out.mkdir(parents=True, exist_ok=False)
    anchor, requests, expires, last_read = None, [], None, 0
    validator_hash = file_digest(validator)
    for i, policy in enumerate(policies):
        source, capture = out / f'policy-{i}.json', out / f'capture-{i}'
        write(source, policy)
        report = collect(source, rpc_url, validator, capture, block_hash, allow_loopback,
                         publication_revision=revision)
        unsigned = load((capture / 'unsigned.json').read_bytes())
        o = unsigned['observation']
        require(report['passed'] and report['validator_sha256'] == validator_hash and
                o['publication_revision'] == revision and o['token'] == policy['token'] and
                unsigned['authorized'] is False, 'MARKET_BOOK_VALIDATION')
        if anchor is None:
            anchor = report['anchor']
            block_hash = anchor['block_hash']
        require(expected_anchor is None or anchor == expected_anchor, 'MARKET_BOOK_BANK_ANCHOR')
        require(o['anchor'] == report['anchor'] == anchor, 'MARKET_BOOK_MIXED_BLOCK')
        last_read = max(last_read, o['market_feed']['source_read_at_ms'])
        # max_age is inclusive; valid_until is exclusive in Router admission.
        end = min(o['valid_until_ms'], o['observed_at_ms'] + policy['max_age_ms'] + 1)
        expires = end if expires is None else min(expires, end)
        requests.append(dict(observation=o, signing_message_hex=unsigned['signing_message_hex'],
                             capture_report=f'capture-{i}/report.json',
                             capture_report_sha256=file_digest(capture / 'report.json')))

    result = dict(schema_version=1, authorized=False, publication_revision=revision, anchor=anchor,
                  config_sha256=digest(raw), validator_sha256=validator_hash,
                  valid_until_ms=expires, signing_requests=requests)
    # Readers watch only the final name. A crash or any failed member leaves no complete request.
    pending = out / 'signing-request.tmp'
    write(pending, result)
    require(last_read <= time.time_ns() // 1_000_000 < expires, 'MARKET_BOOK_NOT_CURRENT')
    pending.replace(out / 'signing-request.json')
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['config', 'rpc-url', 'validator', 'output']:
        parser.add_argument('--' + name, required=True)
    parser.add_argument('--publication-revision', type=int, required=True)
    parser.add_argument('--block-hash')
    args = parser.parse_args()
    result = capture_book(args.config, args.rpc_url, args.validator, args.output,
                          args.publication_revision, args.block_hash)
    print(json.dumps(dict(publication_revision=result['publication_revision'],
                          observations=len(result['signing_requests']), authorized=False)))


if __name__ == '__main__':
    try:
        main()
    except Exception as error:
        print(json.dumps(dict(error=str(error) if isinstance(error, ValueError)
                              else type(error).__name__)), file=sys.stderr)
        sys.exit(1)
