#!/usr/bin/env python3
"""Install a verified book and conditionally publish once through a local Router."""
import argparse
import fcntl
import http.client
import json
import os
from pathlib import Path
import re
import stat
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

from assemble_chainlink_book import read, run
from capture_chainlink import load, NoRedirect, require


def local_client(origin, token):
    u = urllib.parse.urlsplit(origin)
    require(u.scheme in ('http', 'https') and u.hostname in ('127.0.0.1', '::1') and u.port
            and not (u.username or u.password or u.query or u.fragment) and u.path in ('', '/'),
            'MARKET_PUBLISH_LOOPBACK_ORIGIN')
    require(token and not any(c.isspace() for c in token), 'MARKET_PUBLISH_TOKEN')
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())

    def call(operation, **fields):
        req = urllib.request.Request(origin.rstrip('/') + '/internal/native-valuation/publication',
            data=json.dumps(dict(operation=operation, **fields)).encode(),
            headers={'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json'})
        try:
            with opener.open(req, timeout=10) as r:
                raw = r.read(65537)
                require(r.status == 200 and len(raw) <= 65536, 'MARKET_PUBLISH_BAD_RESPONSE')
                return load(raw)
        except urllib.error.HTTPError as e:
            with e:
                raise ValueError('MARKET_PUBLISH_HTTP_' + str(e.code)) from None
    return call


def sync_dir(path):
    fd = os.open(path, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def head(call):
    h = call('status')
    require(h['fsyncConfirmed'] is True and all(type(h[k]) is int and h[k] > 0 for k in
            ('publicationRevision', 'journalSequence')) and all(re.fullmatch('[a-f0-9]{64}', h[k]) for k in
            ('journalEntryHash', 'priceBookHash')), 'MARKET_PUBLISH_BAD_HEAD')
    return h


def reconcile(intent, call):
    actual = head(call)
    base = intent['base']
    require(('bankAnchor' not in base or actual.get('bankAnchor') == base['bankAnchor']) and
            actual['priceBookHash'] == intent['candidateBookHash'] and
            actual['publicationRevision'] == intent['candidateRevision'] and
            actual['journalSequence'] == base['journalSequence'] + 1 and
            actual['journalEntryHash'] != base['journalEntryHash'], 'MARKET_PUBLISH_OUTCOME_UNKNOWN')
    return actual


def publish(config, candidate, expected, cli, recovery_dir, call, *, expected_base=None):
    config, candidate, recovery_dir = map(Path, (config, candidate, recovery_dir))
    policy = load(read(config))
    active = config.parent / policy['observations_file']
    journal = config.parent / policy['publication_journal']
    require(len({p.resolve() for p in (config, candidate, active, journal)}) == 4,
            'MARKET_PUBLISH_PATH_COLLISION')
    candidate_raw = read(candidate)
    book = load(candidate_raw)
    tokens = [p['token'] for p in policy['policies']]
    observations = [p['observation'] for p in book['prices']]
    require(tokens and sorted(tokens) == sorted(o['token'] for o in observations), 'MARKET_PUBLISH_TOKEN_SET')

    def verify(value, at):
        return run([cli, 'verify'], dict(policy=policy, book=value, anchor=value['prices'][0]['observation']['anchor'],
                                        execution_timestamp_ms=at))

    def fresh():
        now = time.time_ns() // 1_000_000
        require(all((o.get('market_feed') or {}).get('source_read_at_ms', 0) <= now for o in observations),
                'MARKET_PUBLISH_FUTURE_READ')
        v = verify(book, now)
        require(v['bookHash'] == expected and v['journalChecked'] is False and v['published'] is False,
                'MARKET_PUBLISH_CANDIDATE_IDENTITY')
        return v['revision']

    revision = fresh()
    parent_fd = os.open(active.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        fcntl.flock(parent_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        base = head(call)
        require(all(o['anchor'] == base.get('bankAnchor') for o in observations), 'MARKET_PUBLISH_BANK_ANCHOR')
        if expected_base is not None:
            require(base['journalEntryHash'] == expected_base, 'MARKET_PUBLISH_BASE_CHANGED')
        meta = active.lstat()
        require(stat.S_ISREG(meta.st_mode) and meta.st_uid == os.geteuid(), 'MARKET_PUBLISH_FILE_OWNER')
        previous = read(active)
        installed = load(previous)
        # Compute the installed book's identity at its own bank time; only the candidate is admitted now.
        previous_hash = verify(installed, installed['prices'][0]['observation']['anchor']['timestamp_ms'])['bookHash']
        require(previous_hash == base['priceBookHash'], 'MARKET_PUBLISH_UNRESOLVED_INSTALL')
        if previous_hash == expected and revision == base['publicationRevision']:
            return dict(state='ALREADY_CURRENT', head=base, applyAttempts=0)
        require(revision > base['publicationRevision'], 'MARKET_PUBLISH_STALE_REVISION')
        recovery_dir.mkdir(mode=0o700, exist_ok=False)
        sync_dir(recovery_dir.parent)
        intent = dict(base=base, candidateBookHash=expected, candidateRevision=revision)
        for name, raw in [('previous.json', previous), ('intent.json', json.dumps(intent).encode())]:
            with (recovery_dir / name).open('xb') as stream:
                stream.write(raw)
                stream.flush()
                os.fsync(stream.fileno())
        sync_dir(recovery_dir)
        tmp = tempfile.NamedTemporaryFile(dir=active.parent, delete=False)
        try:
            with tmp:
                os.fchown(tmp.fileno(), meta.st_uid, meta.st_gid)
                os.fchmod(tmp.fileno(), stat.S_IMODE(meta.st_mode) & 0o777)
                tmp.write(candidate_raw)
                tmp.flush()
                os.fsync(tmp.fileno())
            require(os.path.samestat(meta, active.lstat()) and read(active) == previous,
                    'MARKET_PUBLISH_ACTIVE_CHANGED')
            os.replace(tmp.name, active)
            os.fsync(parent_fd)
        finally:
            Path(tmp.name).unlink(missing_ok=True)
        pre = call('preview')
        require(pre['wouldAdvance'] is True and pre['published'] is False and pre['candidateRevision'] == revision
                and pre.get('bankAnchor') == base['bankAnchor'] and pre['candidateBookHash'] == expected and
                pre['baseEntryHash'] == base['journalEntryHash'],
                'MARKET_PUBLISH_PREVIEW_CHANGED')
        fresh()
        try:
            receipt = call('apply', baseEntryHash=base['journalEntryHash'], candidateBookHash=expected)
        except (OSError, ValueError, http.client.HTTPException):
            receipt = None  # An error may arrive after fsync. Only read the head from here on.
        try:
            actual = reconcile(intent, call)
            if receipt is not None:
                require(receipt['changed'] is True and receipt['fsyncConfirmed'] is True and
                        all(actual[k] == receipt[k] for k in ('priceBookHash','publicationRevision','journalSequence','journalEntryHash')),
                        'MARKET_PUBLISH_OUTCOME_UNKNOWN')
        except (OSError, ValueError, KeyError, TypeError, http.client.HTTPException):
            raise ValueError('MARKET_PUBLISH_OUTCOME_UNKNOWN') from None
        return dict(state='CONFIRMED_BY_READBACK', head=actual, applyAttempts=1, acknowledgementReceived=receipt is not None)
    finally:
        os.close(parent_fd)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(required=True, dest='command')
    install = commands.add_parser('publish')
    for arg in ('config', 'candidate', 'expected-book-hash', 'publication-cli', 'recovery-dir', 'router-origin'):
        install.add_argument('--' + arg, required=True)
    lookup = commands.add_parser('reconcile')
    lookup.add_argument('--intent', type=Path, required=True)
    lookup.add_argument('--router-origin', required=True)
    args = parser.parse_args()
    try:
        call = local_client(args.router_origin, os.environ.get('XONE_ROUTER_TOKEN'))
        if args.command == 'reconcile':
            print(json.dumps(dict(state='CONFIRMED_BY_READBACK', head=reconcile(load(read(args.intent)), call),
                                  applyAttempts=0)))
        else:
            print(json.dumps(publish(args.config, args.candidate, args.expected_book_hash, args.publication_cli,
                                     args.recovery_dir, call)))
    except Exception as error:
        print(json.dumps(dict(error=str(error) if isinstance(error, ValueError) else type(error).__name__)), file=sys.stderr)
        sys.exit(1)
