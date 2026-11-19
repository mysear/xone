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
    reset = mode.startswith('reset-approval')
    sequential = reset or mode in ('approval', 'approval-expired')
    bank = report['executionBank']['resetApprovalReplay' if reset else 'approvalReplay'] if sequential else report['executionBank']
    source = dict(state_anchor=bank['state_anchor'],
                  token_in=tokens[0], token_out=tokens[-1])
    config, book, policy = prepare(directory / 'prices', 'stale' if mode == 'stale' else 'priced', source)
    env = {k: v for k, v in os.environ.items() if not k.startswith('XONE_')}
    env.update(XONE_SPLIT_BANK=str(bank_path), XONE_NATIVE_VALUATION_CONFIG=str(config),
               XONE_CANDIDATE_COST_MAX_PLANS='1' if mode == 'budget' else '16')
    if sequential:
        env['XONE_SPLIT_APPROVAL_REPLAY'] = 'reset' if reset else '1'
    if mode in ('approval-expired', 'reset-approval-expired'):
        from native_valuation_vectors import sign
        until = bank['sequences'][0]['runs'][-1]['execution_environment']['timestamp'] * 1000
        for index, entry in enumerate(book['prices']):
            obs = entry['observation']
            book['prices'][index] = sign(policy['policies'][index], obs['anchor'], int(obs['token_atoms']), int(obs['native_atoms']), until=until)
        Path(policy['observations_file']).write_text(json.dumps(book))
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
                require(ranking['optimizer']['evaluated_candidates'] == 4 and ranking['costAdjustmentApplied'] == (mode in ('priced', 'approval', 'reset-approval')), 'partial pricing influenced ranking')
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
                if sequential:
                    audit_approval(rows, bank, mode, exact, q, selected)
                    continue
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
                    approval = costs['callerApproval']
                    require(approval['bindingHash'] == row['bindingHash'] and approval['spender'] == target and
                            approval['owner'] == row['transaction']['from'] and approval['token'] == tokens[0] and
                            approval['requiredInput'] == outcome['actual_input_consumed'], 'allowance identity')
                    if 'splitExecutor' in row:
                        require(approval['status'] == 'SUFFICIENT' and int(approval['observedAllowance']) >= int(approval['requiredInput'])
                                and costs['approvalNative'] == '0', 'batch caller allowance not reconciled')
                    else:
                        require(approval['status'] == 'UNKNOWN' and costs['approvalNative'] is None, 'absent direct probe priced')
                    scores[plan] = int(economics['feeAdjustedSellAmount' if exact else 'feeAdjustedBuyAmount'])
                require(scores[q['planHash']] == (min if exact else max)(scores.values()), 'wrong fee-adjusted winner')
        finally:
            process.terminate()
            process.wait(timeout=10)


def audit_approval(rows, bank, mode, exact, quote, selected):
    require(len(rows) == 4 and sum('splitExecutor' in row for row in rows.values()) == 2, 'approval candidate coverage')
    runs = bank['sequences'][0]['runs']
    first, approval, last = runs[0], runs[-2], runs[-1]
    scores = {}
    for plan, row in rows.items():
        costs, out = row['costs'], row['outcome']
        economics = costs['transactionEconomics']
        split = 'splitExecutor' in row
        require(economics['additionalOperationsIncluded'] is split, 'candidate scope')
        swap = last if split else first
        expected_fee = lambda run, gas: gas * min(int(run['transaction_context']['gas_price']), int(run['execution_environment']['base_fee']) + int(run['transaction_context']['max_priority_fee_per_gas']))
        swap_fee = expected_fee(swap, out['execution']['gas_used'])
        require(int(costs['gasNative']) == swap_fee and row['transactionContext'] == swap['transaction_context'], 'swap fee/context')
        approval_fee = 0
        if split:
            receipt = out['approval']
            approval_fee = expected_fee(approval, receipt['gas_used'])
            if len(runs) == 3:
                reset_receipt = out['allowance_reset']
                reset_fee = expected_fee(first, reset_receipt['gas_used'])
                require(int(reset_receipt['transaction']['data'][-64:], 16) == 0 and reset_receipt['block_hash'] == first['execution_environment']['block_hash'], 'reset binding')
                require(reset_receipt['payer_after'] == receipt['payer_before'] and reset_receipt['beneficiary_after'] == receipt['beneficiary_before'], 'reset continuity')
                require([f['kind'] for f in costs['feeBreakdown']] == ['CALLER_ALLOWANCE_RESET','CALLER_APPROVAL','EVM_EXECUTION'] and int(costs['feeBreakdown'][0]['amountNative']) == reset_fee, 'reset breakdown')
                approval_fee += reset_fee
            require(int(costs['approvalNative']) == approval_fee and receipt['block_hash'] == approval['execution_environment']['block_hash'], 'approval fee/block')
            require(receipt['payer_after'] == out['native_fees']['caller_before'] and receipt['beneficiary_after'] == out['native_fees']['coinbase_before'], 'sequence continuity')
            require(int(receipt['transaction']['data'][-64:], 16) == int(out['actual_input_consumed']), 'approval amount')
        require(int(costs['nativeKnownSubtotal']) == int(costs['payerNetNativeDebit']) == swap_fee + approval_fee, 'combined fee')
        require(economics['valuationTimestampMs'] == swap['execution_environment']['timestamp'] * 1000, 'settlement time')
        side = 'sell' if exact else 'buy'
        if mode.endswith('-expired') and split:
            require(economics[side + 'UnavailableReason'] == 'NATIVE_VALUATION_TIME', 'expired settlement priced')
        else:
            fee = (swap_fee + approval_fee + 2) // 3 if exact else ((swap_fee + approval_fee) * 3 + 1) // 2
            require(int(economics[side + 'TokenFee']) == fee, 'combined token conversion')
            scores[plan] = int(out['actual_input_consumed']) + fee if exact else int(out['actual_buy_amount']) - fee
    if mode in ('approval', 'reset-approval'):
        require(scores[quote['planHash']] == (min if exact else max)(scores.values()), 'approval fee ranking')
    require(selected is not None and quote['costs']['approvalNative'] == selected['costs']['approvalNative'], 'selected approval disclosure')


if __name__ == '__main__':
    binary, bank, output = map(lambda arg: Path(arg).resolve(), sys.argv[1:])
    report = json.loads(bank.read_bytes())
    for mode in ['priced', 'budget', 'stale', 'missing-executor', 'approval', 'approval-expired', 'reset-approval', 'reset-approval-expired']:
        scenario(binary, bank, report, output / mode, mode)
    (output / 'report.json').write_text(json.dumps(dict(passed=True, scenarios=16)) + '\n')
