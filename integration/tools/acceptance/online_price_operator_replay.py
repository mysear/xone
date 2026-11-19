"""Exercise the publication operator against a running, fixture-time Router."""
import http.client
import json
import os
from pathlib import Path
import subprocess
import sys
from unittest.mock import patch

from evm_dynamic_fee_http import require
from native_valuation_vectors import digest

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / 'market'))
import publish_chainlink_book as operator


def replay(info, config, cli, out, active, journal, books, at):
    out.mkdir()
    original, disk = active.read_bytes(), journal.read_bytes()
    client = operator.local_client(info['api'], info['token'])
    candidate = out/'candidate.json'
    results = {}

    def attempt(label, revision, fault=None, expected_error=None):
        candidate.write_text(json.dumps(books[revision]))
        calls = []

        def call(operation, **fields):
            calls.append(operation)
            if operation == 'apply' and fault == 'not-delivered':
                raise TimeoutError()
            response = client(operation, **fields)
            if operation == 'apply' and fault == 'lost-ack':
                raise http.client.IncompleteRead(b'')
            return response

        with patch.object(operator.time, 'time_ns', return_value=at * 1_000_000):
            try:
                result = operator.publish(config, candidate, digest(books[revision]), cli, out/label, call)
            except ValueError as e:
                require(expected_error and str(e) == expected_error, f'{label}: {e}')
                result = dict(error=str(e))
            else:
                require(expected_error is None, label + ': unexpected publication')
        results[label] = dict(calls=calls, result=result)
        return result, calls

    _, calls = attempt('uncertain', 3, 'not-delivered', 'MARKET_PUBLISH_OUTCOME_UNKNOWN')
    require(calls == ['status', 'preview', 'apply', 'status'] and journal.read_bytes() == disk and
            active.read_bytes() == candidate.read_bytes(), 'Uncertain install retried or rolled back')
    _, calls = attempt('retry', 3, expected_error='MARKET_PUBLISH_UNRESOLVED_INSTALL')
    require(calls == ['status'] and not (out/'retry').exists(), 'Unresolved base overwritten')
    backup = (out/'uncertain/previous.json').read_bytes()
    require(backup == original, 'Original bytes not retained')
    # The fixture knows the first request was never delivered. Only the fixture restores the base.
    active.write_bytes(backup)
    r, calls = attempt('lost-ack', 3, 'lost-ack')
    require(r['state'] == 'CONFIRMED_BY_READBACK' and not r['acknowledgementReceived'] and
            calls == ['status', 'preview', 'apply', 'status'], 'Lost acknowledgement not reconciled')
    lookup = [sys.executable, '-B', operator.__file__, 'reconcile', '--intent', str(out/'lost-ack/intent.json'),
              '--router-origin', info['api']]
    env = dict(os.environ, XONE_ROUTER_TOKEN=info['token'])
    recovered = json.loads(subprocess.check_output(lookup, env=env, timeout=20))
    require(recovered['applyAttempts'] == 0 and recovered['head'] == r['head'], 'Read-only recovery mismatch')
    r, calls = attempt('duplicate', 3)
    require(r['state'] == 'ALREADY_CURRENT' and calls == ['status'], 'Duplicate not idempotent')
    r, calls = attempt('normal', 4)
    require(r['acknowledgementReceived'] and r['head']['publicationRevision'] == 4 and
            calls == ['status', 'preview', 'apply', 'status'], 'Normal publication receipt mismatch')
    require([json.loads(line)['high_water']['revision'] for line in journal.read_bytes().splitlines()] == [1,2,3,4],
            'Unexpected persistent journal history')
    superseded = subprocess.run(lookup, env=env, capture_output=True, timeout=20)
    require(superseded.returncode and json.loads(superseded.stderr)['error'] == 'MARKET_PUBLISH_OUTCOME_UNKNOWN',
            'Read-only recovery claimed a superseded head')
    (out/'results.json').write_text(json.dumps(results, indent=2) + '\n')
