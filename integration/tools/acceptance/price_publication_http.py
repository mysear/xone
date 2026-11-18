#!/usr/bin/env python3
"""Normal HTTP publication, pinned candidates, rollback rejection and old quotes."""
import concurrent.futures
import copy
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import threading
import time
import urllib.error
import urllib.request
from evm_dynamic_fee_http import ROOT, require, sha
from native_valuation_http import prepare
from native_valuation_vectors import digest, sign


def main(binary, corpus_path, output):
    binary, corpus_path, output = Path(binary).resolve(), Path(corpus_path).resolve(), Path(output).resolve()
    output.mkdir(parents=True, exist_ok=False)
    corpus = json.loads(corpus_path.read_bytes())
    source = next(c for c in corpus['cases'] if c['id'] == 'EXACT_IN-1-false')
    config, initial, policy = prepare(output / 'oracle', source=dict(state_anchor=source['fixture']['state_anchor'],
        token_in=source['path'][0], token_out=source['path'][-1]))
    anchor = initial['prices'][0]['observation']['anchor']
    path = Path(policy['observations_file'])

    def book(revision, ratio=10000000, **times):
        return dict(schema_version=1, prices=[sign(p, anchor, ratio, 1, revision=revision, **times)
                                             for p in policy['policies']])

    def install(value, label):
        raw = value if isinstance(value, bytes) else (json.dumps(value, indent=2) + '\n').encode()
        (output / ('book-' + label + '.json')).write_bytes(raw)
        temp = path.with_suffix('.tmp')
        with temp.open('wb') as stream:
            stream.write(raw); stream.flush(); os.fsync(stream.fileno())
        temp.replace(path)

    current = book(0)
    install(current, 'initial')
    env = dict(os.environ, XONE_NATIVE_VALUATION_CONFIG=str(config), XONE_CANDIDATE_COST_MAX_PLANS='16',
               XONE_CANDIDATE_COST_CORPUS=str(corpus_path), XONE_EVM_PATH_CASE=source['id'])
    for key in ['XONE_OP_API_SOURCE', 'XONE_EVM_PREPARED_HTTP', 'XONE_EVM_COINBASE_ALIAS', 'XONE_EVM_LEGACY_FEE']:
        env.pop(key, None)
    info_path = output / 'info.json'
    quotes, rejections, records, race_results, publications = [], [], [], [], []
    with (output / 'process.log').open('wb') as log:
        process = subprocess.Popen([str(binary), str(info_path)], cwd=ROOT, env=env,
                                   stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            until = time.monotonic() + 30
            while not info_path.exists():
                require(process.poll() is None and time.monotonic() < until, 'Publication HTTP startup failed')
                time.sleep(.05)
            info = json.loads(info_path.read_bytes())
            fixture = json.loads(info_path.with_suffix('.state.json').read_bytes())['fixture']

            def call(route, body=None, authorized=True):
                headers = {'Content-Type': 'application/json'}
                if authorized: headers['Authorization'] = 'Bearer ' + info['token']
                request = urllib.request.Request(info['api'] + route,
                    data=None if body is None else json.dumps(body).encode(), headers=headers)
                opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
                try:
                    response = opener.open(request, timeout=20)
                except urllib.error.HTTPError as error:
                    response = error
                with response:
                    raw = response.read()
                    try: value = json.loads(raw)
                    except ValueError: value = {'body': raw.decode(errors='replace')}
                    return response.code, value

            reload_path = '/internal/native-valuation/reload'

            def accept_quote(q, expected, label):
                ranking = q['quality']['costRanking']; expected_hash = digest(expected)
                revision = expected['prices'][0]['observation'].get('publication_revision', 0)
                require(ranking['priceBookHash'] == expected_hash and ranking['publicationRevision'] == revision,
                        'Quote did not pin the authorized publication')
                require(ranking['costAdjustmentApplied'] is True and q['costs']['completeQuoteCosts'] is False,
                        'Publication changed fee completeness')
                require(q['verification']['level'] == 'INDICATIVE' and q['verification']['success'] is False,
                        'Publication upgraded production verification')
                request = info['exactOutRequest'] if q['kind'] == 'EXACT_OUT' else info['request']
                for h, e in ranking['candidateExecutions'].items():
                    economics = e['costs']['transactionEconomics']
                    require(economics['priceBookHash'] == expected_hash, 'One quote mixed price books')
                    for side, token in [('sell', request['sellToken']), ('buy', request['buyToken'])]:
                        observation = next(p['observation'] for p in expected['prices'] if p['observation']['token'] == token)
                        fee = (int(e['costs']['payerNetNativeDebit']) * int(observation['token_atoms'])
                               + int(observation['native_atoms']) - 1) // int(observation['native_atoms'])
                        require(economics[side + 'TokenFee'] == str(fee), 'Published fee differs from independent integer arithmetic')
                        require(economics[side + 'Price']['observationHash'] == digest(observation), 'Published observation detached')
                    records.append(dict(id=label+'-'+h, plan_hash=h, fixture=fixture, execution=e,
                        sell_token=request['sellToken'], buy_token=request['buyToken'], recipient=request['recipient']))
                require(q['costs']['transactionEconomics']['priceBookHash'] == expected_hash, 'Public fee used different publication')
                require(q['costs']['sourceEvidence']['priceBookHash'] == expected_hash
                        and q['costs']['sourceEvidence']['publicationRevision'] == revision, 'Public fee provenance lost publication')
                (output / (label + '-quote.json')).write_text(json.dumps(q, indent=2) + '\n')
                quotes.append(dict(label=label, quote_id=q['quoteId'], price_book_hash=expected_hash, revision=revision))

            def quote_pair(expected, label):
                pair = []
                for exact in [False, True]:
                    route = '/v1/quote-exact-out' if exact else '/v1/quote'
                    request = info['exactOutRequest'] if exact else info['request']
                    status, q = call(route, request); require(status == 200, 'Publication quote failed: ' + str(q))
                    accept_quote(q, expected, label + '-' + str(int(exact)))
                    status, cached = call(route, request); require(status == 200 and cached == q, 'Publication cache identity changed')
                    status, evidence = call('/internal/quotes/' + q['quoteId'] + '/evidence')
                    require(status == 200 and evidence['quote'] == q and evidence['price_current'] is True, 'Publication quote evidence changed')
                    pair.append(q)
                return pair

            old = quote_pair(current, 'legacy')
            status, unchanged = call(reload_path, {}); require(status == 200 and unchanged['changed'] is False, 'Identical book not idempotent')
            publications.append(unchanged)
            current = book(1); install(current, 'revision1')
            status, published = call(reload_path, {})
            require(status == 200 and published['priceBookHash'] == digest(current) and published['publicationRevision'] == 1
                    and published['restartRollbackProtected'] is False and published['productionVerified'] is False, 'Signed publication failed')
            publications.append(published)
            for q in old:
                status, evidence = call('/internal/quotes/' + q['quoteId'] + '/evidence')
                require(status == 200 and evidence['quote'] == q and evidence['price_current'] is False,
                        'Old publication evidence was overwritten or still current')
                (output / ('old-' + q['kind'] + '-evidence.json')).write_text(json.dumps(evidence, indent=2)+'\n')
                for endpoint in ['simulate', 'build']:
                    status, result = call('/v1/quotes/' + q['quoteId'] + '/' + endpoint, {})
                    (output / ('old-' + q['kind'] + '-' + endpoint + '.json')).write_text(json.dumps(dict(status=status,result=result)) + '\n')
                    require(status == 409 and result.get('error') == 'NATIVE_VALUATION_QUOTE_PRICE_CHANGED', 'Old quote rejection mismatch: ' + str((status,result)))
            pair = quote_pair(current, 'revision1')
            require(all(a['quoteId'] != b['quoteId'] and a['requestHash'] != b['requestHash'] for a,b in zip(old,pair)), 'New publication reused old quote identity')

            invalid = {}
            for name, revision, ratio in [('rollback', 0, 10000000), ('same-version', 1, 2)]: invalid[name] = book(revision, ratio)
            invalid['missing-token'] = book(2); invalid['missing-token']['prices'].pop()
            invalid['bad-signature'] = book(2); invalid['bad-signature']['prices'][0]['signatures'][0]['signature'] = '00' * 64
            invalid['mixed-revision'] = book(2); invalid['mixed-revision']['prices'][0] = current['prices'][0]
            invalid['unsigned-revision'] = copy.deepcopy(current)
            for price in invalid['unsigned-revision']['prices']: price['observation']['publication_revision'] = 3
            invalid['stale'] = book(2, observed=anchor['timestamp_ms']-60001, until=anchor['timestamp_ms']-1)
            invalid['future'] = book(2, observed=anchor['timestamp_ms']+1)
            other_anchor = dict(anchor, block_hash='0x'+'44'*32)
            invalid['wrong-anchor'] = dict(schema_version=1, prices=[sign(p, other_anchor, 1, 1, revision=2) for p in policy['policies']])
            invalid['invalid-json'] = b'{'
            invalid['oversized'] = b' ' * ((1 << 20) + 1)
            for name, value in invalid.items():
                install(value, name); status, result = call(reload_path, {})
                require(status >= 400 and result.get('error', '').startswith('NATIVE_VALUATION_'), 'Invalid publication accepted: ' + name)
                rejections.append(dict(mode=name, status=status, result=result))
                for exact, q in zip([False, True], pair):
                    status, cached = call('/v1/quote-exact-out' if exact else '/v1/quote', info['exactOutRequest'] if exact else info['request'])
                    require(status == 200 and cached == q, 'Failed publication changed current snapshot: ' + name)
            install(current, 'restored')
            for name in ['missing-file', 'directory'] + (['fifo'] if os.name == 'posix' else []):
                path.unlink()
                if name == 'directory': path.mkdir()
                elif name == 'fifo': os.mkfifo(path)
                started = time.monotonic(); status, result = call(reload_path, {})
                require(status >= 400 and result.get('error', '').startswith('NATIVE_VALUATION_')
                        and time.monotonic()-started < 5, 'Invalid file blocked or published: ' + name)
                rejections.append(dict(mode=name, status=status, result=result))
                if name == 'directory': path.rmdir()
                elif path.exists(): path.unlink()
                install(current, 'restore-' + name)
            status, _ = call(reload_path, {}, authorized=False); require(status == 401, 'Anonymous reload accepted')
            status, _ = call(reload_path, {'observations_file': '/untrusted', 'prices': []}); require(status == 422, 'Request controlled publication input')

            current = book(2, 1); install(current, 'revision2')
            status, result = call(reload_path, {}); require(status == 200 and result['changed'] is True, 'Second publication failed')
            publications.append(result)
            newest = quote_pair(current, 'revision2')
            for q in newest:
                routes = q['routes'][0]['edges'] if q['kind'] == 'EXACT_OUT' else q['routes'][0]['path']['edges']
                require(len(routes) == 2, 'Updated valuation did not change measured route ranking')
                status, simulation = call('/v1/quotes/' + q['quoteId'] + '/simulate', {})
                require(status == 200 and simulation['costs']['transactionEconomics']['priceBookHash'] == digest(current), 'Current quote simulation lost publication binding')
                (output / ('revision2-' + q['kind'] + '-simulation.json')).write_text(json.dumps(simulation, indent=2) + '\n')

            # Each racing response must contain one complete publication or fail closed.
            for revision in range(3, 7):
                previous, next_book = current, book(revision, 10000000 if revision % 2 else 1)
                install(next_book, 'race' + str(revision)); barrier = threading.Barrier(2)
                request = dict(info['request'], slippageBps=revision)
                def race_quote():
                    barrier.wait(); return call('/v1/quote', request)
                with concurrent.futures.ThreadPoolExecutor(max_workers=1) as executor:
                    future = executor.submit(race_quote); barrier.wait(); status, result = call(reload_path, {})
                    require(status in [200, 400], 'Unexpected publication admission response')
                    if status != 200:
                        require(result.get('error') == 'NATIVE_VALUATION_RELOAD_BUSY', 'Unexpected publication race failure')
                    quote_status, q = future.result()
                if status == 200: current = next_book
                if quote_status == 200:
                    expected = next((b for b in [previous,current] if digest(b) == q['quality']['costRanking']['priceBookHash']), None)
                    require(expected is not None, 'Race quote unknown publication')
                    accept_quote(q, expected, 'race' + str(revision))
                else:
                    require((quote_status == 409 and q.get('error') == 'NATIVE_VALUATION_QUOTE_PRICE_CHANGED')
                            or (quote_status in [400,429] and q.get('error') == 'SIMULATION_BUSY'), 'Race did not fail closed: ' + str((quote_status,q)))
                race_results.append(dict(revision=revision, publication_status=status, publication_result=result,
                                         quote_status=quote_status, quote=q))
        finally:
            if process.poll() is None:
                os.killpg(process.pid, signal.SIGTERM)
                try: process.wait(timeout=5)
                except subprocess.TimeoutExpired: os.killpg(process.pid, signal.SIGKILL); process.wait()
    replay = output / 'original-replay-input.json'; replay.write_text(json.dumps(dict(records=records)) + '\n')
    result = subprocess.run(['node', 'integration/tools/ci/candidate-cost-original-replay.cjs', str(replay), str(output/'original-replay.json')],
                            cwd=ROOT, capture_output=True, text=True, timeout=300)
    (output/'original-replay.log').write_text(result.stdout + result.stderr)
    require(result.returncode == 0, 'Published candidate original replay failed')
    (output/'report.json').write_text(json.dumps(dict(passed=True, quotes=quotes, publications=publications, invalid_publications=rejections,
        concurrent_publications=race_results, candidate_executions=len(records), independent_original_replays=len(records),
        authenticated_reload_required=True, restart_rollback_protected=False, production_ready=False,
        harness_sha256=sha(binary), corpus_sha256=sha(corpus_path),
        artifacts={str(p.relative_to(output)):sha(p) for p in sorted(output.rglob('*')) if p.is_file()}), indent=2)+'\n')


if __name__ == '__main__': main(*sys.argv[1:])
