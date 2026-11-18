#!/usr/bin/env python3
"""Independently signed publication versions; public TEST keys, no live prices."""
import copy
import json
from pathlib import Path
import subprocess
import sys
from native_valuation_vectors import ROOT, digest, policy, sign, vectors

FIXTURES = [ROOT / name / 'test/contracts/native-valuation-publications.json'
            for name in ['integration', 'dex-router']]


def build():
    anchor = vectors()['anchor']
    policies = [policy(anchor['chain_key'], anchor['genesis_hash'], '0x' + '00' * 19 + t)
                for t in ['10', '20']]

    def book(revision, ratio, **times):
        return dict(schema_version=1, prices=[sign(p, anchor, ratio, 3, revision=revision, **times)
                                             for p in policies])

    books = {str(r): book(r, r + 1) for r in [0, 1, 2, 3]}
    books['conflict'] = book(2, 99)
    books['missing'] = book(3, 4)
    books['missing']['prices'].pop()
    books['stale'] = book(4, 5, observed=anchor['timestamp_ms'] - 60001,
                         until=anchor['timestamp_ms'] - 1)
    books['future'] = book(4, 5, observed=anchor['timestamp_ms'] + 1)
    books['mixed'] = copy.deepcopy(books['2'])
    books['mixed']['prices'][0] = books['1']['prices'][0]
    books['tampered'] = copy.deepcopy(books['2'])
    books['tampered']['prices'][0]['observation']['publication_revision'] = 9
    return dict(policies=policies, anchor=anchor, books=books,
                scope='PUBLIC_TEST_SIGNATURES_PROCESS_LOCAL_PUBLICATION_NOT_MARKET_OR_PRODUCTION')


def cli(binary, output):
    from evm_dynamic_fee_http import sha, require
    binary, output = Path(binary).resolve(), Path(output).resolve()
    output.mkdir(parents=True, exist_ok=False)
    source = ROOT/'integration/artifacts/local/approved-market-feed-precheck-20261119/explicit-publicnode'
    candidate = json.loads((source/'candidate.json').read_bytes())
    observation = json.loads((source/'unsigned.json').read_bytes())['observation']
    cases = []
    for i, revision in enumerate([0, 1, 2**64-1, -1, 2**64, '1']):
        c = dict(candidate, publication_revision=revision)
        raw = json.dumps(c).encode()
        (output/f'{i}.input.json').write_bytes(raw)
        result = subprocess.run([str(binary)], input=raw, capture_output=True, timeout=10)
        (output/f'{i}.stdout').write_bytes(result.stdout)
        (output/f'{i}.stderr').write_bytes(result.stderr)
        valid = i < 3
        if valid:
            require(result.returncode == 0, 'Market publication CLI rejected valid revision')
            actual = json.loads(result.stdout); expected = copy.deepcopy(observation)
            if revision: expected['publication_revision'] = revision
            require(actual['observation'] == expected, 'Market publication changed source observation')
            require(actual['signing_message_hex'] == ('xone-native-token-price/1:'+digest(expected)).encode().hex(),
                    'Market publication signing message differs independently')
            require(actual['authorized'] is False and actual['productionVerified'] is False, 'Unsigned CLI authorized a publication')
        else:
            require(result.returncode != 0 and json.loads(result.stderr)['error'] == 'NATIVE_VALUATION_MARKET_INPUT',
                    'Market publication CLI accepted invalid revision width/type')
        cases.append(dict(revision=revision, valid=valid, exit_code=result.returncode))
    (output/'report.json').write_text(json.dumps(dict(passed=True, cases=cases, validator_sha256=sha(binary),
        public_rpc_reads=0, original_capture_time_preserved=True, production_ready=False,
        artifacts={str(p.relative_to(output)):sha(p) for p in sorted(output.iterdir()) if p.is_file()}), indent=2)+'\n')


if __name__ == '__main__' and sys.argv[1:2] == ['--cli']:
    cli(*sys.argv[2:])
elif __name__ == '__main__':
    raw = json.dumps(build(), indent=2) + '\n'
    if sys.argv[1:] == ['--check']:
        for path in FIXTURES:
            if path.read_text() != raw:
                raise ValueError('Independent signed publication vectors changed: ' + str(path))
        print(json.dumps(dict(passed=True, books=10, independent_signer='PINNED_SOLDERS_ED25519')))
    else:
        for path in FIXTURES: path.write_text(raw)
