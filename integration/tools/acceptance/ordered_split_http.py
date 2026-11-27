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
from native_valuation_vectors import digest


def audit_shared_suffix(quote, pools, exact_out=False):
    evidence = quote['quality']['sharedSuffix']
    candidate = evidence['candidate']
    require(evidence['scope'] == 'SNAPSHOT_SHARED_V2_SUFFIX_REPLAY'
            and evidence['basisPlanHash'] == quote['planHash']
            and evidence['candidateHash'] == digest([
                'xone-shared-v2-suffix/1', quote['planHash'], candidate
            ]), 'Shared suffix was detached from the selected snapshot plan')
    state = {pool['pool_id']: [int(pool['reserve0']), int(pool['reserve1'])]
             for pool in pools}

    def swap_path(edges, amount):
        for edge in edges:
            reserves = state[edge['pool_id']]
            direction = 0 if edge['zero_for_one'] else 1
            credit = 1 - direction
            output = amount * 997 * reserves[credit] // (reserves[direction] * 1000 + amount * 997)
            reserves[direction] += amount
            reserves[credit] -= output
            amount = output
        return amount

    def exact_out_path(edges, amount):
        amounts = [amount]
        for edge in reversed(edges):
            reserves = state[edge['pool_id']]
            direction = 0 if edge['zero_for_one'] else 1
            wanted = amounts[-1]
            amounts.append(reserves[direction] * wanted * 1000 //
                           ((reserves[1 - direction] - wanted) * 997) + 1)
        for edge, required, received in zip(edges, reversed(amounts[1:]), reversed(amounts[:-1]), strict=True):
            reserves = state[edge['pool_id']]
            direction = 0 if edge['zero_for_one'] else 1
            reserves[direction] += required
            reserves[1 - direction] -= received
        return amounts[-1]

    steps = candidate['steps']
    branches, suffix = steps[:-1], steps[-1]
    require(len(branches) == len(quote['routes']), 'Shared suffix branch count changed')
    intermediate = input_total = 0
    weight = 0
    last_step = {}
    if exact_out:
        output = int(quote['buyAmount'])
        suffix_input = exact_out_path(suffix['edges'], output)
    for index, (branch, route) in enumerate(zip(branches, quote['routes'], strict=True)):
        edges = branch['edges']
        dependencies = sorted({last_step[edge['pool_id']] for edge in edges if edge['pool_id'] in last_step})
        require(branch['inputFromSteps'] == []
                and branch['poolStateFromSteps'] == dependencies
                and edges + suffix['edges'] == route.get('path', route)['edges'],
                'Shared prefix dependencies mismatch')
        for edge in edges:
            last_step[edge['pool_id']] = index
        if exact_out:
            weight += int(route['amount_out'])
            result = suffix_input * weight // output - intermediate
            amount = exact_out_path(edges, result)
        else:
            amount = int(route['amount_in'])
            result = swap_path(edges, amount)
        require(branch['amountIn'] == str(amount), 'Shared prefix input differs from replay')
        require(branch['amountOut'] == str(result), 'Shared prefix output differs from replay')
        input_total += amount
        intermediate += result
    if exact_out:
        require(intermediate == suffix_input and output == weight and input_total <= int(quote['inputBudget']),
                'ExactOut funding or target mismatch')
        expected_input = str(input_total)
    else:
        output = swap_path(suffix['edges'], intermediate)
        expected_input = quote['sellAmount']
    require(suffix['inputFromSteps'] == list(range(len(branches)))
            and suffix['poolStateFromSteps'] == [] and suffix['amountIn'] == str(intermediate)
            and suffix['amountOut'] == candidate['buyAmount'] == str(output),
            'Merged suffix did not consume each prefix output once')
    require(candidate['sellAmount'] == expected_input == str(input_total),
            'Shared candidate input is not conserved')


def scenario(binary, bank_path, report, directory, mode):
    binding = report['binding']
    tokens = binding['cases'][0]['legs'][0]['tokens']
    reset = mode.startswith('reset-approval')
    wrapping = mode.startswith('wrap')
    sequential = wrapping or reset or mode in ('approval', 'approval-expired')
    policy_mode = mode.removesuffix('-expired')
    replay_key = {'wrap-router-reset': 'wrapRouterResetReplay', 'wrap-router-approval': 'wrapRouterApprovalReplay', 'wrap': 'wrapReplay', 'wrap-approval': 'wrapApprovalReplay', 'wrap-reset': 'wrapResetApprovalReplay', 'reset-approval': 'resetApprovalReplay', 'approval': 'approvalReplay'}
    bank = report['executionBank'][replay_key[policy_mode]] if sequential else report['executionBank']
    source = dict(state_anchor=bank['state_anchor'],
                  token_in=tokens[0], token_out=tokens[-1])
    config, book, policy = prepare(directory / 'prices', 'stale' if mode == 'stale' else 'priced', source)
    env = {k: v for k, v in os.environ.items() if not k.startswith('XONE_')}
    env.update(XONE_SPLIT_BANK=str(bank_path), XONE_NATIVE_VALUATION_CONFIG=str(config),
               XONE_CANDIDATE_COST_MAX_PLANS='1' if mode == 'budget' else '16')
    if sequential:
        env['XONE_SPLIT_APPROVAL_REPLAY'] = policy_mode if wrapping else 'reset' if reset else '1'
    if mode.endswith('-expired'):
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
                audit_shared_suffix(q, binding['pools'], exact)
                require(ranking['optimizer']['evaluated_candidates'] == 4 and ranking['costAdjustmentApplied'] == (mode in ('priced', 'approval', 'reset-approval', 'wrap', 'wrap-approval', 'wrap-reset', 'wrap-router-reset', 'wrap-router-approval')), 'partial pricing influenced ranking')
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
                if wrapping:
                    audit_wrap(rows, bank, mode, exact, q, selected)
                    continue
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


def audit_wrap(rows, bank, mode, exact, quote, selected):
    require(len(rows) == 4 and sum('splitExecutor' in r for r in rows.values()) == 2, 'wrap candidate coverage')
    runs = bank['sequences'][0]['runs']
    deposit = runs[0]
    scores = {}
    for plan, row in rows.items():
        costs, out = row['costs'], row['outcome']
        wrap, native = out['wrap'], out['native_fees']
        approval = costs['callerApproval']
        expected_allowance = 2**256 - 1
        if 'splitExecutor' in row and mode.startswith(('wrap-reset', 'wrap-approval')):
            expected_allowance = int(mode.startswith('wrap-reset'))
        if 'splitExecutor' not in row and mode.startswith('wrap-router-'):
            expected_allowance = int(mode.startswith('wrap-router-reset'))
        require(approval['spender'] == row['transaction']['to'] and int(approval['observedAllowance']) == expected_allowance, 'source spender allowance')
        if expected_allowance >= int(out['actual_input_consumed']):
            require(approval['status'] == 'SUFFICIENT' and costs['approvalNative'] == '0', 'existing allowance fee')
        principal = int(out['actual_input_consumed']) - 1
        require(wrap['token_before'] == '1' and wrap['token_after'] == out['balances']['caller_sell_before'] == out['actual_input_consumed'], 'wrapped token continuity')
        require(int(wrap['transaction']['value']) == principal and wrap['transaction']['to'] == deposit['to'] and wrap['transaction']['data'] == deposit['data'], 'deposit transaction')
        charges, reserves = [], []
        schedule = runs if expected_allowance < int(out['actual_input_consumed']) else runs[:2]
        swap = schedule[-1]
        receipts = [wrap] + [out[key] for key in ('allowance_reset', 'approval') if out.get(key)]
        for run, gas in zip(schedule, [r['gas_used'] for r in receipts] + [out['execution']['gas_used']], strict=True):
            tx, env = run['transaction_context'], run['execution_environment']
            charges.append(gas * min(int(tx['gas_price']), int(env['base_fee']) + int(tx['max_priority_fee_per_gas'])))
            reserves.append(int(tx['gas_price']) * tx['gas_limit'])
        fees = sum(charges)
        require(wrap['gas_used'] == int(deposit['gasUsed']) and int(costs['wrapNative']) == charges[0] and int(costs['gasNative']) == charges[-1], 'operation fees')
        for before, after in zip(receipts, receipts[1:]):
            require(before['payer_after'] == after['payer_before'], 'funding operation continuity')
        if len(receipts) > 1:
            require(int(costs['approvalNative']) == sum(charges[1:-1]) and int(receipts[-1]['transaction']['data'][-64:], 16) == int(out['actual_input_consumed']), 'combined approval bound')
        require(receipts[-1]['payer_after'] == native['caller_before'] and int(wrap['payer_before']) - int(native['caller_after']) == principal + fees, 'payer continuity')
        funding = costs['nativeFunding']
        require(int(funding['wrappedPrincipalNative']) == principal and int(funding['totalNetDebitNative']) == principal + fees, 'funding includes principal')
        require(int(funding['minimumOpeningBalanceNative']) == principal + max(sum(charges[:i]) + reserve for i, reserve in enumerate(reserves)), 'opening balance')
        require(int(costs['payerNetNativeDebit']) == int(costs['nativeKnownSubtotal']) == fees, 'fee-only ranking input')
        economics = costs['transactionEconomics']
        require(economics['additionalOperationsIncluded'] and economics['valuationTimestampMs'] == swap['execution_environment']['timestamp'] * 1000, 'settlement time')
        side = 'sell' if exact else 'buy'
        if mode.endswith('-expired') and swap == runs[-1]:
            require(economics[side + 'UnavailableReason'] == 'NATIVE_VALUATION_TIME', 'expired price used')
        else:
            fee = (fees + 2) // 3 if exact else (fees * 3 + 1) // 2
            require(int(economics[side + 'TokenFee']) == fee, 'principal counted as fee')
            scores[plan] = int(out['actual_input_consumed']) + fee if exact else int(out['actual_buy_amount']) - fee
    if len(scores) == len(rows):
        require(scores[quote['planHash']] == (min if exact else max)(scores.values()), 'wrap fee ranking')
    require(selected is not None and quote['costs']['scope'] == ('SELECTED_LOCAL_WRAP_APPROVAL_AND_SWAP' if selected['outcome'].get('approval') else 'SELECTED_LOCAL_WRAP_AND_SWAP') and quote['costs']['nativeFunding'] == selected['costs']['nativeFunding'], 'selected funding disclosure')


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
        prefix = peak = 0
        schedule = runs if split else [first]
        receipts = ([out['allowance_reset']] if split and len(runs) == 3 else []) + ([out['approval']] if split else [])
        for run, charge in zip(schedule, [int(r['execution_fee']) for r in receipts] + [swap_fee], strict=True):
            tx = run['transaction_context']
            peak = max(peak, prefix + int(tx['gas_limit']) * int(tx['gas_price']))
            prefix += charge
        funding = costs['nativeFunding']
        require(int(funding['minimumOpeningBalanceNative']) == peak and int(funding['totalNetDebitNative']) == prefix, 'sequence funding peak')
        require(funding['bindingHash'] == row['bindingHash'] and int(funding['sourceBalanceNative']) >= peak and funding['additionalNativeRequired'] == '0', 'opening balance evidence')
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


def shared_suffix_scenario(binary, bank, report, output):
    output.mkdir(parents=True)
    info_path = output / 'ready.json'
    env = {key: value for key, value in os.environ.items() if not key.startswith('XONE_')}
    env['XONE_SHARED_SUFFIX_BANK'] = str(bank)
    env['XONE_QUOTE_TTL_MS'] = '10000'
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with (output / 'process.log').open('w') as log:
        process = subprocess.Popen([binary, info_path], cwd=ROOT, env=env, stdout=log, stderr=log)
        try:
            start = time.monotonic()
            while not info_path.is_file():
                require(process.poll() is None and time.monotonic() - start < 30, 'shared suffix startup')
                time.sleep(.05)
            info = json.loads(info_path.read_bytes())

            def post(path, body):
                request = urllib.request.Request(info['api'] + path, data=json.dumps(body).encode(),
                    headers={'Authorization': 'Bearer ' + info['token'], 'Content-Type': 'application/json'})
                try:
                    with opener.open(request, timeout=65) as response:
                        return response.status, json.load(response)
                except urllib.error.HTTPError as error:
                    return error.code, json.load(error)

            exact_out = report['binding']['cases'][0]['exactOut']
            endpoint, key = ('quote-exact-out', 'exactOutRequest') if exact_out else ('quote', 'request')
            status, quote = post('/v1/' + endpoint, info[key])
            require(status == 200, str(quote))
            audit_shared_suffix(quote, report['binding']['pools'], exact_out)
            candidate = quote['quality']['sharedSuffix']
            path = f"/v1/quotes/{quote['quoteId']}/simulate"
            status, replay = post(path, {'sharedSuffixCandidateHash': candidate['candidateHash']})
            require(status == 200 and replay['verification']['success'], str(replay))
            outcome = replay['outcome']
            require(replay['planHash'] == quote['planHash']
                    and replay['sharedSuffixCandidateHash'] == candidate['candidateHash']
                    and replay['transaction']['to'] == report['binding']['executor']['target']
                    and outcome['actual_buy_amount'] == candidate['candidate']['buyAmount']
                    and outcome['actual_input_consumed'] == candidate['candidate']['sellAmount'],
                    'candidate replay mismatch')
            if exact_out:
                required = int(outcome['actual_input_consumed'])
                cap = min((required * 10050 + 9999) // 10000, int(quote['inputBudget']))
                require(int(outcome['unused_input_budget']) == cap - required and
                        outcome['actual_buy_amount'] == quote['buyAmount'], 'ExactOut limit mismatch')
            require(replay['authority'] == 'PROVIDED_LOCAL_STATE' and not replay['verified']
                    and replay['costs']['callerApproval']['status'] == 'SUFFICIENT',
                    'candidate evidence scope or allowance')
            status, invalid = post(path, {'sharedSuffixCandidateHash': '0' * 64})
            require(status == 422 and invalid['error'] == 'EXECUTION_SHARED_SUFFIX_HASH_MISMATCH',
                    str(invalid))
            build_path = f"/v1/quotes/{quote['quoteId']}/build"
            build_request = {'sharedSuffixCandidateHash': candidate['candidateHash']}
            status, built = post(build_path, build_request)
            require(status == 200, str(built))
            for field in ('quoteId', 'quoteViewId', 'planHash', 'snapshotRef', 'sharedSuffixCandidateHash',
                          'bindingHash', 'bundleHash', 'transaction', 'transactionContext', 'costs', 'authority'):
                require(built[field] == replay[field], 'build evidence mismatch for ' + field)
            require(built['requiresSignature'] and not any(built[key] for key in ('signed', 'submitted', 'ready', 'verified')),
                    'build authority or signing status')
            require(built['simulationEvidenceHash'] == digest(replay) and
                    built['buildHash'] == digest({key: value for key, value in built.items() if key != 'buildHash'}),
                    'build or simulation evidence hash mismatch')
            status, invalid = post(build_path, {'sharedSuffixCandidateHash': '0' * 64})
            require(status == 422 and invalid['error'] == 'EXECUTION_SHARED_SUFFIX_HASH_MISMATCH', str(invalid))
            status, invalid = post(build_path, {})
            default_error = 'EXECUTION_ADAPTER_UNSUPPORTED' if exact_out else 'INDEPENDENT_SIMULATION_UNAVAILABLE'
            require(status == 422 and invalid['error'] == default_error, str(invalid))
            expires = quote['validity']['expiresAtMs'] / 1000
            time.sleep(max(0, expires - time.time()) + .05)
            status, invalid = post(build_path, build_request)
            require(status == 409 and invalid['error'] == 'QUOTE_EXPIRED', str(invalid))
            (output / 'quote.json').write_text(json.dumps(quote) + '\n')
            (output / 'simulation.json').write_text(json.dumps(replay) + '\n')
            (output / 'build.json').write_text(json.dumps(built) + '\n')
            (output / 'report.json').write_text(json.dumps(dict(passed=True, scenarios=6)) + '\n')
        finally:
            process.terminate()
            process.wait(timeout=10)


if __name__ == '__main__' and sys.argv[1:2] == ['--shared-suffix']:
    binary, bank, output = map(lambda arg: Path(arg).resolve(), sys.argv[2:])
    shared_suffix_scenario(binary, bank, json.loads(bank.read_bytes()), output)
elif __name__ == '__main__':
    wrapping = sys.argv[1:2] == ['--wrap']
    binary, bank, output = map(lambda arg: Path(arg).resolve(), sys.argv[2:] if wrapping else sys.argv[1:])
    report = json.loads(bank.read_bytes())
    for mode in (['wrap', 'wrap-expired', 'wrap-approval', 'wrap-approval-expired', 'wrap-reset', 'wrap-reset-expired','wrap-router-approval','wrap-router-approval-expired','wrap-router-reset','wrap-router-reset-expired'] if wrapping else ['priced', 'budget', 'stale', 'missing-executor', 'approval', 'approval-expired', 'reset-approval', 'reset-approval-expired']):
        scenario(binary, bank, report, output / mode, mode)
    (output / 'report.json').write_text(json.dumps(dict(passed=True, scenarios=20 if wrapping else 16)) + '\n')
