#!/usr/bin/env python3
"""Restart a real local HTTP harness against one CLI-bootstrapped price journal."""
import contextlib
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
import urllib.error
import urllib.request

from evm_dynamic_fee_http import ROOT, require, sha
from native_valuation_http import prepare
from native_valuation_vectors import digest, sign
from online_price_operator_replay import replay as operator_replay


def main(binary, cli, corpus, output):
    binary, cli, corpus = (Path(x).resolve(strict=True) for x in (binary, cli, corpus))
    out = Path(output).resolve()
    out.mkdir(parents=True, exist_ok=False)
    case = next(c for c in json.loads(corpus.read_bytes())['cases'] if c['id'] == 'EXACT_IN-1-false')
    config, initial, policy = prepare(out/'oracle', source=dict(state_anchor=case['fixture']['state_anchor'],
        token_in=case['path'][0], token_out=case['path'][-1]))
    anchor = initial['prices'][0]['observation']['anchor']
    journal = out/'prices.journal'
    policy['publication_journal'] = str(journal)
    config.write_text(json.dumps(policy))
    path = Path(policy['observations_file'])
    env = dict(os.environ, XONE_NATIVE_VALUATION_CONFIG=str(config), XONE_CANDIDATE_COST_MAX_PLANS='16',
               XONE_CANDIDATE_COST_CORPUS=str(corpus), XONE_EVM_PATH_CASE=case['id'])
    for key in ['XONE_OP_API_SOURCE','XONE_EVM_PREPARED_HTTP','XONE_EVM_COINBASE_ALIAS','XONE_EVM_LEGACY_FEE']:
        env.pop(key, None)
    books = {i:dict(schema_version=1, prices=[sign(p, anchor, i*10000000, 1, revision=i) for p in policy['policies']]) for i in [1,2,3,4]}
    def install(i):
        tmp = path.with_suffix('.tmp'); tmp.write_text(json.dumps(books[i])); tmp.replace(path)
    def disk(i):
        rows = [json.loads(line) for line in journal.read_bytes().splitlines()]
        require(len(rows)==i, 'Unexpected journal length')
        previous = '0'*64
        for rev, row in enumerate(rows,1):
            h = row['high_water']
            require(h['book_hash']==digest(books[rev]) and h['revision']==rev and h['anchor']==anchor and
                    h['policy_hash']==digest(policy['policies']), 'Durable book or authority mismatch')
            previous = digest(dict(domain='xone-price-publication-journal/1',schema_version=1,
                                   sequence=rev,previous_hash=previous,high_water=h))
            require(row['entry_hash']==previous, 'Durable entry hash mismatch')
        return previous
    rejects = []
    def rejected(name, code):
        info = out/(name+'.info.json'); log = out/(name+'.log')
        with log.open('wb') as stream:
            r = subprocess.run([str(binary), str(info)],cwd=ROOT,env=env,stdout=stream,stderr=subprocess.STDOUT,timeout=30)
        require(r.returncode!=0 and not info.exists() and code in log.read_text(), 'Wrong startup rejection: '+name)
        rejects.append(dict(name=name, error=code))

    @contextlib.contextmanager
    def service(name):
        info = out/(name+'.info.json')
        with (out/(name+'.log')).open('wb') as stream:
            p = subprocess.Popen([str(binary),str(info)],cwd=ROOT,env=env,stdout=stream,stderr=subprocess.STDOUT,start_new_session=True)
            try:
                until = time.monotonic()+30
                while not info.exists():
                    require(p.poll() is None and time.monotonic()<until, 'Router did not start: '+name)
                    time.sleep(.05)
                yield json.loads(info.read_bytes())
            finally:
                if p.poll() is None: os.killpg(p.pid,signal.SIGINT)
                try: p.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    os.killpg(p.pid,signal.SIGKILL); p.wait()

    def call(info, label, endpoint, body, status=200):
        request=urllib.request.Request(info['api']+endpoint,data=json.dumps(body).encode(), headers={
            'Authorization':'Bearer '+info['token'],'Content-Type':'application/json'})
        try: response=urllib.request.build_opener(urllib.request.ProxyHandler({})).open(request,timeout=20)
        except urllib.error.HTTPError as e: response=e
        with response:
            raw=response.read(); (out/(label+'.json')).write_bytes(raw)
            require(response.code==status, f'{label}: expected {status}, saw {response.code}')
            return json.loads(raw)
    def quote(info, label, rev):
        q=call(info,label,'/v1/quote',info['request']); rank=q['quality']['costRanking']
        require(rank['priceBookHash']==digest(books[rev]) and rank['publicationRevision']==rev and
                rank['costAdjustmentApplied'] is True and q['verification']['success'] is False, 'Wrong quote scope/version')
        return q['quoteId']

    install(1); rejected('missing','PRICE_PUBLICATION_JOURNAL_UNAVAILABLE')
    require(not journal.exists(),'Router bootstrapped missing journal')
    payload=dict(policy=policy,book=books[1],anchor=anchor,execution_timestamp_ms=anchor['timestamp_ms'])
    r=subprocess.run([str(cli),'initialize',str(journal)],input=json.dumps(payload).encode(),capture_output=True,timeout=10,check=True)
    (out/'bootstrap.json').write_bytes(r.stdout); disk(1)
    route='/internal/native-valuation/reload'
    publication='/internal/native-valuation/publication'
    with service('first') as info:
        old=quote(info,'first-quote',1); rejected('concurrent-writer','PRICE_PUBLICATION_JOURNAL_IN_USE')
        require(call(dict(info,token='invalid'),'unauthenticated',publication,{'operation':'status'},401)['error']=='UNAUTHORIZED','Unauthenticated publication status')
        for label,rev,changed in [('unchanged',1,False),('advance',2,True),('idempotent',2,False)]:
            install(rev)
            if changed:
                before=journal.read_bytes()
                preview=call(info,'preview-two',publication,{'operation':'preview'})
                require(preview['wouldAdvance'] and not preview['published'] and preview['candidateBookHash']==digest(books[2]) and preview['baseEntryHash']==disk(1),'Incorrect preview')
                apply=dict(operation='apply', **{k:preview[k] for k in ['baseEntryHash','candidateBookHash']})
                install(3)
                require(call(info,'candidate-conflict',publication,apply,409)['error']=='NATIVE_VALUATION_PUBLICATION_PRECONDITION','Changed candidate accepted')
                install(2)
                require(call(info,'base-conflict',publication,dict(apply,baseEntryHash='0'*64),409)['error']=='NATIVE_VALUATION_PUBLICATION_PRECONDITION','Changed base accepted')
                status=call(info,'before-apply',publication,{'operation':'status'})
                require(status['priceBookHash']==digest(books[1]) and status['journalEntryHash']==disk(1) and journal.read_bytes()==before,'Preview changed current state')
                v=call(info,label,publication,apply)
                require(call(info,'stale-apply',publication,apply,409)['error']=='NATIVE_VALUATION_PUBLICATION_PRECONDITION','Stale apply accepted')
            else:
                v=call(info,label,route,{})
            require(v['changed'] is changed and v['journalEntryHash']==disk(rev) and v['journalSequence']==rev and
                    v['restartRollbackProtected'] is True and v['fsyncConfirmed'] is True and v['productionVerified'] is False,'Wrong publish receipt')
        v=call(info,'old-build','/v1/quotes/'+old+'/build',{},409)
        require(v['error']=='NATIVE_VALUATION_QUOTE_PRICE_CHANGED','Old quote not revoked')
        quote(info,'new-quote',2)
    install(1); rejected('rollback','PRICE_PUBLICATION_JOURNAL_ROLLBACK_OR_CONFLICT')
    install(3); rejected('unpublished','PRICE_PUBLICATION_JOURNAL_ADVANCE_REQUIRED')
    install(2)
    with service('restarted') as info:
        status=call(info,'after-restart',publication,{'operation':'status'})
        require(status['priceBookHash']==digest(books[2]) and status['journalEntryHash']==disk(2) and status['publicationRevision']==2,'Status did not recover committed head')
        quote(info,'restart-quote',2)
        journal.rename(out/'saved.journal'); journal.write_bytes(b'{}\n'); install(3)
        require(call(info,'replaced',route,{},503)['error']=='PRICE_PUBLICATION_JOURNAL_REPLACED','Replaced writer not rejected')
        require(call(info,'closed-quote','/v1/quote',info['request'],503)['error']=='NATIVE_VALUATION_DURABLE_UNAVAILABLE','Old snapshot after disk error')
    install(2); rejected('corrupt','PRICE_PUBLICATION_JOURNAL_CORRUPT')
    journal.rename(out/'corrupt.journal'); (out/'saved.journal').rename(journal); disk(2)
    with service('recovered') as info: quote(info,'recovered-quote',2)
    operator_journal = out/'operator.journal'
    operator_journal.write_bytes(journal.read_bytes())
    operator_config = out/'operator-config.json'
    operator_config.write_text(json.dumps(dict(policy, publication_journal=str(operator_journal))))
    env['XONE_NATIVE_VALUATION_CONFIG'] = str(operator_config)
    with service('operator') as info:
        operator_replay(info, operator_config, cli, out/'operator-run', path, operator_journal, books, anchor['timestamp_ms'])
    report=dict(passed=True,startup_rejections=rejects,normal_http_quotes=4,durable_revisions=[1,2],
                online_candidate_and_base_preconditions_verified=True,
                operator_apply_once_and_readback_verified=True,
                production_ready=False,publicTransactionsSubmitted=False,harness_sha256=sha(binary),cli_sha256=sha(cli),
                artifacts={str(p.relative_to(out)):sha(p) for p in sorted(out.rglob('*')) if p.is_file()})
    (out/'report.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps(dict(passed=True, startup_rejections=len(rejects), normal_http_quotes=4)))


if __name__=='__main__': main(*sys.argv[1:])
