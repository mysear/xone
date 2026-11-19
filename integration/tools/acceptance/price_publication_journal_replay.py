#!/usr/bin/env python3
"""Offline CLI/process and independent on-disk hash-chain checks; TEST prices."""
import copy
import hashlib
import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import sys
import tempfile
from karst_api_vectors import ROOT, require
from native_valuation_publications import build


def sha(path):
    with Path(path).open('rb') as stream:
        return hashlib.file_digest(stream, 'sha256').hexdigest()


def canonical(value):
    text = json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False)
    for before, after in [('<', '\\u003c'), ('>', '\\u003e'), ('&', '\\u0026'),
                          ('\u2028', '\\u2028'), ('\u2029', '\\u2029')]:
        text = text.replace(before, after)
    return hashlib.sha256(text.encode()).hexdigest()


def replay(path, source, revisions):
    raw = path.read_bytes()
    require(raw.endswith(b'\n'), 'Journal did not end at a complete record')
    entries = [json.loads(line) for line in raw.splitlines()]
    require(len(entries) == len(revisions), 'Journal entry count differs')
    previous = '0' * 64
    for i, (entry, revision) in enumerate(zip(entries, revisions), 1):
        water = entry['high_water']
        require(entry['schema_version'] == water['schema_version'] == 1, 'Journal schema differs')
        require(entry['sequence'] == i and entry['previous_hash'] == previous, 'Journal sequence/link differs')
        require(water['revision'] == revision and water['book_hash'] == canonical(source['books'][str(revision)]), 'Journal book differs')
        require(water['policy_hash'] == canonical(source['policies']) and water['anchor'] == source['anchor'], 'Journal authority/anchor differs')
        require(water['tokens'] == sorted(p['token'] for p in source['policies']), 'Journal token set differs')
        expected = canonical(dict(domain='xone-price-publication-journal/1', schema_version=1,
                                  sequence=i, previous_hash=previous, high_water=water))
        require(entry['entry_hash'] == expected, 'Independent journal hash differs')
        previous = expected
    return entries[-1]


def main(binary, output):
    binary, output = Path(binary).resolve(strict=True), Path(output).resolve()
    output.mkdir(parents=True, exist_ok=False)
    source = build()
    (output/'source.json').write_text(json.dumps(source, indent=2)+'\n')
    policy = dict(observations_file='prices.json', policies=source['policies'])
    journal = output/'prices.journal'
    cases = []
    with tempfile.TemporaryDirectory(prefix='xone-journal-cli-') as stage:
        staged = Path(stage)/binary.name
        shutil.copy2(binary, staged)
        binary_sha = sha(binary)
        require(sha(staged) == binary_sha == sha(binary), 'Journal CLI changed during staging')

        def call(name, mode='check', book='2', *, path=None, error=None, override=None):
            target = journal if path is None else path
            value = dict(policy=policy, book=source['books'][book], anchor=source['anchor'],
                         execution_timestamp_ms=source['anchor']['timestamp_ms'])
            if override is not None: value = override
            raw = json.dumps(value, indent=2).encode()+b'\n'
            (output/(name+'.input.json')).write_bytes(raw)
            result = subprocess.run([str(staged), mode, str(target)], input=raw, capture_output=True, timeout=10)
            (output/(name+'.stdout')).write_bytes(result.stdout)
            (output/(name+'.stderr')).write_bytes(result.stderr)
            if error is not None:
                require(result.returncode != 0 and not result.stdout, 'Rejected journal command succeeded: '+name)
                actual = json.loads(result.stderr)['error']
                require(actual == error, 'Unexpected journal rejection '+name+': '+actual)
                value = None
            else:
                require(result.returncode == 0 and not result.stderr, 'Valid journal command failed: '+name)
                value = json.loads(result.stdout)
                require(value['scope'] == 'AUTHORIZED_LOCAL_PRICE_PUBLICATION_JOURNAL_ONLY' and
                        all(value[k] is False for k in ['normalRouterConnected', 'productionVerified', 'publicTransactionsSubmitted']), 'Journal scope overclaimed')
                receipt = value['receipt']
                require(receipt['fsync_confirmed'] is True and receipt['high_water']['revision'] == int(book), 'Durable receipt differs')
                head = replay(target, source, list(range(1, int(book)+1)))
                require(all(receipt[k] == head[k] for k in ['sequence', 'high_water', 'entry_hash']), 'Receipt differs from journal head')
                require(receipt['changed'] is (name in ['initialize', 'advance-two', 'advance-three']), 'Incorrect mutation receipt: '+name)
            cases.append(dict(name=name, mode=mode, book=book, path=str(target.relative_to(output)),
                              exit_code=result.returncode, expected_error=error))
            return value

        call('missing', error='PRICE_PUBLICATION_JOURNAL_UNAVAILABLE')
        require(not journal.exists(), 'Check auto-created missing journal')
        call('initialize', 'initialize', '1')
        require(stat.S_IMODE(journal.stat().st_mode) == 0o600, 'Journal creation permissions differ')
        first = journal.read_bytes(); (output/'initialized.raw').write_bytes(first)
        replay(journal, source, [1])
        call('duplicate-initialize', 'initialize', '1', error='PRICE_PUBLICATION_JOURNAL_UNAVAILABLE')
        require(journal.read_bytes() == first, 'Duplicate initialize overwrote journal')
        call('check-one', book='1')
        call('newer-needs-advance', error='PRICE_PUBLICATION_JOURNAL_ADVANCE_REQUIRED')
        require(call('advance-two', 'advance')['receipt']['changed'] is True, 'Advance did not append')
        second = journal.read_bytes(); (output/'advanced.raw').write_bytes(second)
        replay(journal, source, [1, 2])
        call('restart-check')
        require(call('idempotent', 'advance')['receipt']['changed'] is False, 'Idempotent advance appended')
        for name, error in [('1', 'PRICE_PUBLICATION_JOURNAL_ROLLBACK_OR_CONFLICT'),
                            ('conflict', 'PRICE_PUBLICATION_JOURNAL_ROLLBACK_OR_CONFLICT'),
                            ('missing', 'PRICE_PUBLICATION_JOURNAL_BINDING'),
                            ('0', 'PRICE_PUBLICATION_POSITIVE_REVISION_REQUIRED'),
                            ('stale', 'NATIVE_VALUATION_TIME'), ('future', 'NATIVE_VALUATION_TIME'),
                            ('mixed', 'NATIVE_VALUATION_BOOK_REVISION'), ('tampered', 'NATIVE_VALUATION_BOOK_REVISION')]:
            call('reject-'+name, 'advance', name, error=error)
            require(journal.read_bytes() == second, 'Rejected publication changed journal')
        bad_signature = dict(policy=policy, book=copy.deepcopy(source['books']['2']),
                             anchor=source['anchor'], execution_timestamp_ms=source['anchor']['timestamp_ms'])
        bad_signature['book']['prices'][0]['observation']['token_atoms'] = '99'
        call('bad-signature', 'advance', override=bad_signature, error='NATIVE_VALUATION_AUTHORIZATION')
        require(journal.read_bytes() == second, 'Bad signature changed journal')
        wrong_policy = copy.deepcopy(policy); wrong_policy['policies'][0]['source_id'] += '-changed'
        # A correctly signed book for the changed authority still cannot reopen the old journal.
        from native_valuation_vectors import sign
        wrong_book = dict(schema_version=1, prices=[sign(p, source['anchor'], 3, 3, revision=2) for p in wrong_policy['policies']])
        call('wrong-policy', override=dict(policy=wrong_policy, book=wrong_book, anchor=source['anchor'],
             execution_timestamp_ms=source['anchor']['timestamp_ms']), error='PRICE_PUBLICATION_JOURNAL_BINDING')
        require(journal.read_bytes() == second, 'Wrong authority changed journal')
        journal.write_bytes(second+b'{partial')
        (output/'partial-tail.raw').write_bytes(journal.read_bytes())
        call('recover-partial')
        require(journal.read_bytes() == second, 'Partial recovery changed committed bytes')
        for name, raw, error in [('empty', b'', 'PRICE_PUBLICATION_JOURNAL_UNINITIALIZED'),
                                 ('partial-bootstrap', b'{partial', 'PRICE_PUBLICATION_JOURNAL_CORRUPT'),
                                 ('complete-corrupt', second+b'{}\n', 'PRICE_PUBLICATION_JOURNAL_CORRUPT')]:
            path = output/(name+'.journal'); path.write_bytes(raw)
            call(name, path=path, error=error)
            require(path.read_bytes() == raw, 'Corruption rejection modified evidence')
        entries = second.splitlines()
        for name, mutate in [('bad-hash', lambda e: e.update(entry_hash='f'*64)),
                             ('bad-sequence', lambda e: e.update(sequence=7)),
                             ('bad-link', lambda e: e.update(previous_hash='f'*64)),
                             ('unknown-field', lambda e: e.update(extra=True))]:
            value = json.loads(entries[-1]); mutate(value)
            raw = entries[0]+b'\n'+json.dumps(value).encode()+b'\n'
            path = output/(name+'.journal'); path.write_bytes(raw)
            call(name, path=path, error='PRICE_PUBLICATION_JOURNAL_CORRUPT')
            require(path.read_bytes() == raw, 'Bad complete record changed on rejection')
        directory = output/'directory'; directory.mkdir()
        link = output/'symlink'; link.symlink_to(journal.name)
        fifo = output/'fifo'; os.mkfifo(fifo)
        for name, path in [('directory', directory), ('symlink', link), ('fifo', fifo)]:
            call(name, path=path, error='PRICE_PUBLICATION_JOURNAL_PATH')
        fifo.unlink(); link.unlink()
        large = output/'oversized.journal'
        with large.open('wb') as stream: stream.truncate(16*1024*1024+1)
        call('oversized', path=large, error='PRICE_PUBLICATION_JOURNAL_LIMIT')
        # Sparse size is retained; digest still covers all logical bytes.
        import fcntl
        with journal.open('rb+') as locked:
            fcntl.flock(locked, fcntl.LOCK_EX | fcntl.LOCK_NB)
            call('external-writer', error='PRICE_PUBLICATION_JOURNAL_IN_USE')
        call('after-writer-exit')
        call('advance-three', 'advance', '3')
        head = replay(journal, source, [1, 2, 3])
        call('restart-three', book='3')
        require(sha(binary) == binary_sha, 'Journal CLI changed during validation')
    artifacts = {str(p.relative_to(output)):sha(p) for p in sorted(output.rglob('*')) if p.is_file()}
    report = dict(passed=True, cases=cases, validator_sha256=binary_sha, journal_head=head,
                  independent_hash_chain_entries=3, local_persistent_volume_assumed=True,
                  normal_router_connected=False, production_ready=False, publicTransactionsSubmitted=False,
                  artifacts=artifacts)
    (output/'report.json').write_text(json.dumps(report, indent=2)+'\n')
    print(json.dumps(dict(passed=True, cli_cases=len(cases), journal_entries=3)))


if __name__ == '__main__': main(*sys.argv[1:])
