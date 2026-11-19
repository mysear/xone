"""Run both normal quote endpoints over a mined shared-pool bank."""
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import urllib.request

from evm_dynamic_fee_http import ROOT, require
from native_valuation_http import prepare


def scenario(binary, bank_path, report, directory, mode):
    binding = report['binding']
    tokens = binding['cases'][0]['legs'][0]['tokens']
    source = dict(state_anchor=report['executionBank']['state_anchor'],
                  token_in=tokens[0], token_out=tokens[-1])
    config, _, _ = prepare(directory / 'prices', 'stale' if mode == 'stale' else 'priced', source)
    env = {k: v for k, v in os.environ.items() if not k.startswith('XONE_')}
    env.update(XONE_SPLIT_BANK=str(bank_path), XONE_NATIVE_VALUATION_CONFIG=str(config),
               XONE_CANDIDATE_COST_MAX_PLANS='1' if mode == 'budget' else '16')
    if mode == 'missing-executor':
        env['XONE_SPLIT_NO_EXECUTOR'] = '1'
    info_path = directory / 'info.json'
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with (directory / 'process.log').open('w') as log:
        process = subprocess.Popen([binary, info_path], cwd=ROOT, env=env, stdout=log, stderr=log)
        try:
            start = time.monotonic()
            while not info_path.is_file():
                require(process.poll() is None and time.monotonic() - start < 30, 'startup: ' + str(directory))
                time.sleep(.05)
            info = json.loads(info_path.read_bytes())
            for exact, endpoint, key in [(False, 'quote', 'request'), (True, 'quote-exact-out', 'exactOutRequest')]:
                request = urllib.request.Request(info['api'] + '/v1/' + endpoint,
                    data=json.dumps(info[key]).encode(), headers={'Authorization': 'Bearer ' + info['token'], 'Content-Type': 'application/json'})
                with opener.open(request, timeout=65) as response:
                    q = json.load(response)
                (directory / (endpoint + '.json')).write_text(json.dumps(q) + '\n')
                ranking = q['quality']['costRanking']
                rows = ranking['candidateExecutions']
                require(len(q['routes']) == 2 and len({r.get('path', r)['edges'][-1]['pool_id'] for r in q['routes']}) == 1, 'expected shared final pool')
                require(ranking['optimizer']['evaluated_candidates'] == 4 and ranking['costAdjustmentApplied'] == (mode == 'priced'), 'partial pricing influenced ranking')
                require(ranking['completeQuoteCostRanking'] is False and q['verification']['level'] == 'INDICATIVE', 'unsupported verification scope')
                selected = rows.get(q['planHash'])
                require(selected == ranking['selectedExecution'], 'selected evidence mismatch')
                if selected is None:
                    require(q['costs']['gasNative'] is None, 'borrowed another plan fee')
                else:
                    require(q['costs']['bindingHash'] == selected['bindingHash'] and q['costs']['gasNative'] == selected['costs']['gasNative'], 'selected fee disclosure mismatch')
                if mode == 'missing-executor':
                    require(ranking['unavailableReasons'].get('EXECUTION_ADAPTER_UNSUPPORTED') == 2 and selected is None, 'missing split executor coverage')
                if mode == 'budget':
                    require(ranking['simulationBudgetExhausted'] and len(rows) == 1, 'candidate budget ignored')
                if mode != 'priced':
                    continue
                require(len(rows) == 4 and sum('splitExecutor' in row for row in rows.values()) == 2, 'direct/split coverage')
                scores = {}
                for plan, row in rows.items():
                    costs, outcome = row['costs'], row['outcome']
                    economics = costs['transactionEconomics']
                    require(row['bundleHash'] == info['bundleHash'] and economics['priceBookHash'] == ranking['priceBookHash'] and economics['bindingHash'] == row['bindingHash'], 'mixed sources')
                    require(costs['balanceReconciliation']['status'] == 'MATCHED' and economics['actualInput'] == outcome['actual_input_consumed'] and economics['actualOutput'] == outcome['actual_buy_amount'], 'missing measured amounts')
                    target = row.get('splitExecutor', row['executor'])['target']
                    require(target == row['transaction']['to'] and row['transactionContext'] == report['executionBank']['runs'][0]['transaction_context'], 'transaction identity')
                    scores[plan] = int(economics['feeAdjustedSellAmount' if exact else 'feeAdjustedBuyAmount'])
                require(scores[q['planHash']] == (min if exact else max)(scores.values()), 'wrong fee-adjusted winner')
        finally:
            process.terminate()
            process.wait(timeout=10)


if __name__ == '__main__':
    binary, bank, output = map(lambda arg: Path(arg).resolve(), sys.argv[1:])
    report = json.loads(bank.read_bytes())
    for mode in ['priced', 'budget', 'stale', 'missing-executor']:
        scenario(binary, bank, report, output / mode, mode)
    (output / 'report.json').write_text(json.dumps(dict(passed=True, scenarios=8)) + '\n')
