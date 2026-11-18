#!/usr/bin/env python3
"""Independently audit native economics against retained signed RPC/SBF operands."""
import base64
import hashlib
import json
from pathlib import Path
import sys
from solana_native_cost_evidence import amount, canonical, require

NATIVE = 'native:solana:sol'
WSOL = 'So11111111111111111111111111111111111111112'
TOKEN = 'TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA'
ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz'


def address(raw):
    n = int.from_bytes(raw, 'big')
    value = ''
    while n:
        n, r = divmod(n, 58)
        value = ALPHABET[r] + value
    return '1' * (len(raw) - len(raw.lstrip(b'\0'))) + value


def signed_number(raw):
    require(isinstance(raw, str), 'Native delta must be a string')
    value = int(raw)
    require(str(value) == raw and abs(value) < 2**64, 'Invalid native delta')
    return value


def verify_costs(request, p, observed, source, transaction_hash):
    c = observed['costs']
    fee = amount(observed['feeLamports'])
    input_value, output = [amount(observed[k]) for k in ['actualInput', 'actualOutput']]
    receipt = source == 'RPC_TRANSACTION_RECEIPT'
    retained = amount(observed['actualPersistentRentLamports']) if receipt else amount(p['rentLamports']) - amount(p['rentRefundLamports'])
    require(retained >= 0, 'Refund exceeds rent deposit')
    delta = signed_number(observed['payerLamportDelta'])
    require(c['modelVersion'] == 'xone-solana-native-economics/1' and c['sourceAuthority'] == source, 'Wrong economics source')
    require(c['scope'] == 'EVALUATED_PREPARED_TRANSACTION_ONLY' and c['amountBasis'] == 'OBSERVED_ACTUAL_SETTLEMENT', 'Wrong economics amount/scope')
    require(c['chainKey'] == request['chainKey'] and c['preparationHash'] == p['preparationHash'] and c['transactionSha256'] == transaction_hash, 'Economics transaction binding changed')
    require(c['nativeUnit'] == 'LAMPORTS' and amount(c['networkFeeNative']) == fee and amount(c['retainedRentDepositNative']) == retained and signed_number(c['actualPayerNativeDelta']) == delta, 'Fees/deposits/native delta mismatch')
    require(c['depositTreatment'] == 'CAPITAL_DEPOSIT_SEPARATE_FROM_NETWORK_FEE', 'Capital treated as expense')
    require(all(c[k] is False for k in ['networkFeeIncludedInSwapAmounts', 'historicalBankVerified', 'independentProductionVerification', 'quoteRankingApplied', 'deductedFromQuoteOutput']), 'Unsupported fee or qualification claim')
    incoming = request['sellToken'] == NATIVE or (request['sellToken'] == WSOL and p.get('refundWrappedSource') is True and p.get('sourceAccountCreation') is True and p['sourceTokenProgram'] == TOKEN)
    outgoing = request['buyToken'] == NATIVE or (request['buyToken'] == WSOL and p.get('unwrapDestinationSol') is True and p['destinationTokenProgram'] == TOKEN)
    require(not (incoming and outgoing), 'Ambiguous two-sided native economics')
    net_input = net_output = None
    buy_reason, sell_reason, reason = 'OUTPUT_NOT_NATIVE_SETTLEMENT_ASSET', 'INPUT_NOT_NATIVE_SETTLEMENT_ASSET', 'NON_NATIVE_SETTLEMENT_ASSET'
    if incoming:
        require(delta == -input_value - fee - retained, 'Input native cash does not reconcile')
        if input_value + fee < 2**64:
            net_input, sell_reason, reason = str(input_value + fee), None, None
        else:
            sell_reason = reason = 'NATIVE_INPUT_COST_EXCEEDS_U64'
    if outgoing:
        require(request['recipient'] == request['caller'] and delta == output - fee - retained, 'Output native cash does not reconcile')
        if output >= fee:
            net_output, buy_reason, reason = str(output - fee), None, None
        else:
            buy_reason = reason = 'NETWORK_FEE_EXCEEDS_GROSS_OUTPUT'
    require(c['netBuyAmountEstimate'] == net_output and c['netSellAmountEstimate'] == net_input, 'Net amount must use actual settlement and network fee only')
    require(c['netBuyUnavailableReason'] == buy_reason and c['netSellUnavailableReason'] == sell_reason and c['unavailableReason'] == reason, 'Unavailable reason changed')
    require(c['tokenValuationStatus'] == ('NATIVE_UNIT_IDENTITY' if incoming or outgoing else 'UNAVAILABLE'), 'Unsupported token valuation')
    if incoming or outgoing:
        require(c['valuationToken'] == request['sellToken' if incoming else 'buyToken'] and c['nativeCashReconciliation'] == 'MATCHED', 'Valuation asset/cash claim changed')
        require(c['valuationRatio'] == dict(numerator='1', denominator='1', method='NATIVE_UNIT_IDENTITY', priceObservedAtMs=None, priceTimePolicy='IDENTICAL_NATIVE_ATOMS_NO_MARKET_PRICE'), 'Untrusted market price substituted')
    return dict(native_input=incoming, native_output=outgoing, retained_rent=retained, fee=fee, actual_input=input_value, actual_output=output, net_input=net_input, net_output=net_output)


def verify(document):
    request, q, p, e, receipt, signed, tx = [document[k] for k in ['request', 'quote', 'preparation', 'evidence', 'receipt', 'signed', 'executed']]
    for value, key in [(p, 'preparationHash'), (e, 'evidenceHash')]:
        require(hashlib.sha256(canonical({k: v for k, v in value.items() if k != key})).hexdigest() == value[key], key + ' mismatch')
    unsigned = base64.b64decode(p['unsignedTransaction'], validate=True)
    wire = base64.b64decode(signed['signedTransaction'], validate=True)
    require(unsigned[:65] == b'\x01' + b'\0' * 64 and wire[:1] == b'\x01' and wire[1:65] != b'\0' * 64 and wire[65:] == unsigned[65:], 'Signed message differs from preparation')
    require(unsigned[65:] == base64.b64decode(p['messageBase64'], validate=True), 'Prepared message differs')
    require(tx['transaction'] == [signed['signedTransaction'], 'base64'] and tx['meta']['err'] is None, 'Executed wire/failure changed')
    require(receipt['state'] == 'FINALIZED' and receipt['signature'] == signed['signature'] == address(wire[1:65]), 'Signed receipt identity changed')
    require(e['success'] is True and e['sourceBankUnchanged'] is True and e['preparationHash'] == p['preparationHash'], 'SBF binding failed')
    require(e['historicalBankVerified'] is False and e['independentProductionVerification'] is False, 'SBF authority promoted')
    require(e['transactionSha256'] == hashlib.sha256(unsigned).hexdigest(), 'SBF wire hash differs')
    observation = receipt['settlement']
    require(observation['verified'] is True and observation['transactionSha256'] == hashlib.sha256(wire).hexdigest(), 'Receipt wire hash differs')
    require(receipt['feePreflight']['transactionSha256'] == hashlib.sha256(wire).hexdigest(), 'Durable fee witness differs')
    # The legacy message in these single-signer fixtures has a one-byte account count.
    require(wire[65] == 1 and wire[68] < 128, 'Unexpected legacy message header')
    keys = [address(wire[69 + i * 32:101 + i * 32]) for i in range(wire[68])]
    require(keys[0] == request['caller'], 'Payer is not the first signed account')
    meta = tx['meta']
    before, after = meta['preBalances'], meta['postBalances']
    require(len(before) == len(after) == len(keys) and all(type(n) is int and 0 <= n < 2**64 for n in before + after), 'Invalid native balance arrays')
    fee = meta['fee']
    require(type(fee) is int and 0 <= fee < 2**64 and fee == amount(observation['feeLamports']), 'Actual fee differs')
    delta = after[0] - before[0]
    require(delta == signed_number(observation['payerLamportDelta']), 'Actual native balances differ')
    rent = 0
    for key, creation, temporary in [('sourceTokenAccount', 'sourceAccountCreation', 'refundWrappedSource'), ('destinationTokenAccount', 'accountCreation', 'unwrapDestinationSol')]:
        i = keys.index(p[key])
        if p.get(temporary) is True:
            require(before[i] == after[i] == 0, 'Temporary account was not closed')
        elif p.get(creation) is True:
            rent += after[i] - before[i]
    require(rent >= 0 and rent == amount(observation['actualPersistentRentLamports']), 'Actual capital deposit differs')
    def balance(side, account, mint, owner):
        rows = [r for r in meta[side] if r['accountIndex'] == keys.index(account)]
        require(len(rows) <= 1, 'Duplicate token balance')
        if not rows:
            return 0
        require(rows[0]['mint'] == mint and rows[0]['owner'] == owner, 'Token balance identity differs')
        return amount(rows[0]['uiTokenAmount']['amount'])
    actual_input = -delta - fee - rent if p.get('sourceNativeSol') or p.get('refundWrappedSource') else balance('preTokenBalances', p['sourceTokenAccount'], request['sellToken'], request['caller']) + amount(p['wrapLamports']) - balance('postTokenBalances', p['sourceTokenAccount'], request['sellToken'], request['caller'])
    actual_output = delta + fee + rent if p.get('destinationNativeSol') or p.get('unwrapDestinationSol') else balance('postTokenBalances', p['destinationTokenAccount'], request['buyToken'], request['recipient']) - balance('preTokenBalances', p['destinationTokenAccount'], request['buyToken'], request['recipient'])
    require(actual_input == amount(observation['actualInput']) == amount(e['actualInput']) and actual_output == amount(observation['actualOutput']) == amount(e['actualOutput']), 'Actual token/native settlement differs')
    require(actual_input <= amount(p['requiredInput']) and actual_output >= amount(p['minBuyAmount']), 'Settlement outside execution bounds')
    require(amount(e['feeLamports']) == amount(p['feeLamports']), 'SBF fee differs from prepared fee')
    left = verify_costs(request, p, e, 'RPC_CAPTURE_SBF_EXECUTION', e['transactionSha256'])
    right = verify_costs(request, p, observation, 'RPC_TRANSACTION_RECEIPT', observation['transactionSha256'])
    require(left == right, 'SBF and RPC economics differ')
    require(q.get('netBuyAmountEstimate') is None, 'FAST quote falsely promoted to fee-adjusted ranking')
    return left


def main(folder):
    folder = Path(folder)
    paths = sorted(folder.glob('case-*.json'))
    require({p.name for p in paths} == {f'case-{i}.json' for i in range(11)}, 'Missing current native economics scenarios')
    rows = [dict(path=p.name, sha256=hashlib.sha256(p.read_bytes()).hexdigest(), **verify(json.loads(p.read_bytes()))) for p in paths]
    require(any(r['native_input'] and r['retained_rent'] for r in rows), 'Missing retained-rent input case')
    require(any(r['native_output'] and r['net_output'] is None for r in rows), 'Missing negative net output case')
    special = next(r for r in rows if r['path'] == 'case-4.json')
    require(special['actual_output'] == 10971 and special['net_output'] == '5971', 'Missing actual excess output control')
    require(any(not r['native_input'] and not r['native_output'] for r in rows), 'Missing unavailable token control')
    report = dict(passed=True, current_api_scenarios=11, original_sbf_executions=11, signed_local_receipts=11, cases=rows, publicTransactionsSubmitted=False, production_ready=False, all_requested_features_complete=False)
    (folder / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({k: report[k] for k in ['passed', 'current_api_scenarios', 'original_sbf_executions', 'signed_local_receipts']}))


if __name__ == '__main__':
    main(sys.argv[1])
