#!/usr/bin/env python3
"""Audit retained failed signed transactions independently of Rust fee results."""
import base64
import copy
import hashlib
import json
from pathlib import Path
import sys
from solana_native_cost_evidence import amount, canonical, require
from solana_native_economics_evidence import address


def digest(value):
    return hashlib.sha256(canonical(value)).hexdigest()


def verify(document):
    request, p, signed, tx, receipt = [document[k] for k in ['request', 'preparation', 'signed', 'executed', 'receipt']]
    wire = base64.b64decode(signed['signedTransaction'], validate=True)
    unsigned = base64.b64decode(p['unsignedTransaction'], validate=True)
    require(wire[:1] == b'\x01' and wire[65:] == unsigned[65:] and unsigned[:65] == b'\x01' + bytes(64), 'Signed message changed')
    require(wire[65] == 1 and wire[68] < 128, 'Unexpected legacy single-signer message')
    keys = [address(wire[69+i*32:101+i*32]) for i in range(wire[68])]
    require(keys[0] == request['caller'] and len(keys) == len(set(keys)), 'Payer or account identity changed')
    signature, transaction_hash = address(wire[1:65]), hashlib.sha256(wire).hexdigest()
    require(signed['signature'] == receipt['signature'] == signature and receipt['transactionSha256'] == transaction_hash, 'Receipt signature/hash changed')
    require(tx['transaction'] == [signed['signedTransaction'], 'base64'] and receipt['slot'] == tx['slot'], 'Executed transaction or slot changed')
    require(receipt['state'] == 'FAILED' and receipt['settlement'] is None and document['restored'] == receipt, 'Failed durable outcome changed')
    require(receipt['feePreflight']['transactionSha256'] == transaction_hash, 'Preflight transaction binding changed')
    meta, result = tx['meta'], receipt['failureFee']
    error = meta['err']
    require(isinstance(error, dict) and set(error) == {'InstructionError'}, 'Unsupported failure class')
    require(result['onchainError'] == receipt['error']['detail'] == error, 'On-chain error differs')
    before, after, fee = meta['preBalances'], meta['postBalances'], meta['fee']
    require(type(fee) is int and 0 <= fee < 2**64, 'Invalid actual fee')
    require(len(before) == len(after) == len(keys) and all(type(n) is int and 0 <= n < 2**64 for n in before + after), 'Invalid native balances')
    require(before[0] - after[0] == fee and before[1:] == after[1:], 'Failure did not roll back all other native balances')
    def tokens(side):
        rows = {}
        for row in meta[side]:
            i = row['accountIndex']
            require(type(i) is int and 0 <= i < len(keys) and i not in rows, 'Invalid/duplicate Token balance')
            rows[i] = (row['mint'], row['owner'], amount(row['uiTokenAmount']['amount']), row['uiTokenAmount']['decimals'], row.get('programId'))
        return rows
    left, right = tokens('preTokenBalances'), tokens('postTokenBalances')
    require(left == right, 'Token state did not roll back')
    for account, mint, owner, existing in [
        ('sourceTokenAccount', request['sellToken'], request['caller'], not p.get('sourceNativeSol') and not p.get('sourceAccountCreation')),
        ('destinationTokenAccount', request['buyToken'], request['recipient'], not p.get('destinationNativeSol') and not p.get('accountCreation') and not p.get('unwrapDestinationSol')),
    ]:
        if existing:
            row = left.get(keys.index(p[account]))
            require(row is not None and row[:2] == (mint, owner), 'Required existing Token metadata missing')
    require(result['modelVersion'] == 'xone-solana-failed-fee/1' and result['scope'] == 'FAILED_PREPARED_RECENT_BLOCKHASH_INSTRUCTION' and result['sourceAuthority'] == 'RPC_TRANSACTION_RECEIPT', 'Unsupported fee authority/model')
    require(result['chainKey'] == request['chainKey'] and result['preparationHash'] == p['preparationHash'] and result['signature'] == signature and result['transactionSha256'] == transaction_hash and result['slot'] == tx['slot'], 'Fee witness binding changed')
    require(result['nativeUnit'] == 'LAMPORTS' and amount(result['networkFeeNative']) == fee and amount(result['payerNativeBefore']) == before[0] and amount(result['payerNativeAfter']) == after[0] and result['payerLamportDelta'] == str(-fee), 'Fee did not come from actual native balances')
    require(result['nativeAccountCount'] == len(keys) and result['tokenAccountCount'] == len(left) and result['retainedRentDepositNative'] == '0', 'Failure capital/coverage differs')
    require(result['transactionResultHash'] == digest(tx) and result['metaProjectionHash'] == digest(meta), 'RPC projection hash differs')
    require(all(result[k] is False for k in ['settlementApplied', 'historicalBankVerified', 'independentProductionVerification', 'quoteRankingApplied']), 'Failed transaction promoted to success/qualification')
    require(all(result[k] is True for k in ['observedNativeRollbackMatched', 'observedTokenRollbackMatched']) and result['netBuyAmountEstimate'] is None and result['netSellAmountEstimate'] is None, 'Unsupported net amount')
    require(document['processed']['state'] == 'RECEIPT_PENDING' and document['processed']['failureFee'] is None and document['confirmed']['state'] == 'RECEIPT_PENDING' and document['confirmed']['failureFee'] == result, 'Premature final failure or fee lost')
    for key in ['sourceTokenAccount', 'destinationTokenAccount']:
        i = keys.index(p[key])
        require(before[i] == after[i] or i == 0, 'Failed funding step retained capital')
    return dict(fee=fee, native_accounts=len(keys), token_accounts=len(left), probes=len(document['probes']))


def audit_guards(document):
    mutations = [
        ('executed', 'meta', 'fee', 1),
        ('executed', 'meta', 'postBalances', 0, 0),
        ('executed', 'meta', 'postBalances', 1, 1),
        ('executed', 'meta', 'postTokenBalances', 0, 'uiTokenAmount', 'amount', '1'),
        ('executed', 'meta', 'err', None),
        ('receipt', 'failureFee', 'networkFeeNative', '1'),
        ('receipt', 'failureFee', 'netBuyAmountEstimate', '0'),
        ('receipt', 'failureFee', 'independentProductionVerification', True),
        ('receipt', 'failureFee', 'transactionResultHash', 'other'),
        ('receipt', 'failureFee', 'sourceAuthority', 'TRUSTED_BANK'),
    ]
    for mutation in mutations:
        changed = copy.deepcopy(document)
        parent = changed
        for key in mutation[:-2]:
            parent = parent[key]
        parent[mutation[-2]] = mutation[-1]
        # Keep restart equality so these probe the underlying operands/result.
        changed['restored'] = copy.deepcopy(changed['receipt'])
        try:
            verify(changed)
        except ValueError:
            pass
        else:
            raise ValueError('Audit accepted mutation: ' + str(mutation))
    return len(mutations)


def main(folder):
    folder = Path(folder)
    files = sorted(folder.glob('case-*.json'))
    require({p.name for p in files} == {f'case-{i}.json' for i in range(3)}, 'Missing actual failed swaps')
    rows = [dict(path=p.name, sha256=hashlib.sha256(p.read_bytes()).hexdigest(), audit_guards=audit_guards(json.loads(p.read_bytes())), **verify(json.loads(p.read_bytes()))) for p in files]
    require(rows[0]['probes'] == 18 and sum(r['probes'] for r in rows) == 18, 'Missing failure metadata guards')
    report = dict(passed=True, current_api_scenarios=3, original_sbf_failed_transactions=3, failure_metadata_probes=18, audit_mutations=sum(r['audit_guards'] for r in rows), cases=rows, publicTransactionsSubmitted=False, production_ready=False, all_requested_features_complete=False)
    (folder / 'report.json').write_text(json.dumps(report, indent=2) + '\n')
    print(json.dumps(report))


if __name__ == '__main__':
    main(sys.argv[1])
