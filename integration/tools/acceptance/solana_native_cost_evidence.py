#!/usr/bin/env python3
"""Audit current real API/captured-SBF artifacts with independent integer sums."""
import base64
import hashlib
import json
from pathlib import Path
import sys


def require(value, reason):
    if not value:
        raise ValueError(reason)


def canonical(value):
    raw = json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False)
    for text, escaped in [('<', '\\u003c'), ('>', '\\u003e'), ('&', '\\u0026'), ('\u2028', '\\u2028'), ('\u2029', '\\u2029')]:
        raw = raw.replace(text, escaped)
    return raw.encode()


def amount(value):
    require(isinstance(value, str) and value.isascii() and value.isdigit(), 'Amount must be a decimal string')
    result = int(value)
    require(str(result) == value and result < 2**64, 'Noncanonical or oversized native amount')
    return result


def verify(document):
    q, p, e = (document[key] for key in ['quote', 'preparation', 'evidence'])
    c = p['costs']
    require(p['ready'] is True and p['signed'] is False and p['submitted'] is False, 'Preparation boundary changed')
    require(hashlib.sha256(canonical({k: v for k, v in p.items() if k != 'preparationHash'})).hexdigest() == p['preparationHash'], 'Preparation hash changed')
    message = base64.b64decode(p['messageBase64'], validate=True)
    wire = base64.b64decode(p['unsignedTransaction'], validate=True)
    require(wire[:65] == b'\x01' + b'\0' * 64 and wire[65:] == message and len(wire) <= 1232, 'Wire/message mismatch')
    require(hashlib.sha256(message).hexdigest() == c['messageSha256'], 'Fee message identity changed')
    require(c['scope'] == 'DECLARED_SINGLE_SIGNER_PREPARED_SOLANA_TRANSACTION' and c['chainKey'] == q['chainKey'], 'Fee chain/scope binding changed')
    require(c['modelVersion'] == 'xone-solana-native-cost/1' and c['status'] == 'MESSAGE_BOUND_RPC_ESTIMATE' and c['nativeUnit'] == 'LAMPORTS', 'Cost model changed')
    require(c['sourceAuthority'] == 'PREPARATION_RPC_OBSERVATION' and c['historicalBankVerified'] is False, 'RPC fee promoted beyond source authority')
    require(c['planHash'] == p['planHash'] == q['planHash'] and c['quoteViewId'] == q['quoteViewId'], 'Cost plan binding changed')
    require(c['accountSnapshotHash'] == p['accountSnapshot']['snapshotHash'], 'Account source binding changed')
    require(c['accountObservedSlot'] == c['feeMinimumContextSlot'] == p['observedSlot'] and c['feeObservedSlot'] >= p['observedSlot'], 'Fee/account slot policy violated')
    require(c['feeContextPolicy'] == 'MIN_CONTEXT_SLOT_NOT_SAME_BANK', 'Fee context incorrectly asserts same bank')
    require(c['validity']['recentBlockhash'] == p['recentBlockhash'] and c['validity']['lastValidBlockHeight'] == p['lastValidBlockHeight'], 'Fee validity changed')
    fee, rent, refund, wrap, funding = [amount(p[key]) for key in ['feeLamports', 'rentLamports', 'rentRefundLamports', 'wrapLamports', 'totalFundingLamports']]
    principal = amount(p['requiredInput']) if p.get('sourceNativeSol') is True else wrap
    require(funding == fee + rent + principal < 2**64 and amount(p['payerLamports']) >= funding and refund <= rent, 'Native funding conservation failed')
    require(c['networkFeeNative'] == c['nativeKnownSubtotal'] == p['feeLamports'] == e['feeLamports'], 'RPC/SBF fee differs or fee double counted')
    require(c['nativeKnownSubtotalKind'] == 'NETWORK_FEE_ONLY' and amount(c['nativeFundingRequired']) == funding, 'Funding treated as fee')
    require(amount(c['rentDepositNative']) == rent and amount(c['expectedRefundableRentNative']) == refund and amount(c['expectedRetainedRentDepositNative']) == rent - refund, 'Rent principal classified as burned fee')
    require(amount(c['swapOrWrapPrincipalNative']) == principal and amount(c['wrapPrincipalNative']) == wrap, 'Swap/wrap principal treated as fee')
    require(all(c[key] is None for key in ['totalNativeEstimate', 'netBuyAmountEstimate', 'netSellAmountEstimate']) and c['tokenValuationStatus'] == 'UNAVAILABLE', 'Untrusted net cost published')
    require(e['success'] is True and e['sourceBankUnchanged'] is True and e['preparationHash'] == p['preparationHash'], 'Independent execution binding failed')
    require(e['actualInput'] == p['quotedInput'] and e['actualOutput'] == p['quotedOutput'], 'Independent actual settlement differs')
    return dict(protocols=q.get('protocolViewIds'), network_fee=fee, rent_deposit=rent, refundable_rent=refund, swap_or_wrap_principal=principal, fee_slot_ahead=c['feeObservedSlot'] > c['accountObservedSlot'])


def main(folder):
    folder = Path(folder)
    paths = sorted(folder.glob('case-*.json'))
    require({p.name for p in paths} == {f'case-{i}.json' for i in range(20)}, 'Missing current HTTP/SBF artifacts')
    rows = [dict(path=p.name, sha256=hashlib.sha256(p.read_bytes()).hexdigest(), **verify(json.loads(p.read_bytes()))) for p in paths]
    require(any(r['refundable_rent'] > 0 for r in rows) and any(r['rent_deposit'] > 0 for r in rows) and any(r['swap_or_wrap_principal'] > 0 for r in rows), 'Missing rent/refund/principal controls')
    require(sum(r['fee_slot_ahead'] for r in rows) == 1, 'Missing explicit ahead fee context')
    report = dict(passed=True, current_api_preparations=len(rows), original_sbf_executions=len(rows), cases=rows, publicTransactionsSubmitted=False, production_ready=False, all_requested_features_complete=False)
    (folder / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps({k: report[k] for k in ['passed', 'current_api_preparations', 'original_sbf_executions']}))


if __name__ == '__main__':
    main(sys.argv[1])
